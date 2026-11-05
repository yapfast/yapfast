//! Encoding on the GPU with no pixel copy: native/d3d11enc.c opens NVENC on a
//! D3D11 device and reads the caller's texture there, so a captured frame never
//! becomes an I420 picture in memory on its way to the encoder.
//!
//! native/hwenc.c is the other way to a GPU's encoder, and the one a stream
//! uses today: it takes I420, which costs the processor a conversion and an
//! upload of every frame. This is the same encoders reading the frame where it
//! already is, and it needs two things of its caller that hwenc.c doesn't: the
//! texture has to live on this encoder's device ([`D3d11Encoder::device`]), and
//! it has to be a concrete format — BGRA, RGBA or NV12 — since the driver reads
//! the surface's own format and refuses a typeless one.
//!
//! The one copy is the bitstream: the driver's mapped output is moved into the
//! caller's buffer once per frame, so a packet can outlive the encoder's lock.
//! Nothing else of a frame is touched by the processor, and
//! [`D3d11Encoder::copied`] answers zero while that holds. The API mirrors
//! [`Encoder`](crate::encode::Encoder)'s where it can — an engine and its
//! settings, a bitrate that moves mid-stream, a frame with its time — so a
//! caller that has textures can hold this instead of that, and the stream's
//! bytes and timing are the same either way.
//!
//! Wiring this into a stream needs a capture source that fills a texture on
//! [`D3d11Encoder::device`] instead of handing over I420: [`D3d11Encoder`]
//! itself changes not at all. It is not wired in yet, because the capture path
//! doesn't produce textures and `stream_worker` was being changed elsewhere.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulonglong, c_void},
    sync::Arc,
};

use anyhow::{Result, bail};
use yapfast_discord::{rtp::Codec, stream_connection::Pool};

use crate::encode::{Configuration, Frame};

/// An encoder NVENC owns, and a source of frames for the tests.
#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

#[repr(C)]
struct RawSource {
    _private: [u8; 0],
}

// SAFETY: the shim's own declarations; every pointer passed is one it returned,
// and the buffers are as large as their sizes say.
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        tuning: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_d3d11enc_encode(
        encoder: *mut Raw,
        texture: *mut c_void,
        subresource: c_uint,
        pts: c_longlong,
        keyframe: c_int,
        out: *mut c_uchar,
        capacity: usize,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_d3d11enc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_d3d11enc_asked(encoder: *mut Raw, asked: *mut c_char, asked_size: usize);
    fn yapfast_d3d11enc_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_d3d11enc_tuning(encoder: *mut Raw, tuning: *mut c_char, tuning_size: usize);
    fn yapfast_d3d11enc_codecs(encoder: *mut Raw, codecs: *mut c_char, codecs_size: usize);
    fn yapfast_d3d11enc_error(encoder: *mut Raw, error: *mut c_char, error_size: usize);
    fn yapfast_d3d11enc_device(encoder: *mut Raw) -> *mut c_void;
    fn yapfast_d3d11enc_copied(encoder: *mut Raw) -> c_ulonglong;
    fn yapfast_d3d11enc_free(encoder: *mut Raw);
    fn yapfast_d3d11enc_source_new(
        encoder: *mut Raw,
        frames: c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut RawSource;
    fn yapfast_d3d11enc_source_draw(source: *mut RawSource, frame: c_uint);
    fn yapfast_d3d11enc_source_texture(source: *mut RawSource) -> *mut c_void;
    fn yapfast_d3d11enc_source_free(source: *mut RawSource);
}

/// A texture the encoder reads, and which of its subresources: the surface a
/// capture source has just filled, which stays the caller's. Its handle is an
/// `ID3D11Texture2D` on [`D3d11Encoder::device`]'s device, and the shim checks
/// that on the first frame it reads it, since a texture from another device
/// can't be encoded by this session.
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    /// The `ID3D11Texture2D` itself.
    pub handle: *mut c_void,
    /// Which subresource of it, for a texture array; 0 for an ordinary one.
    pub subresource: u32,
}

impl Texture {
    /// The whole of an ordinary texture.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            subresource: 0,
        }
    }
}

