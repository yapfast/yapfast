//! Evens out how loud your voice is in a call. Leaning towards the microphone
//! or away from it moves your level a few decibels; the leveler slowly undoes
//! that around your usual speaking level. Whispers and shouts sit much further
//! from it, so they pass through as they are, and so do short bursts. The
//! microphone's volume ([`crate::volume::microphone`]) applies with it, as one
//! gain that never lifts a peak into clipping.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Turns leveling on or off for every call; it's on until this says otherwise.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Levels the microphone in LiveKit's calls too, which WebRTC captures itself,
/// and sets its volume.
pub fn install() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        webrtc_sys::capture::set_capture_processor(|| Box::new(Capture(Leveler::default())));
    });
}

struct Capture(Leveler);

impl webrtc_sys::capture::CaptureProcessor for Capture {
    fn ramp(&mut self, samples: &[f32], sample_rate: u32) -> (f32, f32) {
        self.0
            .ramp(samples, sample_rate, enabled(), crate::volume::microphone())
    }
}

/// Samples are in 16-bit range, as WebRTC processes them.
const FULL_SCALE: f32 = 32768.0;
/// Blocks quieter than this, in RMS dBFS, are pauses or noise: they leave the
/// levels as they are, and the gain holds.
const SPEECH_FLOOR: f32 = -50.0;
/// Blocks this far below your usual level, such as the quiet ends of words,
/// don't count either.
const BELOW_USUAL: f32 = 20.0;
/// How quickly the level of what you're saying now follows you, in seconds.
const RECENT: f32 = 0.4;
/// How quickly your usual level follows you: quickly at first, then slowly, and
/// more slowly still for levels far from it, so a long whisper becomes usual
/// only after a while.
const USUAL_START: f32 = 1.5;
const USUAL: f32 = 30.0;
const USUAL_FAR: f32 = 90.0;
/// Speech heard before the usual level settles, in seconds.
const SETTLING: f32 = 5.0;
/// How much of a drift from your usual level is undone.
const STRENGTH: f32 = 0.8;
/// Drifts up to this many dB are evened out in full; up to `WHOLE`, less and
/// less; and beyond it, not at all.
const EVEN: f32 = 6.0;
const WHOLE: f32 = 12.0;
/// The most the leveler raises or lowers your voice, in dB.
const MAX_GAIN: f32 = 8.0;
/// How quickly the gain follows, in seconds.
const GAIN: f32 = 0.3;
/// A block this many dB above your usual level, such as a shout, lets go of
/// any correction this quickly, so it isn't held down.
const SUDDEN: f32 = 12.0;
const LET_GO: f32 = 0.05;
/// Raising, by leveling or the volume, never pushes a peak past this.
const CEILING: f32 = 0.9 * FULL_SCALE;

/// The leveler for one microphone.
#[derive(Debug)]
pub struct Leveler {
    usual: Option<f32>,
    recent: f32,
    heard: f32,
    /// The gain being approached, in dB.
    gain: f32,
    /// The linear gain the last block ended at.
    applied: f32,
}

impl Default for Leveler {
    fn default() -> Self {
        Self {
            usual: None,
            recent: 0.0,
            heard: 0.0,
            gain: 0.0,
            applied: 1.0,
        }
    }
}

impl Leveler {
    /// Levels a block of about 10 ms in place and sets it to `volume`, a
    /// linear gain. Turned off, it eases back to leaving the voice as it is.
    pub fn process(&mut self, samples: &mut [f32], sample_rate: u32, on: bool, volume: f32) {
        let (from, to) = self.ramp(samples, sample_rate, on, volume);
        let count = samples.len() as f32;
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample *= from + (to - from) * (index + 1) as f32 / count;
        }
    }

    /// The linear gains the block should start and end at, which it ramps
    /// between, after noting its level. The level is heard before `volume`,
    /// so changing it doesn't move your usual level.
    pub fn ramp(&mut self, samples: &[f32], sample_rate: u32, on: bool, volume: f32) -> (f32, f32) {
        if samples.is_empty() || sample_rate == 0 {
            return (self.applied, self.applied);
        }
        let seconds = samples.len() as f32 / sample_rate as f32;
        let level = decibels(samples);
        let mut sudden = false;
        let speech =
            level > SPEECH_FLOOR && self.usual.is_none_or(|usual| level > usual - BELOW_USUAL);
        if speech {
            let usual = self.usual.unwrap_or_else(|| {
                self.recent = level;
                level
            });
            self.recent += (level - self.recent) * smoothing(seconds, RECENT);
            let pace = if self.heard < SETTLING {
                USUAL_START
            } else if (level - usual).abs() > WHOLE {
                USUAL_FAR
            } else {
                USUAL
            };
            self.usual = Some(usual + (level - usual) * smoothing(seconds, pace));
            self.heard += seconds;
            sudden = level > usual + SUDDEN;
        }
        let target = match self.usual {
            Some(usual) if on && !sudden => {
                let drift = self.recent - usual;
                let weight = ((WHOLE - drift.abs()) / (WHOLE - EVEN)).clamp(0.0, 1.0);
                (-drift * STRENGTH * weight).clamp(-MAX_GAIN, MAX_GAIN)
            }
            _ => 0.0,
        };
        let pace = if sudden { LET_GO } else { GAIN };
        self.gain += (target - self.gain) * smoothing(seconds, pace);
        let peak = samples
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        let room = if peak > 0.0 {
            (CEILING / peak).max(1.0)
        } else {
            f32::MAX
        };
        let from = self.applied.min(room);
        let to = (10f32.powf(self.gain / 20.0) * volume).min(room);
        self.applied = to;
        (from, to)
    }
}

