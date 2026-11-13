//! Decoding a watched Go Live stream: VP8 with libvpx and H264 with FFmpeg,
//! both inside libwebrtc, through native/decode.c; AV1 with libwebrtc's
//! dav1d, through native/av1.c; and H265 with the HEVC decoder of
//! vendor/ffmpeg-src's FFmpeg, through native/hevc.c, where it's built.
//!
//! A watched stream first asks the card's decoder, which leaves every picture
//! in a graphics card texture and scales it to the tile on the card through
//! native/d3d11dec.cc: FFmpeg's own Direct3D 11 decoder (native/hwdec.c), which
//! handles H264, HEVC and AV1 on any vendor's card. The software decoders above
//! are what reads a stream the card's can't, so the picture a viewer sees is
//! made either way.
//!
//! On Linux the card's decoder is FFmpeg's VA-API one (native/hwdec_vaapi.c),
//! which leaves each picture in a surface on the card and exports it as a
//! DMA-BUF the tile pass imports: the same pictures on the card, handed over
//! the way Linux's window systems take them, and no copy on the way.
//!
//! Which decoder a stream gets is this module's to say, not the native
//! shim's: [`Hardware::new`] opens the card's decoder for the codec, [`Live`]
//! falls back to the software decoder when it can't, and the keyframe wait and
//! the software fallback after the card's failure are the same either way.

use std::{
    ffi::{c_int, c_uchar, c_uint, c_void},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::codec::Codec;
use anyhow::{Result, bail};
use livekit::webrtc::native::yuv_helper;

unsafe extern "C" {
    fn yapfast_decoder_new(codec: c_int, threads: c_uint) -> *mut Raw;
    fn yapfast_decoder_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_decoder_free(decoder: *mut Raw);
    fn yapfast_av1_new(threads: c_uint) -> *mut Raw;
    fn yapfast_av1_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_av1_free(decoder: *mut Raw);
}

#[cfg(hevcdec)]
unsafe extern "C" {
    fn yapfast_hevc_new(threads: c_uint) -> *mut Raw;
    fn yapfast_hevc_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_hevc_free(decoder: *mut Raw);
}

#[cfg(d3d11dec)]
unsafe extern "C" {
    /// A decoder for `codec` (0 H264, 1 HEVC, 2 AV1), which opens around the
    /// size given and settles the stream's own on its first frame; null when
    /// this build or this machine has no such decoder. `device` is the
    /// `ID3D11Device` the pictures should live on, which the caller keeps
    /// alive, or null for one made here.
    fn yapfast_hwdec_new(
        codec: c_int,
        hint_width: c_int,
        hint_height: c_int,
        device: *mut c_void,
    ) -> *mut Raw;
    fn yapfast_hwdec_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        keyframe: c_int,
    ) -> c_int;
    fn yapfast_hwdec_size(decoder: *const Raw, width: *mut c_int, height: *mut c_int) -> c_int;
    fn yapfast_hwdec_rgba(
        decoder: *mut Raw,
        rgba: *mut c_uchar,
        width: c_int,
        height: c_int,
    ) -> c_int;
    fn yapfast_hwdec_error(decoder: *const Raw) -> *const c_uchar;
    /// The newest picture's 8-bit planes, which only a test asks for: it
    /// converts the decoder's own samples both ways and compares the pictures.
    #[cfg(test)]
    fn yapfast_hwdec_nv12(
        decoder: *mut Raw,
        y: *mut c_uchar,
        y_stride: c_int,
        u: *mut c_uchar,
        u_stride: c_int,
        v: *mut c_uchar,
        v_stride: c_int,
    ) -> c_int;
    fn yapfast_hwdec_free(decoder: *mut Raw);
}

/// The card's decoder on Linux, the twin of the Direct3D 11 one above:
/// FFmpeg's VA-API decoder, through native/hwdec_vaapi.c, which leaves each
/// picture in a surface on the card and exports it as a DMA-BUF that the tile
/// pass imports.
///
/// Compiled where this build's FFmpeg was configured to decode on the card —
/// `cfg(vaapidec)`, which build.rs reads out of that FFmpeg's configure — and
/// in a test build anywhere, where everything that needs no driver is tried: a
/// codec the card has no decoder for, and the descriptor a picture is exported
/// as.
#[cfg(all(target_os = "linux", not(d3d11dec), any(vaapidec, test)))]
pub(crate) mod vaapi {
    #[cfg(test)]
    use std::ffi::c_void;
    use std::{
        ffi::{c_int, c_uchar},
        os::fd::RawFd,
        ptr,
    };

    use crate::codec::Codec;
    use anyhow::{Result, bail};

    use crate::dmabuf::{Colour, Descriptor, MAX_PLANES, Plane};

    use super::message;

    #[repr(C)]
    pub(super) struct Raw {
        _private: [u8; 0],
    }