/// An encoder on the GPU reading D3D11 textures: NVENC through a D3D11 device,
/// one codec to an instance, opened with the tuning the stream ships.
pub struct D3d11Encoder {
    raw: *mut Raw,
    codec: Codec,
    width: u32,
    height: u32,
    kbps: u32,
    /// Which codecs the driver offered this session, as it named them, so a
    /// stream says what its card refused rather than quietly offering less.
    codecs: String,
    configuration: Configuration,
    /// The buffers encoded frames are written into, reused as frames are sent.
    out: Arc<Pool>,
}

// SAFETY: no library keeps thread-local state past a call; one thread uses the
// encoder at a time, as the D3D11 device requires of its immediate context.
unsafe impl Send for D3d11Encoder {}

impl D3d11Encoder {
    /// An encoder for `width` by `height` frames, which must be even, at `fps`
    /// and `kbps`, for one codec: the compositor's size, and the codec Discord
    /// chose. H264, H265 and AV1 all encode here, where the card has them; AV1
    /// needs an RTX 40 series or newer, and a driver without one is refused by
    /// name rather than by status number.
    pub fn new(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let number = match codec {
            Codec::H264 => 0,
            Codec::H265 => 1,
            Codec::Av1 => 2,
            Codec::Vp8 => bail!("VP8 doesn't encode on the GPU; libvpx does it on the processor"),
        };
        let mut report = [0u8; 192];
        // SAFETY: plain values, and a buffer of the size given, which the shim
        // ends with a zero. No tuning named asks for the shipped one, which is
        // what a stream that says nothing about its quality is given.
        let raw = unsafe {
            yapfast_d3d11enc_new(
                number,
                width,
                height,
                fps,
                kbps,
                std::ptr::null(),
                report.as_mut_ptr().cast(),
                report.len(),
            )
        };
        if raw.is_null() {
            let report = c_text(&report);
            bail!("no NVENC encoder opened: {report}");
        }
        // SAFETY: raw is an open encoder, and each buffer is as large as its
        // size; every one of these ends what it writes with a zero.
        let (asked, tuning, config, codecs) = unsafe {
            let mut asked = [0u8; 32];
            let mut tuning = [0u8; 96];
            let mut config = [0u8; 128];
            let mut codecs = [0u8; 64];
            yapfast_d3d11enc_asked(raw, asked.as_mut_ptr().cast(), asked.len());
            yapfast_d3d11enc_tuning(raw, tuning.as_mut_ptr().cast(), tuning.len());
            yapfast_d3d11enc_config(raw, config.as_mut_ptr().cast(), config.len());
            yapfast_d3d11enc_codecs(raw, codecs.as_mut_ptr().cast(), codecs.len());
            (
                c_text(&asked),
                c_text(&tuning),
                c_text(&config),
                c_text(&codecs),
            )
        };
        // What it runs on, what it took, and the settings a change to the
        // picture's quality moves, said once at the open so a stream is
        // compared with the encoder that made it rather than with what it was
        // meant to be.
        let engine = c_text(&report);
        tracing::debug!(
            engine = engine.as_str(),
            ?codec,
            tuning = tuning.as_str(),
            config = config.as_str(),
            codecs = codecs.as_str(),
            "encoding on the GPU from D3D11 textures"
        );
        let pixels = (width * height) as usize;
        Ok(Self {
            raw,
            codec,
            width,
            height,
            kbps,
            codecs,
            configuration: Configuration {
                engine,
                asked,
                tuning,
                // Every setting an NVENC session is opened with here is one the
                // stream needs, so a driver that won't take one fails the open
                // instead of dropping it: there is nothing to warn about later.
                settings_dropped: String::new(),
            },
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        })
    }

    /// Which encoder runs, such as "h264_nvenc/d3d11": the codec's FFmpeg name
    /// and the backend that reads its frames, so a log says which of the two GPU
    /// paths a stream got.
    pub fn engine(&self) -> &str {
        &self.configuration.engine
    }

    /// What this encoder was opened with, for a log or a measurement to quote
    /// beside its numbers.
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    /// The codecs this session's driver offered, such as "h264 hevc av1", and
    /// empty when it wouldn't say: what the stream can offer its viewers, read
    /// from the card that would encode them.
    pub fn codecs(&self) -> &str {
        &self.codecs
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

    /// The D3D11 device the textures have to live on, as an `ID3D11Device`. A
    /// capture source creates its surfaces on it — desktop duplication's own
    /// device has to be this one — or opens its own device and hands this the
    /// textures from it, which the shim refuses on the first frame rather than
    /// encoding the wrong memory.
    pub fn device(&self) -> *mut c_void {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_device(self.raw) }
    }

