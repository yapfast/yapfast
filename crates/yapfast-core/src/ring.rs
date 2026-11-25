//! The sounds of a DM call ringing: an incoming call's ring, and the
//! ringback heard while ringing someone else. Each repeats until dropped,
//! on the speakers chosen for calls.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};

use crate::audio_io::{Output, RATE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Someone is calling this account.
    Incoming,
    /// This account is ringing someone who hasn't answered yet.
    Outgoing,
}

/// One repetition of a tone, mono at 48 kHz: its notes, then a pause.
fn pattern(tone: Tone) -> Vec<f32> {
    // Each note: frequency, start and length in seconds.
    let (notes, length, loudness): (&[(f32, f32, f32)], f32, f32) = match tone {
        // A rising arpeggio, twice, as a phone's ring rises.
        Tone::Incoming => (
            &[
                (659.25, 0.0, 0.16),
                (830.61, 0.16, 0.16),
                (987.77, 0.32, 0.3),
                (659.25, 0.8, 0.16),
                (830.61, 0.96, 0.16),
                (987.77, 1.12, 0.3),
            ],
            3.0,
            0.3,
        ),
        // Two soft notes, then a long wait.
        Tone::Outgoing => (&[(440.0, 0.0, 0.35), (659.25, 0.4, 0.5)], 3.0, 0.16),
    };
    let mut samples = vec![0.0f32; (length * RATE as f32) as usize];
    for &(frequency, start, duration) in notes {
        let first = (start * RATE as f32) as usize;
        let count = (duration * RATE as f32) as usize;
        for index in 0..count {
            let time = index as f32 / RATE as f32;
            // A quick attack and a smooth decay, so notes don't click.
            let envelope = (time / 0.01).min(1.0) * (1.0 - index as f32 / count as f32).powi(2);
            let wave = (std::f32::consts::TAU * frequency * time).sin()
                + 0.25 * (std::f32::consts::TAU * 2.0 * frequency * time).sin();
            if let Some(sample) = samples.get_mut(first + index) {
                *sample += loudness * envelope * wave / 1.25;
            }
        }
    }
    samples
}

/// A theme's own ring and ringback, as 48 kHz interleaved stereo, in place
/// of the tones; none plays the tone.
static THEMED: std::sync::RwLock<[Option<Arc<[i16]>>; 2]> = std::sync::RwLock::new([None, None]);

/// Rings with a theme's sound for `tone` from the next ring on, or with the
/// tone again for none.
pub fn set_themed(tone: Tone, samples: Option<Arc<[i16]>>) {
    if let Ok(mut themed) = THEMED.write() {
        themed[tone as usize] = samples;
    }
}

/// One repetition of what rings for `tone`, mono, as [`pattern`] gives it:
/// a theme's sound, or the tone.
fn repetition(tone: Tone) -> Vec<f32> {
    let themed = THEMED
        .read()
        .ok()
        .and_then(|themed| themed[tone as usize].clone());
    match themed {
        Some(samples) => samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| (f32::from(pair[0]) + f32::from(pair[1])) / 2.0 / f32::from(i16::MAX))
            .collect(),
        None => pattern(tone),
    }
}

/// Plays a tone over and over until dropped.
pub struct Ringer {
    stop: Arc<AtomicBool>,
}

impl Ringer {
    /// Starts ringing on `speaker`, a device id as in [`crate::voice::State`],
    /// or the default output.
    pub fn start(tone: Tone, speaker: Option<&str>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let speaker = speaker.map(str::to_owned);
        std::thread::Builder::new()
            .name("yapfast-ring".into())
            .spawn(move || {
                // Little queued, so the ring stops soon after it's answered.
                let output = Output::<i16>::open_or_default(
                    speaker.as_deref(),
                    "Ringing",
                    RATE,
                    2,
                    Some(Duration::from_millis(100)),
                );
                match output {
                    Ok(output) => play(&repetition(tone), output, &stopped),
                    Err(error) => tracing::warn!(
                        error = format!("{error:#}"),
                        "could not open the speakers to ring"
                    ),
                }
            })
            .context("could not start ringing")?;
        Ok(Self { stop })
    }
}

impl Drop for Ringer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// 20 ms at a time.
const CHUNK: usize = RATE as usize / 50;

fn play(pattern: &[f32], mut output: Output<i16>, stop: &AtomicBool) {
    let mut stereo = [0i16; CHUNK * 2];
    for chunk in pattern.chunks(CHUNK).cycle() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        for (pair, sample) in stereo.as_chunks_mut::<2>().0.iter_mut().zip(chunk) {
            pair.fill((sample * f32::from(i16::MAX)) as i16);
        }
        if let Err(error) = output.write(&stereo[..chunk.len() * 2]) {
            tracing::warn!(error = format!("{error:#}"), "stopped ringing");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tones_sound_then_pause_without_clipping() {
        for tone in [Tone::Incoming, Tone::Outgoing] {
            let samples = pattern(tone);
            assert_eq!(samples.len(), 3 * RATE as usize);
            let peak = samples
                .iter()
                .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
            assert!(peak > 0.1 && peak < 1.0, "{tone:?} peaks at {peak}");
            let tail = &samples[samples.len() - RATE as usize..];
            assert!(
                tail.iter().all(|sample| *sample == 0.0),
                "{tone:?} ends quiet"
            );
        }
    }
}
