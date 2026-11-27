//! Yapfast's own short sounds for calls and messages: joining and leaving, mute and
//! deafen, a new message or mention, someone starting or stopping a stream,
//! and someone starting or stopping watching one. They come in six sets with one
//! character each, all in F major pentatonic so any two sound well together.
//! Four are synthesized as they play; two play the score on one recorded
//! note each, the only sound bundled.

use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

use crate::audio_io::{Output, RATE};
const TAU: f32 = std::f32::consts::TAU;

/// The instrument every sound plays on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundSet {
    /// A recorded kalimba's pluck.
    #[default]
    NewKalimba,
    /// A muffled piano.
    Felt,
    /// A low marimba under a soft mallet.
    Marimba,
    /// A mellow electric piano.
    Rhodes,
    /// Three sine waves a little apart, which shimmer.
    Glow,
    /// A recorded breathy wooden flute, shaped into a struck note.
    Flute,
}

impl SoundSet {
    /// In the order Settings lists them, which `layout.json` doesn't depend on.
    pub const ALL: [Self; 6] = [
        Self::NewKalimba,
        Self::Felt,
        Self::Marimba,
        Self::Rhodes,
        Self::Glow,
        Self::Flute,
    ];

    /// Whether the set plays a recorded note rather than a synthesized one.
    fn recorded(self) -> bool {
        matches!(self, Self::NewKalimba | Self::Flute)
    }
}

/// One recorded note: 24 kHz mono 16-bit little-endian samples, made with
/// ElevenLabs' sound generator, and the note it plays at its own speed. The
/// score plays it faster or slower for each of its notes.
struct Sample {
    pcm: &'static [u8],
    root: f32,
}

/// The rate [`Sample`]s are stored at. Recorded sets are low-passed at
/// [`SAMPLED_CUTOFF`], so nothing above 12 kHz is needed.
const SAMPLE_RATE: f32 = 24_000.0;

/// G4, a few cents sharp.
static KALIMBA: Sample = Sample {
    pcm: include_bytes!("../sounds/kalimba.pcm"),
    root: 67.06,
};

/// Recorded near C6 and played two octaves up from the score, its own register:
/// from C4 the score sounds C6.
static FLUTE: Sample = Sample {
    pcm: include_bytes!("../sounds/flute.pcm"),
    root: 59.95,
};

impl Sample {
    /// The recording at `time` seconds played `speed` times as fast, between
    /// stored samples by a straight line, and silent past its end.
    fn at(&self, time: f32, speed: f32) -> f32 {
        let position = time * SAMPLE_RATE * speed;
        let index = position as usize;
        let read = |index: usize| {
            self.pcm.get(index * 2..index * 2 + 2).map_or(0.0, |bytes| {
                f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0
            })
        };
        let between = position - index as f32;
        read(index) * (1.0 - between) + read(index + 1) * between
    }

    /// One note at `hz`, `time` seconds after it starts. A recording can't
    /// ring for less time, so `fade` below 1 fades it out sooner instead, as
    /// the synthesized sets shorten their notes.
    fn voice(&self, hz: f32, time: f32, fade: f32) -> f32 {
        let sooner = (1.0 / fade - 1.0) / 0.25;
        self.at(time, hz / frequency(self.root)) * (-time * sooner).exp() * attack(time, 0.002)
    }
}

/// What a sound announces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sound {
    /// Someone, this account included, joined the call this account is in.
    Join,
    /// Someone left that call, or this account did.
    Leave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// A message that notifies.
    Message,
    /// A message that mentions this account.
    Mention,
    /// Someone else started watching this account's stream, or the one it
    /// watches.
    ViewerJoin,
    /// Someone else stopped watching it.
    ViewerLeave,
    /// Someone else in the call started streaming.
    StreamStart,
    /// Someone else stopped streaming, staying in the call.
    StreamStop,
}

/// One replacement supplied by the active theme. Several files take turns,
/// so repeated events do not always sound identical.
#[derive(Clone)]
pub struct ThemedSound {
    pub samples: Vec<Arc<[i16]>>,
    pub volume: f32,
}