    /// How many frames' pixels the processor has moved since this encoder
    /// opened: zero, for as long as the picture goes to the encoder on the GPU.
    /// A capture source that started converting frames would say so here without
    /// any frame looking different.
    pub fn copied(&self) -> u64 {
        // SAFETY: raw is an open encoder.
        unsafe { yapfast_d3d11enc_copied(self.raw) as u64 }
    }

    /// Aims for `kbps` from the next frame on, without a keyframe: the session is
    /// reconfigured, so the frames a viewer is watching stay references and the
    /// picture doesn't blink.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw is an open encoder.
        if unsafe { yapfast_d3d11enc_set_bitrate(self.raw, kbps) } != 0 {
            bail!(
                "the {:?} encoder refused {kbps} kbit/s: {}",
                self.codec,
                self.error()
            );
        }
        self.kbps = kbps;
        Ok(())
    }

    /// Encodes `texture`'s picture, shown at `pts` in [`CLOCK`](
    /// crate::encode::CLOCK) units. None when the encoder made no packet. A frame
    /// too big for the buffer grows the pool's buffers, doubling until one holds
    /// it, and is encoded again at the same time.
    pub fn encode_texture(
        &mut self,
        texture: Texture,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let mut out = self.out.checkout();
        loop {
            let mut is_key: c_int = 0;
            let spare = out.spare();
            // SAFETY: the texture is the caller's and lives as long as the call,
            // the buffer is as large as its size, and the flag has a place of its
            // own; all stay put during the call.
            let written = unsafe {
                yapfast_d3d11enc_encode(
                    self.raw,
                    texture.handle,
                    texture.subresource,
                    pts as c_longlong,
                    c_int::from(keyframe),
                    spare.as_mut_ptr(),
                    spare.len(),
                    &mut is_key,
                )
            };
            match written {
                // -2 is the shim's "won't fit", as native/hwenc.c says it.
                -2 => {
                    if !self.out.grow() {
                        bail!(
                            "the {:?} frame doesn't fit {} bytes",
                            self.codec,
                            self.out.capacity()
                        );
                    }
                    out = self.out.checkout();
                }
                ..0 => bail!("{:?} encoding failed: {}", self.codec, self.error()),
                0 => return Ok(None),
                size => {
                    out.finish(size as usize);
                    return Ok(Some(Frame {
                        data: out,
                        keyframe: is_key != 0,
                        // The frame was encoded at this time, whether it took one
                        // call or, grown, two.
                        timestamp: pts as u32,
                    }));
                }
            }
        }
    }

    /// Draws frames for this encoder to read — a stand-in for the capture
    /// source, which doesn't make textures yet. `frames` is how many pictures it
    /// can draw before repeating one, and it holds a texture on the encoder's
    /// device for as long as it lives.
    pub fn source(&self, frames: u32) -> Result<Source> {
        let mut why = [0u8; 192];
        // SAFETY: raw is an open encoder, and a buffer of the size given, which
        // the shim ends with a zero.
        let raw = unsafe {
            yapfast_d3d11enc_source_new(self.raw, frames, why.as_mut_ptr().cast(), why.len())
        };
        if raw.is_null() {
            bail!("no texture to draw on: {}", c_text(&why));
        }
        Ok(Source { raw })
    }

    /// What the driver said about the frame it wouldn't encode.
    fn error(&self) -> String {
        let mut error = [0u8; 192];
        // SAFETY: raw is an open encoder, and a buffer of the size given.
        unsafe { yapfast_d3d11enc_error(self.raw, error.as_mut_ptr().cast(), error.len()) };
        c_text(&error)
    }
}

impl Drop for D3d11Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_d3d11enc_free(self.raw) };
    }
}

/// The frames a [`D3d11Encoder`] reads before a capture source fills its
/// textures: one BGRA texture on the encoder's device, which a compute shader
/// refills there, so nothing of a frame crosses the processor and a measurement
/// of the encode is the encode's own cost.
///
/// It holds a texture on its encoder's device, so it goes before the encoder it
/// draws for: a texture whose device has been released can't be read.
pub struct Source {
    raw: *mut RawSource,
}

