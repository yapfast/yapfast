// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Audio helpers: recording remote tracks, plus synthetic-audio test utilities.
//!
//! [`record_track`] (drain a subscribed remote track into PCM) and
//! [`write_wav`] are shipped API — they are what a recording bot needs after
//! `matrix_rtc_call_sdk::LiveKitCall::join` hands it a subscribed track.
//!
//! The rest is test-only, gated behind `cfg(any(test, feature = "testing"))`:
//! [`publish_tone`] pushes a generated sine wave into a LiveKit track (via the
//! session's public [`LiveKitSession::room`](crate::session::LiveKitSession::room))
//! and [`detect_tone`] confirms a given frequency dominates the received signal
//! (via a Goertzel filter). The SFU re-encodes to Opus, so tests verify
//! *frequency energy*, not sample equality.

use std::time::Duration;

use futures_util::StreamExt;
use livekit::prelude::RemoteAudioTrack;
use livekit::webrtc::audio_stream::native::NativeAudioStream;

#[cfg(any(test, feature = "testing"))]
use std::borrow::Cow;

#[cfg(any(test, feature = "testing"))]
use livekit::options::TrackPublishOptions;
#[cfg(any(test, feature = "testing"))]
use livekit::prelude::{LocalAudioTrack, LocalTrack, RtcAudioSource, TrackSource};
#[cfg(any(test, feature = "testing"))]
use livekit::webrtc::audio_source::AudioSourceOptions;
#[cfg(any(test, feature = "testing"))]
use livekit::webrtc::audio_source::native::NativeAudioSource;
#[cfg(any(test, feature = "testing"))]
use livekit::webrtc::prelude::AudioFrame;
#[cfg(any(test, feature = "testing"))]
use tokio::task::JoinHandle;

#[cfg(any(test, feature = "testing"))]
use crate::Error;
#[cfg(any(test, feature = "testing"))]
use crate::session::LiveKitSession;

/// Sample rate used for the synthetic tone (Hz). 48 kHz is LiveKit's native rate.
pub const SAMPLE_RATE: u32 = 48_000;
/// Channel count used for the synthetic tone (mono).
pub const CHANNELS: u32 = 1;
/// Duration of each captured audio frame (10 ms is the WebRTC convention).
#[cfg(any(test, feature = "testing"))]
const FRAME_MS: u32 = 10;

/// Handle to a running tone generator. Dropping it stops the capture loop.
#[cfg(any(test, feature = "testing"))]
pub struct ToneHandle {
    task: JoinHandle<()>,
}

#[cfg(any(test, feature = "testing"))]
impl Drop for ToneHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Publish a continuous `freq_hz` sine-wave audio track to the SFU, via the
/// session's public [`LiveKitSession::room`]. Publishing continues until the
/// returned [`ToneHandle`] is dropped.
#[cfg(any(test, feature = "testing"))]
pub async fn publish_tone(session: &LiveKitSession, freq_hz: f64) -> Result<ToneHandle, Error> {
    let source = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, CHANNELS, 1000);
    let track = LocalAudioTrack::create_audio_track("tone", RtcAudioSource::Native(source.clone()));
    session
        .room()
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                ..Default::default()
            },
        )
        .await?;
    Ok(spawn_tone(source, freq_hz))
}

/// Spawn a task that pushes a continuous `freq_hz` sine wave into `source` in
/// 10 ms frames until the returned [`ToneHandle`] is dropped.
#[cfg(any(test, feature = "testing"))]
fn spawn_tone(source: NativeAudioSource, freq_hz: f64) -> ToneHandle {
    let samples_per_frame = (SAMPLE_RATE / 1000 * FRAME_MS) as usize;
    let amplitude = 0.5 * f64::from(i16::MAX);
    let task = tokio::spawn(async move {
        let mut n: u64 = 0;
        let mut ticker = tokio::time::interval(Duration::from_millis(u64::from(FRAME_MS)));
        loop {
            ticker.tick().await;
            let mut data = Vec::with_capacity(samples_per_frame);
            for _ in 0..samples_per_frame {
                let t = n as f64 / f64::from(SAMPLE_RATE);
                let sample = (amplitude * (std::f64::consts::TAU * freq_hz * t).sin()) as i16;
                data.push(sample);
                n += 1;
            }
            let frame = AudioFrame {
                data: Cow::Owned(data),
                sample_rate: SAMPLE_RATE,
                num_channels: CHANNELS,
                samples_per_channel: samples_per_frame as u32,
            };
            if source.capture_frame(&frame).await.is_err() {
                // Source closed (track unpublished / room left) — stop.
                break;
            }
        }
    });
    ToneHandle { task }
}

