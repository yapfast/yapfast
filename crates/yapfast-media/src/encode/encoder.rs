//! `encode.rs`'s encoder.

use super::*;

/// DRM's fourcc for NV12, as a descriptor names its format: the one picture a
/// VA-API surface can be made of without converting it, and so the one a
/// descriptor can be encoded in as it lies.
#[cfg(target_os = "linux")]
pub(crate) const NV12: u32 = u32::from_le_bytes(*b"NV12");

/// DRM's fourcc for the two orders a compositor draws a desktop in: `AR24`,
/// which is B, G, R and a byte nothing reads, and `AB24`, the same with red and
/// blue the other way round. They are `capture/linear.rs`'s `Order::fourcc`,
/// which is what a screen's descriptor carries, and neither is a VA-API surface:
/// they are what the card's video processor is asked to make one of.
#[cfg(target_os = "linux")]
pub(crate) const AR24: u32 = u32::from_le_bytes(*b"AR24");
#[cfg(target_os = "linux")]
pub(crate) const AB24: u32 = u32::from_le_bytes(*b"AB24");

/// RTP's video clock.
pub const CLOCK: u64 = 90_000;

/// The time of a point in a stream, `elapsed` into it, in [`CLOCK`] units:
/// where the encoder's frames, the camera's and the RTP timestamps on their
/// packets are all timed from, so a frame can't be timed two ways. RTP's
/// timestamp is 32 bits and wraps after 13 hours, and [`Frame::timestamp`]
/// is the low half of this.
pub fn timestamp(elapsed: Duration) -> u64 {
    // Multiplied before it's divided, or most of a millisecond would be lost.
    elapsed.as_micros() as u64 * CLOCK / 1_000_000
}

pub struct Encoder {
    pub(crate) raw: *mut Raw,
    pub(crate) codec: Codec,
    pub(crate) effort: Effort,
    pub(crate) configuration: Configuration,
    pub(crate) encode: Encode,
    /// Copies the unit the last encode produced out of the encoder, once the
    /// buffer it goes in holds it.
    pub(crate) drain: Drain,
    /// The bytes the encoder keeps for the unit it produces.
    pub(crate) staging: Staging,
    pub(crate) set_bitrate: SetBitrate,
    pub(crate) free: Free,
    /// The bitrate aimed for, in kbit/s.
    pub(crate) kbps: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u32,
    /// How many frames the GPU's encoder has answered late so far, as last
    /// read, so that a warning names each new one once.
    pub(crate) late: u32,
    /// The hardware encoder that opened, and what reaching it costs; `None`
    /// for an encoder on the processor.
    pub(crate) chosen: Option<Candidate>,
    /// The adapter the capture filled its frames on, when the caller said:
    /// which encoder is asked first, and which a failure reopens.
    pub(crate) capture: Option<Luid>,
    /// FFmpeg's names for the hardware encoders this encoder has run as, so a
    /// failure reopens one this stream hasn't already seen fail.
    pub(crate) tried: Vec<String>,
    /// How many pictures the native encoder has been asked for: a test holds
    /// it to one a picture, since a picture encoded twice is one referencing
    /// the copy the receiver never got.
    #[cfg(test)]
    pub(crate) submitted: usize,
    /// The encoder that reads the card's own pictures, from
    /// native/hwenc_dmabuf.c: opened on the first DMA-BUF a stream hands over,
    /// on the device the encoder above opened on. `None` until then, and for
    /// every encoder with no such path.
    #[cfg(dmabufenc)]
    pub(crate) dmabuf: Option<Dmabuf>,
    /// FFmpeg's own name for the encoder that runs, such as "h264_vaapi", which
    /// is which backend it is without the device the report adds. Kept beside
    /// `tried` so that reopening for a picture in memory doesn't read the name
    /// of the encoder it is about to try.
    pub(crate) engine_name: String,
    /// How many pictures went to the encoder that reads the card's own without
    /// the processor moving a pixel: the DMA-BUF wrapped as a surface where it
    /// lay, and the surface the card's video processor made of one it couldn't.
    pub(crate) in_place: u64,
    /// Which encoder runs and how its pictures reach it, for the report while a
    /// stream's pictures are the card's own: empty until the first one is
    /// encoded, so the line is the open's own until there is something to add.
    pub(crate) in_place_engine: String,
    /// How many pictures the card's video processor converted before the encoder
    /// read them, which is the count of the ones not read where they lay. Zero on
    /// every build without the processor, and on every stream that has never
    /// handed one over.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) converted: u64,
    /// Whether this stream's pictures are still offered to the card's video
    /// processor: this build's answer while none has failed, and given up for the
    /// stream once one has — no driver that can't convert a picture converts the
    /// next.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) converts: bool,
    /// Whether this stream's pictures are still offered to the card's encoder
    /// where they lie: given up once one is refused, as `converts` is, so a
    /// driver that won't import one isn't asked again, and warned about, on
    /// every frame.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) imports: bool,
    /// The card's video processor's arguments for this stream's pictures, and the
    /// shape they were made for: `None` until a picture needs a conversion, and
    /// made again when the pictures' shape changes.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) vpp: Option<Vpp>,
    /// How many pictures the processor moved into the encoder's own memory,
    /// which is every picture the plane path hands over.
    pub(crate) moved: u64,
    /// The frame as I420, for [`Self::encode`]: a full-size luma plane, then
    /// two quarter-size chroma planes. Empty until it's first needed: the
    /// screen and the camera arrive as I420.
    pub(crate) planes: Vec<u8>,
    /// The buffers encoded frames are copied into, reused as frames are sent.
    pub(crate) out: Arc<Pool>,
}

// SAFETY: no library keeps thread-local state past a call; one thread uses
// the encoder at a time.
unsafe impl Send for Encoder {}

