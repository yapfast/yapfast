//! The voice message playing in the conversation view. Its recording, a
//! small Ogg Opus file, downloads whole, then decodes a packet at a time on
//! a thread of its own as it plays on the speakers chosen for calls, as
//! fast and loud as the person chose. At its end it goes back to the start,
//! kept to play again. One plays at a time; dropping it stops it and lets
//! its recording go.

use std::{
    future::Future,
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering::Relaxed},
        mpsc,
    },
    time::Duration,
};

use anyhow::{Context, Result};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    audio_io::{Output, RATE},
    chat::{VoicePlayback, VoiceTuning},
    video::Video,
    video_player::Resampler,
};

/// How often the time played goes to the UI as it plays.
const REPORT_EVERY: Duration = Duration::from_millis(250);
/// The stretch of sound [`Tempo`] moves at once: 30 ms, about a syllable's
/// pitch period many times over.
const WINDOW: usize = RATE as usize * 3 / 100;
/// How far apart its windows overlap in what plays: half a window.
const HOP: usize = WINDOW / 2;
/// How far, either way, a window may move to continue the last: 5 ms, the
/// period of a voice's lowest pitch.
const SEEK: usize = RATE as usize / 200;

/// What the player's download and thread tell it.
#[derive(Debug, PartialEq)]
pub enum Report {
    /// The recording downloaded and started playing.
    Loaded,
    /// It has played this many milliseconds.
    Position(u64),
    /// It played to its end, and waits at the start.
    Finished,
    /// It stopped, for this reason for the person.
    Failed(&'static str),
}

/// How fast and loud it plays, which the person may change as it plays: an
/// [`f32`] each, by its bits.
struct Tuning {
    speed: AtomicU32,
    volume: AtomicU32,
}

impl Tuning {
    fn new(tuning: VoiceTuning) -> Self {
        Self {
            speed: AtomicU32::new(tuning.speed.to_bits()),
            volume: AtomicU32::new(tuning.volume.to_bits()),
        }
    }

    fn set(&self, tuning: VoiceTuning) {
        self.speed.store(tuning.speed.to_bits(), Relaxed);
        self.volume.store(tuning.volume.to_bits(), Relaxed);
    }

    /// Its speed, kept between half and twice as fast.
    fn speed(&self) -> f32 {
        f32::from_bits(self.speed.load(Relaxed)).clamp(0.5, 2.0)
    }

    /// The gain of its volume slider, squared as the video's is, so its
    /// middle sounds about half as loud.
    fn gain(&self) -> f32 {
        f32::from_bits(self.volume.load(Relaxed))
            .clamp(0.0, 1.0)
            .powi(2)
    }
}

pub struct Player {
    pub playback: VoicePlayback,
    /// Plays (true) or pauses it; dropping it stops it.
    control: mpsc::Sender<bool>,
    tuning: Arc<Tuning>,
    reports: UnboundedReceiver<Report>,
    download: tokio::task::JoinHandle<()>,
}

impl Drop for Player {
    fn drop(&mut self) {
        self.download.abort();
    }
}

impl Player {
    /// Starts downloading `playback`'s recording with `download`, then plays
    /// it from the start on `speakers`, tuned as `tuning` says.
    pub fn start<F>(
        playback: VoicePlayback,
        speakers: Option<String>,
        tuning: VoiceTuning,
        download: F,
    ) -> Self
    where
        F: Future<Output = Result<Vec<u8>>> + Send + 'static,
    {
        let (control, controls) = mpsc::channel();
        let tuning = Arc::new(Tuning::new(tuning));
        let tuned = tuning.clone();
        let (reported, reports) = tokio::sync::mpsc::unbounded_channel();
        let download = tokio::spawn(async move {
            let data = match download.await {
                Ok(data) => data,
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "could not download a voice message"
                    );
                    let _ = reported.send(Report::Failed(
                        "Yapfast couldn't download this voice message.",
                    ));
                    return;
                }
            };
            let started = std::thread::Builder::new()
                .name("yapfast-voice-message".into())
                .spawn(move || {
                    play_on(&data, speakers.as_deref(), &controls, &tuned, &reported);
                });
            if let Err(error) = started {
                tracing::warn!(%error, "could not start a voice message");
            }
        });
        Self {
            playback: VoicePlayback {
                loading: true,
                playing: true,
                position_ms: 0,
                error: None,
                ..playback
            },
            control,
            tuning,
            reports,
            download,
        }
    }

    /// Plays it as fast and loud as `tuning` says, from its next packet.
    pub fn tune(&self, tuning: VoiceTuning) {
        self.tuning.set(tuning);
    }

    /// Plays it on from where it paused, or pauses it.
    pub fn set_playing(&mut self, playing: bool) {
        self.playback.playing = playing;
        let _ = self.control.send(playing);
    }

    /// Waits for news of it. Cancel safe.
    pub async fn wait(&mut self) -> Option<Report> {
        self.reports.recv().await
    }

    /// Takes news of it. False once it stopped, as when it failed.
    pub fn handle(&mut self, report: Option<Report>) -> bool {
        match report {
            Some(Report::Loaded) => self.playback.loading = false,
            Some(Report::Position(position_ms)) => self.playback.position_ms = position_ms,
            Some(Report::Finished) => {
                self.playback.playing = false;
                self.playback.position_ms = 0;
            }
            None => return false,
            Some(Report::Failed(why)) => {
                self.playback.loading = false;
                self.playback.playing = false;
                self.playback.error = Some(why.into());
                return false;
            }
        }
        true
    }
}