static THEMED: LazyLock<RwLock<HashMap<Sound, ThemedSound>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));
static NEXT_THEMED: AtomicUsize = AtomicUsize::new(0);

/// Replaces Yapfast's event sounds with the active theme's decoded files.
/// An empty map restores the selected built-in sound set.
pub fn set_themed(sounds: HashMap<Sound, ThemedSound>) {
    if let Ok(mut themed) = THEMED.write() {
        *themed = sounds;
    }
}

fn themed(sound: Sound) -> Option<(Arc<[i16]>, f32)> {
    let themed = THEMED.read().ok()?;
    let choice = themed.get(&sound)?;
    if choice.samples.is_empty() {
        return None;
    }
    let index = NEXT_THEMED.fetch_add(1, Ordering::Relaxed) % choice.samples.len();
    Some((choice.samples[index].clone(), choice.volume))
}

impl Sound {
    pub const ALL: [Self; 12] = [
        Self::Join,
        Self::Leave,
        Self::Mute,
        Self::Unmute,
        Self::Deafen,
        Self::Undeafen,
        Self::Message,
        Self::Mention,
        Self::ViewerJoin,
        Self::ViewerLeave,
        Self::StreamStart,
        Self::StreamStop,
    ];
}

/// The MIDI notes the sounds use.
const F3: f32 = 53.0;
const C4: f32 = 60.0;
const F4: f32 = 65.0;
const A4: f32 = 69.0;
const C5: f32 = 72.0;
const F5: f32 = 77.0;

/// A sound's notes (start in seconds, MIDI note, velocity), how much faster
/// than usual they fade, and its loudness against the others in dB.
fn score(sound: Sound) -> (&'static [(f32, f32, f32)], f32, f32) {
    match sound {
        // Rising as someone arrives, falling as they go.
        Sound::Join => (&[(0.0, C4, 0.8), (0.14, F4, 1.0)], 1.0, 0.0),
        Sound::Leave => (&[(0.0, F4, 0.9), (0.16, C4, 0.75)], 1.0, -1.5),
        // Quick flicks, quieter, since they're heard most.
        Sound::Mute => (&[(0.0, A4, 0.8), (0.08, F4, 0.7)], 0.45, -4.0),
        Sound::Unmute => (&[(0.0, F4, 0.7), (0.08, A4, 0.8)], 0.45, -4.0),
        // Mute's flicks, lower.
        Sound::Deafen => (&[(0.0, C4, 0.8), (0.09, F3, 0.7)], 0.5, -3.0),
        Sound::Undeafen => (&[(0.0, F3, 0.7), (0.09, C4, 0.8)], 0.5, -3.0),
        Sound::Message => (&[(0.0, F4, 0.6), (0.0, A4, 0.8)], 0.8, -2.0),
        Sound::Mention => (
            &[(0.0, C4, 0.7), (0.09, F4, 0.8), (0.18, A4, 1.0)],
            0.9,
            0.0,
        ),
        // Join and Leave an octave up, quicker and quieter: a viewer comes
        // and goes more lightly than someone joining the call.
        Sound::ViewerJoin => (&[(0.0, C5, 0.7), (0.1, F5, 0.85)], 0.7, -4.0),
        Sound::ViewerLeave => (&[(0.0, F5, 0.75), (0.11, C5, 0.65)], 0.7, -5.0),
        // An open fifth, then the octave above it: something opening up.
        // Ending, the octave falls back to the fifth, quieter.
        Sound::StreamStart => (
            &[(0.0, F4, 0.7), (0.0, C5, 0.6), (0.12, F5, 0.85)],
            0.9,
            -2.0,
        ),
        Sound::StreamStop => (
            &[(0.0, F5, 0.75), (0.13, F4, 0.65), (0.13, C5, 0.55)],
            0.9,
            -3.5,
        ),
    }
}

