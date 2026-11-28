//! Encoding for Go Live and the camera, through native/vp8.c with the libvpx
//! inside libwebrtc, native/h264.c with x264 (vendor/x264-src), and
//! native/hwenc.c with FFmpeg's hardware encoders (vendor/ffmpeg-src) where
//! the GPU has one. Frames arrive as RGBA or I420 and leave as VP8 frames,
//! H264 or H265 access units in Annex B form, or AV1 temporal units of OBUs
//! that give their sizes.

use std::{
    ffi::{c_int, c_long, c_longlong, c_uchar, c_uint, c_ulong},
    sync::Arc,
};

use anyhow::{Result, bail};
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};
use yapfast_discord::{
    pool::{Encoded, Pool},
    rtp::Codec,
};
// FFmpeg's and x264's libraries, which native/hwenc.c and native/h264.c
// call: linked only while named.
#[cfg(hwenc)]
use ffmpeg_src as _;
use x264_src as _;

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

type Encode = unsafe extern "C" fn(
    *mut Raw,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    c_longlong,
    c_ulong,
    c_int,
    *mut c_int,
) -> c_long;
/// Copies the access unit the last encode produced into the buffer given,
/// which must hold it, and lets the encoder give up its copy.
type Drain = unsafe extern "C" fn(*mut Raw, *mut c_uchar, usize) -> c_long;
type SetBitrate = unsafe extern "C" fn(*mut Raw, c_uint) -> c_int;
type Free = unsafe extern "C" fn(*mut Raw);

unsafe extern "C" {
    fn yapfast_vp8_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments, reason = "the C function's parameters")]
    fn yapfast_vp8_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_vp8_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_vp8_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_vp8_free(encoder: *mut Raw);
    fn yapfast_h264_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
        effort: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments, reason = "the C function's parameters")]
    fn yapfast_h264_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_h264_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_h264_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_h264_free(encoder: *mut Raw);
}

#[cfg(hwenc)]
unsafe extern "C" {
    fn yapfast_hwenc_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        high_quality: c_uint,
        report: *mut std::ffi::c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments, reason = "the C function's parameters")]
    fn yapfast_hwenc_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_hwenc_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_hwenc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_free(encoder: *mut Raw);
}

/// RTP's video clock.
pub const CLOCK: u64 = 90_000;

/// How much work an encoder puts into each frame. VP8 has one preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    #[default]
    Light,
    High,
}

pub struct Encoder {
    raw: *mut Raw,
    codec: Codec,
    effort: Effort,
    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc".
    engine: String,
    encode: Encode,
    drain: Drain,
    set_bitrate: SetBitrate,
    free: Free,
    /// The bitrate aimed for, in kbit/s.
    kbps: u32,
    width: u32,
    height: u32,
    fps: u32,
    /// The frame as I420, for [`Self::encode`]: a full-size luma plane, then
    /// two quarter-size chroma planes. Empty until it's first needed: the
    /// screen and the camera arrive as I420.
    planes: Vec<u8>,
    /// The buffers encoded frames are copied into, reused as frames are sent.
    out: Arc<Pool>,
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
}

/// An opened encoder's handle and functions, and its name.
type Opened = (*mut Raw, Encode, Drain, SetBitrate, Free, String);

