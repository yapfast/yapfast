//! Opus, from the libopus inside Yapfast's libwebrtc archive, which every Yapfast
//! build links already. A second libopus, such as `opus2` builds, would put
//! the same symbols in the binary twice.

use std::{ffi::c_int, ptr::NonNull};

use anyhow::{Result, bail};
// The libopus these call is in libwebrtc, which this crate links.
use webrtc_sys as _;

#[repr(C)]
struct RawEncoder {
    _private: [u8; 0],
}

#[repr(C)]
struct RawDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn opus_encoder_create(
        rate: i32,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut RawEncoder;
    fn opus_encoder_destroy(encoder: *mut RawEncoder);
    fn opus_encode_float(
        encoder: *mut RawEncoder,
        pcm: *const f32,
        frame_size: c_int,
        data: *mut u8,
        max_data_bytes: i32,
    ) -> i32;
    fn opus_encoder_ctl(encoder: *mut RawEncoder, request: c_int, ...) -> c_int;
    fn opus_decoder_create(rate: i32, channels: c_int, error: *mut c_int) -> *mut RawDecoder;
    fn opus_decoder_destroy(decoder: *mut RawDecoder);
    fn opus_decode(
        decoder: *mut RawDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
}

const RATE: i32 = 48_000;
const APPLICATION_VOIP: c_int = 2048;
const APPLICATION_AUDIO: c_int = 2049;
const SET_BITRATE: c_int = 4002;
const SET_INBAND_FEC: c_int = 4012;
const SET_PACKET_LOSS_PERC: c_int = 4014;

/// What an encoder is tuned for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Application {
    /// Speech, as a microphone sends.
    Voice,
    /// Anything else, such as a shared screen's sound.
    Audio,
}

/// An Opus encoder at 48 kHz.
pub struct Encoder {
    raw: NonNull<RawEncoder>,
    channels: usize,
}

// SAFETY: an encoder is used from one thread at a time, through `&mut`.
unsafe impl Send for Encoder {}

/// Sets one int-valued encoder control. libopus answers `OPUS_OK` or a
/// negative code, so a refused setting isn't left looking as if it took.
fn set(encoder: &Encoder, request: c_int, value: c_int, setting: &str) -> Result<()> {
    // SAFETY: these requests take one int, as libopus's macros pass it, and
    // `encoder` owns the pointer it was created with.
    let outcome = unsafe { opus_encoder_ctl(encoder.raw.as_ptr(), request, value) };
    if outcome < 0 {
        bail!("Opus couldn't set {setting} ({outcome})");
    }
    Ok(())
}

impl Encoder {
    pub fn new(channels: u16, application: Application, bitrate: i32) -> Result<Self> {
        let mut error = 0;
        let application = match application {
            Application::Voice => APPLICATION_VOIP,
            Application::Audio => APPLICATION_AUDIO,
        };
        // SAFETY: valid arguments; `error` is written before returning.
        let raw =
            unsafe { opus_encoder_create(RATE, c_int::from(channels), application, &mut error) };
        let Some(raw) = NonNull::new(raw).filter(|_| error == 0) else {
            bail!("could not start the Opus encoder ({error})");
        };
        let encoder = Self {
            raw,
            channels: usize::from(channels),
        };
        // A control libopus refuses leaves the encoder at defaults that don't
        // match what the caller asked for, so report it; returning early drops
        // `encoder`, which frees the encoder just created.
        set(&encoder, SET_BITRATE, bitrate, "the bitrate")?;
        if application == APPLICATION_VOIP {
            // Forward error correction lets a receiver rebuild a lost packet
            // from the next one; it costs bits only when loss is expected.
            set(&encoder, SET_INBAND_FEC, 1, "inband FEC")?;
            set(&encoder, SET_PACKET_LOSS_PERC, 5, "the packet loss")?;
        }
        Ok(encoder)
    }

    /// Encodes one frame of interleaved samples into `out`, returning the
    /// packet's length.
    pub fn encode_float(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize> {
        let frame = pcm.len() / self.channels;
        // SAFETY: `pcm` holds `frame` samples per channel and `out` its length.
        let length = unsafe {
            opus_encode_float(
                self.raw.as_ptr(),
                pcm.as_ptr(),
                frame as c_int,
                out.as_mut_ptr(),
                out.len().min(i32::MAX as usize) as i32,
            )
        };
        if length < 0 {
            bail!("Opus couldn't encode a frame ({length})");
        }
        Ok(length as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: made by opus_encoder_create, destroyed once.
        unsafe { opus_encoder_destroy(self.raw.as_ptr()) };
    }
}

/// An Opus decoder at 48 kHz.
pub struct Decoder {
    raw: NonNull<RawDecoder>,
    channels: usize,
}

// SAFETY: as for the encoder.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(channels: u16) -> Result<Self> {
        let mut error = 0;
        // SAFETY: valid arguments; `error` is written before returning.
        let raw = unsafe { opus_decoder_create(RATE, c_int::from(channels), &mut error) };
        let Some(raw) = NonNull::new(raw).filter(|_| error == 0) else {
            bail!("could not start the Opus decoder ({error})");
        };
        Ok(Self {
            raw,
            channels: usize::from(channels),
        })
    }

