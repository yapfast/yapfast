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
        // SAFETY: requests that take one int, as libopus's macros pass it.
        unsafe { opus_encoder_ctl(encoder.raw.as_ptr(), SET_BITRATE, bitrate as c_int) };
        if application == APPLICATION_VOIP {
            // Forward error correction lets a receiver rebuild a lost packet
            // from the next one; it costs bits only when loss is expected.
            // SAFETY: requests that take one int, as libopus's macros pass it.
            unsafe {
                opus_encoder_ctl(encoder.raw.as_ptr(), SET_INBAND_FEC, 1 as c_int);
                opus_encoder_ctl(encoder.raw.as_ptr(), SET_PACKET_LOSS_PERC, 5 as c_int);
            }
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