impl Encoder {
    /// An encoder for `width` by `height` frames, which must be even. With
    /// a hardware preset, H264 goes to the GPU's encoder where there is one, and to
    /// x264 otherwise. H265 and AV1 have only the GPU's.
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: Option<Effort>,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let opened =
            match hardware.and_then(|gpu| open_hardware(codec, width, height, fps, kbps, gpu)) {
                Some(opened) => opened,
                None => open_software(codec, width, height, fps, kbps, effort)?,
            };
        let (raw, encode, drain, set_bitrate, free, engine) = opened;
        let pixels = (width * height) as usize;
        Ok(Self {
            raw,
            codec,
            effort,
            engine,
            encode,
            drain,
            set_bitrate,
            free,
            kbps,
            width,
            height,
            fps,
            planes: Vec::new(),
            // Starts lean; a keyframe of noise is the most the pool can grow to.
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        })
    }

    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc".
    pub fn engine(&self) -> &str {
        &self.engine
    }

    fn hardware(&self) -> bool {
        !matches!(self.engine.as_str(), "libvpx" | "x264")
    }

    /// Puts `opened` in place of the encoder running now.
    fn replace(&mut self, opened: Opened) {
        // SAFETY: raw came from its codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
        (
            self.raw,
            self.encode,
            self.drain,
            self.set_bitrate,
            self.free,
            self.engine,
        ) = opened;
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

    /// Aims for `kbps` from the next frame on, without a keyframe.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw came from this codec's constructor and is still live.
        if unsafe { (self.set_bitrate)(self.raw, kbps) } != 0 {
            bail!("the {:?} encoder refused {kbps} kbit/s", self.codec);
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
        // ABGR is R, G, B, A in memory.
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
        self.collect(encoded?)
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
        let encoded =
            self.encode_planes([(y, y_stride), (u, u_stride), (v, v_stride)], pts, keyframe);
        self.collect(encoded?)
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
        let encoded = self.encode_planes(
            [
                (picture.y, y_stride),
                (picture.u, u_stride),
                (picture.v, v_stride),
            ],
            pts,
            keyframe,
        );
        self.collect(encoded?)
    }

    /// Encodes three planes, each with its stride: the size the encoder
    /// produced, which it keeps until [`Self::collect`] copies it out, and
    /// whether it's a keyframe, or None when the frame was skipped.
    /// When a hardware H264 encoder fails, x264 takes over from this frame,
    /// with a keyframe; H265 and AV1 have nothing to take over.
    fn encode_planes(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<(usize, bool)>> {
        match self.encode_once(planes, pts, keyframe) {
            Err(error) if self.hardware() && self.codec == Codec::H264 => {
                tracing::warn!(engine = self.engine, %error, "the GPU's encoder failed; encoding with x264");
                let (codec, width, height, fps) = (self.codec, self.width, self.height, self.fps);
                let opened = open_software(codec, width, height, fps, self.kbps, self.effort)?;
                self.replace(opened);
                self.encode_once(planes, pts, true)
            }
            encoded => encoded,
        }
    }

    fn encode_once(
        &mut self,
        [(y, y_stride), (u, u_stride), (v, v_stride)]: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<(usize, bool)>> {
        let mut is_key: c_int = 0;
        // SAFETY: the planes hold a picture of this encoder's size at these
        // strides, and stay put during the call.
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
            0 => Ok(None),
            _ => Ok(Some((written as usize, is_key != 0))),
        }
    }

    /// Copies the access unit `size` bytes long that the encoder has put
    /// aside into a buffer from the encoder's pool. A unit larger than the
    /// buffers grows them, doubling until one holds it, and the picture is not
    /// encoded again: the encoder already moved on from it, so a second
    /// encode would reference a frame the receiver never got. This is the one
    /// copy of a frame's bytes between the encoder and the packetizer.
    fn collect(&mut self, encoded: Option<(usize, bool)>) -> Result<Option<Frame>> {
        let Some((size, keyframe)) = encoded else {
            return Ok(None);
        };
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
        // grown to above, and both stay put during the call.
        let copied = unsafe { (self.drain)(self.raw, spare.as_mut_ptr(), spare.len()) };
        if copied <= 0 {
            bail!("the {:?} encoder lost the frame it produced", self.codec);
        }
        debug_assert_eq!(copied as usize, size, "the encoder gave back another frame");
        out.finish(copied as usize);
        Ok(Some(Frame {
            data: out,
            keyframe,
        }))
    }

    /// How many buffers the encoder's pool has allocated: the tests hold it
    /// to the pool's bound.
    #[cfg(test)]
    fn created(&self) -> usize {
        self.out.created()
    }
}

/// libvpx for VP8, x264 for H264.
fn open_software(
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    effort: Effort,
) -> Result<Opened> {
    let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().min(4)) as u32;
    // Null means the library refused the configuration.
    let opened: Opened = match codec {
        Codec::Vp8 => (
            // SAFETY: plain values.
            unsafe { yapfast_vp8_new(width, height, fps, kbps, threads) },
            yapfast_vp8_encode,
            yapfast_vp8_drain,
            yapfast_vp8_set_bitrate,
            yapfast_vp8_free,
            "libvpx".into(),
        ),
        Codec::H264 => (
            // SAFETY: plain values.
            unsafe { yapfast_h264_new(width, height, fps, kbps, threads, effort as c_uint) },
            yapfast_h264_encode,
            yapfast_h264_drain,
            yapfast_h264_set_bitrate,
            yapfast_h264_free,
            "x264".into(),
        ),
        Codec::H265 | Codec::Av1 => bail!(
            "{} encodes only on a GPU, and this one has no encoder for it",
            codec.name()
        ),
    };
    if opened.0.is_null() {
        bail!("could not start the {codec:?} encoder");
    }
    Ok(opened)
}