    /// One plane of a picture's DMA-BUF, as native/hwdec_vaapi.h describes it:
    /// the buffer holding it, where the plane starts in it, how far apart its
    /// rows are, and the buffer's size.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct RawPlane {
        /// The buffer's descriptor, which the decoder keeps open until its next
        /// picture, its release, or its drop.
        pub fd: c_int,
        pub offset: u32,
        pub stride: u32,
        pub size: u64,
        pub modifier: u64,
    }

    /// The picture a decoder exported, as native/hwdec_vaapi.h describes it: C
    /// values, so that the shim is in no Rust type's debt.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    pub(super) struct RawPicture {
        pub width: c_int,
        pub height: c_int,
        pub fourcc: u32,
        pub nb_planes: u32,
        pub plane: [RawPlane; MAX_PLANES],
        pub colour: RawColour,
    }

    /// The colour signal a picture was coded with, as the shim read it out of
    /// the stream: the range, matrix and curve its samples are in, in the
    /// numbers native/hwdec_vaapi.h lists.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct RawColour {
        pub range: c_int,
        pub matrix: c_int,
        pub transfer: c_int,
    }

    /// The C header's struct is this size: four 32-bit fields, four planes of
    /// 32 bytes, and the colour's three values, rounded up to the eight-byte
    /// alignment the planes' 64-bit fields ask for. A field added on either side
    /// without the other fails here instead of reading a neighbour's memory.
    const _: () = assert!(std::mem::size_of::<RawPicture>() == 160);

    unsafe extern "C" {
        /// A decoder for `codec` (0 H264, 1 HEVC, 2 AV1), on `device` — a
        /// render node — or, for null, the device FFmpeg finds; null when this
        /// build or this machine has no such decoder, and the reason is
        /// `yapfast_vadec_error(NULL)`.
        fn yapfast_vadec_new(
            codec: c_int,
            hint_width: c_int,
            hint_height: c_int,
            device: *const c_uchar,
        ) -> *mut Raw;
        /// 1 a picture is ready and exported, 0 the frame was kept for the
        /// pictures after it, -2 the stream's pictures aren't ones the tile
        /// pass shows, and any other negative number that it wasn't decoded.
        fn yapfast_vadec_decode(
            decoder: *mut Raw,
            data: *const c_uchar,
            size: usize,
            keyframe: c_int,
        ) -> c_int;
        fn yapfast_vadec_picture(decoder: *mut Raw, picture: *mut RawPicture) -> c_int;
        fn yapfast_vadec_release(decoder: *mut Raw);
        fn yapfast_vadec_error(decoder: *const Raw) -> *const c_uchar;
        fn yapfast_vadec_free(decoder: *mut Raw);
        /// [`Hardware::decode`]'s descriptor reading, without a decoder: `drm`
        /// is the `AVDRMFrameDescriptor` a DRM PRIME frame's `data[0]` points
        /// at, which a test builds. Only a test asks for it.
        #[cfg(test)]
        fn yapfast_vadec_describe(
            drm: *const c_void,
            picture: *mut RawPicture,
            error: *mut c_uchar,
            error_size: usize,
        ) -> c_int;
    }

    /// The newest picture, in the arrays a [`Descriptor`] borrows: its places
    /// and the descriptor holding each, which are the decoder's until it lets
    /// go of the picture.
    #[derive(Clone, Copy, Debug)]
    struct Picture {
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
        nb_planes: usize,
        planes: [Plane; MAX_PLANES],
        fds: [RawFd; MAX_PLANES],
        colour: Colour,
    }

    impl Picture {
        /// The shim's picture, in the shape the frames channel speaks: one
        /// modifier for the buffer, since a picture whose planes would be
        /// separate buffers is refused before it gets here.
        fn of(raw: &RawPicture) -> Self {
            let nb_planes = (raw.nb_planes as usize).min(MAX_PLANES);
            let mut planes = [Plane::default(); MAX_PLANES];
            let mut fds = [0; MAX_PLANES];
            for (index, plane) in raw.plane.iter().take(nb_planes).enumerate() {
                planes[index] = Plane {
                    offset: plane.offset,
                    stride: plane.stride,
                };
                fds[index] = plane.fd;
            }
            Self {
                width: raw.width as u32,
                height: raw.height as u32,
                fourcc: raw.fourcc,
                modifier: raw.plane.first().map_or(0, |plane| plane.modifier),
                nb_planes,
                planes,
                fds,
                colour: Colour::of(raw.colour.range, raw.colour.matrix, raw.colour.transfer),
            }
        }

        fn descriptor(&self) -> Descriptor<'_> {
            Descriptor {
                width: self.width,
                height: self.height,
                fourcc: self.fourcc,
                modifier: self.modifier,
                planes: &self.planes[..self.nb_planes],
                colour: self.colour,
                fds: &self.fds[..self.nb_planes],
            }
        }
    }

    /// The card's VA-API decoder for one watched stream: a picture in a surface
    /// on the card, exported as a DMA-BUF and never read by the processor.
    pub struct Hardware {
        raw: *mut Raw,
        /// The newest picture, while the decoder holds it: none until the
        /// stream's first picture, and none after it is released.
        picture: Option<Picture>,
    }

    // SAFETY: one thread uses a decoder at a time, since `Frame::Hardware`
    // holds it mutably, so moving one between threads is sound and using one
    // from two at once is not.
    unsafe impl Send for Hardware {}

    impl Hardware {
        /// The card's decoder for `codec`, on the VA-API device the machine
        /// has: FFmpeg tries each render node for one whose libva driver
        /// initialises, with the driver `LIBVA_DRIVER_NAME` names if it names
        /// one. VP8 never has one — no hardware decoder takes it — and the
        /// software decoder reads the rest.
        pub fn new(codec: Codec) -> Result<Self> {
            Self::open(codec, ptr::null())
        }

        /// The card's decoder on the render node `node` names, rather than the
        /// one FFmpeg finds: a machine with two cards has one it doesn't.
        #[cfg(test)]
        pub fn on(codec: Codec, node: &std::ffi::CStr) -> Result<Self> {
            Self::open(codec, node.as_ptr().cast())
        }

        fn open(codec: Codec, device: *const c_uchar) -> Result<Self> {
            let (which, what) = match codec {
                Codec::H264 => (0, "H264"),
                Codec::H265 => (1, "H265"),
                Codec::Av1 => (2, "AV1"),
                Codec::Vp8 => bail!("the graphics card has no VP8 decoder"),
            };
            // SAFETY: plain values, and a device that is a C string the call
            // reads, or the null one FFmpeg finds and keeps for the decoder.
            let raw = unsafe { yapfast_vadec_new(which, 0, 0, device) };
            if raw.is_null() {
                // SAFETY: the null decoder is what `error` reads the reason off.
                let reason = unsafe { message(yapfast_vadec_error(ptr::null())) };
                // A decoder that failed before it could say why is no reason to
                // quote an empty one.
                if reason.is_empty() {
                    bail!("no {what} hardware decoder: the machine gave no reason")
                }
                bail!("no {what} hardware decoder: {reason}")
            }
            Ok(Self { raw, picture: None })
        }

        /// The newest picture's size, as the decoder made it: (0, 0) until the
        /// stream's first picture.
        pub fn size(&self) -> (u32, u32) {
            self.picture
                .map_or((0, 0), |picture| (picture.width, picture.height))
        }

        /// Feeds one whole frame and exports the newest picture, which
        /// [`Self::dmabuf`] hands over. True says there is one, false that the
        /// decoder kept the frame for the pictures after it. A picture the tile
        /// pass can't show — 4:2:2, 4:4:4, 12-bit, or a 10-bit one whose colour
        /// signal the pass has no curve for — is refused with the reason, and
        /// the stream stays with the software decoder.
        pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<bool> {
            // The picture from the call before goes back to the card as the new
            // one is made: the caller has had it.
            self.picture = None;
            // SAFETY: the frame outlives the call, and one thread uses the
            // decoder at a time.
            let decoded = unsafe {
                yapfast_vadec_decode(self.raw, frame.as_ptr(), frame.len(), keyframe as c_int)
            };
            if decoded < 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the graphics card wouldn't decode a frame: {}", unsafe {
                    message(yapfast_vadec_error(self.raw))
                });
            }
            if decoded == 0 {
                return Ok(false);
            }
            let mut raw = RawPicture::default();
            // SAFETY: the decoder holds the picture its last decode exported,
            // and one thread uses the decoder at a time.
            if unsafe { yapfast_vadec_picture(self.raw, &mut raw) } != 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the picture wasn't readable as a DMA-BUF: {}", unsafe {
                    message(yapfast_vadec_error(self.raw))
                });
            }
            if raw.width <= 0 || raw.height <= 0 {
                bail!("the graphics card gave a picture with no size");
            }
            self.picture = Some(Picture::of(&raw));
            Ok(true)
        }

        /// The newest picture as the DMA-BUF descriptor the frames channel
        /// carries, while the decoder holds it: after [`Self::release`], the
        /// next picture or the decoder's drop, its descriptors are closed.
        pub fn dmabuf(&self) -> Option<Descriptor<'_>> {
            self.picture.as_ref().map(Picture::descriptor)
        }

        /// Says the caller is done with the newest picture: its descriptors are
        /// closed and its surface goes back to the card, once. Asking again
        /// asks about a picture that is already gone, and closes nothing twice.
        pub fn release(&mut self) {
            self.picture = None;
            // SAFETY: the decoder is this one's, and releasing a picture that is
            // already gone releases nothing.
            unsafe { yapfast_vadec_release(self.raw) };
        }
    }

    impl Drop for Hardware {
        fn drop(&mut self) {
            // SAFETY: raw came from the constructor `free` belongs with, and is
            // freed once.
            unsafe { yapfast_vadec_free(self.raw) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// FFmpeg's AVDRMFrameDescriptor, exactly as libavutil/hwcontext_drm.h
        /// declares it: what a DRM PRIME frame's `data[0]` points at, built here
        /// so that the shapes a driver exports are tried without a driver.
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Object {
            fd: c_int,
            size: u64,
            format_modifier: u64,
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        struct PlaneDescriptor {
            object_index: c_int,
            offset: isize,
            pitch: isize,
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Layer {
            format: u32,
            nb_planes: c_int,
            planes: [PlaneDescriptor; MAX_PLANES],
        }

        #[repr(C)]
        struct Drim {
            nb_objects: c_int,
            objects: [Object; MAX_PLANES],
            nb_layers: c_int,
            layers: [Layer; MAX_PLANES],
        }

        /// DRM's fourcc for the format a picture is exported as, and for the
        /// layers libva splits it into.
        const NV12: u32 = 0x3231_564e;
        const R8: u32 = 0x2020_3852;
        const GR88: u32 = 0x3838_5247;
        const P010: u32 = 0x3031_3050;
        const R16: u32 = 0x2036_3152;
        const GR32: u32 = 0x3233_5247;
        /// The modifier a buffer a driver exports without one has: linear.
        const LINEAR: u64 = 0;
        /// The one buffer of a 1080p NV12 picture, its size and its descriptor
        /// as the kernel hands it over.
        const FD: c_int = 7;
        const SIZE: u64 = 1_382_400;
        const LUMA_STRIDE: isize = 1920;
        const CHROMA_OFFSET: isize = 2_073_600;
        /// The same picture in 10 bits: 16 bits a sample, so its rows are twice
        /// as long and its chroma starts where two 16-bit luma planes end.
        const P010_STRIDE: isize = 3840;
        const P010_CHROMA_OFFSET: isize = 4_147_200;

        fn place(object_index: c_int, offset: isize, pitch: isize) -> PlaneDescriptor {
            PlaneDescriptor {
                object_index,
                offset,
                pitch,
            }
        }

        fn layer(format: u32, nb_planes: c_int, planes: [PlaneDescriptor; MAX_PLANES]) -> Layer {
            Layer {
                format,
                nb_planes,
                planes,
            }
        }

        /// One NV12 layer of two planes, which is what Mesa's drivers export.
        fn nv12_layer() -> Drim {
            Drim {
                nb_objects: 1,
                objects: [
                    Object {
                        fd: FD,
                        size: SIZE,
                        format_modifier: LINEAR,
                    },
                    Object {
                        fd: -1,
                        size: 0,
                        format_modifier: 0,
                    },
                    Object {
                        fd: -1,
                        size: 0,
                        format_modifier: 0,
                    },
                    Object {
                        fd: -1,
                        size: 0,
                        format_modifier: 0,
                    },
                ],
                nb_layers: 1,
                layers: [
                    layer(
                        NV12,
                        2,
                        [
                            place(0, 0, LUMA_STRIDE),
                            place(0, CHROMA_OFFSET, LUMA_STRIDE),
                            place(0, 0, 0),
                            place(0, 0, 0),
                        ],
                    ),
                    layer(NV12, 0, [place(0, 0, 0); MAX_PLANES]),
                    layer(NV12, 0, [place(0, 0, 0); MAX_PLANES]),
                    layer(NV12, 0, [place(0, 0, 0); MAX_PLANES]),
                ],
            }
        }

        /// What the shim makes of a descriptor a driver could hand over, and
        /// why, when it makes nothing of it.
        fn describe(drm: &Drim) -> Result<Picture, String> {
            let mut raw = RawPicture::default();
            let mut error = [0 as c_uchar; 256];
            // SAFETY: `drm` is laid out as FFmpeg's AVDRMFrameDescriptor, which
            // is what the shim reads, and the error buffer is one C string wide.
            let described = unsafe {
                yapfast_vadec_describe(
                    drm as *const Drim as *const c_void,
                    &mut raw,
                    error.as_mut_ptr(),
                    error.len(),
                )
            };
            if described != 0 {
                let text = String::from_utf8_lossy(&error);
                return Err(text.trim_end_matches('\0').to_owned());
            }
            Ok(Picture::of(&raw))
        }

        /// A picture libva exports as one NV12 layer of two planes is the
        /// buffer the tile pass imports, its planes' offsets and strides taken
        /// as they were exported, and both of them the one descriptor.
        #[test]
        fn a_picture_is_the_buffer_the_tile_pass_imports() {
            let picture = describe(&nv12_layer()).expect("one NV12 layer of two planes");
            assert_eq!(picture.fourcc, NV12);
            assert_eq!(picture.nb_planes, 2, "luma and chroma");
            assert_eq!(
                (picture.width, picture.height),
                (0, 0),
                "the shim says the size"
            );
            assert_eq!(
                (picture.planes[0].offset, picture.planes[0].stride),
                (0, LUMA_STRIDE as u32)
            );
            assert_eq!(
                (picture.planes[1].offset, picture.planes[1].stride),
                (CHROMA_OFFSET as u32, LUMA_STRIDE as u32)
            );
            assert_eq!(
                picture.fds[..2],
                [FD, FD],
                "both planes are one buffer, which the frames channel carries as one modifier"
            );
            assert_eq!(picture.modifier, LINEAR);
            let descriptor = picture.descriptor();
            assert_eq!(descriptor.planes.len(), 2);
            assert_eq!(descriptor.fds.len(), 2);
            assert_eq!(descriptor.fourcc, NV12);
        }

        /// libva's other export of the same picture: an R8 layer and a GR88
        /// one, a plane each. Both are the one NV12 the tile pass shows, and a
        /// reader that knew only the first shape would refuse a working card.
        #[test]
        fn separate_plane_layers_are_one_picture() {
            let mut drm = nv12_layer();
            drm.nb_layers = 2;
            drm.layers[0] = layer(R8, 1, [place(0, 0, LUMA_STRIDE); MAX_PLANES]);
            drm.layers[1] = layer(GR88, 1, [place(0, CHROMA_OFFSET, LUMA_STRIDE); MAX_PLANES]);
            let picture = describe(&drm).expect("an R8 layer and a GR88 one");
            assert_eq!(picture.fourcc, NV12);
            assert_eq!(picture.nb_planes, 2);
            assert_eq!(
                (picture.planes[0].offset, picture.planes[1].offset),
                (0, CHROMA_OFFSET as u32)
            );
            assert_eq!(picture.fds[..2], [FD, FD]);
        }

        /// A 10-bit picture is the buffer the tile pass imports as well: libva
        /// exports it as one P010 layer of two planes, or, as a driver with
        /// one plane a layer does, as an R16 layer and a GR32 one. Both are the
        /// same 10-bit 4:2:0, 16 bits a sample, and both are read as P010.
        #[test]
        fn a_ten_bit_picture_is_the_buffer_the_tile_pass_imports() {
            let mut ten_bit = nv12_layer();
            ten_bit.layers[0].format = P010;
            ten_bit.layers[0].planes[0].pitch = P010_STRIDE;
            ten_bit.layers[0].planes[1] = place(0, P010_CHROMA_OFFSET, P010_STRIDE);
            let picture = describe(&ten_bit).expect("one P010 layer of two planes");
            assert_eq!(picture.fourcc, P010, "the format the tile pass reads");
            assert_eq!(picture.nb_planes, 2, "luma and chroma");
            assert_eq!(
                (picture.planes[0].offset, picture.planes[0].stride),
                (0, P010_STRIDE as u32),
                "16 bits a sample, so twice an 8-bit row"
            );
            assert_eq!(
                (picture.planes[1].offset, picture.planes[1].stride),
                (P010_CHROMA_OFFSET as u32, P010_STRIDE as u32)
            );
            assert_eq!(picture.fds[..2], [FD, FD]);

            // The same picture, split into a layer a plane.
            let mut split = nv12_layer();
            split.nb_layers = 2;
            split.layers[0] = layer(R16, 1, [place(0, 0, P010_STRIDE); MAX_PLANES]);
            split.layers[1] = layer(
                GR32,
                1,
                [place(0, P010_CHROMA_OFFSET, P010_STRIDE); MAX_PLANES],
            );
            let picture = describe(&split).expect("an R16 layer and a GR32 one");
            assert_eq!(picture.fourcc, P010);
            assert_eq!(picture.nb_planes, 2);
            assert_eq!(
                (picture.planes[0].offset, picture.planes[1].offset),
                (0, P010_CHROMA_OFFSET as u32)
            );
        }

        /// A picture the tile pass can't show is still refused with the reason,
        /// before anything is imported: a 4:4:4 stream's three layers, and a
        /// picture in two buffers, which the frames channel has one modifier's
        /// room for.
        #[test]
        fn a_picture_the_tile_pass_can_t_show_is_refused() {
            let mut four_four_four = nv12_layer();
            four_four_four.nb_layers = 3;
            for index in 0..3 {
                four_four_four.layers[index] = layer(R8, 1, [place(0, 0, LUMA_STRIDE); MAX_PLANES]);
            }
            let reason = describe(&four_four_four).expect_err("three layers aren't 4:2:0");
            assert!(
                reason.contains("3 layers"),
                "the reason says what it was: {reason}"
            );

            let mut two_buffers = nv12_layer();
            two_buffers.nb_objects = 2;
            two_buffers.objects[1] = Object {
                fd: 8,
                size: SIZE,
                format_modifier: LINEAR,
            };
            two_buffers.layers[0].planes[1] = place(1, 0, LUMA_STRIDE);
            let reason = describe(&two_buffers).expect_err("two buffers, one modifier");
            assert!(
                reason.contains("different buffers"),
                "the reason says what it was: {reason}"
            );

            let mut nothing = nv12_layer();
            nothing.nb_layers = 0;
            assert!(describe(&nothing).is_err(), "a picture with nothing in it");
        }

        /// A codec the card has no decoder for — VP8, which no hardware decoder
        /// takes — is a reason and not a watch that ends.
        #[test]
        fn a_codec_the_card_has_no_decoder_for_says_so() {
            let Err(reason) = Hardware::new(Codec::Vp8) else {
                panic!("no card decodes VP8")
            };
            assert_eq!(
                format!("{reason:#}"),
                "the graphics card has no VP8 decoder"
            );
        }

        /// Opening the card's decoder either gives one — a machine whose driver
        /// takes a session — or says why it didn't, in the shim's words, so that
        /// a stream read on the processor is never one nobody explained.
        #[test]
        fn the_card_decoder_is_opened_or_the_reason_is_said() {
            for (codec, what) in [
                (Codec::H264, "H264"),
                (Codec::H265, "H265"),
                (Codec::Av1, "AV1"),
            ] {
                match Hardware::new(codec) {
                    Ok(mut hardware) => {
                        eprintln!("this machine's card decodes {what}");
                        assert_eq!(hardware.size(), (0, 0), "no picture before a frame");
                        assert!(hardware.dmabuf().is_none());
                        hardware.release();
                    }
                    Err(reason) => {
                        let reason = format!("{reason:#}");
                        assert!(
                            reason.starts_with(&format!("no {what} hardware decoder: ")),
                            "a refusal says which decoder and why: {reason}"
                        );
                        assert!(
                            !reason.ends_with("the machine gave no reason"),
                            "the shim says why: {reason}"
                        );
                        eprintln!("{codec:?} isn't decoded on this machine's card: {reason}");
                    }
                }
            }
        }
    }
}

/// NVIDIA's card decoding a watched stream, where VA-API has nothing to open:
/// NVDEC through the driver's own API (`native/hwdec_cuda.c`), which leaves each
/// decoded picture on the card and exports it as a DMA-BUF the tile imports where
/// it lies. The same shape as [`vaapi`], since what the tile pass does with a
/// picture is the same either way.
#[cfg(all(target_os = "linux", not(d3d11dec), nvdec))]
mod cuda {
    use std::{
        ffi::{c_int, c_uchar},
        os::fd::RawFd,
    };

    use crate::codec::Codec;
    use anyhow::{Result, bail};

    use super::message;
    use crate::dmabuf::{Colour, Descriptor, MAX_PLANES, Plane};

    /// The planes the shim's picture names: luma and its interleaved chroma,
    /// which is NV12 at 8 bits and P016 at 10.
    const YAPFAST_PLANES: usize = 2;

    /// One plane of a picture's buffer, laid out as `struct yapfast_cuplane`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct RawPlane {
        fd: c_int,
        offset: u32,
        stride: u32,
        size: u64,
        modifier: u64,
    }

    /// The newest picture, laid out as `struct yapfast_cupicture`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct RawPicture {
        width: c_int,
        height: c_int,
        fourcc: u32,
        planes: u32,
        plane: [RawPlane; YAPFAST_PLANES],
        colour: RawColour,
    }

    /// The colour signal a picture was coded with, laid out as `struct
    /// yapfast_cucolour`: the range, matrix and curve its samples are in, in the
    /// numbers native/hwdec_cuda.h lists.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct RawColour {
        range: c_int,
        matrix: c_int,
        transfer: c_int,
    }

    /// The C header's struct is this size: two planes of 32 bytes after four
    /// 32-bit fields, and the colour's three values, rounded up to the eight-byte
    /// alignment the planes' 64-bit fields ask for. A field added on either side
    /// without the other fails here instead of reading a neighbour's memory.
    const _: () = assert!(std::mem::size_of::<RawPicture>() == 96);

    /// The decoder, which only the shim knows the inside of.
    #[repr(C)]
    struct Raw {
        _private: [u8; 0],
    }

    unsafe extern "C" {
        /// A decoder for `codec` (0 H264, 1 HEVC, 2 AV1) on the machine's card.
        /// The card's own decoder is made when the stream's first sequence header
        /// says how large its pictures are, so no size is asked for here.
        fn yapfast_cuvadec_new(codec: c_int) -> *mut Raw;
        fn yapfast_cuvadec_decode(
            decoder: *mut Raw,
            data: *const c_uchar,
            size: usize,
            keyframe: c_int,
        ) -> c_int;
        fn yapfast_cuvadec_picture(decoder: *mut Raw, picture: *mut RawPicture) -> c_int;
        fn yapfast_cuvadec_release(decoder: *mut Raw);
        fn yapfast_cuvadec_error(decoder: *const Raw) -> *const c_uchar;
        fn yapfast_cuvadec_free(decoder: *mut Raw);
    }

    /// The newest picture, in the arrays a [`Descriptor`] borrows: its places and
    /// the descriptor holding each, which are the decoder's until it lets go of
    /// the picture.
    #[derive(Clone, Copy, Debug)]
    struct Picture {
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
        nb_planes: usize,
        planes: [Plane; MAX_PLANES],
        fds: [RawFd; MAX_PLANES],
        colour: Colour,
    }

    impl Picture {
        /// The shim's picture in the shape the frames channel speaks: one
        /// modifier for the buffer, and both planes in it, which is what an NVDEC
        /// surface and the shim's own copy of one are.
        fn of(raw: &RawPicture) -> Self {
            let nb_planes = (raw.planes as usize).min(MAX_PLANES);
            let mut planes = [Plane::default(); MAX_PLANES];
            let mut fds = [0; MAX_PLANES];
            for (index, plane) in raw.plane.iter().take(nb_planes).enumerate() {
                planes[index] = Plane {
                    offset: plane.offset,
                    stride: plane.stride,
                };
                fds[index] = plane.fd;
            }
            Self {
                width: raw.width as u32,
                height: raw.height as u32,
                fourcc: raw.fourcc,
                modifier: raw.plane.first().map_or(0, |plane| plane.modifier),
                nb_planes,
                planes,
                fds,
                colour: Colour::of(raw.colour.range, raw.colour.matrix, raw.colour.transfer),
            }
        }

        fn descriptor(&self) -> Descriptor<'_> {
            Descriptor {
                width: self.width,
                height: self.height,
                fourcc: self.fourcc,
                modifier: self.modifier,
                planes: &self.planes[..self.nb_planes],
                colour: self.colour,
                fds: &self.fds[..self.nb_planes],
            }
        }
    }

    /// The card's NVDEC decoder for one watched stream: a picture on the card,
    /// exported as a DMA-BUF and never read by the processor.
    pub struct Hardware {
        raw: *mut Raw,
        /// The newest picture, while the decoder holds it: none until the
        /// stream's first picture, and none after it is released.
        picture: Option<Picture>,
    }

    // SAFETY: one thread uses a decoder at a time, since `Frame::Hardware` holds
    // it mutably, so moving one between threads is sound and using one from two
    // at once is not.
    unsafe impl Send for Hardware {}

    impl Hardware {
        /// The card's decoder for `codec`, on the NVIDIA card `CUDA_VISIBLE_DEVICES`
        /// names, or the one there is. VP8 never has one — no hardware decoder
        /// takes it — and the software decoder reads the rest.
        pub fn new(codec: Codec) -> Result<Self> {
            let (which, what) = match codec {
                Codec::H264 => (0, "H264"),
                Codec::H265 => (1, "H265"),
                Codec::Av1 => (2, "AV1"),
                Codec::Vp8 => bail!("the graphics card has no VP8 decoder"),
            };
            // SAFETY: a plain value, and the driver's own loader is what answers.
            let raw = unsafe { yapfast_cuvadec_new(which) };
            if raw.is_null() {
                // SAFETY: the null decoder is what `error` reads the reason off.
                let reason = unsafe { message(yapfast_cuvadec_error(std::ptr::null())) };
                if reason.is_empty() {
                    bail!("no {what} hardware decoder: the machine gave no reason")
                }
                bail!("no {what} hardware decoder: {reason}")
            }
            Ok(Self { raw, picture: None })
        }

        /// The newest picture's size, as the decoder made it: (0, 0) until the
        /// stream's first picture.
        pub fn size(&self) -> (u32, u32) {
            self.picture
                .map_or((0, 0), |picture| (picture.width, picture.height))
        }

        /// Feeds one whole frame and exports the newest picture, which
        /// [`Self::dmabuf`] hands over. True says there is one, false that the
        /// decoder kept the frame for the pictures after it.
        pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<bool> {
            // The picture from the call before goes back to the card as the new
            // one is made: the caller has had it.
            self.picture = None;
            // SAFETY: the frame outlives the call, and one thread uses the
            // decoder at a time.
            let decoded = unsafe {
                yapfast_cuvadec_decode(self.raw, frame.as_ptr(), frame.len(), keyframe as c_int)
            };
            if decoded < 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the graphics card wouldn't decode a frame: {}", unsafe {
                    message(yapfast_cuvadec_error(self.raw))
                });
            }
            if decoded == 0 {
                return Ok(false);
            }
            let mut raw = RawPicture::default();
            // SAFETY: the decoder holds the picture its last decode exported, and
            // one thread uses the decoder at a time.
            if unsafe { yapfast_cuvadec_picture(self.raw, &mut raw) } != 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the picture wasn't readable as a DMA-BUF: {}", unsafe {
                    message(yapfast_cuvadec_error(self.raw))
                });
            }
            if raw.width <= 0 || raw.height <= 0 {
                bail!("the graphics card gave a picture with no size");
            }
            self.picture = Some(Picture::of(&raw));
            Ok(true)
        }

        /// The newest picture as the DMA-BUF descriptor the frames channel
        /// carries, while the decoder holds it.
        pub fn dmabuf(&self) -> Option<Descriptor<'_>> {
            self.picture.as_ref().map(Picture::descriptor)
        }

        /// Says the caller is done with the newest picture: its descriptor is
        /// closed and its surface goes back to the card, once.
        pub fn release(&mut self) {
            self.picture = None;
            // SAFETY: the decoder is this one's, and releasing a picture that is
            // already gone releases nothing.
            unsafe { yapfast_cuvadec_release(self.raw) };
        }
    }

    impl Drop for Hardware {
        fn drop(&mut self) {
            // SAFETY: raw came from the constructor `free` belongs with, and is
            // freed once.
            unsafe { yapfast_cuvadec_free(self.raw) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The fixture called `name`, as the repository keeps it.
        fn fixture(name: &str) -> Vec<u8> {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../testing/fixtures")
                    .join(name),
            )
            .unwrap_or_else(|error| panic!("the {name} fixture: {error}"))
        }

        /// The fixture called `name`, split into its access units at the
        /// delimiters a viewer reassembles them at — H264's NAL 9 and HEVC's 35
        /// — each NAL with a four-byte start code.
        fn units_of(name: &str) -> Vec<Vec<u8>> {
            let file = fixture(name);
            let starts: Vec<usize> = (0..file.len() - 2)
                .filter(|&at| file[at..at + 3] == [0, 0, 1])
                .map(|at| at + 3)
                .collect();
            let mut units: Vec<Vec<u8>> = vec![];
            for (index, &start) in starts.iter().enumerate() {
                let mut end = starts.get(index + 1).map_or(file.len(), |next| next - 3);
                if file[end - 1] == 0 {
                    end -= 1;
                }
                let nal = &file[start..end];
                let delimiter = nal[0] & 0x1f == 9 || nal[0] >> 1 == 35;
                if delimiter || units.is_empty() {
                    units.push(vec![]);
                }
                if !delimiter {
                    let unit = units.last_mut().expect("a unit to append to");
                    unit.extend_from_slice(&[0, 0, 0, 1]);
                    unit.extend_from_slice(nal);
                }
            }
            units.retain(|unit| !unit.is_empty());
            units
        }

        /// A 10-bit stream decodes on NVIDIA's card as a P016 picture — 4:2:0 in
        /// 16-bit samples with the 10 bits at their bottom, which is the layout
        /// NVDEC decodes into — and carries out the matrix, range and curve its
        /// VUI declared, which here is HDR10: BT.2020 in the PQ curve, limited
        /// range. A machine whose driver can't decode 10 bits says so here, and
        /// the stream stays with the software decoder.
        #[test]
        fn a_ten_bit_stream_decodes_on_the_card_as_a_p016_picture() {
            const P016: u32 = 0x3631_3050;
            let mut decoder = match Hardware::new(Codec::H265) {
                Ok(decoder) => decoder,
                Err(reason) => {
                    eprintln!("this machine's card didn't decode H265: {reason:#}");
                    return;
                }
            };
            let mut pictures = 0;
            // The access units, so that the parser sees each picture whole as a
            // stream's viewer reassembles them.
            for (index, unit) in units_of("stream-h265-10bit.h265").iter().enumerate() {
                let ready = match decoder.decode(unit, index == 0) {
                    Ok(ready) => ready,
                    Err(reason) => {
                        eprintln!("this machine's card didn't decode 10 bits: {reason:#}");
                        return;
                    }
                };
                if !ready {
                    continue;
                }
                let picture = decoder.dmabuf().expect("a picture, as a descriptor");
                assert_eq!(picture.fourcc, P016, "16-bit 4:2:0, as NVDEC decodes it");
                assert_eq!(picture.planes.len(), 2, "luma and chroma");
                assert_eq!(
                    (picture.width, picture.height),
                    (320, 180),
                    "the stream's own size, without the padded surface"
                );
                assert!(
                    picture.planes.iter().all(|plane| plane.stride >= 640),
                    "16 bits a sample, so rows of twice the pixels in bytes"
                );
                assert_eq!(
                    picture.colour,
                    Colour::of(0, 1, 1),
                    "limited range, BT.2020's matrix, the PQ curve, as the stream declares"
                );
                if pictures == 0 {
                    eprintln!(
                        "{}x{} as {:#010x}, modifier {:#018x}, {} planes, {:?}",
                        picture.width,
                        picture.height,
                        picture.fourcc,
                        picture.modifier,
                        picture.planes.len(),
                        picture.colour
                    );
                }
                pictures += 1;
                decoder.release();
            }
            assert!(
                pictures > 0,
                "the card decoded no picture of the 10-bit stream"
            );
        }

        /// A profile the card has no decoder for is refused with the reason, so
        /// that the stream falls back to the software decoder rather than
        /// showing nothing: a GeForce's NVDEC takes H264 at 8 bits, and reads a
        /// H264 High 10 stream without ever saying what its pictures are.
        #[test]
        fn a_picture_the_card_has_no_decoder_for_is_refused_by_name() {
            let mut decoder = match Hardware::new(Codec::H264) {
                Ok(decoder) => decoder,
                Err(reason) => {
                    eprintln!("this machine's card didn't decode H264: {reason:#}");
                    return;
                }
            };
            let units = units_of("stream-h264-10bit.h264");
            let mut refusal = None;
            for (index, unit) in units.iter().enumerate() {
                if let Err(reason) = decoder.decode(unit, index == 0) {
                    refusal = Some(format!("{reason:#}"));
                    break;
                }
            }
            let Some(reason) = refusal else {
                // A driver that decodes 10-bit H264 says so by decoding it, and
                // then there is nothing to refuse.
                eprintln!("this machine's card decodes H264 High 10, so nothing is refused");
                return;
            };
            assert!(
                reason.contains("10-bit") || reason.contains("profile"),
                "the reason says what the card couldn't read: {reason}"
            );
        }
    }
}