/// The share of the way to a new value to move in `seconds`, for a change that
/// takes about `pace` seconds.
fn smoothing(seconds: f32, pace: f32) -> f32 {
    1.0 - (-seconds / pace).exp()
}

/// A block's RMS level in dBFS.
fn decibels(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    10.0 * (power / (FULL_SCALE * FULL_SCALE)).max(1e-12).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480;

    /// Speech-like tone at an RMS level in dBFS, a block at a time.
    struct Tone {
        phase: f32,
    }

    impl Tone {
        fn block(&mut self, level: f32) -> Vec<f32> {
            let amplitude = FULL_SCALE * 10f32.powf(level / 20.0) * std::f32::consts::SQRT_2;
            (0..BLOCK)
                .map(|_| {
                    self.phase += std::f32::consts::TAU * 220.0 / RATE as f32;
                    amplitude * self.phase.sin()
                })
                .collect()
        }
    }

    /// Feeds `seconds` at `level` and returns the output level of the last block.
    fn speak(leveler: &mut Leveler, tone: &mut Tone, level: f32, seconds: f32, on: bool) -> f32 {
        let mut out = level;
        for _ in 0..(seconds * 100.0) as usize {
            let mut block = tone.block(level);
            leveler.process(&mut block, RATE, on, 1.0);
            out = decibels(&block);
        }
        out
    }

    #[test]
    fn evens_out_leaning_away() {
        let (mut leveler, mut tone) = (Leveler::default(), Tone { phase: 0.0 });
        let usual = speak(&mut leveler, &mut tone, -26.0, 10.0, true);
        assert!((usual + 26.0).abs() < 0.5, "the usual level stays: {usual}");
        let away = speak(&mut leveler, &mut tone, -32.0, 3.0, true);
        assert!(away > -28.5, "6 dB further away comes back up: {away}");
        let closer = speak(&mut leveler, &mut tone, -21.0, 3.0, true);
        assert!(closer < -23.0, "5 dB closer comes down: {closer}");
    }

    #[test]
    fn whispers_and_shouts_pass() {
        let (mut leveler, mut tone) = (Leveler::default(), Tone { phase: 0.0 });
        speak(&mut leveler, &mut tone, -26.0, 10.0, true);
        let whisper = speak(&mut leveler, &mut tone, -45.0, 3.0, true);
        assert!(
            (whisper + 45.0).abs() < 1.0,
            "a whisper stays quiet: {whisper}"
        );
        speak(&mut leveler, &mut tone, -26.0, 5.0, true);
        let mut loudest = 0.0f32;
        let mut shout = 0.0;
        for _ in 0..50 {
            let mut block = tone.block(-8.0);
            leveler.process(&mut block, RATE, true, 1.0);
            loudest = block
                .iter()
                .fold(loudest, |peak, sample| peak.max(sample.abs()));
            shout = decibels(&block);
        }
        assert!((shout + 8.0).abs() < 1.0, "a shout stays loud: {shout}");
        assert!(loudest <= FULL_SCALE, "and doesn't clip: {loudest}");
    }

    #[test]
    fn off_and_silence_leave_the_voice_alone() {
        let (mut leveler, mut tone) = (Leveler::default(), Tone { phase: 0.0 });
        speak(&mut leveler, &mut tone, -26.0, 10.0, false);
        let away = speak(&mut leveler, &mut tone, -32.0, 3.0, false);
        assert!((away + 32.0).abs() < 0.1, "off: {away}");
        let mut quiet = vec![0.0; BLOCK];
        leveler.process(&mut quiet, RATE, true, 1.0);
        assert!(quiet.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn the_volume_sets_the_voice_without_clipping_it() {
        let (mut leveler, mut tone) = (Leveler::default(), Tone { phase: 0.0 });
        let level = |leveler: &mut Leveler, tone: &mut Tone, level: f32, volume: f32| {
            let (mut out, mut loudest) = (level, 0.0f32);
            for _ in 0..100 {
                let mut block = tone.block(level);
                leveler.process(&mut block, RATE, false, volume);
                loudest = block
                    .iter()
                    .fold(loudest, |peak, sample| peak.max(sample.abs()));
                out = decibels(&block);
            }
            (out, loudest)
        };
        let (half, _) = level(&mut leveler, &mut tone, -26.0, 0.5);
        assert!((half + 32.0).abs() < 0.5, "-6 dB: {half}");
        let (twice, _) = level(&mut leveler, &mut tone, -26.0, 2.0);
        assert!((twice + 20.0).abs() < 0.5, "+6 dB: {twice}");
        let (shout, loudest) = level(&mut leveler, &mut tone, -4.0, 2.0);
        assert!(
            shout < -2.0 && loudest <= FULL_SCALE,
            "a loud voice doesn't clip: {shout}, {loudest}"
        );
        let (silent, _) = level(&mut leveler, &mut tone, -26.0, 0.0);
        assert!(silent < -100.0, "0% sends nothing: {silent}");
    }
}