/// One encoded frame, with the buffer it was encoded into: the bytes stay its
/// own as long as it lives, and the buffer goes back to the encoder's pool
/// when it's dropped.
pub struct Frame {
    pub data: Encoded,
    pub keyframe: bool,
    /// Its time in the stream, in 90 kHz units, cut to the 32 bits RTP sends
    /// it in. It travels with the bytes rather than being worked out again
    /// for the packet, so a frame is never sent under another frame's time.
    pub timestamp: u32,
}

/// What one call of an encoder made of a frame.
#[derive(Clone, Copy)]
pub(crate) enum Written {
    /// Its size, and whether it's a keyframe. The size may be larger than the
    /// buffer the frame will be sent in, which grows to hold it: the encoder
    /// keeps the frame until [`Encoder::drain_produced`] copies it out.
    Frame(usize, bool),
    /// The encoder skipped it.
    Skipped,
}

impl Encoder {
    /// An encoder for `width` by `height` frames, which must be even. With
    /// `hardware`, H264 goes to the GPU's encoder where there is one, and to
    /// x264 otherwise. H265 and AV1 have only the GPU's.
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
    ) -> Result<Self> {
        Self::open(codec, width, height, fps, kbps, effort, hardware, None)
    }

    /// An encoder opened on the device a capture fills its textures on, which is
    /// what lets Quick Sync and AMF read a frame where the capture drew it: a
    /// surface is only readable by the device it lives on, and only a session
    /// opened there can be handed one.
    ///
    /// The device travels with the open rather than with the encoder: the shim
    /// takes it once, in the request that opens the session, and keeps a
    /// reference for as long as that session lives.
    #[cfg(all(target_os = "windows", hwenc))]
    #[allow(clippy::too_many_arguments)]
    pub fn on_device(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        device: *mut std::ffi::c_void,
    ) -> Result<Self> {
        /// Clears the capture device however the open ends, a panic included: the
        /// device is only valid for the call it was set for.
        struct Carried;

        impl Drop for Carried {
            fn drop(&mut self) {
                CAPTURE_DEVICE.with(|slot| *slot.borrow_mut() = None);
            }
        }

        CAPTURE_DEVICE.with(|slot| *slot.borrow_mut() = Some(device));
        let _carried = Carried;
        Self::new(codec, width, height, fps, kbps, effort, hardware)
    }

    /// Whether the session that opened reads a capture's own surfaces rather
    /// than frames in memory: Quick Sync and AMF do, and NVENC's session here
    /// does not.
    #[cfg(all(target_os = "windows", hwenc))]
    pub fn took_surfaces(&self) -> bool {
        // SAFETY: raw is an open encoder, and the shim answers whether this
        // session reads a capture's surfaces or frames in memory.
        unsafe { yapfast_hwenc_shared(self.raw) != 0 }
    }

    /// An encoder like [`Self::new`], for a capture whose frames were filled on
    /// `capture`: the adapter is what the preference and the adapter list put
    /// first, and a stream whose capture is on another adapter is told which
    /// encoder crossed to it rather than which one merely opened. `None` is a
    /// frame with no adapter of its own — a camera's, a clip's — and the
    /// preference alone decides.
    #[allow(clippy::too_many_arguments)]
    pub fn on_adapter(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        capture: Option<Luid>,
    ) -> Result<Self> {
        Self::open(codec, width, height, fps, kbps, effort, hardware, capture)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        capture: Option<Luid>,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let opened = match hardware.then(|| {
            open_hardware(
                codec,
                width,
                height,
                fps,
                kbps,
                Tuning::default(),
                capture,
                &[],
            )
        }) {
            Some(Ok(opened)) => opened,
            refused => {
                // The GPU was asked for and passed this codec over: the reason
                // it gave says hardware was refused, rather than the stream
                // being on the processor for no stated reason.
                if let Some(Err(why)) = refused {
                    tracing::warn!(
                        why = why.as_str(),
                        fallback = true,
                        "the GPU's encoder was refused; encoding on the processor"
                    );
                    // A driver built against a newer glibc than this build was
                    // made with can't be loaded into it, and its dlopen failure
                    // names the symbol version and nothing to do about it. NixOS
                    // is where this is met: the drivers in /run/opengl-driver
                    // are built from the channel the machine runs, and a build
                    // from a pinned flake older than that channel can't load
                    // them. Both versions are named, since which channel to
                    // build against is the reader's to pick.
                    #[cfg(target_os = "linux")]
                    if let Some(wanted) = wanted_glibc(&why) {
                        tracing::warn!(
                            wanted = wanted,
                            here = glibc_here().as_str(),
                            "a GPU driver here asks for a newer glibc than this build has, \
                             so it can't load at all: build against the machine's own \
                             nixpkgs (nix develop path:./nix --override-input nixpkgs \
                             <its channel>), or use the AppImage, which runs on the \
                             host's glibc"
                        );
                    }
                }
                open_software(codec, width, height, fps, kbps, effort)?
            }
        };
        let pixels = (width * height) as usize;
        let engine_name = opened.name.clone();
        Ok(Self {
            raw: opened.raw,
            codec,
            effort,
            configuration: opened.configuration,
            encode: opened.encode,
            drain: opened.drain,
            staging: opened.staging,
            set_bitrate: opened.set_bitrate,
            free: opened.free,
            kbps,
            width,
            height,
            fps,
            late: 0,
            chosen: opened.candidate,
            capture,
            tried: vec![opened.name],
            #[cfg(dmabufenc)]
            dmabuf: None,
            engine_name,
            in_place: 0,
            in_place_engine: String::new(),
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            converted: 0,
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            // SAFETY: the shim answers whether it was built with the video
            // processor without touching an encoder or a device.
            converts: unsafe { yapfast_hwenc_dmabuf_converts() != 0 },
            #[cfg(all(target_os = "linux", dmabufenc))]
            imports: true,
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            vpp: None,
            moved: 0,
            #[cfg(test)]
            submitted: 0,
            planes: Vec::new(),
            // Starts lean; a keyframe of noise is the most the pool can grow to.
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        })
    }

    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc". Once a picture has been encoded
    /// where it lay, the line says that too: the encoder and the device, and
    /// that its pictures are the card's own rather than uploaded to it.
    pub fn engine(&self) -> &str {
        if self.in_place > 0 {
            return &self.in_place_engine;
        }
        &self.configuration.engine
    }

    /// What this encoder was opened with, for a log or a measurement to quote
    /// beside its numbers.
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub(crate) fn hardware(&self) -> bool {
        // The line [`Self::engine`] reports is not what decides this: it grows
        // a note about where the pictures come from, and an encoder's kind is
        // what it opened as.
        !matches!(self.configuration.engine.as_str(), "libvpx" | "x264")
    }

    /// FFmpeg's own name for the encoder that runs, such as "h264_vaapi":
    /// which backend it is, without the device the report adds beside it.
    pub(crate) fn ffmpeg_name(&self) -> &str {
        &self.engine_name
    }

    /// Whether this encoder reads a picture where the card holds it, rather than
    /// taking one uploaded to it: VA-API does, and NVENC, Quick Sync, AMF and x264
    /// do not. A caller holding a picture on the card asks before it hands one
    /// over, so that a camera filmed on the card is not encoded by one that
    /// uploads every frame it is given.
    pub fn reads_descriptors(&self) -> bool {
        takes_dmabuf(self.ffmpeg_name())
    }

    /// How many pictures the processor moved into the encoder's own memory
    /// since this encoder opened: every picture the plane path hands over, and
    /// none of the ones the card's encoder reads where they lie. A capture that
    /// started converting DMA-BUFs into pictures would say so here without any
    /// picture looking different.
    pub fn copied(&self) -> u64 {
        self.moved
    }

    /// How many pictures were made into the encoder's own frames on the card: no
    /// pixel of one of these came back to the processor. A picture is one of them
    /// whether the encoder read the DMA-BUF where it lay or the card's video
    /// processor made the surface of it first. Zero while every picture is a
    /// picture in memory, which is every stream on an encoder with no path from a
    /// descriptor, and every stream that has never handed one over.
    pub fn encoded_in_place(&self) -> u64 {
        self.in_place
    }

    /// The hardware encoder that runs, and what reaching it costs: the
    /// backend, the adapter it is on and whether a captured frame crosses to
    /// it. `None` for an encoder on the processor, whose frames are already
    /// the processor's, and for a hardware one opened where this machine's
    /// adapters couldn't be listed.
    ///
    /// A hardware encoder here is fed from system memory either way: the
    /// zero-copy path is the native D3D11 one in `d3d11.rs`, not this. What
    /// this says is which adapter the upload ends up on.
    pub fn selection(&self) -> Option<Candidate> {
        self.chosen
    }

    /// Puts `opened` in place of the encoder running now.
    pub(crate) fn replace(&mut self, opened: Opened) {
        // SAFETY: raw came from its codec's constructor and is freed once; an
        // encoder given up for the card's own pictures has none.
        if !self.raw.is_null() {
            unsafe { (self.free)(self.raw) };
        }
        self.raw = opened.raw;
        self.encode = opened.encode;
        self.drain = opened.drain;
        self.staging = opened.staging;
        self.set_bitrate = opened.set_bitrate;
        self.free = opened.free;
        self.configuration = opened.configuration;
        self.engine_name = opened.name.clone();
        self.chosen = opened.candidate;
        self.tried.push(opened.name);
        // The encoder that replaced it has dropped none of its own frames: a
        // count left from the one that failed would read 0 until this one's
        // drops overtook it.
        self.late = 0;
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The bitrate aimed for, in kbit/s.
    pub fn bitrate(&self) -> u32 {
        self.kbps
    }

    /// The bytes the encoder's output buffers hold now, and the most they have
    /// held: what a stream's report shows so a pool that grew for one large
    /// frame says what it kept.
    pub fn pool_bytes(&self) -> (u64, u64) {
        let (live, peak) = self.out.bytes();
        (live as u64, peak as u64)
    }

    /// Gives back the output buffers lying idle, for a machine short of memory:
    /// a buffer being encoded into or waiting to be sent is never touched.
    pub fn trim_pool(&self) {
        self.out.trim();
    }

    /// How many buffers the encoder's pool has allocated: the tests hold it
    /// to the pool's bound.
    #[cfg(test)]
    pub(crate) fn created(&self) -> usize {
        self.out.created()
    }

    /// How many pictures the native encoder was asked to encode: more than one
    /// for a picture is one encoded a second time, which a test fails on.
    #[cfg(test)]
    pub(crate) fn submitted(&self) -> usize {
        self.submitted
    }

    /// The bytes the encoder keeps to copy the unit it produced out of, reused
    /// for the next frame: one frame's worth at most, which is what a test
    /// holds it to rather than a growing history.
    #[cfg(test)]
    pub(crate) fn staging(&self) -> usize {
        // SAFETY: raw came from this codec's constructor and is still live; an
        // encoder given up for the card's own pictures has nothing staged.
        if self.raw.is_null() {
            return 0;
        }
        unsafe { (self.staging)(self.raw) }
    }

    /// Aims for `kbps` from the next frame on, without a keyframe.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw came from this codec's constructor and is still live; an
        // encoder given up for the card's own pictures takes the rate when it
        // is opened again.
        if !self.raw.is_null() && unsafe { (self.set_bitrate)(self.raw, kbps) } != 0 {
            bail!("the {:?} encoder refused {kbps} kbit/s", self.codec);
        }
        // The encoder that reads the card's pictures is the one the frames come
        // from while it runs, so the new rate has to reach it as well.
        #[cfg(dmabufenc)]
        if let Some(dmabuf) = &self.dmabuf {
            // SAFETY: dmabuf is an open encoder.
            unsafe { yapfast_hwenc_dmabuf_set_bitrate(dmabuf.raw, kbps) };
        }
        self.kbps = kbps;
        Ok(())
    }

    /// Encodes RGBA pixels of the encoder's size, shown at `pts` in
    /// [`CLOCK`] units. None when the encoder skipped the frame.
    pub fn encode(&mut self, rgba: &[u8], pts: u64, keyframe: bool) -> Result<Option<Frame>> {
        let (width, height) = (self.width, self.height);
        let pixels = (width * height) as usize;
        if rgba.len() < pixels * 4 {
            bail!("a frame smaller than the encoder's");
        }
        let mut planes = std::mem::take(&mut self.planes);
        planes.resize(pixels + pixels / 2, 0);
        let (y, chroma) = planes.split_at_mut(pixels);
        let (u, v) = chroma.split_at_mut(pixels / 4);
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory. This is the conversion the encoders'
        // colour claim is about: libyuv writes BT.601's matrix over its
        // limited range, so that is what native/h264.c and native/hwenc.c tell
        // each encoder, and neither claim is right apart from the other.
        yuv_helper::abgr_to_i420(
            rgba,
            width * 4,
            y,
            width,
            u,
            width / 2,
            v,
            width / 2,
            width as i32,
            height as i32,
        );
        let encoded =
            self.encode_planes([(y, width), (u, width / 2), (v, width / 2)], pts, keyframe);
        self.planes = planes;
        encoded
    }

    /// Encodes an I420 picture of the encoder's size, like [`Self::encode`].
    pub fn encode_i420(
        &mut self,
        picture: &I420Buffer,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        if (picture.width(), picture.height()) != (self.width, self.height) {
            bail!("a picture of another size than the encoder's");
        }
        let (y, u, v) = picture.data();
        let (y_stride, u_stride, v_stride) = picture.strides();
        self.encode_planes([(y, y_stride), (u, u_stride), (v, v_stride)], pts, keyframe)
    }

    /// Encodes a camera's picture, at least the encoder's size: its top left
    /// corner, when it's larger, as a picture an odd size loses a row or a
    /// column.
    pub fn encode_picture(
        &mut self,
        picture: &crate::picture::I420<'_>,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        if picture.width < self.width || picture.height < self.height {
            bail!("a picture smaller than the encoder's");
        }
        let [y_stride, u_stride, v_stride] = picture.strides;
        self.encode_planes(
            [
                (picture.y, y_stride),
                (picture.u, u_stride),
                (picture.v, v_stride),
            ],
            pts,
            keyframe,
        )
    }

    /// Encodes a picture the card already holds, in the DMA-BUF it arrived in:
    /// its time is `time_ms` into the stream, which is [`timestamp`]ed into
    /// [`CLOCK`] units, so a picture is encoded at one time whichever way it
    /// reached the encoder.
    ///
    /// A VA-API encoder wraps the descriptor into a VA-API surface with FFmpeg's
    /// `av_hwframe_map` and encodes that surface (native/hwenc_dmabuf.c), so no
    /// pixel of the picture comes back to the processor.
    ///
    /// A picture the encoder can't read as it arrived — the BGRA a compositor
    /// draws a desktop in, of the screen's size rather than the stream's — is not
    /// a picture for the processor either: where this build carries the card's
    /// video processor, it scales and converts the picture into the NV12 surface
    /// the encoder reads, on the card (native/hwenc_dmabuf.c), so no pixel of it
    /// comes back to the processor.
    ///
    /// Every other encoder here — NVENC, Quick Sync, AMF, x264 — reads its
    /// pictures from memory, and answers [`Refused`] rather than copying a
    /// DMA-BUF into one. Any outer `Err` is one line in the log and then the same
    /// picture through [`Self::encode_i420`], which is the fallback and the only
    /// path that moves pixels; nothing here copies a picture on the way to saying
    /// it can't take one. The inner result is the encode itself, for an encoder
    /// that took the picture and then failed.
    #[cfg(target_os = "linux")]
    pub fn encode_dmabuf(
        &mut self,
        picture: &crate::dmabuf::Descriptor<'_>,
        keyframe: bool,
        time_ms: u64,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        let at = timestamp(Duration::from_millis(time_ms));
        // Which encoder runs, and whether it reads a picture where it lies: the
        // name answers that, so a stream on NVENC or x264 is told before its
        // picture is looked at.
        if !takes_dmabuf(self.ffmpeg_name()) {
            return Err(Refused::Uploads);
        }
        let refused = match self.importable(picture) {
            Ok(picture) => return self.encode_in_place(&picture, at, keyframe),
            Err(refused) => refused,
        };
        // The one NV12 picture a surface is made of is not what a screen hands
        // over, so the card's video processor makes the surface instead: only
        // where this build has one, and only for a picture whose format it takes.
        // Everything else is the processor's copy, and `refused` is why.
        #[cfg(vaapivpp)]
        if let Some(picture) = self.convertible(picture) {
            return self.encode_converted(&picture, at, keyframe);
        }
        Err(refused)
    }

    /// Encodes the picture in `texture` — a capture's own D3D11 texture, from
    /// the device the session was opened on — where it lies: Quick Sync's shared
    /// path hands the surface straight to the encoder, so no pixel of the picture
    /// is converted or uploaded. What comes back is the access unit for the
    /// picture, or none when the encoder wants another first.
    ///
    /// Only a session that opened its shared path reads surfaces: one opened from
    /// memory answers -1, which is a picture for [`Self::encode_i420`] and the
    /// reason the caller falls back.
    // The texture is the capture's and outlives the call, and the shim writes
    // nothing through it.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[cfg(all(target_os = "windows", hwenc))]
    pub fn encode_texture(
        &mut self,
        texture: *mut std::ffi::c_void,
        subresource: u32,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let mut is_key: c_int = 0;
        // SAFETY: the texture is the capture's and lives through the call, and
        // the flag has a place of its own.
        let written = unsafe {
            yapfast_hwenc_encode_texture(
                self.raw,
                texture,
                subresource,
                pts as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        let (size, keyframe) = match written {
            ..0 => bail!(
                "{:?} wouldn't encode the capture's own surface; it goes through the processor \
                 instead",
                self.codec
            ),
            0 => return Ok(None),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded where it lay: a failure here is the copy of its
        // bitstream, and handing the picture to the processor now would encode it
        // twice, against a reference the receiver never got.
        let frame = self.collect(size, keyframe, pts, self.raw, yapfast_hwenc_drain)?;
        self.in_place += 1;
        Ok(Some(frame))
    }

    /// `picture`, as native/hwenc_dmabuf.c reads it, when the encoder can take it
    /// where it lies: one DRM object holding an NV12 picture of the encoder's
    /// size, which is the shape a VA-API surface is made of without a
    /// conversion. These checks are this side's, so a descriptor that can't be a
    /// surface is answered before the card is asked, and a picture the encoder
    /// would have to convert is never taken for one it can encode.
    #[cfg(target_os = "linux")]
    pub(crate) fn importable(
        &self,
        picture: &crate::dmabuf::Descriptor<'_>,
    ) -> Result<DmabufPicture, Refused> {
        if (picture.width, picture.height) != (self.width, self.height) {
            return Err(Refused::Unimportable(
                "a picture of another size than the encoder's",
            ));
        }
        if picture.fourcc != NV12 {
            return Err(Refused::Unimportable(
                "a picture that isn't NV12, which a VA-API surface is made of",
            ));
        }
        Self::described(picture, NV12, 2)
    }

    /// `picture` as the descriptor hands it to the card whichever way it goes
    /// there: the one DRM object it is in, the `planes` planes of `fourcc`'s
    /// format in that object, and how much of the buffer the picture needs. These
    /// checks are this side's, so a descriptor no import could name is answered
    /// before the card is asked, and both the encoder's import and the video
    /// processor's conversion read the same picture.
    #[cfg(target_os = "linux")]
    pub(crate) fn described(
        picture: &crate::dmabuf::Descriptor<'_>,
        fourcc: u32,
        planes: usize,
    ) -> Result<DmabufPicture, Refused> {
        if picture.planes.len() != planes || picture.fds.len() != planes {
            return Err(Refused::Unimportable(match planes {
                2 => "a picture that isn't NV12's two planes in one buffer",
                _ => "a picture that isn't one 32-bit plane in one buffer",
            }));
        }
        if picture.fds.iter().any(|fd| *fd < 0) {
            return Err(Refused::Unimportable("a plane with no buffer"));
        }
        // One DRM object is what FFmpeg imports as a surface; two buffers would
        // have to be one, and this won't pretend they are.
        if picture.fds.iter().any(|fd| *fd != picture.fds[0]) {
            return Err(Refused::Unimportable(
                "planes in two buffers, which one surface can't be made of",
            ));
        }
        let width = u64::from(picture.width);
        if picture
            .planes
            .iter()
            .any(|plane| u64::from(plane.stride) < width)
        {
            return Err(Refused::Unimportable("a pitch narrower than the picture"));
        }
        // Where the picture ends in the buffer, which is the size the import is
        // told: a descriptor carries the offset and pitch of each plane and no
        // size of its own, and a plane's last row has to fit. The planes after
        // the first are subsampled by two in each direction, which is NV12's
        // chroma.
        let (mut offset, mut pitch, mut size) = ([0; 2], [0; 2], 0u64);
        for (index, plane) in picture.planes.iter().enumerate() {
            offset[index] = plane.offset;
            pitch[index] = plane.stride;
            let rows = if index == 0 {
                u64::from(picture.height)
            } else {
                u64::from(picture.height).div_ceil(2)
            };
            size = size.max(u64::from(plane.offset) + u64::from(plane.stride) * rows);
        }
        Ok(DmabufPicture {
            width: picture.width as c_int,
            height: picture.height as c_int,
            fd: picture.fds[0],
            size,
            modifier: picture.modifier,
            fourcc,
            planes: planes as u32,
            offset,
            pitch,
        })
    }

    /// `picture` as the card's video processor reads it, when this build carries
    /// one and it takes the picture's format: `None` when there is no such path,
    /// and then the picture is the processor's — read back and converted with
    /// libyuv, which is the copy this one saves.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) fn convertible(
        &mut self,
        picture: &crate::dmabuf::Descriptor<'_>,
    ) -> Option<DmabufPicture> {
        let (planes, _) = vpp_format(picture.fourcc)?;
        let described = Self::described(picture, picture.fourcc, planes).ok()?;
        // Whether there is a processor to convert it on is this build's answer,
        // and whether this stream still offers it its pictures is a session's: a
        // build without one never opens a session for a screen, and a stream
        // whose conversion failed has stopped asking.
        if !self.converts {
            return None;
        }
        self.descriptor_encoder().ok()?;
        Some(described)
    }

    /// Encodes `picture` on the card by the card's video processor: the picture
    /// is mapped as a surface of the format it arrived in, scaled to the stream's
    /// size and converted to NV12 there, and the encoder is handed that surface,
    /// so no pixel of it comes back to the processor.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) fn encode_converted(
        &mut self,
        picture: &DmabufPicture,
        at: u64,
        keyframe: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        let raw = self.descriptor_encoder()?;
        let args = self.vpp_args(picture);
        let mut is_key: c_int = 0;
        // SAFETY: raw is the descriptor encoder's and is open, the picture is one
        // of this encoder's planes, and the arguments outlive the call.
        let written = unsafe {
            yapfast_hwenc_dmabuf_convert(
                raw,
                picture,
                at as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
                args.as_ptr(),
            )
        };
        let (size, keyframe) = match written {
            ..0 => {
                // The processor couldn't make the surface of this picture, so it
                // goes the processor's way — and the stream stops offering its
                // pictures, since a driver that can't convert one won't convert
                // the next.
                self.converts = false;
                tracing::warn!(
                    engine = self.engine(),
                    why = self.descriptor_error().as_str(),
                    "the card's video processor wouldn't make the picture's surface; it goes \
                     through the processor instead"
                );
                return Err(Refused::Failed);
            }
            0 => return Ok(Ok(None)),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded on the card, so a failure here is the copy of
        // its bitstream, not a picture for the fallback.
        let frame = match self.collect(size, keyframe, at, raw, yapfast_hwenc_dmabuf_drain) {
            Ok(frame) => frame,
            Err(error) => return Ok(Err(error)),
        };
        if self.converted == 0 {
            // Said once, as the processor's own copy is: which way a stream's
            // pictures are converted does not change under it. The engine's own
            // line grows the note, so a reader of the report sees it there too.
            tracing::info!(
                engine = self.configuration.engine.as_str(),
                why = %Refused::Converted,
                "converting on the card: the video processor scales every picture to the \
                 stream's size and turns it into NV12, and the encoder reads that surface"
            );
            self.in_place_engine = format!(
                "{}, scaling and converting DMA-BUFs on the card",
                self.configuration.engine
            );
        }
        self.converted += 1;
        self.note_dmabuf_late();
        self.in_place += 1;
        Ok(Ok(Some(frame)))
    }

    /// Encodes a picture that is on the card, where it lies, and gives the
    /// access unit it produced the time `at` in [`CLOCK`] units.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) fn encode_in_place(
        &mut self,
        picture: &DmabufPicture,
        at: u64,
        keyframe: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        if !self.imports {
            return Err(Refused::Failed);
        }
        let raw = self.descriptor_encoder()?;
        let mut is_key: c_int = 0;
        // SAFETY: raw is the descriptor encoder's and is open, and the picture
        // is one of this encoder's size, mapped from a descriptor.
        let written = unsafe {
            yapfast_hwenc_dmabuf_encode(
                raw,
                picture,
                at as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        let (size, keyframe) = match written {
            ..0 => {
                self.imports = false;
                tracing::warn!(
                    engine = self.engine(),
                    why = self.descriptor_error().as_str(),
                    "the card's encoder wouldn't take the picture where it lies; it and the \
                     pictures after it go through the processor instead"
                );
                return Err(Refused::Failed);
            }
            0 => return Ok(Ok(None)),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded where it lay, so a failure here is the copy of
        // its bitstream, not a picture for the fallback: the encoder's state has
        // moved on and encoding it again would reference one nobody got.
        let frame = match self.collect(size, keyframe, at, raw, yapfast_hwenc_dmabuf_drain) {
            Ok(frame) => frame,
            Err(error) => return Ok(Err(error)),
        };
        self.note_dmabuf_late();
        self.in_place += 1;
        Ok(Ok(Some(frame)))
    }

    /// A VA-API encoder, but this build has nothing to wrap a descriptor with:
    /// FFmpeg wasn't configured for the device and the DRM PRIME import it
    /// needs, so no encoder in it can read one.
    #[cfg(all(target_os = "linux", not(dmabufenc)))]
    pub(crate) fn encode_in_place(
        &mut self,
        _: &DmabufPicture,
        _: u64,
        _: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        Err(Refused::NotBuilt)
    }

    /// The encoder that reads the card's pictures, opened on the first one: the
    /// same codec on the same GPU as the encoder this stream opened, with the
    /// same settings, reading a surface rather than a frame in memory. Its
    /// reason for not opening is logged once, and every picture after it gets
    /// [`Refused::Failed`] without the card being asked again.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) fn descriptor_encoder(&mut self) -> Result<*mut Raw, Refused> {
        if let Some(dmabuf) = &self.dmabuf {
            return Ok(dmabuf.raw);
        }
        // An encoder a stream has already handed pictures in memory to may
        // still be given one, so a failure here changes nothing: it is one line
        // and the plane path.
        let Ok(name) = std::ffi::CString::new(self.ffmpeg_name()) else {
            return Err(Refused::Failed);
        };
        // The device the encoder opened on, so that the picture is imported on
        // the GPU the stream chose. A report with no device in it leaves FFmpeg
        // to place the session, which is what a machine whose adapters couldn't
        // be listed gets.
        let (path, driver) = vaapi_device(&self.configuration.engine).unzip();
        let device = path.and_then(|path| std::ffi::CString::new(path).ok());
        let driver = driver.and_then(|driver| std::ffi::CString::new(driver).ok());
        let (device, driver) = (
            device
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            driver
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
        );
        let codec = match self.codec {
            Codec::H264 => 0,
            Codec::H265 => 1,
            Codec::Av1 => 2,
            Codec::Vp8 => return Err(Refused::Uploads),
        };
        let mut report = [0u8; 128];
        // SAFETY: the strings outlive the call, and the report is a buffer of the
        // size given, which the shim ends with a zero.
        let raw = unsafe {
            yapfast_hwenc_dmabuf_new(
                codec,
                self.width,
                self.height,
                self.fps,
                self.kbps,
                name.as_ptr(),
                device,
                driver,
                report.as_mut_ptr().cast(),
                report.len(),
            )
        };
        if raw.is_null() {
            tracing::warn!(
                engine = self.configuration.engine.as_str(),
                // SAFETY: a failed open leaves its reason in the shim's own
                // thread-local, which needs no encoder to read.
                why = self.descriptor_error().as_str(),
                "no encoder here reads the card's pictures where they lie; every picture goes \
                 through the processor"
            );
            return Err(Refused::Failed);
        }
        // Two sessions on the card are two hungers for its encode engines, so
        // the encoder this opened for pictures in memory is given up when it has
        // never encoded one: a stream whose pictures are the card's own runs one
        // session, and a picture that has to come through memory opens the other
        // again.
        if self.moved == 0 {
            self.give_up_planes();
        }
        let mut config = [0u8; 80];
        // SAFETY: raw is an open encoder, and the buffer is as large as its size.
        unsafe { yapfast_hwenc_dmabuf_config(raw, config.as_mut_ptr().cast(), config.len()) };
        let report = c_text(&report);
        self.in_place_engine = format!("{report}, encoding DMA-BUFs where they lie");
        tracing::info!(
            engine = self.in_place_engine.as_str(),
            config = c_text(&config),
            "encoding the card's own pictures: a DMA-BUF is wrapped as a VA-API surface for the \
             encoder to read, and no pixel of it is uploaded"
        );
        self.dmabuf = Some(Dmabuf { raw, late: 0 });
        Ok(raw)
    }

    /// The card's video processor's arguments for `picture`, made once for a
    /// shape and kept while it stays that: a stream's pictures arrive the same
    /// size and format, so a frame builds no string.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    pub(crate) fn vpp_args(&mut self, picture: &DmabufPicture) -> &std::ffi::CStr {
        let from = (picture.fourcc, picture.width as u32, picture.height as u32);
        if self.vpp.as_ref().is_none_or(|vpp| vpp.from != from) {
            let args = vpp_filter(from.0, (self.width, self.height));
            // The text is this file's own, and has no zero in it.
            let args = std::ffi::CString::new(args).expect("the arguments carry no nul");
            self.vpp = Some(Vpp { from, args });
        }
        &self.vpp.as_ref().expect("made for this picture").args
    }

    /// The card's encoder's own last word about a picture, or about not opening
    /// at all, in words: what the log line beside a fallback says.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) fn descriptor_error(&self) -> String {
        let encoder = self
            .dmabuf
            .as_ref()
            .map_or(std::ptr::null_mut(), |dmabuf| dmabuf.raw);
        // SAFETY: an open encoder, or null, which the shim answers with the
        // reason its last open failed.
        let why = unsafe { yapfast_hwenc_dmabuf_error(encoder) };
        if why.is_null() {
            return String::new();
        }
        // SAFETY: the shim keeps the text alive until its next call.
        unsafe { std::ffi::CStr::from_ptr(why) }
            .to_string_lossy()
            .into_owned()
    }

    /// Gives up the encoder that reads pictures from the processor's memory,
    /// when the card's own path has taken the stream over: one session on the
    /// card while every picture arrives where it is encoded. The encoder given
    /// up is not one that failed, so it is the first this stream opens again if a
    /// picture has to come through memory.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) fn give_up_planes(&mut self) {
        if self.raw.is_null() {
            return;
        }
        // SAFETY: raw came from its codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
        self.raw = std::ptr::null_mut();
        self.tried.pop();
    }

    /// Warns about pictures the card's encoder answered late, when one was: a
    /// driver falling behind, whose picture is dropped rather than sent under
    /// this one's time, as the plane path's own encoder is watched.
    #[cfg(all(target_os = "linux", dmabufenc))]
    pub(crate) fn note_dmabuf_late(&mut self) {
        let fresh = {
            let Some(dmabuf) = self.dmabuf.as_mut() else {
                return;
            };
            // SAFETY: dmabuf is an open encoder.
            let late = unsafe { yapfast_hwenc_dmabuf_late(dmabuf.raw) };
            let seen = std::mem::replace(&mut dmabuf.late, late);
            late.saturating_sub(seen)
        };
        if fresh == 0 {
            return;
        }
        tracing::warn!(
            engine = self.engine(),
            pictures = fresh,
            "the card's encoder answered pictures late; they were dropped"
        );
    }

    /// Encodes three planes, each with its stride, and copies the access unit
    /// the encoder produced into a buffer from the encoder's pool: the frame, or
    /// None when the encoder skipped it. The picture is not submitted again: an
    /// encoder that had already encoded it moved on, so a second submission
    /// would reference a copy the receiver never got and cost the work again.
    /// When a hardware H264 encoder fails, x264 takes over from this frame, with
    /// a keyframe, and that submission does encode the picture a second time;
    /// H265 and AV1 have nothing to take over.
    pub(crate) fn encode_planes(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        // An encoder given up for the card's own pictures is opened again here,
        // on the adapter and with the settings the open chose: a picture in
        // memory is the one case that path can't take.
        if self.raw.is_null() {
            self.reopen("a picture in memory after the card's own")?;
        }
        let written = self.encode_once_fallback(planes, pts, keyframe)?;
        self.note_late();
        let (size, keyframe) = match written {
            Written::Skipped => return Ok(None),
            Written::Frame(size, keyframe) => (size, keyframe),
        };
        Ok(Some(
            self.collect(size, keyframe, pts, self.raw, self.drain)?,
        ))
    }

    /// Copies an access unit `size` bytes long, which the encoder has put aside
    /// for its own copy of it, into a buffer from the encoder's pool: the frame,
    /// given the time it was encoded at. A unit larger than the buffers grows
    /// them, doubling until one holds it. This is the one copy of a frame's
    /// bytes between the encoder and the packetizer, and it happens once per
    /// picture: `encoder` and `drain` are the ones that produced it.
    pub(crate) fn collect(
        &mut self,
        size: usize,
        keyframe: bool,
        pts: u64,
        encoder: *mut Raw,
        drain: Drain,
    ) -> Result<Frame> {
        while self.out.capacity() < size {
            if !self.out.grow() {
                bail!(
                    "the {:?} frame doesn't fit {} bytes",
                    self.codec,
                    self.out.capacity()
                );
            }
        }
        let mut out = self.out.checkout();
        let spare = out.spare();
        // SAFETY: the buffer is at least the unit's size, which the pool was
        // grown to before this call, and both stay put during it.
        let copied = unsafe { drain(encoder, spare.as_mut_ptr(), spare.len()) };
        if copied <= 0 {
            bail!("the {:?} encoder lost the frame it produced", self.codec);
        }
        debug_assert_eq!(copied as usize, size, "the encoder gave back another frame");
        out.finish(copied as usize);
        Ok(Frame {
            data: out,
            keyframe,
            // The frame was encoded at this time, whatever the pool had to do
            // to hold it: one picture, one submission.
            timestamp: pts as u32,
        })
    }

    /// Encodes the planes, with x264 taking over when the GPU's H264 encoder
    /// failed.
    pub(crate) fn encode_once_fallback(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Written> {
        match self.encode_once(planes, pts, keyframe) {
            Err(error) if self.hardware() => {
                tracing::warn!(
                    engine = self.engine(),
                    %error,
                    "the GPU's encoder failed; reopening another"
                );
                self.reopen("the GPU's encoder failed")?;
                // The picture is submitted again, with a keyframe: the
                // receiver never got the one that failed, so the frame it
                // holds is one this stream's pictures no longer reference.
                self.encode_once(planes, pts, true)
            }
            written => written,
        }
    }

    /// Replaces the encoder that is no longer there with the next one that
    /// opens: another hardware encoder on this machine — the same codec, on
    /// another adapter or backend, which is what the adapter preference's
    /// fallbacks are for — or, when none is left, the processor's, which H264
    /// alone has. `reason` is what the log says happened, which is a failure
    /// for one and a picture in memory for another.
    ///
    /// The codec is kept rather than renegotiated: a stream's viewers were
    /// promised it, so a failure changes the encoder and not the format.
    pub(crate) fn reopen(&mut self, reason: &'static str) -> Result<()> {
        let (codec, width, height, fps, kbps, effort) = (
            self.codec,
            self.width,
            self.height,
            self.fps,
            self.kbps,
            self.effort,
        );
        if let Ok(opened) = open_hardware(
            codec,
            width,
            height,
            fps,
            kbps,
            Tuning::default(),
            self.capture,
            &self.tried,
        ) {
            tracing::warn!(
                engine = opened.name.as_str(),
                fallback = true,
                reason,
                "the GPU's encoder was replaced by another on this machine"
            );
            self.replace(opened);
            return Ok(());
        }
        // Nothing left on a GPU, so it is the processor's or nothing: H265 and
        // AV1 have no encoder there, and the error stays the GPU's.
        let opened = open_software(codec, width, height, fps, kbps, effort)?;
        // Replaced first, so the encoder this names is the one that runs now
        // rather than the one that just failed.
        self.replace(opened);
        tracing::warn!(
            engine = self.engine(),
            fallback = true,
            "the GPU's encoder failed; encoding on the processor"
        );
        Ok(())
    }

    /// How many frames the GPU's encoder has answered late, and which
    /// native/hwenc.c dropped rather than let them be sent under a newer
    /// frame's time. An encoder on the processor has none to report, and
    /// neither has one given up for the card's own pictures.
    pub(crate) fn late(&self) -> u32 {
        #[cfg(hwenc)]
        {
            if !self.raw.is_null() && self.hardware() {
                // SAFETY: a hardware engine is native/hwenc.c's, and is live.
                return unsafe { yapfast_hwenc_late(self.raw) };
            }
        }
        0
    }

    /// Warns about frames the GPU's encoder answered late, when one is
    /// answered: a driver falling behind, whose frame is dropped rather than
    /// sent under this frame's time.
    pub(crate) fn note_late(&mut self) {
        let late = self.late();
        let fresh = late.saturating_sub(self.late);
        self.late = late;
        if fresh == 0 {
            return;
        }
        tracing::warn!(
            engine = self.engine(),
            frames = fresh,
            "the GPU's encoder answered frames late; they were dropped"
        );
    }

    /// Encodes the planes, and reports what the encoder produced. What it
    /// produced stays in the encoder, for [`Self::collect`] to copy out: no
    /// output buffer crosses this boundary, so a frame too big for the one
    /// waiting at the other side is copied rather than encoded again.
    pub(crate) fn encode_once(
        &mut self,
        [(y, y_stride), (u, u_stride), (v, v_stride)]: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Written> {
        let mut is_key: c_int = 0;
        // Every picture the plane path hands over is one the processor moves
        // into the encoder's own frame, whatever the encoder does with it
        // afterwards: this is the count a stream says moved pixels for.
        self.moved += 1;
        #[cfg(test)]
        {
            self.submitted += 1;
        }
        // SAFETY: the planes hold a picture of this encoder's size at these
        // strides, and the flag has a place of its own; all stay put during
        // the call.
        let written = unsafe {
            (self.encode)(
                self.raw,
                y.as_ptr(),
                y_stride as c_int,
                u.as_ptr(),
                u_stride as c_int,
                v.as_ptr(),
                v_stride as c_int,
                pts as c_longlong,
                (CLOCK / u64::from(self.fps)) as c_ulong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        match written {
            ..0 => bail!("{:?} encoding failed ({written})", self.codec),
            0 => Ok(Written::Skipped),
            _ => Ok(Written::Frame(written as usize, is_key != 0)),
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from its codec's constructor and is freed once; it is
        // null when the card's own pictures took the stream over and the encoder
        // on the processor's was given up.
        if !self.raw.is_null() {
            unsafe { (self.free)(self.raw) };
        }
    }
}