/// Intel's card decoding a watched stream through oneVPL, which is what is left
/// when VA-API won't open (`native/hwdec_mfx.c`): an iHD driver that fails, a
/// libva missing from the path, a generation whose VA driver is broken while the
/// card's own engine works. The surfaces are VA surfaces and exported as a
/// DMA-BUF, so the tile pass sees the same picture shape as every other shim.
#[cfg(all(target_os = "linux", not(d3d11dec), mfxdec))]
mod mfx {
    use std::{
        ffi::{c_int, c_uchar},
        os::fd::RawFd,
    };

    use crate::codec::Codec;
    use anyhow::{Result, bail};

    use super::message;
    use crate::dmabuf::{Colour, Descriptor, MAX_PLANES, Plane};

    /// The planes the shim's picture names: NV12's luma and its chroma.
    const YAPFAST_PLANES: usize = 2;

    /// One plane of a picture's buffer, laid out as `struct yapfast_cuplane`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct RawPlane {
        fd: c_int,
        offset: u32,
        stride: u32,
        size: u64,
        modifier: u64,
    }

    /// The newest picture, laid out as `struct yapfast_cupicture`.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct RawPicture {
        width: c_int,
        height: c_int,
        fourcc: u32,
        planes: u32,
        plane: [RawPlane; YAPFAST_PLANES],
    }

    /// The C header's struct is this size: two planes of 32 bytes after four
    /// 32-bit fields. A field added on either side without the other fails here
    /// instead of reading a neighbour's memory.
    const _: () = assert!(std::mem::size_of::<RawPicture>() == 80);

    /// The decoder, which only the shim knows the inside of.
    #[repr(C)]
    struct Raw {
        _private: [u8; 0],
    }

    unsafe extern "C" {
        /// A decoder for `codec` (0 H264, 1 HEVC, 2 AV1) on the machine's card.
        /// The card's own decoder is made when the stream's first sequence header
        /// says how large its pictures are, so no size is asked for here.
        fn yapfast_mfxdec_new(codec: c_int) -> *mut Raw;
        fn yapfast_mfxdec_decode(
            decoder: *mut Raw,
            data: *const c_uchar,
            size: usize,
            keyframe: c_int,
        ) -> c_int;
        fn yapfast_mfxdec_picture(decoder: *mut Raw, picture: *mut RawPicture) -> c_int;
        fn yapfast_mfxdec_release(decoder: *mut Raw);
        fn yapfast_mfxdec_error(decoder: *const Raw) -> *const c_uchar;
        fn yapfast_mfxdec_free(decoder: *mut Raw);
    }

    /// The newest picture, in the arrays a [`Descriptor`] borrows: its places and
    /// the descriptor holding each, which are the decoder's until it lets go of
    /// the picture.
    #[derive(Clone, Copy, Debug)]
    struct Picture {
        width: u32,
        height: u32,
        fourcc: u32,
        modifier: u64,
        nb_planes: usize,
        planes: [Plane; MAX_PLANES],
        fds: [RawFd; MAX_PLANES],
    }

    impl Picture {
        /// The shim's picture in the shape the frames channel speaks: one
        /// modifier for the buffer, and both planes in it, which is what an NVDEC
        /// surface and the shim's own copy of one are.
        fn of(raw: &RawPicture) -> Self {
            let nb_planes = (raw.planes as usize).min(MAX_PLANES);
            let mut planes = [Plane::default(); MAX_PLANES];
            let mut fds = [0; MAX_PLANES];
            for (index, plane) in raw.plane.iter().take(nb_planes).enumerate() {
                planes[index] = Plane {
                    offset: plane.offset,
                    stride: plane.stride,
                };
                fds[index] = plane.fd;
            }
            Self {
                width: raw.width as u32,
                height: raw.height as u32,
                fourcc: raw.fourcc,
                modifier: raw.plane.first().map_or(0, |plane| plane.modifier),
                nb_planes,
                planes,
                fds,
            }
        }

        fn descriptor(&self) -> Descriptor<'_> {
            Descriptor {
                width: self.width,
                height: self.height,
                fourcc: self.fourcc,
                modifier: self.modifier,
                planes: &self.planes[..self.nb_planes],
                colour: Colour::SDR,
                fds: &self.fds[..self.nb_planes],
            }
        }
    }

    /// The card's oneVPL decoder for one watched stream: a picture on the card,
    /// exported as a DMA-BUF and never read by the processor.
    pub struct Hardware {
        raw: *mut Raw,
        /// The newest picture, while the decoder holds it: none until the
        /// stream's first picture, and none after it is released.
        picture: Option<Picture>,
    }

    // SAFETY: one thread uses a decoder at a time, since `Frame::Hardware` holds
    // it mutably, so moving one between threads is sound and using one from two
    // at once is not.
    unsafe impl Send for Hardware {}

    impl Hardware {
        /// The card's decoder for `codec`, on the Intel card `the machine's Intel card`
        /// names, or the one there is. VP8 never has one — no hardware decoder
        /// takes it — and the software decoder reads the rest.
        pub fn new(codec: Codec) -> Result<Self> {
            let (which, what) = match codec {
                Codec::H264 => (0, "H264"),
                Codec::H265 => (1, "H265"),
                Codec::Av1 => (2, "AV1"),
                Codec::Vp8 => bail!("the graphics card has no VP8 decoder"),
            };
            // SAFETY: a plain value, and the driver's own loader is what answers.
            let raw = unsafe { yapfast_mfxdec_new(which) };
            if raw.is_null() {
                // SAFETY: the null decoder is what `error` reads the reason off.
                let reason = unsafe { message(yapfast_mfxdec_error(std::ptr::null())) };
                if reason.is_empty() {
                    bail!("no {what} hardware decoder: the machine gave no reason")
                }
                bail!("no {what} hardware decoder: {reason}")
            }
            Ok(Self { raw, picture: None })
        }

        /// The newest picture's size, as the decoder made it: (0, 0) until the
        /// stream's first picture.
        pub fn size(&self) -> (u32, u32) {
            self.picture
                .map_or((0, 0), |picture| (picture.width, picture.height))
        }

        /// Feeds one whole frame and exports the newest picture, which
        /// [`Self::dmabuf`] hands over. True says there is one, false that the
        /// decoder kept the frame for the pictures after it.
        pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<bool> {
            // The picture from the call before goes back to the card as the new
            // one is made: the caller has had it.
            self.picture = None;
            // SAFETY: the frame outlives the call, and one thread uses the
            // decoder at a time.
            let decoded = unsafe {
                yapfast_mfxdec_decode(self.raw, frame.as_ptr(), frame.len(), keyframe as c_int)
            };
            if decoded < 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the graphics card wouldn't decode a frame: {}", unsafe {
                    message(yapfast_mfxdec_error(self.raw))
                });
            }
            if decoded == 0 {
                return Ok(false);
            }
            let mut raw = RawPicture::default();
            // SAFETY: the decoder holds the picture its last decode exported, and
            // one thread uses the decoder at a time.
            if unsafe { yapfast_mfxdec_picture(self.raw, &mut raw) } != 0 {
                // SAFETY: the decoder is this one's, alive for as long as it is.
                bail!("the picture wasn't readable as a DMA-BUF: {}", unsafe {
                    message(yapfast_mfxdec_error(self.raw))
                });
            }
            if raw.width <= 0 || raw.height <= 0 {
                bail!("the graphics card gave a picture with no size");
            }
            self.picture = Some(Picture::of(&raw));
            Ok(true)
        }

        /// The newest picture as the DMA-BUF descriptor the frames channel
        /// carries, while the decoder holds it.
        pub fn dmabuf(&self) -> Option<Descriptor<'_>> {
            self.picture.as_ref().map(Picture::descriptor)
        }

        /// Says the caller is done with the newest picture: its descriptor is
        /// closed and its surface goes back to the card, once.
        pub fn release(&mut self) {
            self.picture = None;
            // SAFETY: the decoder is this one's, and releasing a picture that is
            // already gone releases nothing.
            unsafe { yapfast_mfxdec_release(self.raw) };
        }
    }

    impl Drop for Hardware {
        fn drop(&mut self) {
            // SAFETY: raw came from the constructor `free` belongs with, and is
            // freed once.
            unsafe { yapfast_mfxdec_free(self.raw) };
        }
    }
}