fn frequency(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// A raised-cosine fade in over `seconds`, so notes don't click.
fn attack(time: f32, seconds: f32) -> f32 {
    if time < seconds {
        0.5 - 0.5 * (std::f32::consts::PI * time / seconds).cos()
    } else {
        1.0
    }
}

/// One note of `set` at `time` seconds after it starts. Lower notes ring a
/// little longer, as a real instrument's do; `fade` shortens them all.
fn voice(set: SoundSet, hz: f32, time: f32, velocity: f32, fade: f32) -> f32 {
    let sine = |ratio: f32| (TAU * hz * ratio * time).sin();
    let decay = |seconds: f32| (-time / seconds).exp();
    match set {
        SoundSet::Felt => {
            let longer = (440.0 / hz).powf(0.3);
            let partials = [
                (1.0, 1.0),
                (2.0, 0.35 * velocity),
                (3.0, 0.12 * velocity),
                (4.0, 0.05 * velocity),
            ];
            let sum: f32 = partials
                .iter()
                .map(|&(n, amplitude)| {
                    amplitude * sine(n) * decay(0.18 * longer * fade / n.powf(0.7))
                })
                .sum();
            sum * attack(time, 0.010) / 1.2
        }
        SoundSet::Marimba => {
            let longer = (440.0 / hz).powf(0.35);
            let tone = sine(1.0) * decay(0.16 * longer * fade)
                + 0.12 * velocity * sine(3.93) * decay(0.04 * longer * fade);
            tone * attack(time, 0.008)
        }
        SoundSet::Rhodes => {
            let longer = (440.0 / hz).powf(0.3);
            // Frequency modulation, brightest as the note is struck.
            let index = 0.15 + 0.6 * velocity * decay(0.1);
            (TAU * hz * time + index * sine(1.0)).sin()
                * decay(0.2 * longer * fade)
                * attack(time, 0.006)
        }
        SoundSet::NewKalimba => KALIMBA.voice(hz, time, fade),
        SoundSet::Flute => FLUTE.voice(hz, time, fade),
        SoundSet::Glow => {
            let longer = (440.0 / hz).powf(0.3);
            let tone = (sine(0.9975) + sine(1.0) + sine(1.0025)) / 3.0
                + 0.10 * velocity * sine(2.0) * decay(0.15);
            tone * decay(0.15 * longer * fade) * attack(time, 0.018)
        }
    }
}

/// A second-order Butterworth low-pass at `cutoff` Hz, in place.
fn low_pass(samples: &mut [f32], cutoff: f32) {
    let w = TAU * cutoff / RATE as f32;
    let alpha = w.sin() / std::f32::consts::SQRT_2;
    let a0 = 1.0 + alpha;
    let b0 = (1.0 - w.cos()) / 2.0 / a0;
    let b1 = (1.0 - w.cos()) / a0;
    let a1 = -2.0 * w.cos() / a0;
    let a2 = (1.0 - alpha) / a0;
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for sample in samples {
        let y = b0 * *sample + b1 * x1 + b0 * x2 - a1 * y1 - a2 * y2;
        (x2, x1, y2, y1) = (x1, *sample, y1, y);
        *sample = y;
    }
}

/// A small, dark room for each ear: 10 ms of quiet, then low-passed noise
/// fading by 60 dB in 0.3 s, at unit energy. The same every time.
fn room() -> [Vec<f32>; 2] {
    let mut seed: u32 = 7;
    let mut uniform = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed as f32 / 4_294_967_296.0
    };
    let silent = (0.010 * RATE as f32) as usize;
    let length = (0.4 * RATE as f32) as usize;
    std::array::from_fn(|_| {
        let mut response = vec![0.0f32; silent + length];
        for (index, sample) in response[silent..].iter_mut().enumerate() {
            let time = index as f32 / RATE as f32;
            let noise = (-2.0 * (1.0 - uniform()).ln()).sqrt() * (TAU * uniform()).cos();
            *sample = noise * (-6.9 * time / 0.3).exp();
        }
        low_pass(&mut response[silent..], 2000.0);
        let energy = response.iter().map(|sample| sample * sample).sum::<f32>();
        response
            .iter_mut()
            .for_each(|sample| *sample /= energy.sqrt());
        response
    })
}

/// Where recorded sets are low-passed: the synthesized ones' 3 kHz would take
/// the kalimba's ring and the flute's breath, so only the top goes.
const SAMPLED_CUTOFF: f32 = 9000.0;

/// How much of the room is heard against the sound itself.
const ROOM: f32 = 0.07;