    /// Decodes a packet into interleaved samples, returning the samples per
    /// channel. `fec` reads the forward error correction a packet carries
    /// for the one before it.
    pub fn decode(&mut self, packet: &[u8], pcm: &mut [i16], fec: bool) -> Result<usize> {
        self.run(Some(packet), pcm, fec)
    }

    /// Conceals a lost packet, filling all of `pcm`: a frame's worth of
    /// sound that carries on from the last.
    pub fn conceal(&mut self, pcm: &mut [i16]) -> Result<usize> {
        self.run(None, pcm, false)
    }

    fn run(&mut self, packet: Option<&[u8]>, pcm: &mut [i16], fec: bool) -> Result<usize> {
        let (data, length) = packet.map_or((std::ptr::null(), 0), |packet| {
            (packet.as_ptr(), packet.len() as i32)
        });
        // SAFETY: `pcm` holds this many samples per channel; a null packet
        // asks for concealment.
        let samples = unsafe {
            opus_decode(
                self.raw.as_ptr(),
                data,
                length,
                pcm.as_mut_ptr(),
                (pcm.len() / self.channels) as c_int,
                c_int::from(fec),
            )
        };
        if samples < 0 {
            bail!("Opus couldn't decode a packet ({samples})");
        }
        Ok(samples as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: made by opus_decoder_create, destroyed once.
        unsafe { opus_decoder_destroy(self.raw.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// libopus's own special bitrates: `OPUS_AUTO`, and the highest rate the
    /// encoder can spend.
    const OPUS_AUTO: i32 = -1000;
    const OPUS_BITRATE_MAX: i32 = -1;

    /// A quiet 20 ms stereo frame, for checks that only need a real packet.
    fn tone() -> Vec<f32> {
        (0..960)
            .flat_map(|at| {
                let sample = (at as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [sample, sample]
            })
            .collect()
    }

    #[test]
    fn every_application_takes_every_legal_kind_of_bitrate() {
        // Only libopus decides which bitrates its rate control takes, from
        // the encoder's own configuration, so the wrapper must not vet them:
        // its sentinels and the rates it accepts are legal here.
        for (application, name) in [(Application::Voice, "voice"), (Application::Audio, "audio")] {
            for bitrate in [OPUS_AUTO, OPUS_BITRATE_MAX, 500, 32_000, 64_000, 128_000] {
                if let Err(error) = Encoder::new(2, application, bitrate) {
                    panic!("{name} at {bitrate} bit/s: {error}");
                }
            }
        }
    }

    #[test]
    fn every_application_still_encodes_after_configuring() {
        // Voice sets FEC and a loss estimate; Audio sets neither. Both must
        // come out of `new` working, with those controls accepted.
        let mut packet = [0u8; 1500];
        for (application, name) in [(Application::Voice, "voice"), (Application::Audio, "audio")] {
            let mut encoder = Encoder::new(2, application, 64_000)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let length = encoder
                .encode_float(&tone(), &mut packet)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(length > 3, "{name}: a real packet, not {length} bytes");
        }
    }

    #[test]
    fn a_bitrate_libopus_refuses_names_the_setting() {
        // The bitrate control runs after the encoder exists, so this also
        // covers the post-creation failure path: the `?` returns, and the
        // `Encoder` built above it frees the native encoder on the way out.
        let error = match Encoder::new(2, Application::Audio, -1_500) {
            Ok(_) => panic!("libopus took a bitrate it has no such value for"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("bitrate"), "{message}");
        assert!(message.contains("Opus"), "{message}");
    }

    #[test]
    fn a_refused_control_names_itself_and_spares_the_encoder() {
        // libopus takes a loss estimate of 0 to 100. A refusal is reported
        // with the setting that failed, and the encoder is still usable: the
        // helper borrows it rather than consuming or freeing it.
        let mut encoder = Encoder::new(1, Application::Audio, 64_000).unwrap();
        let error = set(&encoder, SET_PACKET_LOSS_PERC, 101, "the packet loss")
            .expect_err("libopus refuses a loss estimate over 100");
        assert!(error.to_string().contains("packet loss"), "{error}");
        let mut packet = [0u8; 1500];
        let mono: Vec<f32> = tone().chunks_exact(2).map(|pair| pair[0]).collect();
        assert!(encoder.encode_float(&mono, &mut packet).unwrap() > 3);
    }

    #[test]
    fn a_tone_survives_encoding_and_loss() {
        let mut encoder = Encoder::new(1, Application::Voice, 64_000).unwrap();
        let mut decoder = Decoder::new(2).unwrap();
        let mut packet = [0u8; 1500];
        let mut pcm = [0i16; 960 * 2];
        let mut loudest = 0;
        for frame in 0..10 {
            let tone: Vec<f32> = (0..960)
                .map(|at| {
                    ((frame * 960 + at) as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin()
                        * 0.5
                })
                .collect();
            let length = encoder.encode_float(&tone, &mut packet).unwrap();
            assert!(length > 3, "a real packet");
            assert_eq!(
                decoder.decode(&packet[..length], &mut pcm, false).unwrap(),
                960
            );
            loudest = loudest.max(
                pcm.iter()
                    .map(|sample| sample.unsigned_abs())
                    .max()
                    .unwrap(),
            );
        }
        assert!(loudest > 8_000, "the tone came through: {loudest}");
        assert_eq!(
            decoder.conceal(&mut pcm).unwrap(),
            960,
            "a lost frame fills in"
        );
    }
}