/// The card's decoder on Linux: VA-API where the machine's driver answers — AMD
/// and Intel — and NVDEC where it doesn't, which is NVIDIA's. Both hand out a
/// DMA-BUF, so what the tile pass does with a picture is the same either way.
#[cfg(all(target_os = "linux", not(d3d11dec), any(vaapidec, nvdec)))]
pub use linux::Hardware;

#[cfg(all(target_os = "linux", not(d3d11dec), any(vaapidec, nvdec)))]
mod linux {
    use crate::codec::Codec;
    use anyhow::{Result, bail};

    use crate::dmabuf::Descriptor;

    /// Which of the two decoders this machine's card took.
    pub struct Hardware(Kind);

    enum Kind {
        #[cfg(vaapidec)]
        Vaapi(super::vaapi::Hardware),
        #[cfg(nvdec)]
        Cuda(super::cuda::Hardware),
        #[cfg(mfxdec)]
        Mfx(super::mfx::Hardware),
    }

    impl Hardware {
        /// The card's decoder for `codec`: VA-API first, which is what AMD's and
        /// Intel's drivers answer, and NVDEC where that opens nothing. A machine
        /// with neither says why both were passed over, and the stream falls to
        /// the software decoder.
        pub fn new(codec: Codec) -> Result<Self> {
            let mut why = Vec::new();
            #[cfg(vaapidec)]
            match super::vaapi::Hardware::new(codec) {
                Ok(decoder) => return Ok(Self(Kind::Vaapi(decoder))),
                Err(error) => why.push(format!("VA-API: {error:#}")),
            }
            #[cfg(nvdec)]
            match super::cuda::Hardware::new(codec) {
                Ok(decoder) => return Ok(Self(Kind::Cuda(decoder))),
                Err(error) => why.push(format!("NVDEC: {error:#}")),
            }
            #[cfg(mfxdec)]
            match super::mfx::Hardware::new(codec) {
                Ok(decoder) => return Ok(Self(Kind::Mfx(decoder))),
                Err(error) => why.push(format!("oneVPL: {error:#}")),
            }
            bail!("no {} hardware decoder: {}", codec.name(), why.join("; "))
        }

