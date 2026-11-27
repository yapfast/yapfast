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

use crate::{
    audio_io::{Output, RATE},
    sounds::{self, SoundSet},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Someone is calling this account.
    Incoming,
    /// This account is ringing someone who hasn't answered yet.
    Outgoing,
}

impl Tone {
    pub const ALL: [Self; 2] = [Self::Incoming, Self::Outgoing];
}

/// One repetition of a tone, stereo at 48 kHz: its notes, then a pause.
fn pattern(tone: Tone) -> Arc<[i16]> {
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
    let mut samples: Arc<[i16]> =
        std::iter::repeat_n(0, (length * RATE as f32) as usize * 2).collect();
    let frames = Arc::get_mut(&mut samples)
        .expect("new tone")
        .as_chunks_mut::<2>()
        .0;
    for &(frequency, start, duration) in notes {
        let first = (start * RATE as f32) as usize;
        let count = (duration * RATE as f32) as usize;
        for index in 0..count {
            let time = index as f32 / RATE as f32;
            // A quick attack and a smooth decay, so notes don't click.
            let envelope = (time / 0.01).min(1.0) * (1.0 - index as f32 / count as f32).powi(2);
            let wave = (std::f32::consts::TAU * frequency * time).sin()
                + 0.25 * (std::f32::consts::TAU * 2.0 * frequency * time).sin();
            if let Some(pair) = frames.get_mut(first + index) {
                let sample = (loudness * envelope * wave / 1.25 * f32::from(i16::MAX)) as i16;
                *pair = [pair[0].saturating_add(sample); 2];
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

#[derive(Clone)]
pub enum CustomTone {
    Set(SoundSet),
    File(Arc<[i16]>),
}

static CUSTOM: std::sync::RwLock<[Option<CustomTone>; 2]> = std::sync::RwLock::new([None, None]);

/// Applies both personal tones for subsequent calls and previews.
pub fn set_custom(tones: [Option<CustomTone>; 2]) {
    if let Ok(mut custom) = CUSTOM.write() {
        *custom = tones;
    }
}

fn repetition(tone: Tone) -> (Arc<[i16]>, usize) {
    let pcm = |sample: f32| (sample * f32::from(i16::MAX)) as i16;
    let custom = CUSTOM
        .read()
        .ok()
        .and_then(|custom| custom[tone as usize].clone());
    if let Some(custom) = custom {
        let samples = match custom {
            CustomTone::Set(set) => sounds::render_ring(set, tone)
                .into_iter()
                .map(pcm)
                .collect(),
            CustomTone::File(samples) => samples,
        };
        let pause = (3 * RATE as usize * 2).saturating_sub(samples.len());
        return (samples, pause);
    }
    if let Some(samples) = THEMED
        .read()
        .ok()
        .and_then(|themed| themed[tone as usize].clone())
    {
        return (samples, 0);
    }
    (pattern(tone), 0)
}

/// Plays a tone over and over until dropped.
pub struct Ringer {
    stop: Arc<AtomicBool>,
}

impl Ringer {
    /// Starts ringing on `speaker`, a device id as in [`crate::voice::State`],
    /// or the default output.
    pub fn start(tone: Tone, speaker: Option<&str>) -> Result<Self> {
        Self::spawn(tone, speaker, true)
    }

    /// Plays one repetition, which stops early if the preview is dropped.
    pub fn preview(tone: Tone, speaker: Option<&str>) -> Result<Self> {
        Self::spawn(tone, speaker, false)
    }

    fn spawn(tone: Tone, speaker: Option<&str>, repeat: bool) -> Result<Self> {
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
                    Ok(output) => {
                        let (samples, pause) = repetition(tone);
                        if let Err(error) = play(&samples, pause, output, &stopped, repeat) {
                            tracing::warn!(error = format!("{error:#}"), "stopped ringing");
                        }
                    }
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
const CHUNK: usize = RATE as usize / 50 * 2;

fn play(
    samples: &[i16],
    pause: usize,
    mut output: Output<i16>,
    stop: &AtomicBool,
    repeat: bool,
) -> Result<()> {
    const QUIET: [i16; CHUNK] = [0; CHUNK];
    let pause = if repeat { pause } else { 0 };
    loop {
        let silence = (0..pause)
            .step_by(CHUNK)
            .map(|at| &QUIET[..(pause - at).min(CHUNK)]);
        let chunks = samples.chunks(CHUNK).chain(silence);
        for chunk in chunks {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            output.write(chunk)?;
        }
        if !repeat {
            return output.drain().context("could not finish the ring preview");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_set_ring_phrases_vary_by_direction_and_instrument() {
        let incoming = sounds::render_ring(SoundSet::Felt, Tone::Incoming);
        let outgoing = sounds::render_ring(SoundSet::Felt, Tone::Outgoing);
        let flute = sounds::render_ring(SoundSet::Flute, Tone::Incoming);
        assert!(incoming.len() > outgoing.len());
        assert_ne!(incoming, flute);
        for phrase in [incoming, outgoing, flute] {
            assert!(phrase.len() <= 3 * RATE as usize * 2);
            assert!(phrase.iter().any(|sample| sample.abs() > 0.1));
        }
    }

    #[test]
    fn tones_sound_then_pause_without_clipping() {
        for tone in [Tone::Incoming, Tone::Outgoing] {
            let samples = pattern(tone);
            assert_eq!(samples.len(), 3 * RATE as usize * 2);
            let peak = samples
                .iter()
                .map(|sample| f32::from(sample.unsigned_abs()) / f32::from(i16::MAX))
                .fold(0.0f32, f32::max);
            assert!(peak > 0.1 && peak < 1.0, "{tone:?} peaks at {peak}");
            let tail = &samples[samples.len() - RATE as usize * 2..];
            assert!(
                tail.iter().all(|sample| *sample == 0),
                "{tone:?} ends quiet"
            );
        }
    }
}