/// Where a voice message's sound goes: the speakers, or a test's buffer.
trait Speakers {
    /// Queues interleaved stereo samples, waiting while the queue is full.
    fn write(&mut self, samples: &[i16]) -> Result<()>;
    /// How long until what's written now is heard.
    fn latency(&self) -> Duration;
}

impl Speakers for Output<i16> {
    fn write(&mut self, samples: &[i16]) -> Result<()> {
        Output::write(self, samples)
    }

    fn latency(&self) -> Duration {
        Output::latency(self)
    }
}

/// The player's thread: plays the recording, from the start each time it's
/// played after its end, until the player is dropped. The speakers are open
/// only while it plays or pauses.
fn play_on(
    data: &[u8],
    speakers: Option<&str>,
    controls: &mpsc::Receiver<bool>,
    tuning: &Tuning,
    reports: &UnboundedSender<Report>,
) {
    loop {
        // Little queued, so pausing is heard at once.
        let played = Output::<i16>::open_or_default(
            speakers,
            "Voice message",
            RATE,
            2,
            Some(Duration::from_millis(100)),
        )
        .context("could not open the speakers")
        .and_then(|mut output| {
            let ended = play(data, &mut output, controls, tuning, reports)?;
            if ended {
                output.drain().context("could not finish playing")?;
            }
            Ok(ended)
        });
        match played {
            Ok(true) => {
                let _ = reports.send(Report::Finished);
            }
            Ok(false) => return,
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "could not play a voice message"
                );
                let _ = reports.send(Report::Failed("Yapfast couldn't play this voice message."));
                return;
            }
        }
        loop {
            match controls.recv() {
                Ok(true) => break,
                Ok(false) => {}
                Err(_) => return,
            }
        }
    }
}

