//! A microphone's rate to the rate a caller asked for. The HAL unit refuses
//! to convert rates on its input side (its render fails with
//! `kAudioUnitErr_CannotDoInCurrentContext`), so the unit delivers the
//! device's own rate as 32-bit floats and this converts, on the reader's
//! thread, which may allocate.

use std::collections::VecDeque;

use anyhow::{Context as _, Result};
use rubato::{FftFixedIn, Resampler};

use super::super::Format;

/// How much input one conversion takes, in hundredths of a second: 10 ms.
const CHUNKS_PER_SECOND: u32 = 100;

/// One 16-bit step as a float, for turning samples into floats and back.
const I16_SCALE: f32 = 32_768.0;

/// Converts interleaved floats from one rate to another, a chunk at a time.
pub struct Converter {
    resampler: FftFixedIn<f32>,
    channels: usize,
    planar: Vec<Vec<f32>>,
}

impl Converter {
    pub fn new(from: u32, to: u32, channels: u16) -> Result<Self> {
        let channels = usize::from(channels);
        let chunk = (from / CHUNKS_PER_SECOND).max(1) as usize;
        let resampler = FftFixedIn::<f32>::new(from as usize, to as usize, chunk, 2, channels)
            .context("could not set up the rate conversion")?;
        Ok(Self {
            resampler,
            channels,
            planar: vec![vec![0.0; chunk]; channels],
        })
    }

    /// How many frames of input one [`Converter::convert`] takes.
    pub fn chunk_frames(&self) -> usize {
        self.resampler.input_frames_next()
    }

    /// Converts one chunk of interleaved `input`, and appends what comes
    /// out to `output`, interleaved.
    pub fn convert(&mut self, input: &[f32], output: &mut Vec<f32>) -> Result<()> {
        for (frame, samples) in input.chunks_exact(self.channels).enumerate() {
            for (channel, sample) in samples.iter().enumerate() {
                self.planar[channel][frame] = *sample;
            }
        }
        let converted = self
            .resampler
            .process(&self.planar, None)
            .context("rate conversion failed")?;
        let frames = converted.first().map_or(0, Vec::len);
        for frame in 0..frames {
            for channel in &converted {
                output.push(channel[frame]);
            }
        }
        Ok(())
    }
}

/// Float samples in the bytes a unit renders them in.
pub fn floats_from_bytes(bytes: &[u8], out: &mut Vec<f32>) {
    out.clear();
    out.extend(
        bytes
            .chunks_exact(4)
            .map(|sample| f32::from_ne_bytes([sample[0], sample[1], sample[2], sample[3]])),
    );
}

/// Appends `samples` to `out` as the bytes of `format`: floats as they are,
/// or 16-bit integers, clipped where a float goes past full scale.
pub fn push_samples(out: &mut VecDeque<u8>, samples: &[f32], format: Format) {
    for sample in samples {
        if format.float {
            out.extend(sample.to_ne_bytes());
        } else {
            let scaled = (sample * I16_SCALE)
                .round()
                .clamp(f32::from(i16::MIN), f32::from(i16::MAX));
            out.extend((scaled as i16).to_ne_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| (n as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    /// Rising zero crossings a second, which is the pitch.
    fn pitch(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        crossings as f32 * rate as f32 / samples.len() as f32
    }

    #[test]
    fn a_tone_keeps_its_pitch_from_44_1_to_48_khz() {
        let mut converter = Converter::new(44_100, 48_000, 1).unwrap();
        let chunk = converter.chunk_frames();
        let input = sine(44_100, 1_000.0, chunk * 100);
        let mut out = Vec::new();
        for block in input.chunks_exact(chunk) {
            converter.convert(block, &mut out).unwrap();
        }
        let ratio = out.len() as f32 / input.len() as f32;
        assert!((ratio - 48_000.0 / 44_100.0).abs() < 0.01, "{ratio}");
        let heard = pitch(&out[out.len() / 4..], 48_000);
        assert!((heard - 1_000.0).abs() < 30.0, "{heard}");
    }

    #[test]
    fn stereo_channels_stay_apart() {
        let mut converter = Converter::new(16_000, 48_000, 2).unwrap();
        let chunk = converter.chunk_frames();
        let mut out = Vec::new();
        for _ in 0..20 {
            let block: Vec<f32> = (0..chunk).flat_map(|_| [0.5, 0.0]).collect();
            converter.convert(&block, &mut out).unwrap();
        }
        let right_peak = out
            .chunks_exact(2)
            .skip(2_000)
            .map(|frame| frame[1].abs())
            .fold(0.0, f32::max);
        let left_mean = out
            .chunks_exact(2)
            .skip(2_000)
            .map(|frame| frame[0])
            .sum::<f32>()
            / (out.len() / 2 - 2_000) as f32;
        assert!(right_peak < 0.001, "{right_peak}");
        assert!((left_mean - 0.5).abs() < 0.01, "{left_mean}");
    }

    #[test]
    fn floats_become_16_bit_samples_with_clipping() {
        let format = Format {
            rate: 48_000,
            channels: 1,
            float: false,
        };
        let mut out = VecDeque::new();
        push_samples(&mut out, &[0.0, 0.5, 2.0, -2.0], format);
        let samples: Vec<i16> = out
            .make_contiguous()
            .chunks_exact(2)
            .map(|pair| i16::from_ne_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(samples, [0, 16_384, i16::MAX, i16::MIN]);
    }

    #[test]
    fn floats_stay_floats_and_read_back() {
        let format = Format {
            rate: 48_000,
            channels: 1,
            float: true,
        };
        let mut out = VecDeque::new();
        push_samples(&mut out, &[0.25, -0.5], format);
        let mut back = Vec::new();
        floats_from_bytes(out.make_contiguous(), &mut back);
        assert_eq!(back, [0.25, -0.5]);
    }
}