        pub fn size(&self) -> (u32, u32) {
            match &self.0 {
                #[cfg(vaapidec)]
                Kind::Vaapi(decoder) => decoder.size(),
                #[cfg(nvdec)]
                Kind::Cuda(decoder) => decoder.size(),
                #[cfg(mfxdec)]
                Kind::Mfx(decoder) => decoder.size(),
            }
        }

        pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<bool> {
            match &mut self.0 {
                #[cfg(vaapidec)]
                Kind::Vaapi(decoder) => decoder.decode(frame, keyframe),
                #[cfg(nvdec)]
                Kind::Cuda(decoder) => decoder.decode(frame, keyframe),
                #[cfg(mfxdec)]
                Kind::Mfx(decoder) => decoder.decode(frame, keyframe),
            }
        }

        pub fn dmabuf(&self) -> Option<Descriptor<'_>> {
            match &self.0 {
                #[cfg(vaapidec)]
                Kind::Vaapi(decoder) => decoder.dmabuf(),
                #[cfg(nvdec)]
                Kind::Cuda(decoder) => decoder.dmabuf(),
                #[cfg(mfxdec)]
                Kind::Mfx(decoder) => decoder.dmabuf(),
            }
        }

        pub fn release(&mut self) {
            match &mut self.0 {
                #[cfg(vaapidec)]
                Kind::Vaapi(decoder) => decoder.release(),
                #[cfg(nvdec)]
                Kind::Cuda(decoder) => decoder.release(),
                #[cfg(mfxdec)]
                Kind::Mfx(decoder) => decoder.release(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// A stream's share of the machine's decoder threads, as the Yapfast that
    /// starts a worker works it out: a stream alone takes the four one decoder
    /// asks for however many cores there are, four streams on eight cores take
    /// two each, and a stream past the cores still takes the one thread a
    /// decoder can't do without.
    #[test]
    fn a_streams_share_of_the_decoder_threads() {
        assert_eq!(share(4, 1), 4, "a stream alone takes four threads");
        assert_eq!(share(32, 1), 4, "cores to spare don't buy one stream more");
        assert_eq!(share(8, 4), 2, "four streams on eight cores take two each");
        assert_eq!(share(4, 4), 1, "four streams on four cores take one each");
        assert_eq!(share(4, 8), 1, "past the cores, a stream still takes one");
        assert_eq!(share(1, 1), 1, "a one-core machine has one to give");
        assert_eq!(share(2, 2), 1, "a small machine gives what it has");
    }

    /// Watches that decode at once share one budget of threads: the first on
    /// a four-core machine gets the four threads a lone stream always got, the
    /// next gets the one a decoder can't do without rather than four more, and
    /// what one gives back another may claim.
    #[test]
    fn decoders_share_one_budget_of_threads() {
        let budget = Budget::new();
        let first = budget.claim(4);
        assert_eq!(
            first.count(),
            4,
            "a stream on a four-core machine gets four"
        );
        let second = budget.claim(4);
        assert_eq!(second.count(), 1, "a stream past the budget gets one");
        assert_eq!(budget.claimed(), 5, "five threads on four cores, not eight");
        // Cores to spare don't buy one stream more than four of them.
        assert_eq!(Budget::new().claim(32).count(), 4);
        // A small machine gives what it has, not four.
        assert_eq!(Budget::new().claim(2).count(), 2);
        // A decoder that goes gives its threads back, and the next claims as
        // many as the budget has left then.
        drop(first);
        assert_eq!(budget.claimed(), 1);
        let third = budget.claim(4);
        assert_eq!(third.count(), 3);
        assert_eq!(budget.claimed(), 4);
        drop((second, third));
        assert_eq!(budget.claimed(), 0, "every thread went back");
    }

    fn check(codec: Codec, frame: &[u8]) {
        let mut decoder = Decoder::new(codec).unwrap();
        let picture = decoder
            .decode(frame)
            .unwrap()
            .expect("a picture from a keyframe");
        assert_eq!((picture.width, picture.height), (320, 180), "{codec:?}");
        let mut rgba = vec![0; 320 * 180 * 4];
        picture.to_rgba(&mut rgba);
        assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        let colours: std::collections::HashSet<_> =
            rgba.chunks_exact(4).map(|pixel| pixel[0] / 32).collect();
        assert!(
            colours.len() > 3,
            "{codec:?} decoded to a picture, not a flat colour"
        );
        assert!(decoder.decode(b"not video").is_err() || codec != Codec::Vp8);
    }

    /// The first access unit of an Annex B recording: up to its second
    /// delimiter, whose NAL header starts with `delimiter`.
    fn first_unit(file: &[u8], delimiter: u8) -> &[u8] {
        let second = (1..file.len() - 4)
            .find(|&at| file[at..at + 5] == [0, 0, 0, 1, delimiter])
            .unwrap();
        &file[..second]
    }

    /// The first temporal unit of an AV1 recording: its OBUs, each giving
    /// its size, up to the second temporal delimiter (type 2).
    fn first_temporal_unit(file: &[u8]) -> Vec<u8> {
        let mut at = 0;
        loop {
            let (mut size, mut used) = (0, 0);
            while file[at + 1 + used] & 0x80 != 0 {
                size |= usize::from(file[at + 1 + used] & 0x7f) << (7 * used);
                used += 1;
            }
            size |= usize::from(file[at + 1 + used]) << (7 * used);
            let next = at + 2 + used + size;
            if at > 0 && (file[at] >> 3) & 0x0f == 2 {
                return file[..at].to_vec();
            }
            at = next;
        }
    }

    #[test]
    fn decodes_the_recorded_streams() {
        let vp8 = fixture("stream-vp8.ivf");
        let header = usize::from(u16::from_le_bytes([vp8[6], vp8[7]]));
        let size = u32::from_le_bytes(vp8[header..header + 4].try_into().unwrap()) as usize;
        check(Codec::Vp8, &vp8[header + 12..header + 12 + size]);
        check(Codec::H264, first_unit(&fixture("stream-h264.h264"), 9));
        // AV1 as it comes, and as DAVE leaves it, the last OBU without its
        // size.
        let unit = first_temporal_unit(&fixture("stream-av1.obu"));
        check(Codec::Av1, &unit);
        let elements = crate::bitstream::obu_elements(&unit).unwrap();
        let last = elements.last().unwrap();
        // Its header, then a size of one LEB128 byte, or two from 128 on.
        let sized_last = 1 + last.len() + usize::from(last.len() > 128);
        check(
            Codec::Av1,
            &[&unit[..unit.len() - sized_last], last.as_slice()].concat(),
        );
        // H265's delimiter is NAL type 35.
        if cfg!(hevcdec) {
            check(
                Codec::H265,
                first_unit(&fixture("stream-h265.h265"), 35 << 1),
            );
        }
    }

    /// A stream the card can decode is watched on the card: the decoder the
    /// watch asks for hands back a picture that never came to the processor,
    /// which is the path a viewer's frame takes when nothing goes wrong.
    #[cfg(d3d11dec)]
    #[test]
    // The card's own decoder, which this machine's Direct3D 11 path takes the
    // test process down with (an access violation before the first frame, with
    // no output) — so it is asked for by hand until that is understood.
    #[ignore = "needs a card whose Direct3D 11 decoder opens in the test process"]
    fn a_stream_the_card_can_decode_is_watched_on_the_card() {
        if Hardware::new(Codec::H264).is_err() {
            eprintln!("this machine's card has no H264 decoder");
            return;
        }
        let stream = fixture("stream-h264.h264");
        let unit = first_unit(&stream, 9);
        let mut live = Live::new(Codec::H264).unwrap();
        assert!(
            live.on_the_card(),
            "a card decoder was found and should be used"
        );
        match live.decode(unit, true).unwrap() {
            Some(Frame::Hardware(hardware)) => {
                let (width, height) = hardware.size();
                assert_eq!((width, height), (320, 180));
                let mut pixels = vec![0; width as usize * height as usize * 4];
                hardware.rgba(&mut pixels, width, height).unwrap();
                let lit = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [0; 3]);
                assert!(lit.count() > 1000, "the picture has a picture in it");
            }
            _ => panic!("the card decoded a keyframe and gave no picture"),
        }
    }

    /// A stream the card can decode is decoded there, and its newest picture
    /// exported as the DMA-BUF the tile pass would import: the path a viewer's
    /// frame takes where the driver takes a session. A machine whose driver or
    /// whose FFmpeg can't says so, and the stream stays with the software
    /// decoder either way.
    #[cfg(all(target_os = "linux", not(d3d11dec), any(vaapidec, test)))]
    #[test]
    fn a_stream_the_card_can_decode_is_a_buffer_on_the_card() {
        let mut hardware = match vaapi::Hardware::new(Codec::H264) {
            Ok(hardware) => hardware,
            Err(reason) => {
                eprintln!("this machine's card didn't decode H264: {reason:#}");
                return;
            }
        };
        let stream = fixture("stream-h264.h264");
        let unit = first_unit(&stream, 9);
        // A keyframe's picture is there once the decoder has read the unit, and
        // the same unit read again is the same picture for a decoder that held
        // the first one.
        let ready = matches!(hardware.decode(unit, true), Ok(true))
            || matches!(hardware.decode(unit, true), Ok(true));
        if !ready {
            eprintln!("the card wasn't given a keyframe this decoder shows");
            return;
        }
        assert_eq!(
            hardware.size(),
            (320, 180),
            "the stream's own size, without the padded surface"
        );
        let buffer = hardware.dmabuf().expect("a picture was exported");
        assert_eq!((buffer.width, buffer.height), (320, 180));
        assert_eq!(buffer.fourcc, 0x3231_564e, "'NV12', as DRM names it");
        assert_eq!(buffer.planes.len(), 2, "luma and chroma");
        assert!(buffer.planes.iter().all(|plane| plane.stride >= 320));
        assert!(
            buffer.fds.iter().all(|fd| *fd >= 0),
            "the picture's own descriptors"
        );
        // The caller says it is done: the descriptors are the decoder's from
        // then, and no picture is handed out until the next one.
        hardware.release();
        assert!(hardware.dmabuf().is_none());
    }

    /// The process's own kernel and user time, in milliseconds, as the
    /// measurement method counts it: Windows charges it in 15.6 ms ticks, so a
    /// measurement has to run long enough for the ticks to add up.
    #[cfg(d3d11dec)]
    fn cpu_millis() -> f64 {
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessTimes(
                process: *mut std::ffi::c_void,
                created: *mut FileTime,
                exited: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }
        let blank = || FileTime { low: 0, high: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (blank(), blank(), blank(), blank());
        // SAFETY: plain out-parameters for the call, and the process is this one.
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        let ticks = |time: FileTime| f64::from(time.high) * 4_294_967_296.0 + f64::from(time.low);
        (ticks(kernel) + ticks(user)) / 10_000.0
    }

    /// This process's working set, in megabytes.
    #[cfg(d3d11dec)]
    fn working_set_megabytes() -> f64 {
        #[repr(C)]
        struct Counters {
            size: u32,
            page_faults: u32,
            peak_working_set: usize,
            working_set: usize,
            quota_peak_paged: usize,
            quota_paged: usize,
            quota_peak_unpaged: usize,
            quota_unpaged: usize,
            pagefile: usize,
            peak_pagefile: usize,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                counters: *mut Counters,
                size: u32,
            ) -> i32;
        }
        let mut counters = Counters {
            size: size_of::<Counters>() as u32,
            page_faults: 0,
            peak_working_set: 0,
            working_set: 0,
            quota_peak_paged: 0,
            quota_paged: 0,
            quota_peak_unpaged: 0,
            quota_unpaged: 0,
            pagefile: 0,
            peak_pagefile: 0,
        };
        // SAFETY: the counters are this call's own out-parameter, sized as the
        // call expects, for this process.
        unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) };
        counters.working_set as f64 / (1024.0 * 1024.0)
    }

