//! Encoding for Go Live and the camera, through native/vp8.c with the libvpx
//! inside libwebrtc, native/h264.c with x264 (vendor/x264-src), and
//! native/hwenc.c with FFmpeg's hardware encoders (vendor/ffmpeg-src) where
//! the GPU has one. Frames arrive as RGBA or I420 and leave as VP8 frames,
//! H264 or H265 access units in Annex B form, or AV1 temporal units of OBUs
//! that give their sizes.
//!
//! On Linux a frame can also arrive as a DMA-BUF, the picture still on the
//! card that made it: [`Encoder::encode_dmabuf`] wraps it into a VA-API surface
//! with FFmpeg's `av_hwframe_map` and the VA-API encoder reads that surface,
//! through native/hwenc_dmabuf.c. No pixel of such a picture comes back to the
//! processor. A picture the import can't take — the BGRA a compositor drew a
//! desktop in — is offered to the card's video processor instead, which scales
//! and converts it into the surface the encoder reads (`cfg(vaapivpp)`, and
//! [`vpp_filter`] is what the processor is asked for); every other encoder here
//! reads its frames from memory, says so, and is handed the picture through the
//! plane path instead.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulong},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::codec::Codec;
use crate::pool::{Encoded, Pool};
use anyhow::{Result, bail};
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

use crate::gpu::{self, Candidate, Luid, Reach};
// FFmpeg's and x264's libraries are yapfast-core's to link, since the shims
// that call them are built there; what this file declares of them is extern.

unsafe extern "C" {
    fn yapfast_vp8_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
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
    fn yapfast_vp8_staging(encoder: *mut Raw) -> usize;
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
    #[allow(clippy::too_many_arguments)]
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
    fn yapfast_h264_staging(encoder: *mut Raw) -> usize;
    fn yapfast_h264_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_h264_free(encoder: *mut Raw);
}