/// The loudest sounds peak at a quarter of full scale, 12 dB under, so twice
/// the volume, the slider's 200%, reaches it.
const PEAK: f32 = 0.25;

/// `sound` in `set`, as interleaved stereo at 48 kHz peaking at [`PEAK`] less
/// its own quieting, before any volume. It ends once 40 dB below its peak.
pub fn render(set: SoundSet, sound: Sound) -> Vec<f32> {
    let (notes, fade, loudness) = score(sound);
    let last = notes.iter().fold(0.0f32, |last, note| last.max(note.0));
    let mut dry = vec![0.0f32; ((last + 2.0) * RATE as f32) as usize];
    for &(start, midi, velocity) in notes {
        let hz = frequency(midi);
        let first = (start * RATE as f32) as usize;
        for (index, sample) in dry[first..].iter_mut().enumerate() {
            *sample += velocity * voice(set, hz, index as f32 / RATE as f32, velocity, fade);
        }
    }
    low_pass(
        &mut dry,
        if set.recorded() {
            SAMPLED_CUTOFF
        } else {
            3000.0
        },
    );

    let room = room();
    let length = (dry.len() + room[0].len()).next_power_of_two();
    let mut planner = RealFftPlanner::<f32>::new();
    let forward = planner.plan_fft_forward(length);
    let inverse = planner.plan_fft_inverse(length);
    let spectrum = |samples: &[f32]| {
        let mut input = samples.to_vec();
        input.resize(length, 0.0);
        let mut output = forward.make_output_vec();
        forward
            .process(&mut input, &mut output)
            .expect("buffers sized by the plan");
        output
    };
    let sound = spectrum(&dry);
    let wet: Vec<Vec<f32>> = room
        .iter()
        .map(|response| {
            let mut product: Vec<_> = sound
                .iter()
                .zip(spectrum(response))
                .map(|(a, b)| a * b * ROOM / length as f32)
                .collect();
            // Real signals' products are real at either end; rounding aside.
            for end in [0, product.len() - 1] {
                product[end].im = 0.0;
            }
            let mut output = inverse.make_output_vec();
            inverse
                .process(&mut product, &mut output)
                .expect("buffers sized by the plan");
            output
        })
        .collect();

    let frames = dry.len() + room[0].len() - 1;
    let mut stereo: Vec<f32> = (0..frames)
        .flat_map(|frame| {
            let dry = dry.get(frame).copied().unwrap_or(0.0);
            [dry + wet[0][frame], dry + wet[1][frame]]
        })
        .collect();
    let peak = stereo
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    let end = stereo
        .chunks(2)
        .rposition(|frame| frame.iter().any(|sample| sample.abs() > peak * 0.01))
        .map_or(0, |frame| frame + 1);
    stereo.truncate(end * 2);
    let gain = PEAK * 10f32.powf(loudness / 20.0) / peak;
    let tail = end.min(RATE as usize / 20);
    for (frame, pair) in stereo.chunks_mut(2).enumerate() {
        let left = end - frame;
        let fade = if left <= tail {
            (left as f32 / tail as f32).powi(2)
        } else {
            1.0
        };
        pair.iter_mut().for_each(|sample| *sample *= gain * fade);
    }
    stereo
}

/// A volume slider's percentage, up to 200%, as a gain. Loudness is heard on a
/// log scale, so the square spreads quiet settings along more of the slider.
/// At 200% the loudest sounds reach full scale.
pub fn gain(percent: u8) -> f32 {
    (f32::from(percent.min(200)) / 100.0).powi(2)
}

/// Plays `sound` once on `speaker`, a device id as in [`crate::voice::State`],
/// or the default output, at `volume` from [`gain`]. It's made and played on a
/// thread of its own, and overlaps any sound still playing.
pub fn play(set: SoundSet, sound: Sound, volume: f32, speaker: Option<&str>) {
    if volume <= 0.0 {
        return;
    }
    if let Some((samples, themed_volume)) = themed(sound) {
        play_decoded(samples, volume * themed_volume, speaker);
        return;
    }
    let speaker: Option<Arc<str>> = speaker.map(Into::into);
    let started = std::thread::Builder::new()
        .name("yapfast-sound".into())
        .spawn(move || {
            let mut samples = render(set, sound);
            samples.iter_mut().for_each(|sample| *sample *= volume);
            if let Err(error) = play_samples(&samples, speaker.as_deref()) {
                tracing::warn!(
                    error = format!("{error:#}"),
                    ?sound,
                    "could not play a sound"
                );
            }
        });
    if let Err(error) = started {
        tracing::warn!(%error, "could not start a sound");
    }
}