    /// What a 1080p stream costs the processor a frame, before and after: the
    /// same recording decoded and shown at a 480×270 tile through both paths —
    /// software decode, then libyuv's conversion and scale on the processor;
    /// the card's decode, then its video processor scaling the picture into
    /// the tile, of which only the tile comes back.
    ///
    /// The recording is named by `YAPFAST_MEASURE_STREAM`, an Annex B H264
    /// file, because the repository's fixtures are 320×180 and a tile's worth
    /// of that measures nothing. CPU is the process's own, and four decoders
    /// opened at the end say what a stream's session costs the process on top.
    #[cfg(d3d11dec)]
    #[test]
    #[ignore = "measures a 1080p stream's cost; set YAPFAST_MEASURE_STREAM to an Annex B H264 file"]
    fn measures_a_1080p_stream_at_a_tile() {
        use std::time::Instant;
        let path =
            std::env::var("YAPFAST_MEASURE_STREAM").expect("an Annex B H264 file to measure");
        let file = std::fs::read(&path).expect("the stream to measure");
        // Every access unit: a new one starts at each parameter set and each
        // slice, so that a keyframe's SPS and PPS are read with its IDR, the
        // way a stream's first frame carries them.
        let mut starts = vec![];
        let mut slice_before = false;
        let mut at = 0;
        while let Some(found) = file[at..].windows(3).position(|window| window == [0, 0, 1]) {
            let code = at + found;
            let kind = file.get(code + 3).map(|byte| byte & 0x1f).unwrap_or(0);
            let slice = matches!(kind, 1 | 5);
            // A parameter set starts the unit a keyframe's picture comes with;
            // a slice starts one of its own once a slice came before it.
            if kind == 7 || (slice && slice_before) {
                starts.push(code);
            }
            slice_before = slice;
            at = code + 3;
        }
        let units: Vec<(usize, usize)> = starts
            .iter()
            .zip(starts.iter().skip(1).map(Some).chain([None]))
            .map(|(from, to)| (*from, to.copied().unwrap_or(file.len())))
            .collect();
        let passes = 2;
        let mut pixels = vec![0; 480 * 270 * 4];
        let mut scratch = vec![];

        let mut decoder = Decoder::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if let Some(picture) = decoder.decode(&file[from..to]).unwrap() {
                    picture.i420().to_rgba(&mut scratch, &mut pixels, 480, 270);
                    frames += 1;
                }
            }
        }
        let software = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "software: {software:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        // The card's decoder alone, and the same decode with the video
        // processor's pass and the tile's way back, so that what the pass
        // costs is the difference rather than a guess.
        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    frames += 1;
                }
            }
        }
        let decoding = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, decoding only: {decoding:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    hardware.rgba(&mut pixels, 480, 270).unwrap();
                    frames += 1;
                }
            }
        }
        let on_the_card = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, shown at 480x270: {on_the_card:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        // The pass's own cost with the tile it has to bring back cut to one
        // pixel, and with only every fourth picture brought back at all: what
        // is left is the wait for the card's pipeline, not the copy.
        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let mut tiny = [0u8; 4];
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    hardware.rgba(&mut tiny, 1, 1).unwrap();
                    frames += 1;
                }
            }
        }
        let one_pixel = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, shown at one pixel: {one_pixel:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        let before = working_set_megabytes();
        let mut kept = vec![];
        for _ in 0..4 {
            let mut decoder = Hardware::new(Codec::H264).unwrap();
            // A picture takes the first frames of a stream to come out of the
            // card's low-latency decoder.
            let mut ready = false;
            for (index, &(from, to)) in units.iter().take(8).enumerate() {
                ready |= decoder.decode(&file[from..to], index == 0).unwrap();
            }
            assert!(ready, "a keyframe and seven frames made no picture");
            decoder.rgba(&mut pixels, 480, 270).unwrap();
            kept.push(decoder);
        }
        let after = working_set_megabytes();
        eprintln!(
            "four 1080p decoders hold {:.0} MB of working set (the process had {before:.0} MB)",
            after - before
        );
    }

    /// The machine's decoder's picture in the same colours as the software
    /// path's: one H264, HEVC and AV1 keyframe from `testing/fixtures/` is
    /// decoded on the card, its own 8-bit planes are converted twice — by
    /// libyuv on the processor, as `I420::to_rgba` does for the watch, and by
    /// the video processor on the card, matrix, range, scale and channel order
    /// in its one pass — and the two pictures are compared.
    ///
    /// The fixtures carry no colour metadata, so both read them as BT.601
    /// limited, which is what the software path has always assumed for a
    /// stream that says nothing. What is left between them is the video
    /// processor's own 4:2:0 upsampling: it interpolates chroma with H.264's
    /// siting where libyuv repeats it, so a chroma edge can be tens of codes
    /// apart on a test card full of them, while every flat area matches. The
    /// luma carries the range — a limited/full mix-up moves it by ten codes —
    /// and a matrix or channel order a shader got wrong moves all three
    /// channels, so the mean holds those to five codes a sample, luma to 1.5
    /// with 95% of it within four, half the samples to one code, and the same
    /// comparison with red and blue exchanged is required to be far worse:
    /// the fixtures are saturated enough (a mean of 162 codes between red and
    /// blue) that a swapped channel order could not pass.
    #[cfg(d3d11dec)]
    #[test]
    // The card's own decoder takes this test process down here, as its sibling
    // above does — an access violation with no output — so it is asked for by
    // hand until that is understood.
    #[ignore = "needs a card whose Direct3D 11 decoder opens in the test process"]
    fn the_card_shows_a_stream_in_the_same_colours_as_software() {
        let units = [
            (
                Codec::H264,
                first_unit(&fixture("stream-h264.h264"), 9).to_vec(),
            ),
            (
                Codec::H265,
                first_unit(&fixture("stream-h265.h265"), 35 << 1).to_vec(),
            ),
            (Codec::Av1, first_temporal_unit(&fixture("stream-av1.obu"))),
        ];
        let mut compared = 0;
        for (codec, unit) in units {
            let mut hardware = match Hardware::new(codec) {
                Ok(hardware) => hardware,
                Err(error) => {
                    eprintln!("{codec:?} isn't decoded on this machine's card: {error:#}");
                    continue;
                }
            };
            let mut software = Decoder::new(codec).unwrap();
            let picture = software
                .decode(&unit)
                .unwrap()
                .expect("a picture from the keyframe");
            // A keyframe's picture is there once the decoder has read the unit;
            // the same unit read again is the same picture, so a decoder that
            // held the first one gives it here.
            let ready =
                hardware.decode(&unit, true).unwrap() || hardware.decode(&unit, true).unwrap();
            assert!(ready, "{codec:?}: the card made no picture");
            assert_eq!(
                hardware.size(),
                (picture.width, picture.height),
                "{codec:?}: the card's picture is the stream's size, without the padding its surface carries"
            );
            let planes = hardware.nv12().unwrap();
            let (width, height) = (planes.width, planes.height);
            let planes = crate::picture::I420 {
                width,
                height,
                y: &planes.y,
                u: &planes.u,
                v: &planes.v,
                strides: [width, width.div_ceil(2), width.div_ceil(2)],
            };
            let mut expected = vec![0; width as usize * height as usize * 4];
            planes.to_rgba(&mut vec![], &mut expected, width, height);
            let mut shown = vec![0; expected.len()];
            hardware.rgba(&mut shown, width, height).unwrap();
            let mut differences = Vec::with_capacity(expected.len() / 4 * 3);
            let mut luma = Vec::with_capacity(expected.len() / 4);
            let mut exchanged = 0u64;
            let light = |pixel: &[u8]| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
            };
            for (want, got) in expected.chunks_exact(4).zip(shown.chunks_exact(4)) {
                for channel in 0..3 {
                    differences.push(u16::from(want[channel].abs_diff(got[channel])));
                }
                luma.push(light(want).abs_diff(light(got)));
                exchanged +=
                    u64::from(want[0].abs_diff(got[2])) + u64::from(want[2].abs_diff(got[0]));
            }
            differences.sort_unstable();
            luma.sort_unstable();
            let mean = differences.iter().map(|&d| u64::from(d)).sum::<u64>() as f64
                / differences.len() as f64;
            let median = differences[differences.len() / 2];
            let luma_mean =
                luma.iter().map(|&d| u64::from(d)).sum::<u64>() as f64 / luma.len() as f64;
            let luma_95 = luma[luma.len() * 95 / 100];
            let exchanged = exchanged as f64 / (luma.len() as f64 / 3.0 * 2.0);
            eprintln!(
                "{codec:?}: {mean:.2} codes a sample on average, {median} at the median,                  luma {luma_mean:.2} on average and {luma_95} at the 95th, {exchanged:.1} with red and blue exchanged"
            );
            assert!(mean <= 5.0, "{codec:?}: {mean:.2} codes a sample");
            assert!(median <= 1, "{codec:?}: {median} codes at the median");
            assert!(luma_mean <= 1.5, "{codec:?}: {luma_mean:.2} codes of luma");
            assert!(
                luma_95 <= 4,
                "{codec:?}: {luma_95} codes of luma at the 95th"
            );
            assert!(
                exchanged > mean * 4.0,
                "{codec:?}: exchanging red and blue leaves {exchanged:.1} codes, so the comparison can't tell a channel order"
            );
            compared += 1;
        }
        assert!(
            compared > 0,
            "no codec was compared: this machine's card decodes none of them"
        );
    }
}