/// Decodes `data` a packet at a time into `speakers`, pausing and playing
/// as `controls` says, at the speed and volume `tuning` has at each packet.
/// True once it played to its end; false when the player was dropped.
fn play(
    data: &[u8],
    speakers: &mut impl Speakers,
    controls: &mpsc::Receiver<bool>,
    tuning: &Tuning,
    reports: &UnboundedSender<Report>,
) -> Result<bool> {
    let mut file = Video::open_sound(Cursor::new(data)).context("not a sound Yapfast can play")?;
    let (rate, channels) = file.sound.context("the file has no sound")?;
    let mut resampler = Resampler::new(rate, channels);
    let mut tempo = Tempo::new();
    let _ = reports.send(Report::Loaded);
    // How far into the recording what was written reaches, in frames: at
    // another speed, more or less than was written.
    let mut through = 0.0f64;
    let mut reported = 0;
    let mut frames = Vec::new();
    let mut samples = Vec::new();
    let millis = |frames: f64| (frames.max(0.0) * 1000.0 / f64::from(RATE)) as u64;
    while file.decode_sound()?.is_some() {
        let mut playing = true;
        loop {
            match controls.try_recv() {
                Ok(play) => playing = play,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(false),
            }
        }
        // Paused, it waits until told to play on: what's queued, 100 ms at
        // most, plays out.
        while !playing {
            let _ = reports.send(Report::Position(millis(through)));
            match controls.recv() {
                Ok(play) => playing = play,
                Err(_) => return Ok(false),
            }
        }
        let sound = file.sound();
        resampler.follow(sound.rate, sound.channels);
        let speed = tuning.speed();
        frames.clear();
        through += tempo.run(&resampler.run(sound.samples), speed, &mut frames);
        louder(&frames, tuning.gain(), &mut samples);
        // The time stretcher gives nothing while it fills its first window,
        // and PulseAudio refuses an empty write.
        if !samples.is_empty() {
            speakers.write(&samples).context("could not play")?;
        }
        // What's queued is heard later, and stands for more or less of the
        // recording at another speed.
        let queued = speakers.latency().as_secs_f64() * f64::from(RATE) * f64::from(speed);
        let played = millis(through - queued);
        if played >= reported + REPORT_EVERY.as_millis() as u64 {
            reported = played;
            let _ = reports.send(Report::Position(played));
        }
    }
    frames.clear();
    tempo.finish(tuning.speed(), &mut frames);
    louder(&frames, tuning.gain(), &mut samples);
    if !samples.is_empty() {
        speakers.write(&samples).context("could not play")?;
    }
    Ok(true)
}