#[cfg(hwenc)]
unsafe extern "C" {
    /// Whether this FFmpeg build has the encoder `name`, which is how this side
    /// learns what the shipped build was configured with: an encoder the build
    /// hasn't got is one no stream can offer, and one it has may still fail to
    /// open on this machine.
    fn yapfast_hwenc_has(name: *const c_char) -> c_int;
    /// Whether this machine has the card NVENC encodes on, which FFmpeg's own
    /// CUDA device answers: what a Linux share's DMA-BUF path asks before it
    /// opens one of FFmpeg's NVENC sessions to find out.
    #[cfg(target_os = "linux")]
    fn yapfast_hwenc_nvidia() -> c_int;
    /// What an encoder is asked for beyond the picture: the frame's shape and
    /// rate, the bitrate to aim at, the tuning, the encoders to try and the
    /// kind of GPU the stream asked for.
    fn yapfast_hwenc_new(
        ask: *const OpenRequest,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
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
    #[cfg(target_os = "windows")]
    fn yapfast_hwenc_encode_texture(
        encoder: *mut Raw,
        texture: *mut std::ffi::c_void,
        subresource: c_uint,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    #[cfg(target_os = "windows")]
    fn yapfast_hwenc_shared(encoder: *mut Raw) -> c_int;
    fn yapfast_hwenc_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_hwenc_staging(encoder: *mut Raw) -> usize;
    fn yapfast_hwenc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_notes(encoder: *mut Raw, notes: *mut c_char, notes_size: usize);
    /// FFmpeg's own name for the encoder that opened, such as "h264_nvenc",
    /// which is what this side matches its choice of backend back to: the
    /// report beside it names VA-API's device too.
    fn yapfast_hwenc_engine(encoder: *mut Raw, name: *mut c_char, name_size: usize);
    fn yapfast_hwenc_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_hwenc_tuning(encoder: *mut Raw, tuning: *mut c_char, tuning_size: usize);
    fn yapfast_hwenc_late(encoder: *mut Raw) -> c_uint;
    fn yapfast_hwenc_free(encoder: *mut Raw);
}

// The VA-API encoder that reads the card's own pictures, from
// native/hwenc_dmabuf.c: a DMA-BUF wrapped into a VA-API surface and encoded
// there, with no upload. It is a session of its own, on the device the encoder
// above opened on, with the same settings; see [`Encoder::encode_dmabuf`].
#[cfg(dmabufenc)]
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_hwenc_dmabuf_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        encoder: *const c_char,
        device: *const c_char,
        driver: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    fn yapfast_hwenc_dmabuf_encode(
        encoder: *mut Raw,
        picture: *const DmabufPicture,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_hwenc_dmabuf_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    /// Whether the card's encoder's shim has a video processor at all — FFmpeg's
    /// filters and its `scale_vaapi` — which is a fact about the build, asked
    /// before a picture is looked at: a build without one never opens a session
    /// for a screen.
    #[cfg(vaapivpp)]
    fn yapfast_hwenc_dmabuf_converts() -> c_int;
    /// Encodes a picture the encoder can't read as it arrived, by the card's
    /// video processor making the NV12 surface it reads out of it first:
    /// mapped, scaled to the stream's size, converted, and handed over as that
    /// surface. `args` are the processor's own, from [`vpp_filter`].
    #[cfg(vaapivpp)]
    fn yapfast_hwenc_dmabuf_convert(
        encoder: *mut Raw,
        picture: *const DmabufPicture,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
        args: *const c_char,
    ) -> c_long;
    fn yapfast_hwenc_dmabuf_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_dmabuf_late(encoder: *mut Raw) -> c_uint;
    fn yapfast_hwenc_dmabuf_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_hwenc_dmabuf_error(encoder: *const Raw) -> *const c_char;
    fn yapfast_hwenc_dmabuf_free(encoder: *mut Raw);
    /// The settings the encoder that reads the card's own pictures is opened
    /// with, for a test to hold them to native/hwenc.c's VA-API branch without a
    /// GPU in the shape of the question. Nothing else in this build asks: what
    /// the session took is its `config`, logged at the open.
    #[cfg(test)]
    fn yapfast_hwenc_dmabuf_settings(
        fps: c_uint,
        kbps: c_uint,
        out: *mut c_char,
        size: usize,
    ) -> c_int;
}

#[cfg(all(target_os = "windows", hwenc))]
thread_local! {
    /// The capture's device, for the open that is happening now on this thread:
    /// a session reads a texture only on the device it was opened on.
    static CAPTURE_DEVICE: std::cell::RefCell<Option<*mut std::ffi::c_void>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;
    #[cfg(target_os = "linux")]
    use crate::dmabuf;
    use std::{
        collections::VecDeque,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
    };

    // The settings native/hwenc.c opens NVENC with for the tuning named, and
    // whether the stream can't do without each: the list the open reads, so a
    // test holds the shipped tuning to what the stream sends rather than to a
    // copy of it, and needs no GPU in the shape of the question.
    #[cfg(hwenc)]
    unsafe extern "C" {
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const std::ffi::c_char,
            asked: *mut std::ffi::c_char,
            asked_size: usize,
        ) -> c_int;
    }

    /// Four frames of moving colour, the third a forced keyframe, and what
    /// the encoder that made them was opened with.
    fn encode_four(
        codec: Codec,
        effort: Effort,
        (width, height): (u32, u32),
        hardware: bool,
    ) -> (Vec<(bool, Vec<u8>)>, Configuration) {
        let mut encoder = Encoder::new(codec, width, height, 30, 500, effort, hardware).unwrap();
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut frames = vec![];
        for frame in 0..4u64 {
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
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
            assert_eq!(
                encoded.timestamp,
                (frame * 3000) as u32,
                "the frame carries the time it was encoded at"
            );
            frames.push((encoded.keyframe, encoded.data.to_vec()));
        }
        assert!(Encoder::new(codec, 63, 36, 30, 500, effort, hardware).is_err());
        (frames, encoder.configuration().clone())
    }

    /// The stream's clock is worked out in [`timestamp`] and nowhere else:
    /// 90 kHz over RTP's 32 bits, which wrap after 13 hours.
    #[test]
    fn the_stream_clock_is_one_derivation() {
        assert_eq!(timestamp(Duration::ZERO), 0);
        assert_eq!(timestamp(Duration::from_millis(1)), 90);
        assert_eq!(timestamp(Duration::from_secs(1)), CLOCK);
        // 48,000 seconds is 4,320,000,000 ticks: more than 32 bits holds.
        assert_eq!(timestamp(Duration::from_secs(48_000)), 4_320_000_000);
        assert_eq!(timestamp(Duration::from_secs(48_000)) as u32, 25_032_704);
    }

    /// Three frames alive at once keep the bytes they were encoded with while
    /// the encoder encodes on: each owns its buffer, rather than viewing one
    /// the encoder overwrites.
    #[test]
    fn frames_keep_their_bytes_past_more_encodes() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, false).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let paint = |rgba: &mut [u8], frame: u64| {
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
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

    /// A frame too big for the lean buffer grows the pool's buffer, is
    /// encoded into it, and leaves the pool at the size that held it. The
    /// shim is a stand-in, since no real encoder here makes a 700 KiB frame
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
            Encoder::new(Codec::H264, 1280, 720, 30, 500, Effort::Light, false).unwrap();
        assert_eq!(encoder.out.capacity(), 512 * 1024);
        encoder.encode = produces_bulky;
        encoder.drain = drains_bulky;
        let rgba = vec![0u8; 1280 * 720 * 4];
        let frame = encoder.encode(&rgba, 0, true).unwrap().unwrap();
        assert_eq!(frame.data.len(), BULKY);
        assert!(frame.keyframe);
        assert!(frame.data.iter().all(|&byte| byte == 0xab));
        // The frame that outgrew the pool was produced once and copied once:
        // one frame out, at the time it was captured, not a frame later.
        assert_eq!(frame.timestamp, 0);
        assert_eq!(SUBMITTED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(DRAINED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(encoder.out.capacity(), 1024 * 1024);
        drop(frame);
        // The grown buffer came back, and is the one the next frame uses.
        let again = encoder.encode(&rgba, 3000, false).unwrap().unwrap();
        assert_eq!(again.data.len(), BULKY);
        assert_eq!(again.timestamp, 3000);
        // The buffer is taken at the size that holds the frame, so the pool never
        // made one of the size it outgrew: one buffer, not two.
        assert_eq!(encoder.created(), 1);
        assert_eq!(
            SUBMITTED.load(AtomicOrdering::Relaxed),
            2,
            "two pictures were encoded, whatever the pool had to do"
        );
    }

    /// A frame that outgrows the pool is copied out of the encoder, not encoded
    /// again: every access unit decodes in order, one submission went in for each
    /// picture, and the encoder keeps one frame's worth of memory for the copy
    /// rather than a growing history of them.
    ///
    /// The pool's opening buffer is 512 KiB, so a frame has to be bigger than
    /// that to provoke the growth a stream sees when a picture of a hard scene
    /// arrives; a keyframe of noise at a high rate is. The frame after the
    /// keyframe then goes through a pool at its opening size, which is where the
    /// old code encoded that picture a second time. A second submission would be
    /// an inter frame referencing the copy the receiver never got, which is what
    /// decoding every access unit says isn't happening.
    #[test]
    fn a_frame_that_outgrows_the_pool_is_copied_not_encoded_again() {
        const FRAMES: u64 = 10;
        // 1080p at a rate a screen of noise can fill, so a frame of it is larger
        // than the pool's opening 512 KiB.
        const WIDTH: u32 = 1920;
        const HEIGHT: u32 = 1080;
        const KBPS: u32 = 200_000;
        // What a pool hands out before a frame grows it.
        const OPENING: usize = 512 * 1024;
        for (codec, hardware) in [
            (Codec::Vp8, false),
            (Codec::H264, false),
            (Codec::H264, true),
        ] {
            let mut encoder =
                match Encoder::new(codec, WIDTH, HEIGHT, 30, KBPS, Effort::Light, hardware) {
                    Ok(encoder) => encoder,
                    Err(why) => {
                        eprintln!("no {codec:?} encoder here (hardware {hardware}): {why:#}");
                        continue;
                    }
                };
            if hardware && !encoder.hardware() {
                eprintln!(
                    "no GPU encoder for {codec:?} here: the open fell back to {}",
                    encoder.engine()
                );
                continue;
            }
            eprintln!("outgrowing the pool on {}", encoder.engine());
            assert_eq!(encoder.out.capacity(), OPENING);
            // A picture that changes every frame, so the encoder has bits to
            // spend and a delta frame can outgrow a buffer as a keyframe of a
            // harsh scene does.
            let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
            let paint = |rgba: &mut [u8], frame: u64| {
                let mut state =
                    0x9e37_79b9_7f4a_7c15u64 ^ frame.wrapping_mul(0x2545_f491_4f6c_dd1d);
                for pixel in rgba.chunks_exact_mut(4) {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    pixel.copy_from_slice(&[
                        state as u8,
                        (state >> 8) as u8,
                        (state >> 16) as u8,
                        255,
                    ]);
                }
            };

            // The keyframe goes through the pool the encoder opened with, and
            // succeeds: it is the picture the pool may grow to hold.
            paint(&mut rgba, 0);
            let keyframe = encoder.encode(&rgba, 0, true).unwrap().unwrap();
            assert!(
                keyframe.keyframe,
                "{codec:?}: the first frame isn't a keyframe"
            );
            assert_eq!(encoder.submitted(), 1);
            let mut frames = vec![keyframe];

            // From here a frame meets the pool at its opening size: the case a
            // stream hits when the picture after a keyframe is bigger than the
            // buffer waiting for it.
            encoder.out = Arc::new(Pool::new((WIDTH * HEIGHT * 2 + 4096) as usize));
            assert_eq!(encoder.out.capacity(), OPENING);
            for frame in 1..FRAMES {
                paint(&mut rgba, frame);
                let pts = frame * 3000;
                let encoded = encoder
                    .encode(&rgba, pts, false)
                    .unwrap_or_else(|why| panic!("{codec:?} lost frame {frame}: {why:#}"))
                    .unwrap_or_else(|| panic!("{codec:?} made no frame for {frame}"));
                assert_eq!(
                    encoded.timestamp, pts as u32,
                    "{codec:?}: another frame's time"
                );
                frames.push(encoded);
                // One picture, one submission: the encoder's state moved on with
                // the first, so a retry would be a second picture referencing a
                // frame that was never sent.
                assert_eq!(
                    encoder.submitted() as u64,
                    frame + 1,
                    "{codec:?} submitted the picture at {pts} more than once"
                );
            }
            // Each frame after the keyframe is larger than the buffer the pool
            // had at its opening size, or nothing about growth was proved here.
            let grown = encoder.out.capacity();
            let smallest = frames[1..]
                .iter()
                .map(|frame| frame.data.len())
                .min()
                .unwrap();
            assert!(
                smallest > OPENING,
                "{codec:?}: a frame of {smallest} bytes fits the pool's opening buffer, so the \
                 pool never had to grow and nothing was proved"
            );
            assert!(
                grown >= smallest,
                "{codec:?}: a {smallest} byte frame in a {grown} byte pool"
            );

            // Every access unit decodes, in order: a frame encoded twice would
            // have referenced the copy the receiver never got.
            let mut decoder = Decoder::new(codec).unwrap();
            let mut shown = 0;
            for (at, frame) in frames.iter().enumerate() {
                let picture = decoder
                    .decode(&frame.data)
                    .unwrap_or_else(|why| panic!("{codec:?}: frame {at} doesn't decode: {why:#}"));
                if picture.is_some() {
                    shown += 1;
                }
            }
            assert_eq!(
                shown as u64, FRAMES,
                "{codec:?}: {shown} pictures from {FRAMES} frames"
            );
            // What the encoder keeps for a frame to be copied out of is one
            // frame's worth at most, not the sum of them: libvpx's frame is
            // gathered into a buffer as big as the largest, and x264's is
            // copied straight out of its own memory, so nothing is kept.
            let largest = frames.iter().map(|frame| frame.data.len()).max().unwrap();
            let kept = if codec == Codec::H264 { 0 } else { largest };
            assert_eq!(
                encoder.staging(),
                kept,
                "{codec:?} keeps {} bytes for its frame to be copied out of",
                encoder.staging()
            );
            assert!(
                encoder.staging() <= (WIDTH * HEIGHT * 2 + 4096) as usize,
                "{codec:?} keeps {} bytes, more than a frame of this size can need",
                encoder.staging()
            );
        }
    }

    /// Thirty frames in a row, with three alive at once, stay within the
    /// pool's three buffers: the encoder allocates no more than the first
    /// three.
    #[test]
    fn encoding_thirty_frames_reuses_three_buffers() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, false).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let mut held = VecDeque::new();
        for frame in 0..30u64 {
            // The next frame is encoded before an old one is dropped, so this
            // is three alive at once: what the pool has to cover.
            if held.len() == 3 {
                held.pop_front();
            }
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
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

    /// A fallback starts the new encoder's drop count at none: the count a
    /// stream warns about is the encoder's own, and one kept from the encoder
    /// that failed would read 0 until the replacement dropped more frames than
    /// its predecessor did.
    #[test]
    fn a_replaced_encoder_starts_its_drop_count_at_none() {
        let mut encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
        encoder.late = 7;
        let opened = open_software(Codec::H264, 64, 36, 30, 500, Effort::Light).unwrap();
        encoder.replace(opened);
        assert_eq!(encoder.late, 0);
    }

    #[test]
    fn encodes_vp8_keyframes_and_deltas() {
        let (frames, _) = encode_four(Codec::Vp8, Effort::Light, (64, 36), false);
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

    /// The colour the encoders declare is the colour the pixels were
    /// converted with, and nothing else is: `abgr_to_i420` writes BT.601's
    /// matrix over its limited range, which is what h264.c's VUI (6, 6, 6,
    /// not full range) and hwenc.c's `AVCOL_SPC_SMPTE170M` and
    /// `AVCOL_RANGE_MPEG` say the stream is. Black and white land on the
    /// range's ends rather than on 0 and 255, and green's luma is 601's
    /// weights, not 709's. A converter that changed would fail this, and both
    /// declarations would have to move with it.
    #[test]
    fn the_declared_colour_is_the_colour_that_was_converted() {
        let (width, height) = (16u32, 16u32);
        let planes = |pixel: [u8; 4]| {
            let mut frame = vec![0u8; (width * height * 4) as usize];
            for at in frame.chunks_exact_mut(4) {
                at.copy_from_slice(&pixel);
            }
            let chroma = (width / 2 * (height / 2)) as usize;
            let (mut y, mut u, mut v) = (
                vec![0; (width * height) as usize],
                vec![0; chroma],
                vec![0; chroma],
            );
            yuv_helper::abgr_to_i420(
                &frame,
                width * 4,
                &mut y,
                width,
                &mut u,
                width / 2,
                &mut v,
                width / 2,
                width as i32,
                height as i32,
            );
            (y[0], u[0], v[0])
        };
        // Limited range: 16 at the bottom, 235 at the top, so a plane of
        // mid-grey sits below the middle rather than on it.
        assert_eq!(planes([0, 0, 0, 255]).0, 16);
        assert_eq!(planes([255, 255, 255, 255]).0, 235);
        assert!((124..=127).contains(&planes([128, 128, 128, 255]).0));
        // 601's matrix: green is 0.587 of the way up the range, where 709's
        // 0.715 would put it at 173.
        let (green, ..) = planes([0, 255, 0, 255]);
        assert!((143..=147).contains(&green), "green at Y {green}");
    }

    /// The report the worker logs as "no GPU encoder opened" names every
    /// encoder the list holds, including the ones passed over before they
    /// could be tried: a size no H264 encoder takes leaves a reason for each.
    /// Which reason VA-API gives depends on the build, and the box this was
    /// written on showed that plainly: FFmpeg for Windows has no h264_vaapi
    /// in it, so it is "not in this FFmpeg" there rather than the no-device
    /// reason a Linux build without a driver would give. Quick Sync is named
    /// whether or not this build carries it — h264_qsv here, or "not in this
    /// FFmpeg" — so a machine with Intel's GPU is told which of the two it is
    /// rather than left out of the report.
    #[cfg(hwenc)]
    #[test]
    fn a_backend_passed_over_says_why() {
        let mut report = [0u8; 512];
        // Nothing named but the picture: this build's own order of encoders and
        // the shipped tuning, which is what a machine whose adapters can't be
        // listed is given.
        let ask = OpenRequest {
            codec: 0,
            width: 8192,
            height: 8192,
            fps: 30,
            kbps: 500,
            tuning: std::ptr::null(),
            encoders: std::ptr::null(),
            preference: std::ptr::null(),
            // No capture's device: this asks what a machine with none opens.
            device: std::ptr::null_mut(),
        };
        // SAFETY: plain values and a buffer of the size given, which the shim
        // ends with a zero.
        let raw = unsafe { yapfast_hwenc_new(&ask, report.as_mut_ptr().cast(), report.len()) };
        if !raw.is_null() {
            // SAFETY: raw came from the call above and is freed once.
            unsafe { yapfast_hwenc_free(raw) };
            panic!("an H264 encoder opened on 8192 by 8192, so nothing was passed over");
        }
        let report = c_text(&report);
        // Each name, and a reason after it rather than a bare mention.
        for name in ["h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi"] {
            assert!(
                report.contains(&format!("{name}: ")) || report.contains(&format!("{name} (")),
                "{name} is unmentioned in {report:?}"
            );
        }
    }

    /// Quick Sync is asked for by FFmpeg's name for each codec, and not for
    /// VP8, which has no hardware encoder anywhere: a name that doesn't exist
    /// is one the probe would report as missing from the build on every Intel
    /// machine, so the mapping is pinned here rather than found from a log on
    /// hardware nobody has run this on.
    #[test]
    fn quick_sync_names_the_encoder_of_each_codec_it_encodes() {
        for (codec, name) in [
            (Codec::H264, "h264_qsv"),
            (Codec::H265, "hevc_qsv"),
            (Codec::Av1, "av1_qsv"),
        ] {
            assert_eq!(engine_name(gpu::Backend::Qsv, codec), Some(name));
        }
        assert_eq!(engine_name(gpu::Backend::Qsv, Codec::Vp8), None);
    }

    /// Main profile H264 at `size` that decodes: parameter sets and an IDR
    /// first, a forced IDR third. Returns what the encoder was opened with.
    fn check_h264(effort: Effort, size: (u32, u32), hardware: bool) -> Configuration {
        let (frames, configuration) = encode_four(Codec::H264, effort, size, hardware);
        {
            let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
            assert_eq!(keys[..3], [true, false, true]);
            let units = |data| crate::bitstream::nal_units(data);
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
                "{shown} pictures from {} at {effort:?} effort",
                configuration.engine
            );
        }
        configuration
    }

    #[test]
    fn encodes_main_profile_h264_that_decodes() {
        for effort in [Effort::Light, Effort::High] {
            assert_eq!(check_h264(effort, (64, 36), false).engine, "x264");
        }
    }

    /// H265 on the GPU, where it has an encoder for it: parameter sets and
    /// an IDR first, a delta, then a forced IDR. Yapfast can't decode H265,
    /// so this reads the units' types.
    #[test]
    fn encodes_h265_on_the_gpu() {
        if Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for H265 here");
            return;
        }
        assert!(Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, configuration) = encode_four(Codec::H265, Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        let types = |data| -> Vec<u8> {
            crate::bitstream::nal_units(data)
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
        if Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for AV1 here");
            return;
        }
        assert!(Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, configuration) = encode_four(Codec::Av1, Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        let sequence = |data| {
            crate::bitstream::obu_elements(data)
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
        let configuration = check_h264(Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        // What the stream was tuned with is said at the open, and the keyframe
        // interval in it is the encoder's own answer rather than this side's
        // assumption. x264 has its settings in native/h264.c and reports none
        // of them, so it is held to claiming nothing instead.
        assert!(
            configuration.engine == "x264" || configuration.tuning.contains("keyframe every"),
            "{configuration:?}"
        );
    }

    /// An encoder on the processor says which effort it was asked for and
    /// claims nothing that nobody read back: the picture's settings are
    /// native/h264.c's, and this side never says what they came out as.
    #[test]
    fn a_software_encoder_reports_the_effort_it_was_asked_for() {
        let encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::High, false).unwrap();
        assert_eq!(
            encoder.configuration(),
            &Configuration {
                engine: "x264".into(),
                asked: "High effort".into(),
                tuning: String::new(),
                settings_dropped: String::new(),
            }
        );
    }

    /// The shipped tuning is the one the stream was measured with, and the
    /// settings a driver may drop are still marked as ones it may drop.
    ///
    /// The list is native/hwenc.c's own, not a copy here, so this reads what
    /// the open applies: p4 at low-latency tuning, capped VBR, no frame
    /// allowed to wait and an IDR on demand. The optional ones are the three
    /// that only improve the picture, which is what makes a driver refusing
    /// one a note the stream warns about ("the GPU's encoder dropped optional
    /// settings") rather than a failed open; a required one marked optional
    /// would go missing in silence instead, and a new picture is a change
    /// nobody has run a comparison of, so it has to be a failing test before
    /// it is a shipped default.
    #[cfg(hwenc)]
    #[test]
    fn the_shipped_tuning_is_the_one_that_was_measured() {
        let tuning = Tuning::default();
        assert_eq!(tuning.name(), c"p4");
        let mut asked = [0u8; 192];
        // SAFETY: a name from the table above, and a buffer of the size given.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(
                tuning.name().as_ptr(),
                asked.as_mut_ptr().cast(),
                asked.len(),
            )
        };
        assert_eq!(found, 0);
        assert_eq!(
            c_text(&asked),
            "preset=p4(optional) tune=ll(required) rc=vbr(optional) \
             zerolatency=1(optional) delay=0(required) forced-idr=1(required)"
        );
        // Naming nothing asks for the same row, so an encoder opened without a
        // word from Rust is the tuning that was tested.
        let mut unnamed = [0u8; 192];
        // SAFETY: as above, with no name at all.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(
                std::ptr::null(),
                unnamed.as_mut_ptr().cast(),
                unnamed.len(),
            )
        };
        assert_eq!((found, c_text(&unnamed)), (0, c_text(&asked)));
        // A tuning nobody has is refused rather than quietly replaced by this
        // one: a name that stopped matching would otherwise encode a tuning
        // the stream never chose.
        let mut none = [0u8; 192];
        // SAFETY: as above, with a name no row has.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(c"p7".as_ptr(), none.as_mut_ptr().cast(), none.len())
        };
        assert_eq!(found, -1);
    }
    /// A shim's `yapfast_hwenc_notes` for a driver that dropped the preset,
    /// writing the names as native/hwenc.c does, and one for a driver that
    /// kept every setting and so leaves the buffer empty.
    #[cfg(hwenc)]
    unsafe extern "C" fn drops_preset(_: *mut Raw, notes: *mut std::ffi::c_char, size: usize) {
        let names = c"preset,tune";
        let bytes = names.to_bytes_with_nul();
        // SAFETY: the caller gives a buffer of `size` bytes, and the shim
        // writes what fits, ending it with a zero.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().cast(), notes, bytes.len().min(size));
        }
    }
    #[cfg(hwenc)]
    unsafe extern "C" fn keeps_every_setting(
        _: *mut Raw,
        notes: *mut std::ffi::c_char,
        size: usize,
    ) {
        if size > 0 {
            // SAFETY: the caller gives a buffer of `size` bytes.
            unsafe { *notes = 0 };
        }
    }
    /// An encoder whose open read `notes` from a shim, as the GPU's open does
    /// with the shim's own: x264 here, so no driver has to refuse anything.
    #[cfg(hwenc)]
    fn encoder_with_notes(
        notes: unsafe extern "C" fn(*mut Raw, *mut std::ffi::c_char, usize),
    ) -> Encoder {
        let mut encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
        encoder.configuration.settings_dropped = read_notes(notes, std::ptr::null_mut());
        encoder
    }
    /// A setting the driver dropped is not only logged: the name the shim
    /// wrote at the open stays on the encoder, which is what the report the
    /// Stream tab shows copies, so a stream says what its picture isn't
    /// getting rather than leaving it in a log nobody reads.
    #[cfg(hwenc)]
    #[test]
    fn a_dropped_setting_is_kept_for_the_stream_report() {
        let encoder = encoder_with_notes(drops_preset);
        assert_eq!(encoder.configuration().settings_dropped, "preset,tune");
    }
    /// An encoder that kept every setting reports no names, so the Stream
    /// tab's row for them is absent rather than empty.
    #[cfg(hwenc)]
    #[test]
    fn an_encoder_that_dropped_nothing_reports_nothing() {
        let encoder = encoder_with_notes(keeps_every_setting);
        assert!(encoder.configuration().settings_dropped.is_empty());
    }

    /// Which encoders read a picture where it lies, by the name FFmpeg knows
    /// them by: VA-API's, and no other. NVENC, AMF and Quick Sync are handed
    /// frames in system memory, so a DMA-BUF given to one of them could only be
    /// copied into one; x264 and libvpx read the processor's memory too.
    ///
    /// The names come from `engine_name`, so this is the answer for every
    /// backend of every codec a stream can ask for, and a backend added to it
    /// without one here fails rather than quietly uploading.
    #[test]
    fn only_vaapi_reads_a_picture_where_it_lies() {
        for backend in [
            gpu::Backend::Nvenc,
            gpu::Backend::Amf,
            gpu::Backend::Qsv,
            gpu::Backend::Vaapi,
        ] {
            for codec in [Codec::H264, Codec::H265, Codec::Av1] {
                let name = engine_name(backend, codec).unwrap();
                assert_eq!(
                    takes_dmabuf(name),
                    backend == gpu::Backend::Vaapi,
                    "{name} is on {backend:?}, and its answer changed"
                );
            }
        }
        // The two encoders on the processor report themselves by these names.
        assert!(!takes_dmabuf("x264"));
        assert!(!takes_dmabuf("libvpx"));
        assert!(!takes_dmabuf(""));
    }

    /// An NV12 descriptor becomes one DRM object with two planes, which is what
    /// native/hwenc_dmabuf.c hands FFmpeg to import as a VA-API surface: the
    /// descriptor, the modifier, where each plane starts and how wide its rows
    /// are, and a buffer size that holds both planes. This is the mapping on its
    /// own, so it needs no card to prove it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_descriptor_maps_to_one_nv12_object() {
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let planes = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        let fds = [7, 7];
        let picture = dmabuf::Descriptor {
            width: 320,
            height: 180,
            fourcc: NV12,
            modifier: 0x0100_0000_0000_0002,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        assert_eq!(
            encoder.importable(&picture),
            Ok(DmabufPicture {
                width: 320,
                height: 180,
                fd: 7,
                // The chroma plane's last row ends at 73,728 + 384 × 90, past
                // where the luma plane's does: that is how much of the buffer
                // the picture needs, and a descriptor carries no size to be
                // told instead.
                size: 108_288,
                modifier: 0x0100_0000_0000_0002,
                fourcc: NV12,
                planes: 2,
                offset: [0, 73_728],
                pitch: [384, 384],
            })
        );
    }

    /// Every descriptor that isn't one NV12 picture in one buffer is refused by
    /// name, rather than imported as a surface that would show something else:
    /// the BGRA a screen capture's DMA-BUF holds, a picture of another size, a
    /// pitch narrower than the picture, planes in two buffers, a plane with no
    /// buffer, and a shape that isn't NV12's two planes.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_picture_that_isnt_nv12_is_refused_by_name() {
        fn picture<'a>(
            fourcc: u32,
            (width, height): (u32, u32),
            planes: &'a [dmabuf::Plane],
            fds: &'a [c_int],
        ) -> dmabuf::Descriptor<'a> {
            dmabuf::Descriptor {
                width,
                height,
                fourcc,
                modifier: 0,
                planes,
                colour: dmabuf::Colour::SDR,
                fds,
            }
        }
        fn refuses(encoder: &Encoder, picture: &dmabuf::Descriptor<'_>, why: &'static str) {
            assert_eq!(encoder.importable(picture), Err(Refused::Unimportable(why)));
        }
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let nv12 = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        // A screen capture hands over BGRA, which a VA-API surface can't be made
        // of without converting it: that conversion is the copy this doesn't
        // make, so it is refused rather than done.
        let bgra = u32::from_le_bytes(*b"XR24");
        refuses(
            &encoder,
            &picture(bgra, (320, 180), &nv12, &[7, 7]),
            "a picture that isn't NV12, which a VA-API surface is made of",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 182), &nv12, &[7, 7]),
            "a picture of another size than the encoder's",
        );
        let narrow = [
            dmabuf::Plane {
                offset: 0,
                stride: 256,
            },
            nv12[1],
        ];
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &narrow, &[7, 7]),
            "a pitch narrower than the picture",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &nv12, &[7, 8]),
            "planes in two buffers, which one surface can't be made of",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &nv12, &[-1, -1]),
            "a plane with no buffer",
        );
        let one = [nv12[0]];
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &one, &[7]),
            "a picture that isn't NV12's two planes in one buffer",
        );
    }

    /// The render node and libva driver the stream's encoder opened on are read
    /// back out of the report native/hwenc.c wrote, so that the encoder reading
    /// the card's pictures is on the GPU the stream chose. A report with no
    /// device in it — an encoder on the processor, or a VA-API one whose shim
    /// named no node — leaves FFmpeg to place the session.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_device_the_encoder_opened_on_is_read_back() {
        assert_eq!(
            vaapi_device("h264_vaapi (/dev/dri/renderD128, radeonsi)"),
            Some(("/dev/dri/renderD128", "radeonsi"))
        );
        assert_eq!(
            vaapi_device("av1_vaapi (/dev/dri/renderD129, iHD)"),
            Some(("/dev/dri/renderD129", "iHD"))
        );
        for report in [
            // The encoders on the processor, and NVENC, which has no device to
            // name.
            "x264",
            "libvpx",
            "h264_nvenc",
            // A node with no driver named for it is not one to open.
            "h264_vaapi (/dev/dri/renderD128)",
            "h264_vaapi ()",
        ] {
            assert_eq!(vaapi_device(report), None, "{report:?}");
        }
    }

    /// A driver refused because it was built against a newer glibc says which
    /// one in its own words, and that version is read back out and logged
    /// beside this build's: on NixOS, the size of the gap is what says the
    /// machine's channel is newer than the flake's pin.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_driver_that_wants_a_newer_glibc_says_which() {
        let refusal = "av1_vaapi (renderD128, radeonsi): libva: dlopen of \
                       /run/opengl-driver/lib/dri/radeonsi_drv_video.so failed: \
                       /nix/store/x-glibc-2.40-224/lib/libm.so.6: version `GLIBC_2.42' not \
                       found (required by /run/opengl-driver/lib/dri/radeonsi_drv_video.so)";
        assert_eq!(wanted_glibc(refusal), Some("2.42"));
        // A refusal of any other kind has no gap to report.
        for refusal in [
            "av1_vaapi (renderD128, radeonsi): the driver said no",
            "h264_nvenc: no NVIDIA GPU here",
            "h264_vaapi: no GPU here with a VA-API driver",
        ] {
            assert_eq!(wanted_glibc(refusal), None, "{refusal:?}");
        }
        // This build's own, which is a version and not an empty string.
        let here = glibc_here();
        assert!(here.starts_with(char::is_numeric), "glibc {here}");
    }

    /// A DMA-BUF handed to an encoder on the processor is answered with a
    /// refusal rather than a copy: the caller logs one line and hands the same
    /// picture to the plane path, which is the fallback and the only path that
    /// moves pixels. The counters say which of the two happened.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_encoder_on_the_processor_cannot_take_a_dmabuf() {
        let (width, height) = (320u32, 180u32);
        let mut encoder =
            Encoder::new(Codec::H264, width, height, 30, 500, Effort::Light, false).unwrap();
        assert_eq!(encoder.engine(), "x264");
        let planes = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        let fds = [7, 7];
        let picture = dmabuf::Descriptor {
            width,
            height,
            fourcc: NV12,
            modifier: 0,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        assert!(matches!(
            encoder.encode_dmabuf(&picture, true, 0),
            Err(Refused::Uploads)
        ));
        // The refusal is an answer, not a picture: nothing was encoded, nothing
        // was moved, and nothing was encoded where it lay.
        assert_eq!((encoder.copied(), encoder.encoded_in_place()), (0, 0));
        assert_eq!(encoder.engine(), "x264", "the line reports what ran");
        // The same picture through the plane path is the fallback, and it is the
        // path the processor moves pixels for.
        let rgba = vec![0u8; (width * height * 4) as usize];
        let frame = encoder.encode(&rgba, 0, true).unwrap().unwrap();
        assert!(frame.keyframe);
        assert_eq!(frame.timestamp, 0);
        assert_eq!((encoder.copied(), encoder.encoded_in_place()), (1, 0));
    }

    /// A refusal says in words what it is, since that is the one log line a
    /// caller writes before falling back; nothing it names is a copy.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_refusal_says_which_it_is() {
        assert_eq!(
            Refused::Uploads.to_string(),
            "the encoder reads its pictures from memory"
        );
        assert_eq!(
            Refused::NotBuilt.to_string(),
            "this build has no VA-API path for a DMA-BUF"
        );
        assert_eq!(
            Refused::Unimportable("a pitch narrower than the picture").to_string(),
            "a pitch narrower than the picture"
        );
        assert_eq!(
            Refused::Failed.to_string(),
            "the card's encoder wouldn't take the picture"
        );
        assert_eq!(
            Refused::Converted.to_string(),
            "the card's video processor scaled and converted it to NV12 before the encoder read it"
        );
    }

    /// The card's video processor takes the two orders a compositor draws a
    /// desktop in, and NV12 itself, which it only scales: a picture of any other
    /// format is one no processor here converts, and so one for the processor's
    /// own copy.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn the_video_processor_takes_a_desktop_and_nv12() {
        assert_eq!(vpp_format(AR24), Some((1, "BGRA")));
        assert_eq!(vpp_format(AB24), Some((1, "RGBA")));
        assert_eq!(vpp_format(NV12), Some((2, "NV12")));
        // XRGB8888 is another order a compositor could draw a desktop in, and
        // P010 is a picture of a depth this build converts nothing from.
        for fourcc in [
            u32::from_le_bytes(*b"XR24"),
            u32::from_le_bytes(*b"P010"),
            0,
        ] {
            assert_eq!(vpp_format(fourcc), None, "{fourcc:#x}");
        }
    }

    /// The conversion is asked for the stream's size and NV12, and — only where
    /// the picture arrives as 32-bit colour — the BT.601 limited range that the
    /// processor's copy of the same picture was made with. The arguments are
    /// `scale_vaapi`'s own options, so the test reads them as the text the shim
    /// is handed.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn the_conversions_arguments_are_the_streams_size_and_nv12() {
        assert_eq!(
            vpp_filter(AR24, (1280, 720)),
            "w=1280:h=720:format=nv12:colour_matrix=smpte170m:range=limited"
        );
        assert_eq!(
            vpp_filter(AB24, (640, 360)),
            "w=640:h=360:format=nv12:colour_matrix=smpte170m:range=limited"
        );
        // NV12 is already luma and chroma: the processor scales it and leaves
        // its colour alone.
        assert_eq!(vpp_filter(NV12, (1280, 720)), "w=1280:h=720:format=nv12");
    }

    /// A screen's picture is one 32-bit plane of the compositor's buffer, which
    /// the encoder can't import where it lies and the video processor can take:
    /// the same descriptor that is refused by name is read as one plane of the
    /// screen's size for the conversion, which is what the processor scales to
    /// the stream's.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn a_screens_picture_is_the_video_processors_not_the_imports() {
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 2560,
        }];
        let fds = [7];
        let picture = dmabuf::Descriptor {
            width: 640,
            height: 360,
            fourcc: AR24,
            modifier: 0,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        // The import takes one NV12 picture of the encoder's size and nothing
        // else: this is a screen's size, in a format no surface is made of.
        assert_eq!(
            encoder.importable(&picture),
            Err(Refused::Unimportable(
                "a picture of another size than the encoder's"
            ))
        );
        // The conversion reads the same descriptor, and its size is the
        // picture's own: the processor is what scales it.
        assert_eq!(
            Encoder::described(&picture, AR24, 1),
            Ok(DmabufPicture {
                width: 640,
                height: 360,
                fd: 7,
                // The one plane's last row ends at 2560 × 360.
                size: 921_600,
                modifier: 0,
                fourcc: AR24,
                planes: 1,
                offset: [0, 0],
                pitch: [2560, 0],
            })
        );
        // It is still one plane in one buffer: NV12's two are not there, so an
        // import of it would show something else.
        assert_eq!(
            Encoder::described(&picture, NV12, 2),
            Err(Refused::Unimportable(
                "a picture that isn't NV12's two planes in one buffer"
            ))
        );
    }

    /// The encoder that reads the card's own pictures is opened with the same
    /// settings as the one fed from the processor — Main profile H264, a keyframe
    /// every three seconds, no B-frames, capped VBR with the cap equal to the aim
    /// over half a second, one picture in flight — because a stream's picture
    /// must not depend on which way its frames reached the encoder.
    ///
    /// The list is native/hwenc_dmabuf.c's own, so this reads what its open
    /// applies rather than a copy here, and needs no GPU in the shape of the
    /// question. A setting that changed without a measurement behind it is a
    /// failing test rather than a picture nobody compared.
    #[cfg(dmabufenc)]
    #[test]
    fn the_descriptor_encoders_settings_are_the_plane_paths() {
        let mut asked = [0u8; 192];
        // SAFETY: plain values, and a buffer of the size given.
        let found = unsafe {
            yapfast_hwenc_dmabuf_settings(30, 2500, asked.as_mut_ptr().cast(), asked.len())
        };
        assert_eq!(found, 0);
        assert_eq!(
            c_text(&asked),
            "profile=main(required) gop=90(required) b_frames=0(required) \
             rc_mode=VBR(required) async_depth=1(required) rate=2500 kbit/s capped at 2500 \
             kbit/s over 500 ms(required)"
        );
    }
}