/// VA-API's decoder hands its picture over as a DMA-BUF, on whichever card's
/// libva driver initialises: the export is asked for as DRM PRIME on every
/// picture, where an unreferenced frame would have mapped it into memory and
/// been refused. Needs a VA-API driver, and says so rather than failing where
/// there is none.
#[cfg(all(test, vaapidec))]
pub(crate) mod vaapi_export {
    use super::vaapi::Hardware;
    use crate::codec::Codec;

    /// The fixture's access units, split at its delimiters as a viewer
    /// reassembles them, each NAL with a four-byte start code. An access unit
    /// delimiter is H264's NAL 9 and HEVC's 35, whose two-byte header starts
    /// with 35 << 1.
    pub(crate) fn units() -> Vec<Vec<u8>> {
        units_of("stream-h264.h264")
    }

    /// The access units of the fixture called `name`, split as [`units`].
    pub(crate) fn units_of(name: &str) -> Vec<Vec<u8>> {
        let file = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures")
                .join(name),
        )
        .unwrap_or_else(|error| panic!("the {name} fixture: {error}"));
        let starts: Vec<usize> = (0..file.len() - 2)
            .filter(|&at| file[at..at + 3] == [0, 0, 1])
            .map(|at| at + 3)
            .collect();
        let mut units: Vec<Vec<u8>> = vec![];
        for (index, &start) in starts.iter().enumerate() {
            let mut end = starts.get(index + 1).map_or(file.len(), |next| next - 3);
            if file[end - 1] == 0 {
                end -= 1;
            }
            let nal = &file[start..end];
            let delimiter = nal[0] & 0x1f == 9 || nal[0] >> 1 == 35;
            if delimiter || units.is_empty() {
                units.push(vec![]);
            }
            if !delimiter {
                let unit = units.last_mut().unwrap();
                unit.extend_from_slice(&[0, 0, 0, 1]);
                unit.extend_from_slice(nal);
            }
        }
        units.retain(|unit| !unit.is_empty());
        units
    }

    #[test]
    fn a_decoded_picture_is_handed_over_as_a_dmabuf() {
        // The node `YAPFAST_VAAPI_NODE` names, for a machine whose first card
        // isn't the one with a VA-API driver; FFmpeg's own pick otherwise.
        let node = std::env::var("YAPFAST_VAAPI_NODE").ok();
        let opened = match node {
            Some(node) => Hardware::on(Codec::H264, &std::ffi::CString::new(node).unwrap()),
            None => Hardware::new(Codec::H264),
        };
        let mut decoder = match opened {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("no VA-API decoder here: {error:#}");
                return;
            }
        };
        let mut pictures = 0;
        for (index, unit) in units().iter().enumerate() {
            if decoder
                .decode(unit, index == 0)
                .unwrap_or_else(|error| panic!("unit {index}: {error:#}"))
            {
                let picture = decoder.dmabuf().expect("a picture, as a descriptor");
                assert!(!picture.fds.is_empty(), "the picture has its memory");
                assert!(picture.width > 0 && picture.height > 0);
                if pictures == 0 {
                    eprintln!(
                        "{}x{} as {:#010x}, modifier {:#018x}, {} planes",
                        picture.width,
                        picture.height,
                        picture.fourcc,
                        picture.modifier,
                        picture.planes.len()
                    );
                }
                pictures += 1;
                decoder.release();
            }
        }
        assert!(pictures > 0, "the card decoded no picture of the stream");
    }

    /// The colour signal the shim reads out of a stream's VUI, which is what the
    /// tile pass needs to show a 10-bit picture in the colour it was coded with.
    /// A stream that declares nothing is BT.709 limited range, and one that
    /// declares HDR10 is BT.2020's matrix in the PQ curve.
    #[cfg(vaapidec)]
    #[test]
    fn a_stream_says_the_colour_its_pictures_are_in() {
        // The node `YAPFAST_VAAPI_NODE` names, for a machine whose first card
        // isn't the one with a VA-API driver; FFmpeg's own pick otherwise.
        let node = std::env::var("YAPFAST_VAAPI_NODE").ok();
        let mut seen = 0;
        for (codec, name, wanted) in [
            (Codec::H264, "stream-h264.h264", crate::dmabuf::Colour::SDR),
            (
                Codec::H265,
                "stream-h265-10bit.h265",
                crate::dmabuf::Colour::of(0, 1, 1),
            ),
        ] {
            let opened = match &node {
                Some(node) => Hardware::on(codec, &std::ffi::CString::new(node.as_str()).unwrap()),
                None => Hardware::new(codec),
            };
            let mut decoder = match opened {
                Ok(decoder) => decoder,
                Err(error) => {
                    eprintln!("no VA-API decoder for {codec:?} here: {error:#}");
                    return;
                }
            };
            let mut pictures = 0;
            for (index, unit) in units_of(name).iter().enumerate() {
                let ready = match decoder.decode(unit, index == 0) {
                    Ok(ready) => ready,
                    Err(error) => {
                        eprintln!("this machine's card didn't decode {name}: {error:#}");
                        return;
                    }
                };
                if !ready {
                    continue;
                }
                let picture = decoder.dmabuf().expect("a picture, as a descriptor");
                assert_eq!(picture.colour, wanted, "{name}'s colour signal");
                pictures += 1;
                decoder.release();
            }
            assert!(pictures > 0, "the card decoded no picture of {name}");
            seen += 1;
        }
        assert_eq!(seen, 2);
    }

    /// A 10-bit stream decodes on the card as a P010 picture — 4:2:0 in 16-bit
    /// samples at the top — which is what the tile pass reads with the matrix,
    /// range and PQ curve it carries. A machine whose driver can't decode 10 bits
    /// says so here, and the stream stays with the software decoder.
    ///
    /// The stream is H265 Main 10 rather than H264 High 10, because a UHD 770's
    /// VA-API driver has no H264 High 10 profile at all — the profiles a driver
    /// answers are `vainfo`'s — and its HEVC one decodes 10 bits. H264 High 10
    /// is what the other card's decoder is refused for, in `cuda`'s tests.
    #[cfg(vaapidec)]
    #[test]
    fn a_ten_bit_stream_decodes_on_the_card_as_a_p010_picture() {
        const P010: u32 = 0x3031_3050;
        let node = std::env::var("YAPFAST_VAAPI_NODE").ok();
        let opened = match node {
            Some(node) => Hardware::on(Codec::H265, &std::ffi::CString::new(node).unwrap()),
            None => Hardware::new(Codec::H265),
        };
        let mut decoder = match opened {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("no VA-API decoder here: {error:#}");
                return;
            }
        };
        let mut pictures = 0;
        for (index, unit) in units_of("stream-h265-10bit.h265").iter().enumerate() {
            let ready = match decoder.decode(unit, index == 0) {
                Ok(ready) => ready,
                Err(error) => {
                    eprintln!("this machine's card didn't decode 10 bits: {error:#}");
                    return;
                }
            };
            if !ready {
                continue;
            }
            let picture = decoder.dmabuf().expect("a picture, as a descriptor");
            assert_eq!(picture.fourcc, P010, "10-bit 4:2:0, as DRM names it");
            assert_eq!(picture.planes.len(), 2, "luma and chroma");
            assert_eq!(
                (picture.width, picture.height),
                (320, 180),
                "the stream's own size, without the padded surface"
            );
            assert!(
                picture.planes.iter().all(|plane| plane.stride >= 640),
                "16 bits a sample, so rows of twice the pixels in bytes"
            );
            assert_eq!(
                picture.colour,
                crate::dmabuf::Colour::of(0, 1, 1),
                "limited range, BT.2020's matrix, the PQ curve, as the stream declares"
            );
            if pictures == 0 {
                eprintln!(
                    "{}x{} as {:#010x}, modifier {:#018x}, {} planes, {:?}",
                    picture.width,
                    picture.height,
                    picture.fourcc,
                    picture.modifier,
                    picture.planes.len(),
                    picture.colour
                );
            }
            pictures += 1;
            decoder.release();
        }
        assert!(
            pictures > 0,
            "the card decoded no picture of the 10-bit stream"
        );
    }
}

mod decoding;
mod threads;

pub(crate) use decoding::*;
pub(crate) use threads::*;

pub use decoding::DECODES;
pub use decoding::Decoder;
pub use decoding::Frame;
#[cfg(d3d11dec)]
pub use decoding::Hardware;
pub use decoding::Live;
pub use decoding::Picture;
#[cfg(all(d3d11dec, test))]
pub use decoding::Planes;
pub use threads::Budget;
pub use threads::Threads;
pub use threads::allow;
pub use threads::share;