/// Puts stereo frames into `samples` at `gain`, as the speakers take them.
fn louder(frames: &[[f32; 2]], gain: f32, samples: &mut Vec<i16>) {
    samples.clear();
    samples.extend(
        frames
            .iter()
            .flatten()
            .map(|sample| ((sample * gain).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16),
    );
}

/// Plays interleaved stereo sound faster or slower at the same pitch, as
/// FFmpeg's `atempo` does, by WSOLA: windows of it, taken a speed's worth of
/// hops apart, overlap in what plays a hop apart, each moved a little to
/// where it best continues the one before, so a voice doesn't warble. At
/// the normal speed each window continues the last exactly, and the sound
/// comes out as it went in.
struct Tempo {
    /// Sound still needed, from where the oldest window may start.
    input: Vec<[f32; 2]>,
    /// Where the next window is taken from, around, in `input`.
    next: f64,
    /// Where the sound after the last window's first half is, which the
    /// next window should sound like.
    follows: Option<usize>,
    /// The last window's second half, which the next one's first overlaps.
    tail: Vec<[f32; 2]>,
    /// A Hann window, whose halves add to 1 when overlapped a hop apart.
    window: Vec<f32>,
}

impl Tempo {
    fn new() -> Self {
        Self {
            input: Vec::new(),
            next: 0.0,
            follows: None,
            tail: vec![[0.0; 2]; HOP],
            window: (0..WINDOW)
                .map(|at| 0.5 - 0.5 * (std::f32::consts::TAU * at as f32 / WINDOW as f32).cos())
                .collect(),
        }
    }

    /// Takes interleaved stereo sound and adds to `output` what plays at
    /// `speed`; returns how many frames of the sound that stands for.
    fn run(&mut self, sound: &[f32], speed: f32, output: &mut Vec<[f32; 2]>) -> f64 {
        self.input
            .extend(sound.chunks_exact(2).map(|frame| [frame[0], frame[1]]));
        let mut through = 0.0;
        loop {
            let around = self.next.round() as usize;
            if self.input.len() < around + SEEK + WINDOW {
                return through;
            }
            let start = match self.follows {
                Some(follows) if follows != around => self.best(around, follows),
                _ => around,
            };
            // Its first half overlaps the last's second, read before its own
            // second half takes that one's place.
            let frames = self.input[start..start + WINDOW].iter().zip(&self.window);
            for (at, (frame, weight)) in frames.enumerate() {
                let frame = [frame[0] * weight, frame[1] * weight];
                if at < HOP {
                    let tail = self.tail[at];
                    output.push([tail[0] + frame[0], tail[1] + frame[1]]);
                } else {
                    self.tail[at - HOP] = frame;
                }
            }
            self.follows = Some(start + HOP);
            let hop = HOP as f64 * f64::from(speed);
            self.next += hop;
            through += hop;
            // What no window can reach any more goes, a few windows at once.
            let gone = (self.next as usize).saturating_sub(SEEK).min(start + HOP);
            if gone > WINDOW * 4 {
                self.input.drain(..gone);
                self.next -= gone as f64;
                self.follows = Some(start + HOP - gone);
            }
        }
    }

    /// Where near `around` a window starts that sounds most like the sound at
    /// `follows`, over the hop they overlap, by their correlation. Every
    /// fourth frame of each, in mono, is enough to tell.
    fn best(&self, around: usize, follows: usize) -> usize {
        let mono = |frame: &[f32; 2]| frame[0] + frame[1];
        let target: Vec<f32> = self.input[follows..follows + HOP]
            .iter()
            .step_by(4)
            .map(mono)
            .collect();
        let likeness = |start: usize| {
            let (mut together, mut energy) = (0.0f32, 1e-9f32);
            for (frame, wanted) in self.input[start..].iter().step_by(4).zip(&target) {
                let sample = mono(frame);
                together += sample * wanted;
                energy += sample * sample;
            }
            together / energy.sqrt()
        };
        (around.saturating_sub(SEEK)..=around + SEEK)
            .map(|start| (likeness(start), start))
            .max_by(|one, other| one.0.total_cmp(&other.0))
            .map_or(around, |(_, start)| start)
    }

    /// Adds to `output` what's left at the sound's end: windows over the
    /// last of it, which no whole window reached, with silence after, then
    /// the last window's fading half.
    fn finish(&mut self, speed: f32, output: &mut Vec<[f32; 2]>) {
        if self.input.is_empty() {
            return;
        }
        // Enough for windows a half hop apart, the slowest, to pass its end.
        let silence = [0.0; 2].repeat(2 * WINDOW + SEEK);
        self.run(&silence, speed, output);
        output.append(&mut self.tail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Speakers that keep what they're given, at once.
    #[derive(Default)]
    struct Kept(Vec<i16>);

    impl Speakers for Kept {
        fn write(&mut self, samples: &[i16]) -> Result<()> {
            // As PulseAudio does.
            anyhow::ensure!(!samples.is_empty(), "Invalid argument");
            self.0.extend_from_slice(samples);
            Ok(())
        }

        fn latency(&self) -> Duration {
            Duration::ZERO
        }
    }

    fn normal() -> Tuning {
        Tuning::new(VoiceTuning::default())
    }

    /// How many times a second the left channel crosses zero: its pitch,
    /// for a tone.
    fn crossings(samples: &[i16]) -> f32 {
        let left: Vec<i16> = samples.iter().step_by(2).copied().collect();
        let crossed = left
            .windows(2)
            .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
            .count();
        crossed as f32 / (left.len() as f32 / RATE as f32)
    }

    fn recording() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures/voice-tone.ogg"),
        )
        .unwrap()
    }

    #[test]
    fn plays_a_recording_to_its_end_telling_how_far_it_got() {
        let (_control, controls) = mpsc::channel();
        let (reported, mut reports) = tokio::sync::mpsc::unbounded_channel();
        let mut speakers = Kept::default();
        assert!(play(&recording(), &mut speakers, &controls, &normal(), &reported).unwrap());
        // Two seconds of 48 kHz stereo.
        let seconds = speakers.0.len() as f32 / 2.0 / RATE as f32;
        assert!((seconds - 2.0).abs() < 0.1, "played {seconds} s");
        let reports: Vec<Report> = std::iter::from_fn(|| reports.try_recv().ok()).collect();
        assert_eq!(reports[0], Report::Loaded);
        let positions: Vec<u64> = reports[1..]
            .iter()
            .map(|report| match report {
                Report::Position(at) => *at,
                other => panic!("{other:?}"),
            })
            .collect();
        assert!(positions.len() >= 6, "{positions:?}");
        assert!(positions.is_sorted());
        assert!(*positions.last().unwrap() >= 1750, "{positions:?}");
    }

    #[test]
    fn pauses_until_told_to_play_and_stops_once_dropped() {
        let (control, controls) = mpsc::channel();
        let (reported, mut reports) = tokio::sync::mpsc::unbounded_channel();
        // Paused before it starts, then played, it plays everything.
        control.send(false).unwrap();
        control.send(true).unwrap();
        let mut speakers = Kept::default();
        assert!(play(&recording(), &mut speakers, &controls, &normal(), &reported).unwrap());
        assert!(speakers.0.len() > RATE as usize * 3);
        let not_sound = play(
            b"not a sound",
            &mut Kept::default(),
            &controls,
            &normal(),
            &reported,
        );
        assert!(not_sound.is_err());
        while reports.try_recv().is_ok() {}

        // Paused at the start, it says where it waits; dropped, it plays
        // nothing more.
        control.send(false).unwrap();
        let playing = std::thread::spawn(move || {
            let mut speakers = Kept::default();
            let ended = play(&recording(), &mut speakers, &controls, &normal(), &reported).unwrap();
            (ended, speakers.0.len())
        });
        assert_eq!(reports.blocking_recv(), Some(Report::Loaded));
        assert_eq!(reports.blocking_recv(), Some(Report::Position(0)));
        drop(control);
        assert_eq!(playing.join().unwrap(), (false, 0));
    }

    #[test]
    fn keeps_the_recordings_end() {
        // A steady tone 2.3 windows long, which no whole number of windows
        // covers: its last frames still play, at full level, at 1x.
        let frames = WINDOW * 23 / 10;
        let sound: Vec<f32> = [0.5; 2].repeat(frames);
        let mut tempo = Tempo::new();
        let mut output = Vec::new();
        tempo.run(&sound, 1.0, &mut output);
        tempo.finish(1.0, &mut output);
        assert!(
            output.len() >= frames,
            "{} of {frames} frames",
            output.len()
        );
        for frame in &output[HOP..frames] {
            assert!((frame[0] - 0.5).abs() < 1e-3, "{frame:?}");
        }
    }

    #[test]
    fn plays_twice_as_fast_at_the_same_pitch_and_silent_at_no_volume() {
        let (_control, controls) = mpsc::channel();
        let (reported, mut reports) = tokio::sync::mpsc::unbounded_channel();
        let mut normal_speed = Kept::default();
        play(
            &recording(),
            &mut normal_speed,
            &controls,
            &normal(),
            &reported,
        )
        .unwrap();
        while reports.try_recv().is_ok() {}
        let tuning = Tuning::new(VoiceTuning {
            speed: 2.0,
            volume: 1.0,
        });
        let mut fast = Kept::default();
        assert!(play(&recording(), &mut fast, &controls, &tuning, &reported).unwrap());
        // Half the frames, at the same pitch.
        let seconds = fast.0.len() as f32 / 2.0 / RATE as f32;
        assert!((seconds - 1.0).abs() < 0.1, "played {seconds} s");
        let (pitch, was) = (crossings(&fast.0), crossings(&normal_speed.0));
        assert!(
            (pitch / was - 1.0).abs() < 0.1,
            "{pitch} crossings a second, not {was}"
        );
        // The time it tells is the recording's.
        let last = std::iter::from_fn(|| reports.try_recv().ok())
            .filter_map(|report| match report {
                Report::Position(at) => Some(at),
                _ => None,
            })
            .last();
        assert!(last.is_some_and(|at| at >= 1750), "{last:?}");

        tuning.set(VoiceTuning {
            speed: 1.5,
            volume: 0.0,
        });
        let mut silent = Kept::default();
        assert!(play(&recording(), &mut silent, &controls, &tuning, &reported).unwrap());
        assert!(!silent.0.is_empty());
        assert!(silent.0.iter().all(|sample| *sample == 0));
    }
}