/// Drain a subscribed remote audio track into interleaved i16 PCM for `dur`.
///
/// Reads at [`SAMPLE_RATE`] / [`CHANNELS`] regardless of the sender's original
/// rate — libwebrtc resamples for us.
pub async fn record_track(track: &RemoteAudioTrack, dur: Duration) -> Vec<i16> {
    let mut stream = NativeAudioStream::new(track.rtc_track(), SAMPLE_RATE as i32, CHANNELS as i32);
    let mut pcm = Vec::new();
    let deadline = tokio::time::Instant::now() + dur;
    // Stops when the deadline is reached (timeout `Err`) or the stream ends
    // (`Ok(None)`).
    while let Ok(Some(frame)) = tokio::time::timeout_at(deadline, stream.next()).await {
        pcm.extend_from_slice(&frame.data);
    }
    pcm
}

/// Write mono i16 PCM to a 16-bit little-endian WAV file for manual inspection.
///
/// Hand-rolled (no external crate) — a canonical 44-byte PCM header followed by
/// the interleaved samples.
pub fn write_wav(path: &str, pcm: &[i16], sample_rate: u32) -> std::io::Result<()> {
    use std::io::Write;

    let channels = CHANNELS as u16;
    let bits_per_sample: u16 = 16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = (pcm.len() * 2) as u32;

    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // PCM fmt chunk size
    file.write_all(&1u16.to_le_bytes())?; // audio format 1 = PCM
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for &sample in pcm {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}

/// Ratio of signal energy at `target_hz` to total signal energy, in `[0, 1]`.
///
/// Computed with a Goertzel filter. A clean tone at `target_hz` yields a value
/// near `1.0`; silence or an unrelated signal yields a value near `0.0`.
#[cfg(any(test, feature = "testing"))]
pub fn detect_tone(pcm: &[i16], sample_rate: u32, target_hz: f64) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let n = pcm.len() as f64;
    // Goertzel coefficient for the target frequency.
    let k = (target_hz * n / f64::from(sample_rate)).round();
    let omega = std::f64::consts::TAU * k / n;
    let coeff = 2.0 * omega.cos();

    let mut s_prev = 0.0_f64;
    let mut s_prev2 = 0.0_f64;
    let mut total_energy = 0.0_f64;
    for &sample in pcm {
        let x = f64::from(sample);
        let s = x + coeff * s_prev - s_prev2;
        s_prev2 = s_prev;
        s_prev = s;
        total_energy += x * x;
    }
    let target_energy = s_prev2 * s_prev2 + s_prev * s_prev - coeff * s_prev * s_prev2;

    if total_energy <= 0.0 {
        return 0.0;
    }
    (target_energy / total_energy).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sample_rate: u32, samples: usize) -> Vec<i16> {
        (0..samples)
            .map(|i| {
                let t = i as f64 / f64::from(sample_rate);
                (16_000.0 * (std::f64::consts::TAU * freq * t).sin()) as i16
            })
            .collect()
    }

    #[test]
    fn detects_matching_tone() {
        let pcm = sine(440.0, SAMPLE_RATE, SAMPLE_RATE as usize); // 1 second
        assert!(detect_tone(&pcm, SAMPLE_RATE, 440.0) > 0.5);
    }

    #[test]
    fn rejects_wrong_frequency() {
        let pcm = sine(1000.0, SAMPLE_RATE, SAMPLE_RATE as usize);
        assert!(detect_tone(&pcm, SAMPLE_RATE, 440.0) < 0.1);
    }

    #[test]
    fn silence_has_no_energy() {
        assert_eq!(detect_tone(&[0; 4800], SAMPLE_RATE, 440.0), 0.0);
        assert_eq!(detect_tone(&[], SAMPLE_RATE, 440.0), 0.0);
    }
}
