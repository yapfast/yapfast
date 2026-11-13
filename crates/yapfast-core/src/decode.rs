//! Decoding a watched Go Live stream: VP8 with libvpx and H264 with FFmpeg,
//! both inside libwebrtc, through native/decode.c; AV1 with libwebrtc's
//! dav1d, through native/av1.c; and H265 with the HEVC decoder of
//! vendor/ffmpeg-src's FFmpeg, through native/hevc.c, where it's built.

use std::ffi::{c_int, c_uchar, c_uint};

use anyhow::{Result, bail};
use livekit::webrtc::native::yuv_helper;
use yapfast_discord::rtp::Codec;

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

#[repr(C)]
struct RawPicture {
    width: c_int,
    height: c_int,
    y: *const c_uchar,
    u: *const c_uchar,
    v: *const c_uchar,
    y_stride: c_int,
    u_stride: c_int,
    v_stride: c_int,
}

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

type DecodeFn = unsafe extern "C" fn(*mut Raw, *const c_uchar, usize, *mut RawPicture) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut Raw);

/// The codecs a viewer tells Discord it decodes, most wanted first: AV1
/// everywhere, H265 only where FFmpeg's HEVC decoder is built in, on Linux
/// and Windows.
pub const DECODES: &[Codec] = if cfg!(hevcdec) {
    &[Codec::Av1, Codec::H265, Codec::H264, Codec::Vp8]
} else {
    &[Codec::Av1, Codec::H264, Codec::Vp8]
};

pub struct Decoder {
    raw: *mut Raw,
    codec: Codec,
    decode: DecodeFn,
    free: FreeFn,
}

// SAFETY: no library keeps thread-local state; one thread uses the decoder at a time.
unsafe impl Send for Decoder {}

/// A decoded picture, valid until the decoder's next frame.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
    strides: [u32; 3],
}

impl Decoder {
    /// A decoder on up to four threads, for a stream shown large.
    pub fn new(codec: Codec) -> Result<Self> {
        let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().min(4));
        Self::with_threads(codec, threads as u32)
    }

    /// A decoder on `threads` threads; each keeps frames of its own.
    pub fn with_threads(codec: Codec, threads: u32) -> Result<Self> {
        let threads = threads.max(1) as c_uint;
        // SAFETY: plain values; null means the codec couldn't start.
        let (raw, decode, free): (_, DecodeFn, FreeFn) = match codec {
            Codec::Vp8 => (
                unsafe { yapfast_decoder_new(0, threads) },
                yapfast_decoder_decode,
                yapfast_decoder_free,
            ),
            Codec::H264 => (
                unsafe { yapfast_decoder_new(1, threads) },
                yapfast_decoder_decode,
                yapfast_decoder_free,
            ),
            #[cfg(hevcdec)]
            Codec::H265 => (
                unsafe { yapfast_hevc_new(threads) },
                yapfast_hevc_decode,
                yapfast_hevc_free,
            ),
            // Not in DECODES, so Discord sends this viewer H264.
            #[cfg(not(hevcdec))]
            Codec::H265 => bail!("this build of Yapfast can't decode H265"),
            Codec::Av1 => (
                unsafe { yapfast_av1_new(threads) },
                yapfast_av1_decode,
                yapfast_av1_free,
            ),
        };
        if raw.is_null() {
            bail!("could not start the {codec:?} decoder");
        }
        Ok(Self {
            raw,
            codec,
            decode,
            free,
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Decodes a whole frame. None while the decoder holds it back.
    pub fn decode(&mut self, frame: &[u8]) -> Result<Option<Picture<'_>>> {
        let mut raw = RawPicture {
            width: 0,
            height: 0,
            y: std::ptr::null(),
            u: std::ptr::null(),
            v: std::ptr::null(),
            y_stride: 0,
            u_stride: 0,
            v_stride: 0,
        };
        // SAFETY: the frame outlives the call; the picture points into the
        // decoder, which `Picture` borrows.
        let decoded = unsafe { (self.decode)(self.raw, frame.as_ptr(), frame.len(), &mut raw) };
        match decoded {
            ..0 => bail!("could not decode a {:?} frame ({decoded})", self.codec),
            0 => Ok(None),
            _ => {
                if raw.width <= 0
                    || raw.height <= 0
                    || raw.y_stride <= 0
                    || raw.u_stride <= 0
                    || raw.v_stride <= 0
                {
                    bail!("the {:?} decoder gave an empty picture", self.codec);
                }
                let (width, height) = (raw.width as u32, raw.height as u32);
                let chroma = height.div_ceil(2) as usize;
                // SAFETY: the decoder's planes hold `height` rows of luma and
                // half as many of each chroma plane, at these strides.
                let plane = |data: *const c_uchar, stride: c_int, rows: usize| unsafe {
                    std::slice::from_raw_parts(data, stride as usize * rows)
                };
                Ok(Some(Picture {
                    width,
                    height,
                    y: plane(raw.y, raw.y_stride, height as usize),
                    u: plane(raw.u, raw.u_stride, chroma),
                    v: plane(raw.v, raw.v_stride, chroma),
                    strides: [
                        raw.y_stride as u32,
                        raw.u_stride as u32,
                        raw.v_stride as u32,
                    ],
                }))
            }
        }
    }
}

impl Picture<'_> {
    /// The picture's planes, as a feed's sink takes them.
    pub fn i420(&self) -> crate::picture::I420<'_> {
        crate::picture::I420 {
            width: self.width,
            height: self.height,
            y: self.y,
            u: self.u,
            v: self.v,
            strides: self.strides,
        }
    }

    /// Writes the picture as RGBA, `width * 4` bytes a row.
    pub fn to_rgba(&self, out: &mut [u8]) {
        // libyuv's ABGR is R, G, B, A in memory.
        yuv_helper::i420_to_abgr(
            self.y,
            self.strides[0],
            self.u,
            self.strides[1],
            self.v,
            self.strides[2],
            out,
            self.width * 4,
            self.width as i32,
            self.height as i32,
        );
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: raw came from the constructor `free` belongs with, and
        // is freed once.
        unsafe { (self.free)(self.raw) };
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
        let elements = yapfast_discord::rtp::obu_elements(&unit).unwrap();
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
}