/// The GPU's encoder for `codec`, if it has one that opens.
#[cfg(hwenc)]
fn open_hardware(
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    effort: Effort,
) -> Option<Opened> {
    let number = match codec {
        Codec::H264 => 0,
        Codec::H265 => 1,
        Codec::Av1 => 2,
        Codec::Vp8 => return None,
    };
    let mut report = [0u8; 512];
    // SAFETY: plain values and a buffer of the size given, which the shim
    // ends with a zero.
    let raw = unsafe {
        yapfast_hwenc_new(
            number,
            width,
            height,
            fps,
            kbps,
            c_uint::from(effort == Effort::High),
            report.as_mut_ptr().cast(),
            report.len(),
        )
    };
    let report = std::ffi::CStr::from_bytes_until_nul(&report).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    );
    if raw.is_null() {
        tracing::info!(?codec, tried = report, "no GPU encoder opened");
        return None;
    }
    tracing::debug!(engine = report, ?codec, "encoding on the GPU");
    Some((
        raw,
        yapfast_hwenc_encode,
        yapfast_hwenc_drain,
        yapfast_hwenc_set_bitrate,
        yapfast_hwenc_free,
        report,
    ))
}

#[cfg(not(hwenc))]
fn open_hardware(_: Codec, _: u32, _: u32, _: u32, _: u32, _: Effort) -> Option<Opened> {
    None
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from this codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    fn test_encoder(
        codec: Codec,
        effort: Effort,
        (width, height): (u32, u32),
        hardware: Option<Effort>,
    ) -> Result<Encoder> {
        assert!(Encoder::new(codec, 63, 36, 30, 500, effort, hardware).is_err());
        Encoder::new(codec, width, height, 30, 500, effort, hardware)
    }

    /// Four frames of moving colour, the third a forced keyframe, and the
    /// engine that encoded them.
    fn encode_four(mut encoder: Encoder) -> (Vec<(bool, Vec<u8>)>, String) {
        let (width, height) = encoder.size();
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut frames = vec![];
        for frame in 0..4u64 {
            for (index, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
            if frame == 1 {
                encoder.set_bitrate(250).unwrap();
                assert_eq!(encoder.bitrate(), 250);
            }
            let encoded = encoder
                .encode(&rgba, frame * 3000, frame == 2)
                .unwrap()
                .unwrap();
            frames.push((encoded.keyframe, encoded.data.to_vec()));
        }
        (frames, encoder.engine().to_owned())
    }

    #[test]
    fn encodes_vp8_keyframes_and_deltas() {
        let encoder = test_encoder(Codec::Vp8, Effort::Light, (64, 36), None).unwrap();
        let (frames, _) = encode_four(encoder);
        for (keyframe, data) in &frames {
            // A keyframe's tag has bit 0 clear, and its size follows the start code.
            assert_eq!(*keyframe, data[0] & 1 == 0);
            if *keyframe {
                assert_eq!(&data[3..6], &[0x9d, 0x01, 0x2a]);
                let size = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]) & 0x3fff;
                assert_eq!((size(6), size(8)), (64, 36));
            }
        }
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// Main profile H264 at `size` that decodes: parameter sets and an IDR
    /// first, a forced IDR third. Returns the engine.
    fn check_h264(effort: Effort, size: (u32, u32), hardware: Option<Effort>) -> String {
        let encoder = test_encoder(Codec::H264, effort, size, hardware).unwrap();
        let (frames, engine) = encode_four(encoder);
        {
            let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
            assert_eq!(keys[..3], [true, false, true]);
            let units = |data| yapfast_discord::rtp::nal_units(data);
            let types =
                |data| -> Vec<u8> { units(data).iter().map(|unit| unit[0] & 0x1f).collect() };
            let first = types(&frames[0].1);
            assert!(
                first.starts_with(&[7, 8]) && first.contains(&5),
                "parameter sets, then an IDR slice: {first:?}"
            );
            // The sequence parameter set's profile_idc: Main, as Discord's
            // client sends.
            assert_eq!(units(&frames[0].1)[0][1], 77);
            assert!(!types(&frames[1].1).contains(&5), "a delta frame");
            let mut decoder = Decoder::new(Codec::H264).unwrap();
            let mut shown = 0;
            for (_, data) in &frames {
                if let Some(picture) = decoder.decode(data).unwrap() {
                    assert_eq!((picture.width, picture.height), size);
                    shown += 1;
                }
            }
            assert!(
                shown >= 3,
                "{shown} pictures from {engine} at {effort:?} effort"
            );
        }
        engine
    }

    #[test]
    fn encodes_main_profile_h264_that_decodes() {
        for effort in [Effort::Light, Effort::High] {
            assert_eq!(check_h264(effort, (64, 36), None), "x264");
        }
    }

    /// H265 on the GPU, where it has an encoder for it: parameter sets and
    /// an IDR first, a delta, then a forced IDR. Yapfast can't decode H265,
    /// so this reads the units' types.
    #[test]
    fn encodes_h265_on_the_gpu() {
        assert!(Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, None).is_err());
        let Ok(encoder) = test_encoder(Codec::H265, Effort::Light, (320, 180), Some(Effort::High))
        else {
            eprintln!("no GPU encoder for H265 here");
            return;
        };
        let (frames, engine) = encode_four(encoder);
        eprintln!("encoded with {engine}");
        let types = |data| -> Vec<u8> {
            yapfast_discord::rtp::nal_units(data)
                .iter()
                .map(|unit| (unit[0] >> 1) & 0x3f)
                .collect()
        };
        let idr = |types: &[u8]| types.iter().any(|kind| matches!(kind, 19 | 20));
        let first = types(&frames[0].1);
        assert!(
            first.starts_with(&[32, 33, 34]) && idr(&first),
            "VPS, SPS, PPS, IDR: {first:?}"
        );
        assert!(!idr(&types(&frames[1].1)), "a delta frame");
        assert!(idr(&types(&frames[2].1)), "the forced keyframe");
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// AV1 on the GPU, where it has an encoder for it: a sequence header
    /// before the first frame and the forced keyframe, not the delta, and
    /// OBUs that packetize whole.
    #[test]
    fn encodes_av1_on_the_gpu() {
        assert!(Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, None).is_err());
        let Ok(encoder) = test_encoder(Codec::Av1, Effort::Light, (320, 180), Some(Effort::High))
        else {
            eprintln!("no GPU encoder for AV1 here");
            return;
        };
        let (frames, engine) = encode_four(encoder);
        eprintln!("encoded with {engine}");
        let sequence = |data| {
            yapfast_discord::rtp::obu_elements(data)
                .expect("OBUs within the frame")
                .first()
                .is_some_and(|element| (element[0] >> 3) & 0x0f == 1)
        };
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
        assert!(sequence(&frames[0].1) && sequence(&frames[2].1));
        assert!(!sequence(&frames[1].1), "a delta frame");
    }

    /// The GPU's encoder where this machine has one, x264 where it hasn't:
    /// the same stream either way.
    #[test]
    fn encodes_h264_on_the_gpu_or_falls_back() {
        for preset in [Effort::Light, Effort::High] {
            let engine = check_h264(Effort::Light, (320, 180), Some(preset));
            eprintln!("encoded with {engine} at {preset:?} GPU effort");
        }
    }

    /// Three frames alive at once keep the bytes they were encoded with while
    /// the encoder encodes on: each owns its buffer, rather than viewing one
    /// the encoder overwrites.
    #[test]
    fn frames_keep_their_bytes_past_more_encodes() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, None).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let paint = |rgba: &mut [u8], frame: u64| {
            for (index, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
        };
        let mut held = vec![];
        for frame in 0..3u64 {
            paint(&mut rgba, frame);
            held.push(
                encoder
                    .encode(&rgba, frame * 3000, frame == 0)
                    .unwrap()
                    .unwrap(),
            );
        }
        let encoded: Vec<Vec<u8>> = held.iter().map(|frame| frame.data.to_vec()).collect();
        for frame in 3..12u64 {
            paint(&mut rgba, frame);
            let _ = encoder.encode(&rgba, frame * 3000, false).unwrap();
        }
        for (frame, encoded) in held.iter().zip(&encoded) {
            assert_eq!(&*frame.data, encoded, "bytes changed under a live frame");
        }
    }

    /// A frame's buffer goes back to the pool when the frame is dropped, and
    /// the frames after it take that buffer rather than allocating.
    #[test]
    fn the_pool_takes_its_buffers_back() {
        let pool = Arc::new(Pool::new(1024));
        let held: Vec<Encoded> = (0..3).map(|_| pool.checkout()).collect();
        assert_eq!(pool.created(), 3);
        drop(held);
        let again: Vec<Encoded> = (0..3).map(|_| pool.checkout()).collect();
        assert_eq!(
            pool.created(),
            3,
            "the pool allocated a buffer while three were idle"
        );
        drop(again);
    }

    /// Growing doubles towards the cap a frame of the encoder's size can
    /// need, and stops there; a buffer of the size it left is dropped when
    /// it comes back.
    #[test]
    fn the_pool_grows_up_to_its_max() {
        let pool = Arc::new(Pool::new(1536 * 1024));
        assert_eq!(pool.capacity(), 512 * 1024);
        let old = pool.checkout();
        assert!(pool.grow());
        assert_eq!(pool.capacity(), 1024 * 1024);
        assert!(pool.grow());
        assert_eq!(pool.capacity(), 1536 * 1024, "clamped to the frame's cap");
        assert!(!pool.grow(), "already as large as a frame can need");
        drop(old);
        let mut grown = pool.checkout();
        assert_eq!(grown.spare().len(), 1536 * 1024);
        assert_eq!(
            pool.created(),
            2,
            "the buffer of the size it outgrew was dropped"
        );
    }

    /// A frame too big for the lean buffer grows the pool's buffer, is copied
    /// into it from the encoder's own copy, and leaves the pool at the size
    /// that held it. The shim is a stand-in, since no real encoder here makes a
    /// 700 KiB frame on demand: it answers the size it produced, as
    /// native/h264.c and native/hwenc.c now do, and hands that frame over once.
    #[test]
    fn a_frame_too_big_for_the_lean_buffer_grows_the_pool() {
        const BULKY: usize = 700 * 1024;
        // What each encoder call did: a picture this size is one submission and
        // one copy, so a submitted count above the pictures encoded is a frame
        // encoded twice.
        static SUBMITTED: AtomicUsize = AtomicUsize::new(0);
        static DRAINED: AtomicUsize = AtomicUsize::new(0);

        // Produces a frame bigger than the pool's opening buffer, and keeps it
        // the way a real shim does: the size says how big it is, not whether it
        // fits.
        unsafe extern "C" fn produces_bulky(
            _encoder: *mut Raw,
            _y: *const c_uchar,
            _y_stride: c_int,
            _u: *const c_uchar,
            _u_stride: c_int,
            _v: *const c_uchar,
            _v_stride: c_int,
            _pts: c_longlong,
            _duration: c_ulong,
            _keyframe: c_int,
            is_key: *mut c_int,
        ) -> c_long {
            SUBMITTED.fetch_add(1, AtomicOrdering::Relaxed);
            // SAFETY: the caller gives a place for the flag.
            unsafe { *is_key = 1 };
            BULKY as c_long
        }

        // Hands the frame it kept to a buffer that holds it.
        unsafe extern "C" fn drains_bulky(
            _encoder: *mut Raw,
            out: *mut c_uchar,
            capacity: usize,
        ) -> c_long {
            if capacity < BULKY {
                return -1;
            }
            // SAFETY: the caller gives a buffer of `capacity` bytes, at least
            // BULKY here.
            unsafe { std::ptr::write_bytes(out, 0xab, BULKY) };
            DRAINED.fetch_add(1, AtomicOrdering::Relaxed);
            BULKY as c_long
        }

        let mut encoder =
            Encoder::new(Codec::H264, 1280, 720, 30, 500, Effort::Light, None).unwrap();
        assert_eq!(encoder.out.capacity(), 512 * 1024);
        encoder.encode = produces_bulky;
        encoder.drain = drains_bulky;
        let rgba = vec![0u8; 1280 * 720 * 4];
        let frame = encoder.encode(&rgba, 0, true).unwrap().unwrap();
        assert_eq!(frame.data.len(), BULKY);
        assert!(frame.keyframe);
        assert!(frame.data.iter().all(|&byte| byte == 0xab));
        // The frame that outgrew the pool was produced once and copied once.
        assert_eq!(SUBMITTED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(DRAINED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(encoder.out.capacity(), 1024 * 1024);
        drop(frame);
        // The grown buffer came back, and is the one the next frame uses.
        let again = encoder.encode(&rgba, 3000, false).unwrap().unwrap();
        assert_eq!(again.data.len(), BULKY);
        // The buffer is taken at the size that holds the frame, so the pool never
        // made one of the size it outgrew: one buffer, not two.
        assert_eq!(encoder.created(), 1);
        assert_eq!(
            SUBMITTED.load(AtomicOrdering::Relaxed),
            2,
            "two pictures were encoded, whatever the pool had to do"
        );
    }

    /// Thirty frames in a row, with three alive at once, stay within the
    /// pool's three buffers: the encoder allocates no more than the first
    /// three.
    #[test]
    fn encoding_thirty_frames_reuses_three_buffers() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, None).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let mut held = VecDeque::new();
        for frame in 0..30u64 {
            // The next frame is encoded before an old one is dropped, so this
            // is three alive at once: what the pool has to cover.
            if held.len() == 3 {
                held.pop_front();
            }
            for (index, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
            if let Some(frame) = encoder.encode(&rgba, frame * 3000, frame == 0).unwrap() {
                held.push_back(frame);
            }
        }
        assert_eq!(held.len(), 3);
        assert!(
            encoder.created() <= 3,
            "the encoder allocated {} buffers over 30 frames",
            encoder.created()
        );
    }

    /// A frame crosses to the sending thread, so the buffer it owns has to.
    #[test]
    fn encoded_frames_are_send() {
        fn send<T: Send>() {}
        send::<Encoded>();
        send::<Frame>();
    }
}