impl Source {
    /// Refills the texture on the GPU with the picture `frame` shows.
    pub fn draw(&self, frame: u32) {
        // SAFETY: raw is a source that hasn't been freed, and the frame index is
        // plain data.
        unsafe { yapfast_d3d11enc_source_draw(self.raw, frame) };
    }

    /// The texture to encode next, which the source keeps and refills.
    pub fn texture(&self) -> Texture {
        // SAFETY: raw is a source that hasn't been freed, and its texture lives
        // as long as it does.
        Texture::new(unsafe { yapfast_d3d11enc_source_texture(self.raw) })
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_d3d11enc_source_free(self.raw) };
    }
}

/// The text the shim wrote into `bytes`, which ends with a zero.
fn c_text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the shim measures for a test that asks it, and nothing a stream
    // runs: the devices this process has opened, so a second frame can be held
    // to the first's session, and the processor time an encode costs, which a
    // wall clock would charge the driver's wait on the GPU to.
    unsafe extern "C" {
        fn yapfast_d3d11enc_devices_created() -> c_uint;
        fn yapfast_d3d11enc_cpu_time() -> c_ulonglong;
    }

    // The settings native/d3d11enc.c opens NVENC with for the tuning named, and
    // whether the stream can't do without each: the list that open reads, so a
    // test holds it to what native/hwenc.c's own list says rather than to a copy
    // of either.
    #[cfg(hwenc)]
    unsafe extern "C" {
        fn yapfast_d3d11enc_tuning_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const c_char,
            asked: *mut c_char,
            asked_size: usize,
        ) -> c_int;
    }

    /// The two GPU backends ask NVENC for the same thing: p4 at low-latency
    /// tuning, capped VBR, no frame allowed to wait and an IDR on demand, with the
    /// same settings marked as the ones the stream can't do without. A tuning that
    /// changed on one side and not the other would encode a picture nobody
    /// measured, and neither a stream nor a comparison would say which it got.
    #[cfg(hwenc)]
    #[test]
    fn both_gpu_backends_ask_nvenc_for_the_same_thing() {
        let mut d3d11 = [0u8; 192];
        let mut ffmpeg = [0u8; 192];
        // SAFETY: a name the row has, or none at all for the shipped one, and a
        // buffer of the size given on each side.
        let (ours, theirs) = unsafe {
            (
                yapfast_d3d11enc_tuning_settings(
                    std::ptr::null(),
                    d3d11.as_mut_ptr().cast(),
                    d3d11.len(),
                ),
                yapfast_hwenc_nvenc_settings(
                    std::ptr::null(),
                    ffmpeg.as_mut_ptr().cast(),
                    ffmpeg.len(),
                ),
            )
        };
        assert_eq!((ours, theirs), (0, 0));
        assert_eq!(
            c_text(&d3d11),
            c_text(&ffmpeg),
            "the two backends' settings"
        );
        // A tuning nobody has is refused rather than quietly replaced by this
        // one, on both sides of the comparison.
        let mut none = [0u8; 192];
        // SAFETY: as above, with a name no row has.
        let refused = unsafe {
            yapfast_d3d11enc_tuning_settings(c"p7".as_ptr(), none.as_mut_ptr().cast(), none.len())
        };
        assert_eq!(refused, -1);
    }

    /// Thirty frames of each codec NVENC has here, read from a texture the GPU
    /// fills: every frame is an access unit, the first is a keyframe and the rest
    /// follow it, and the session encodes all thirty — one device, one session, no
    /// pixel of any frame touched by the processor. The first frames are read as
    /// the codec's own structure, since that is what the packetizer and every
    /// viewer's client depend on and the one thing a wrong driver call would break
    /// without looking broken.
    #[test]
    #[ignore = "needs an NVIDIA GPU: it opens an NVENC session on one"]
    fn thirty_frames_of_each_codec_encode_from_a_texture() {
        // 1080p at 60, the shape a stream of a game is, so the numbers this prints
        // are the ones a stream would see.
        let (width, height, fps, kbps) = (1920u32, 1080u32, 60u32, 8000u32);
        let before = unsafe { yapfast_d3d11enc_devices_created() };
        let mut opened = 0;
        for codec in [Codec::H264, Codec::H265, Codec::Av1] {
            let mut encoder = match D3d11Encoder::new(codec, width, height, fps, kbps) {
                Ok(encoder) => encoder,
                Err(error) => {
                    // A card without the codec is not a failure of this backend:
                    // AV1 needs an RTX 40 series or newer, and the open says which
                    // codecs the driver has.
                    eprintln!("{codec:?} didn't open here: {error:#}");
                    continue;
                }
            };
            opened += 1;
            eprintln!(
                "{codec:?}: {} ({}), {} fps, driver has {}",
                encoder.engine(),
                encoder.configuration().tuning,
                fps,
                encoder.codecs()
            );
            let device = encoder.device();
            assert!(!device.is_null());
            let source = encoder.source(60).unwrap();
            let texture = source.texture();
            assert!(!texture.handle.is_null());

            // The frames' processor time alone: the driver waits on the GPU inside
            // this call, and a wall clock would charge that to the processor.
            let mut frames = vec![];
            for frame in 0..30u32 {
                source.draw(frame);
                // Half a millisecond a frame at 60 fps, in 90 kHz units: one
                // stream's clock, the same one every other engine runs on.
                let pts = u64::from(frame) * 1500;
                if frame == 15 {
                    // A bitrate that moves mid-stream, without a keyframe: the
                    // network changed and the viewers keep their references.
                    encoder.set_bitrate(kbps / 2).unwrap();
                    assert_eq!(encoder.bitrate(), kbps / 2);
                }
                let encoded = encoder
                    .encode_texture(texture, pts, frame == 0)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is an empty access unit"
                );
                assert_eq!(encoded.timestamp, pts as u32);
                if frame == 0 {
                    assert!(
                        encoded.keyframe,
                        "the first frame of {codec:?} isn't a keyframe"
                    );
                    // Its structure, as the packetizer reads it: parameter sets
                    // before the picture, in the form this codec's packets take.
                    match codec {
                        Codec::H264 => assert_eq!(
                            yapfast_discord::rtp::nal_units(&encoded.data)[0][0] & 0x1f,
                            7,
                            "an H.264 keyframe starts with its sequence parameter set"
                        ),
                        Codec::H265 => assert_eq!(
                            (yapfast_discord::rtp::nal_units(&encoded.data)[0][0] >> 1) & 0x3f,
                            32,
                            "an H.265 keyframe starts with its video parameter set"
                        ),
                        _ => assert_eq!(
                            (yapfast_discord::rtp::obu_elements(&encoded.data)
                                .expect("OBUs within the frame")[0][0]
                                >> 3)
                                & 0x0f,
                            1,
                            "an AV1 keyframe starts with its sequence header"
                        ),
                    }
                }
                frames.push(encoded);
            }
            let bytes: usize = frames.iter().map(|frame| frame.data.len()).sum();
            // Only the frame it was asked for is a keyframe: the interval is three
            // seconds, and these are half a second.
            assert_eq!(
                frames.iter().filter(|frame| frame.keyframe).count(),
                1,
                "a keyframe the stream wasn't asked for"
            );
            // What a frame costs the processor, over a run long enough to read:
            // the process's own counter moves in ticks of about 15 ms, so thirty
            // frames that take a quarter of one can only be timed by the score.
            // Three hundred of them are two and a half seconds of a stream, and
            // the same run the reference measurement used, so the number below is
            // comparable with it.
            const TIMED: u32 = 300;
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() };
            for frame in 30..30 + TIMED {
                source.draw(frame);
                let encoded = encoder
                    .encode_texture(texture, u64::from(frame) * 1500, false)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{codec:?} made no packet for frame {frame}"));
                assert!(
                    !encoded.data.is_empty(),
                    "frame {frame} of {codec:?} is empty"
                );
            }
            let cpu = unsafe { yapfast_d3d11enc_cpu_time() }.saturating_sub(cpu);
            eprintln!(
                "  {bytes} bytes in the first {}, {:.4} ms of the processor per frame",
                frames.len(),
                cpu as f64 / f64::from(TIMED) / 10_000.0
            );
            // The pixels went to the encoder on the GPU: a converted frame would
            // say so here without any frame looking wrong.
            assert_eq!(encoder.copied(), 0, "the processor moved a frame's pixels");
            // And the thirty frames took the session that was opened, not another
            // device or another encoder.
            assert_eq!(encoder.device(), device);
            assert_eq!(
                unsafe { yapfast_d3d11enc_devices_created() },
                before + opened,
                "an encode opened another D3D11 device"
            );
        }
        assert_eq!(opened, 3, "not every codec opened on this GPU");
    }
}