/// A picture VA-API decoded is encoded where it lies by VA-API's encoder: the
/// NV12 DMA-BUF the decoder exported is wrapped as a surface on the same card,
/// encoded, and decodes again to the stream it came from. The wrap is the one a
/// capture's NV12 picture takes, and gives FFmpeg the counted buffer its map
/// references. Needs a VA-API driver that decodes and encodes H264, on the node
/// `YAPFAST_VAAPI_NODE` names; says so rather than failing where there is none.
#[cfg(all(test, dmabufenc, vaapidec))]
mod in_place {
    use super::*;
    use crate::decode::{Decoder, vaapi::Hardware, vaapi_export::units};
    use std::ffi::CStr;

    #[test]
    fn a_decoded_picture_is_encoded_where_it_lies() {
        let Ok(node) = std::env::var("YAPFAST_VAAPI_NODE") else {
            eprintln!("YAPFAST_VAAPI_NODE names no VA-API node to test on");
            return;
        };
        let driver = std::env::var("LIBVA_DRIVER_NAME").unwrap_or_default();
        let node_c = std::ffi::CString::new(node.as_str()).unwrap();
        let mut decoder = match Hardware::on(Codec::H264, &node_c) {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("no VA-API decoder on {node}: {error:#}");
                return;
            }
        };
        let units = units();
        let (name, driver_c) = (c"h264_vaapi", std::ffi::CString::new(driver).unwrap());
        let mut encoder: *mut Raw = std::ptr::null_mut();
        let mut reference = Decoder::with_threads(Codec::H264, 1).unwrap();
        let mut again = Decoder::with_threads(Codec::H264, 1).unwrap();
        let (mut encoded, mut compared) = (0, 0);
        for (index, unit) in units.iter().enumerate() {
            // The stream as the software decoder reads it, for the comparison.
            let want = reference
                .decode(unit)
                .unwrap()
                .map(|picture| (picture.width, picture.height, mean(picture.i420().y)));
            if !decoder.decode(unit, index == 0).expect("a VA-API decode") {
                continue;
            }
            let descriptor = decoder.dmabuf().expect("the picture as a descriptor");
            let picture = Encoder::described(&descriptor, NV12, 2).expect("an NV12 picture");
            if encoder.is_null() {
                let mut report = [0u8; 128];
                // SAFETY: the strings outlive the call; the report is its size.
                encoder = unsafe {
                    yapfast_hwenc_dmabuf_new(
                        0,
                        descriptor.width,
                        descriptor.height,
                        30,
                        2_000,
                        name.as_ptr(),
                        node_c.as_ptr(),
                        driver_c.as_ptr(),
                        report.as_mut_ptr().cast(),
                        report.len(),
                    )
                };
                if encoder.is_null() {
                    // SAFETY: a failed open leaves its reason in the shim.
                    let why = unsafe { CStr::from_ptr(yapfast_hwenc_dmabuf_error(encoder)) };
                    eprintln!(
                        "no VA-API H264 encoder on {node}: {}",
                        why.to_string_lossy()
                    );
                    return;
                }
            }
            let mut is_key = 0;
            // SAFETY: the encoder is open; the picture names the decoder's
            // surface, which it holds until `release`.
            let size = unsafe {
                yapfast_hwenc_dmabuf_encode(
                    encoder,
                    &picture,
                    index as c_longlong * 3000,
                    c_int::from(encoded == 0),
                    &mut is_key,
                )
            };
            decoder.release();
            assert!(
                size >= 0,
                "the picture wasn't encoded where it lies: {}",
                unsafe { CStr::from_ptr(yapfast_hwenc_dmabuf_error(encoder)).to_string_lossy() }
            );
            if size == 0 {
                continue;
            }
            let mut unit = vec![0u8; size as usize];
            // SAFETY: the unit the encode above produced, into a buffer its size.
            let drained =
                unsafe { yapfast_hwenc_dmabuf_drain(encoder, unit.as_mut_ptr(), unit.len()) };
            assert_eq!(drained, size as c_long);
            encoded += 1;
            if let (Some(picture), Some((width, height, luma))) =
                (again.decode(&unit).unwrap(), want)
            {
                assert_eq!((picture.width, picture.height), (width, height));
                let ours = mean(picture.i420().y);
                assert!(
                    (ours - luma).abs() < 6.0,
                    "unit {index}: the picture encoded in place averages {ours:.1}, the stream {luma:.1}"
                );
                compared += 1;
            }
        }
        if !encoder.is_null() {
            // SAFETY: opened above, freed once.
            unsafe { yapfast_hwenc_dmabuf_free(encoder) };
        }
        eprintln!("{encoded} pictures encoded where they lay, {compared} compared");
        assert!(compared > 0, "no picture came back to compare");
    }

    fn mean(plane: &[u8]) -> f64 {
        plane.iter().map(|&value| f64::from(value)).sum::<f64>() / plane.len().max(1) as f64
    }

    /// The camera's two encoders against each other at a call's settings —
    /// 1280×720 at 30 frames a second and 1.5 Mbit/s, `camera.rs` and
    /// `discord_video.rs` — on the same moving picture: the bits each spends,
    /// what the picture it decodes to is worth, and the processor time a frame
    /// costs it.
    ///
    /// The source's pictures and each engine's access units go to files under
    /// the build directory, `target/tmp/camera-encoders`, and FFmpeg decodes
    /// them there and measures them against the source with VMAF and SSIM: the
    /// quality figures are the ones a stream is judged by, not the test's own
    /// reading of the encoder's output. A run leaves its streams there, so they
    /// can be looked at afterwards.
    ///
    /// Ignored, and it prints rather than asserts: which encoder is better is
    /// what it is asked, and it needs the machine's own encoders and FFmpeg.
    /// An engine that can't open here is skipped with a line that says so —
    /// the GPU's only opens outside the dev shell, whose library path has no
    /// `libcuda.so.1`.
    ///
    /// `cargo test --profile measure -p yapfast-core -- --ignored --nocapture the_camera_s_encoders`
    #[test]
    #[ignore = "measurement: compares the camera's encoders on this machine"]
    fn the_camera_s_encoders() {
        use std::io::Write as _;

        const WIDTH: u32 = crate::camera::WIDTH;
        const HEIGHT: u32 = crate::camera::HEIGHT;
        /// A call's camera bitrate, as `discord_video` sets it.
        const KBPS: u32 = 1500;
        const FRAMES: u32 = 60;
        /// What decodes each stream and measures it against the source.
        const FFMPEG: &str = "/usr/bin/ffmpeg";

        /// A picture of a room: a gradient that drifts, blocks that move, and a
        /// little grain, so it is neither a still nor a whole new picture every
        /// frame.
        struct Scene {
            y: Vec<u8>,
            u: Vec<u8>,
            v: Vec<u8>,
        }

        impl Scene {
            fn at(frame: u32) -> Self {
                let (width, height) = (WIDTH as usize, HEIGHT as usize);
                let mut state = 0x2545_f491_4f6c_dd1du64 ^ u64::from(frame);
                let mut noise = || {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    ((state >> 24) & 0xff) as i32 - 127
                };
                let mut y = Vec::with_capacity(width * height);
                for row in 0..height {
                    for column in 0..width {
                        let drift = (frame * 6) as usize % width;
                        let base = 40 + (column + drift) % width * 150 / width;
                        let shadow = (row + height / 3) % height;
                        let block = ((column + drift) / 80).is_multiple_of(4) && row > height / 4;
                        let value = base as i32 + if block && shadow > height / 2 { 40 } else { 0 };
                        y.push((value + noise() / 8).clamp(16, 235) as u8);
                    }
                }
                let chroma = width.div_ceil(2) * height.div_ceil(2);
                let mut u = Vec::with_capacity(chroma);
                let mut v = Vec::with_capacity(chroma);
                for index in 0..chroma {
                    let shift = ((index / 64 + frame as usize / 4) % 24) as i32;
                    u.push((128 + shift + noise() / 16) as u8);
                    v.push((128 - shift + noise() / 16) as u8);
                }
                Self { y, u, v }
            }

            fn picture(&self) -> crate::picture::I420<'_> {
                crate::picture::I420 {
                    width: WIDTH,
                    height: HEIGHT,
                    y: &self.y,
                    u: &self.u,
                    v: &self.v,
                    strides: [WIDTH, WIDTH / 2, WIDTH / 2],
                }
            }
        }

        let scenes: Vec<Scene> = (0..FRAMES).map(Scene::at).collect();

        // Two files side by side, so the measurement is FFmpeg's on a stream a
        // player would read: the source's pictures as they were made, and the
        // access units an engine made of them. In the build directory, where
        // the raw pictures are kept for a look rather than thrown away; a
        // library's own test gets no CARGO_TARGET_TMPDIR, so the crate's path
        // reaches the same place.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp/camera-encoders");
        std::fs::create_dir_all(&dir).expect("a directory for the measurement");
        let source = dir.join("source.i420");
        {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(&source).expect("a file for the source"),
            );
            for scene in &scenes {
                for plane in [&scene.y, &scene.u, &scene.v] {
                    file.write_all(plane).expect("the source's pictures");
                }
            }
            file.flush().expect("the source's pictures");
        }
        eprintln!(
            "the source and each engine's stream are in {}",
            dir.display()
        );

        /// One engine's row of the table: its name, the bits it spent, what VMAF
        /// and SSIM made of its picture, and the processor time a frame cost it.
        type Row = (String, f64, Option<f64>, Option<f64>, f64);

        let mut rows: Vec<Row> = Vec::new();
        for hardware in [false, true] {
            let mut encoder = match Encoder::new(
                Codec::H264,
                WIDTH,
                HEIGHT,
                crate::camera::FPS,
                KBPS,
                Effort::Light,
                hardware,
            ) {
                Ok(encoder) => encoder,
                Err(why) => {
                    eprintln!("no H264 encoder here (hardware {hardware}): {why:#}");
                    continue;
                }
            };
            if hardware && !encoder.hardware() {
                eprintln!(
                    "no GPU encoder here: the open fell back to {}",
                    encoder.engine()
                );
                continue;
            }
            let engine = encoder.engine().to_owned();
            let configuration = encoder.configuration().clone();
            let stem: String = engine
                .chars()
                .map(|letter| {
                    if letter.is_ascii_alphanumeric() {
                        letter
                    } else {
                        '-'
                    }
                })
                .collect();
            let stream = dir.join(format!("{stem}.h264"));
            // The stream is held in memory while it is timed, so writing it
            // doesn't count as a frame's cost, and written out afterwards for
            // FFmpeg to read.
            let mut units = Vec::new();
            let (mut encoded, mut keyframes) = (0, 0);
            let start = std::time::Instant::now();
            for (index, scene) in scenes.iter().enumerate() {
                let picture = scene.picture();
                let pts = index as u64 * 1000 / u64::from(crate::camera::FPS);
                let Some(frame) = encoder
                    .encode_picture(&picture, pts, index == 0)
                    .unwrap_or_else(|why| panic!("{engine}: frame {index}: {why:#}"))
                else {
                    continue;
                };
                encoded += 1;
                keyframes += usize::from(frame.keyframe);
                units.extend_from_slice(&frame.data);
            }
            let elapsed = start.elapsed();
            std::fs::write(&stream, &units).expect("a file for the stream");
            if encoded == 0 {
                eprintln!("{engine}: no picture came out of it");
                continue;
            }
            let frames = f64::from(encoded);
            // Bits over the pictures actually taken, at the camera's rate.
            let kbit_s = units.len() as f64 * 8.0 / 1000.0 * f64::from(crate::camera::FPS) / frames;
            // What a viewer would make of the stream: VMAF for the eye, SSIM
            // for the picture, both measured by FFmpeg against the source.
            let measure =
                |filter: &str, mark: &str| match filter_score(&stream, &source, filter, mark) {
                    Ok(score) => Some(score),
                    Err(why) => {
                        eprintln!("{engine}: {filter} measured nothing: {why:#}");
                        None
                    }
                };
            let vmaf = measure("libvmaf", "VMAF score:");
            let ssim = measure("ssim", "All:");
            eprintln!(
                "{engine}: {encoded} pictures, {keyframes} keyframes, {} (dropped {}), written to {}",
                if configuration.tuning.is_empty() {
                    "no settings reported"
                } else {
                    configuration.tuning.as_str()
                },
                if configuration.settings_dropped.is_empty() {
                    "none"
                } else {
                    configuration.settings_dropped.as_str()
                },
                stream.display()
            );
            rows.push((
                engine,
                kbit_s,
                vmaf,
                ssim,
                elapsed.as_secs_f64() * 1000.0 / frames,
            ));
        }

        let shown = |value: Option<f64>, places: usize| match value {
            Some(value) => format!("{:.places$}", value),
            None => "-".to_owned(),
        };
        eprintln!();
        eprintln!(
            "{:<24} {:>9} {:>7} {:>7} {:>9}",
            "engine", "kbit/s", "VMAF", "SSIM", "ms/frame"
        );
        for (engine, kbit_s, vmaf, ssim, ms) in &rows {
            eprintln!(
                "{:<24} {:>9.0} {:>7} {:>7} {:>9.2}",
                engine,
                kbit_s,
                shown(*vmaf, 2),
                shown(*ssim, 4),
                ms
            );
        }
        if rows.is_empty() {
            eprintln!("no encoder opened here, so there is nothing to measure");
        }

        /// What FFmpeg's `filter` makes of `stream` against `source`, read from
        /// the line the filter prints. FFmpeg decodes the stream itself, so a
        /// picture that only looks cheap because it is broken shows here.
        fn filter_score(
            stream: &std::path::Path,
            source: &std::path::Path,
            filter: &str,
            mark: &str,
        ) -> Result<f64> {
            let arguments: Vec<String> = vec![
                "-hide_banner".to_owned(),
                "-nostats".to_owned(),
                "-i".to_owned(),
                stream.display().to_string(),
                "-f".to_owned(),
                "rawvideo".to_owned(),
                "-pix_fmt".to_owned(),
                "yuv420p".to_owned(),
                "-s".to_owned(),
                format!("{}x{}", crate::camera::WIDTH, crate::camera::HEIGHT),
                "-framerate".to_owned(),
                crate::camera::FPS.to_string(),
                "-i".to_owned(),
                source.display().to_string(),
                "-lavfi".to_owned(),
                filter.to_owned(),
                "-f".to_owned(),
                "null".to_owned(),
                "-".to_owned(),
            ];
            let ran = match std::process::Command::new(FFMPEG).args(&arguments).output() {
                Ok(ran) => ran,
                Err(why) => bail!("{FFMPEG} couldn't run: {why}"),
            };
            let printed = String::from_utf8_lossy(&ran.stderr);
            let Some((_, after)) = printed.rsplit_once(mark) else {
                let tail: Vec<&str> = printed.lines().rev().take(3).collect();
                bail!("{filter} printed no \"{mark}\": {tail:?}");
            };
            let number: String = after
                .trim_start()
                .chars()
                .take_while(|letter| letter.is_ascii_digit() || *letter == '.' || *letter == '-')
                .collect();
            match number.parse() {
                Ok(found) => Ok(found),
                Err(_) => bail!("{filter} printed \"{after:.40}\" rather than a number"),
            }
        }
    }
}

mod backends;
mod dmabuf;
mod encoder;
mod settings;

pub(crate) use backends::*;
pub(crate) use settings::*;

#[cfg(target_os = "linux")]
pub use backends::Refused;
#[cfg(all(target_os = "linux", hwenc))]
pub use backends::nvidia_card;
#[cfg(target_os = "windows")]
pub use backends::planned;
pub use encoder::CLOCK;
pub use encoder::Encoder;
pub use encoder::Frame;
pub use encoder::timestamp;
pub use settings::Configuration;
pub use settings::Effort;
pub use settings::hardware;
pub use settings::set_hardware;