/// How many of a theme's sounds play at once; more are dropped, so a pointer
/// sweeping across buttons can't pile up streams.
const MOST_AT_ONCE: usize = 4;

static PLAYING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Plays decoded 48 kHz interleaved stereo, a theme's sound decoded once and
/// shared, at `volume` from [`gain`], as [`play`] does. Dropped while
/// [`MOST_AT_ONCE`] are still playing.
pub fn play_decoded(samples: Arc<[i16]>, volume: f32, speaker: Option<&str>) {
    use std::sync::atomic::Ordering;
    if volume <= 0.0 {
        return;
    }
    if PLAYING.fetch_add(1, Ordering::AcqRel) >= MOST_AT_ONCE {
        PLAYING.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    let speaker: Option<Arc<str>> = speaker.map(Into::into);
    let started = std::thread::Builder::new()
        .name("yapfast-sound".into())
        .spawn(move || {
            let scaled: Vec<f32> = samples
                .iter()
                .map(|sample| f32::from(*sample) / f32::from(i16::MAX) * volume)
                .collect();
            if let Err(error) = play_samples(&scaled, speaker.as_deref()) {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not play a theme's sound"
                );
            }
            PLAYING.fetch_sub(1, Ordering::AcqRel);
        });
    if let Err(error) = started {
        PLAYING.fetch_sub(1, Ordering::AcqRel);
        tracing::warn!(%error, "could not start a sound");
    }
}

/// Plays interleaved stereo samples to the end.
fn play_samples(samples: &[f32], speaker: Option<&str>) -> anyhow::Result<()> {
    use anyhow::Context;
    // Speakers that are gone play on the default ones.
    let mut output = Output::<i16>::open_or_default(speaker, "Sound", RATE, 2, None)
        .context("could not open the speakers")?;
    let mut chunk = [0i16; RATE as usize / 50 * 2];
    for part in samples.chunks(chunk.len()) {
        for (sample, value) in chunk.iter_mut().zip(part) {
            *sample = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        }
        output
            .write(&chunk[..part.len()])
            .context("could not play")?;
    }
    output.drain().context("could not finish playing")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_short_quiet_and_fade_out() {
        for set in SoundSet::ALL {
            for sound in Sound::ALL {
                let samples = render(set, sound);
                let seconds = samples.len() as f32 / 2.0 / RATE as f32;
                assert!(
                    (0.2..1.5).contains(&seconds),
                    "{set:?} {sound:?} lasts {seconds} s"
                );
                let peak = samples
                    .iter()
                    .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
                let (_, _, loudness) = score(sound);
                let expected = PEAK * 10f32.powf(loudness / 20.0);
                assert!(
                    (peak - expected).abs() < 1e-3,
                    "{set:?} {sound:?} peaks at {peak}"
                );
                assert!(
                    samples[samples.len() - 2..].iter().all(|s| s.abs() < 1e-4),
                    "{set:?} {sound:?} ends silent"
                );
                assert!(
                    samples[..2].iter().all(|s| s.abs() < 1e-3),
                    "{set:?} {sound:?} starts without a click"
                );
            }
        }
    }

    #[test]
    fn volume_follows_the_square_of_the_slider() {
        assert_eq!(gain(0), 0.0);
        assert_eq!(gain(100), 1.0);
        assert!((gain(50) - 0.25).abs() < 1e-6);
        assert_eq!(gain(200), 4.0);
        assert_eq!(gain(255), 4.0);
        // Twice as loud as their peak is full scale, and no further.
        let loudest = render(SoundSet::Felt, Sound::Join);
        let peak = loudest.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak * gain(200) <= 1.0 + 1e-3);
    }
}
