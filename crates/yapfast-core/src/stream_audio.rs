//! Go Live's sound. Sharing: what every other application plays, mixed
//! into 20 ms Opus frames for the stream. Yapfast's own sound is left out, so
//! viewers never hear the call echoed back, as Discord's client and Vesktop
//! do; on Linux so are other Discord clients', whose sound is a call too, which
//! a stream would echo back. Windows can leave out only what Yapfast's own
//! processes play, so another Discord client's call reaches viewers there.
//! [`crate::audio_io::others`] hears the applications.
//! Watching: the streamer's Opus, decoded and played ([`Player`]).

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use yapfast_discord::{
    opus,
    stream_connection::{ReceivedSound, Sound, SoundSender},
    voice::Surplus,
};

use crate::audio_io::{Heard, LastingOutput, RATE};

const CHANNELS: usize = 2;
/// Samples of every channel in one 20 ms frame.
const FRAME: usize = RATE as usize / 50 * CHANNELS;
/// An application's sound joins the mix once this much waits, which
/// absorbs the system delivering it in chunks.
const PRIMED: usize = FRAME * 2;
/// The most kept for an application; older sound is dropped, so clock
/// drift never grows into delay.
const BACKLOG: usize = FRAME * 10;
/// What Discord's client gives a stream's sound.
const BITRATE: i32 = 64_000;
/// Where a mixed sample starts bending towards full scale rather than
/// flat-topping, in sample values. Under it every sample is mixed as it came:
/// an application playing quietly is never made quieter for the company it
/// keeps, and one whose sound stays under it sounds exactly as it played. No
/// curve can be the identity all the way to full scale and still round the
/// corner there, so the bend starts a little under it; a single application at
/// full scale gives up 0.45 dB, which is under the step an ear hears as a
/// change in loudness.
const KNEE: f32 = 0.9;
/// The room a sample over [`KNEE`] has to bend in, up to full scale.
const BEND: f32 = 1.0 - KNEE;

#[cfg(all(test, windows))]
#[path = "stream_audio/windows_tests.rs"]
mod windows_tests;

mod player;

pub use player::Player;

/// Whether an application, by its name or program, is Yapfast or a Discord
/// client, whose sound is a call or a stream: Yapfast's other workers, such as
/// one playing a watched stream, and Discord's own client and those built
/// on it.
fn is_discord_client(name: &str) -> bool {
    if name == "Yapfast" {
        return true;
    }
    let name = name.to_lowercase();
    [
        "discord", "vesktop", "equibop", "webcord", "legcord", "armcord",
    ]
    .iter()
    .any(|client| name.contains(client))
}

/// One application's sound, waiting to be mixed.
#[derive(Default)]
struct Input {
    samples: VecDeque<f32>,
    primed: bool,
    /// The process playing it, when the system says.
    process: Option<u32>,
}

impl Input {
    /// Adds sound to the backlog, keeping only the newest [`BACKLOG`]
    /// samples: the oldest go first, so a long callback's chunk needs no more
    /// room than its last [`BACKLOG`] samples ever take.
    fn push(&mut self, samples: impl IntoIterator<Item = f32>) {
        for sample in samples {
            // A driver that lost its place can hand over a number that isn't
            // one, and the mix would carry it: a clamp returns a NaN as it
            // came, and Opus has no way to answer for it. Silence costs one
            // sample of one application, which the rest of the frame covers.
            let sample = if sample.is_finite() { sample } else { 0.0 };
            if self.samples.len() >= BACKLOG {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }
}

/// Each monitored application's sound, by the system's number for it.
type Inputs = Arc<Mutex<HashMap<u32, Input>>>;

/// A mixed sample, bent rather than clipped at full scale: up to [`KNEE`]
/// exactly as it summed, and above it towards full scale without reaching it.
/// The bend is what several loud applications adding up need, since their sum
/// would otherwise flat-top at the first sample past full scale and tear the
/// sound; 1.4 comes out at 0.98 of full scale instead. Slope is one where the
/// bend starts, so nothing jumps where it begins, and the curve carries
/// nothing from frame to frame: a loud passage can't pull down the sound after
/// it, and the bend is not a gain heard as the mix pumping.
fn bend(sample: f32) -> f32 {
    let over = (sample.abs() - KNEE) / BEND;
    if over <= 0.0 {
        return sample;
    }
    // The one over one plus it, written so a sum that overflowed to infinity
    // still lands on full scale rather than on a NaN.
    let eased = 1.0 - 1.0 / (1.0 + over);
    (KNEE + BEND * eased).copysign(sample)
}

/// Mixes the next frame from every application with enough waiting into
/// `out`, interleaved stereo. False when none had any, so nothing plays.
fn mix(inputs: &mut HashMap<u32, Input>, out: &mut [f32]) -> bool {
    out.fill(0.0);
    let mut heard = false;
    for input in inputs.values_mut() {
        input.primed |= input.samples.len() >= PRIMED;
        if !input.primed {
            continue;
        }
        heard = true;
        let taken = input.samples.len().min(out.len());
        for (mixed, sample) in out.iter_mut().zip(input.samples.drain(..taken)) {
            *mixed += sample;
        }
        // Ran dry: wait to fill up again rather than crackle.
        input.primed = taken == out.len();
    }
    for sample in out.iter_mut() {
        *sample = bend(*sample);
    }
    heard
}

/// Hears the other applications into `inputs` until `stop`.
fn listen(
    inputs: Inputs,
    stop: Arc<AtomicBool>,
    audio: crate::voice::StreamAudio,
) -> Result<JoinHandle<()>> {
    let heard = move |heard: Heard<'_>| {
        let mut inputs = inputs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match heard {
            Heard::Started { source, process } => {
                inputs.insert(
                    source,
                    Input {
                        process,
                        ..Input::default()
                    },
                );
            }
            Heard::Samples { source, samples } => {
                if let Some(input) = inputs.get_mut(&source) {
                    input.push(samples.iter().copied());
                }
            }
            Heard::Stopped { source } => {
                if let Some(forgotten) = inputs.remove(&source) {
                    tracing::debug!(
                        process = ?forgotten.process,
                        "stopped capturing an application's sound"
                    );
                }
            }
        }
    };
    match audio {
        crate::voice::StreamAudio::All => crate::audio_io::others(is_discord_client, heard, stop),
        crate::voice::StreamAudio::Window(window) => {
            crate::audio_io::window_audio(window, heard, stop)
        }
        crate::voice::StreamAudio::Off => anyhow::bail!("stream audio is disabled"),
    }
}

/// Captures and sends the stream's sound until dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Capture {
    /// Starts capturing other applications' sound and sending it as Opus.
    pub fn start(sound: SoundSender, audio: crate::voice::StreamAudio) -> Result<Self> {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let mut encoder = opus::Encoder::new(2, opus::Application::Audio, BITRATE)?;
        let capture = listen(inputs.clone(), stop.clone(), audio)?;
        let stopping = stop.clone();
        let mut capture = Self {
            stop,
            threads: vec![capture],
        };
        let mixer = std::thread::Builder::new()
            .name("yapfast-stream-sound".into())
            .spawn(move || {
                let mut frame = vec![0.0; FRAME];
                let mut opus = vec![0; 4000];
                let mut timestamp: u32 = rand::random();
                let started = Instant::now();
                let mut tick = 0u32;
                while !stopping.load(Ordering::Relaxed) {
                    tick += 1;
                    let due = started + Duration::from_millis(20) * tick;
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    let heard = mix(
                        &mut inputs
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()),
                        &mut frame,
                    );
                    if heard {
                        match encoder.encode_float(&frame, &mut opus) {
                            Ok(size) => sound.send(Sound {
                                opus: opus[..size].to_vec(),
                                timestamp,
                            }),
                            Err(error) => {
                                tracing::warn!(%error, "could not encode the stream's sound");
                            }
                        }
                    }
                    timestamp = timestamp.wrapping_add((FRAME / CHANNELS) as u32);
                }
            })
            .context("could not start mixing the stream's sound")?;
        capture.threads.push(mixer);
        Ok(capture)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// How loud a watched stream plays, which Yapfast can change while it does.
#[derive(Debug)]
pub struct Volume(std::sync::atomic::AtomicU32);

impl Volume {
    pub fn new(volume: f32) -> Self {
        Self(std::sync::atomic::AtomicU32::new(volume.to_bits()))
    }

    pub fn set(&self, volume: f32) {
        self.0.store(volume.to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Sets `pcm` to `volume`, clipping at full scale.
fn scale(pcm: &mut [i16], volume: f32) {
    if volume == 1.0 {
        return;
    }
    for sample in pcm {
        *sample = (f32::from(*sample) * volume)
            .round()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_applications_once_each_has_enough() {
        let mut inputs = HashMap::new();
        let mut out = vec![0.0; FRAME];
        let mut music = Input::default();
        music.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(1, music);
        let mut late = Input::default();
        late.push(std::iter::repeat_n(0.5, FRAME));
        inputs.insert(2, late);
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.25, "the late one waits to fill up");
        inputs
            .get_mut(&2)
            .unwrap()
            .push(std::iter::repeat_n(0.5, FRAME));
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.75);
        // The first ran dry: it waits again, and the second goes loud.
        inputs
            .get_mut(&2)
            .unwrap()
            .push(std::iter::repeat_n(0.9, FRAME * 3));
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.5);
        inputs
            .get_mut(&1)
            .unwrap()
            .push(std::iter::repeat_n(0.9, PRIMED));
        assert!(mix(&mut inputs, &mut out));
        // 1.8 summed: bent towards full scale rather than flat-topped at it.
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(out[0] > 0.98, "{} of full scale, not pulled down", out[0]);
        inputs.clear();
        assert!(!mix(&mut inputs, &mut out), "silence sends nothing");
    }

    #[test]
    fn loud_applications_stay_inside_full_scale_and_quiet_ones_keep_their_loudness() {
        let mut inputs = HashMap::new();
        let mut out = vec![0.0; FRAME];
        for (source, loudness) in [(1, 0.25), (2, 0.5)] {
            let mut input = Input::default();
            input.push(std::iter::repeat_n(loudness, PRIMED));
            inputs.insert(source, input);
        }
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.75, "under the knee, mixed exactly as it summed");

        for (source, loudness) in [(1, 0.9), (2, 0.9)] {
            let mut input = Input::default();
            input.push(std::iter::repeat_n(loudness, PRIMED));
            inputs.insert(source, input);
        }
        assert!(mix(&mut inputs, &mut out));
        assert!(
            out.iter().all(|sample| sample.abs() <= 1.0),
            "two applications at 1.8 summed stay inside full scale"
        );
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(out[0] > 0.98, "{} of full scale, not pulled down", out[0]);

        inputs.clear();
        let mut quiet = Input::default();
        quiet.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(1, quiet);
        assert!(mix(&mut inputs, &mut out));
        assert_eq!(out[0], 0.25, "one quiet application alone is untouched");

        // The one thing the bend costs a single application: full scale it
        // played, and 0.95 of full scale it comes out, so the loudest sound
        // gives up under half a decibel.
        inputs.clear();
        let mut loudest = Input::default();
        loudest.push(std::iter::repeat_n(1.0, PRIMED));
        inputs.insert(1, loudest);
        assert!(mix(&mut inputs, &mut out));
        assert!(out[0] < 1.0, "{} is inside full scale", out[0]);
        assert!(
            out[0] > 0.94,
            "{} of full scale at most a half decibel down",
            out[0]
        );
    }

    /// Plays a quiet tone with paplay and hears it through the monitor:
    /// `YAPFAST_TEST_AUDIO=1 YAPFAST_TONE=/path/to/tone.wav cargo test --
    /// --ignored captures_`, on a private PulseAudio.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn captures_what_another_application_plays() {
        let tone = std::env::var("YAPFAST_TONE").expect("YAPFAST_TONE names a WAV file");
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), stop.clone(), crate::voice::StreamAudio::All).unwrap();
        let mut player = std::process::Command::new("paplay")
            .args(["--volume=100", &tone])
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let loudest = loop {
            let loudest = inputs
                .lock()
                .unwrap()
                .values()
                .flat_map(|input| input.samples.iter().copied())
                .fold(0.0f32, |loudest, sample| loudest.max(sample.abs()));
            if loudest > 0.0 || Instant::now() > deadline {
                break loudest;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = player.wait();
        stop.store(true, Ordering::Relaxed);
        capture.join().unwrap();
        assert!(loudest > 0.0, "heard nothing");
    }

    /// Plays a quiet tone from this process, as Yapfast plays a call, and hears
    /// none of it.
    /// With `YAPFAST_TEST_AUDIO=1`, on a private PulseAudio.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn leaves_out_yapfasts_own_sound() {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), stop.clone(), crate::voice::StreamAudio::All).unwrap();
        let mut player =
            crate::audio_io::Output::<f32>::open(None, "Discord voice", RATE, 2, None).unwrap();
        // A second of a 440 Hz tone at about -56 dB.
        let tone: Vec<f32> = (0..RATE)
            .flat_map(|at| {
                let level =
                    0.0016 * (at as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin();
                [level, level]
            })
            .collect();
        player.write(&tone).unwrap();
        player.drain().unwrap();
        let ours = inputs
            .lock()
            .unwrap()
            .values()
            .filter(|input| input.process == Some(std::process::id()))
            .count();
        let others = inputs.lock().unwrap().len();
        stop.store(true, Ordering::Relaxed);
        capture.join().unwrap();
        assert_eq!(ours, 0, "Yapfast's own playback was captured");
        eprintln!("monitored {others} other applications' streams");
    }

    #[test]
    fn leaves_out_other_discord_clients() {
        for client in [
            "Yapfast",
            "Discord",
            "DiscordCanary",
            ".Discord-wrapped",
            "vesktop",
            "Equibop",
            "WebCord",
            "legcord",
        ] {
            assert!(is_discord_client(client), "{client}");
        }
        for other in [
            "Firefox",
            "mpv",
            "electron",
            "Spotify",
            "steam",
            "talkative",
        ] {
            assert!(!is_discord_client(other), "{other}");
        }
    }

    /// Plays 400 ms of Opus silence on the default speakers, then ends with
    /// the sound.
    #[test]
    fn plays_a_stream_at_its_volume() {
        let volume = Volume::new(0.5);
        let pcm = [1000i16, -1000, 30_000];
        let mut scaled = pcm;
        scale(&mut scaled, volume.get());
        assert_eq!(scaled, [500, -500, 15_000]);
        volume.set(2.0);
        let mut scaled = pcm;
        scale(&mut scaled, volume.get());
        assert_eq!(
            scaled,
            [2000, -2000, i16::MAX],
            "louder clips at full scale"
        );
    }

    #[test]
    fn unavailable_audio_window_never_captures_all_applications() {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let result = listen(inputs.clone(), stop, crate::voice::StreamAudio::Window(0));
        assert!(
            result.is_err(),
            "a closed window must fail instead of falling back"
        );
        assert!(inputs.lock().unwrap().is_empty());
    }

    #[test]
    fn keeps_a_bounded_backlog() {
        let mut input = Input::default();
        input.push(std::iter::repeat_n(0.1, BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
    }

    /// Samples numbered in the order they were played, so what a backlog
    /// keeps says which ones did.
    fn counted(count: usize) -> impl Iterator<Item = f32> {
        (0..count).map(|sample| sample as f32)
    }

    #[test]
    fn a_sample_that_is_not_a_number_becomes_silence() {
        let mut input = Input::default();
        input.push([0.25, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5]);
        assert_eq!(
            input.samples.len(),
            5,
            "a replaced sample keeps its place in the frame"
        );
        assert!(
            input.samples.iter().all(|sample| sample.is_finite()),
            "nothing Opus can't encode gets past the boundary"
        );
        assert_eq!(input.samples[0], 0.25);
        assert_eq!(input.samples[1], 0.0);
        assert_eq!(input.samples[2], 0.0);
        assert_eq!(input.samples[4], 0.5);
    }

    #[test]
    fn keeps_the_newest_samples_of_a_huge_callback() {
        let mut input = Input::default();
        input.push(counted(BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(
            input.samples[0],
            (BACKLOG * 2) as f32,
            "the oldest go first"
        );
        assert_eq!(input.samples[BACKLOG - 1], (BACKLOG * 3 - 1) as f32);
        for (older, newer) in input.samples.iter().zip(input.samples.iter().skip(1)) {
            assert_eq!(newer - older, 1.0, "kept samples stay in the order played");
        }
    }

    #[test]
    fn a_huge_callback_needs_no_more_room_than_the_backlog() {
        let mut input = Input::default();
        input.push(counted(BACKLOG * 3));
        let room = input.samples.capacity();
        assert!(room >= BACKLOG, "room for the backlog it keeps");
        for scale in [10, 100] {
            input.push(std::iter::repeat_n(0.0, BACKLOG * scale));
            assert_eq!(input.samples.len(), BACKLOG);
            assert_eq!(
                input.samples.capacity(),
                room,
                "a bigger callback reuses the room instead of growing it"
            );
        }
    }

    #[test]
    fn pushes_that_cross_the_bound_keep_the_newest() {
        let mut input = Input::default();
        input.push(counted(BACKLOG - 2));
        assert_eq!(input.samples.len(), BACKLOG - 2);
        input.push(std::iter::repeat_n(-1.0, 2));
        assert_eq!(
            input.samples.len(),
            BACKLOG,
            "exactly full, nothing dropped yet"
        );
        assert_eq!(input.samples[0], 0.0);
        input.push(std::iter::repeat_n(-2.0, 3));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(input.samples[0], 3.0, "the three oldest went");
        assert_eq!(input.samples[BACKLOG - 1], -2.0, "the newest is still last");
    }

    #[test]
    fn an_empty_push_changes_nothing() {
        let mut input = Input::default();
        input.push(std::iter::empty());
        assert!(input.samples.is_empty());
        input.push(counted(PRIMED));
        input.push(std::iter::empty());
        assert_eq!(
            input.samples.len(),
            PRIMED,
            "waiting sound is not disturbed"
        );
        assert_eq!(input.samples[0], 0.0);
        assert_eq!(input.samples[PRIMED - 1], (PRIMED - 1) as f32);
    }

    #[test]
    fn an_exact_bound_push_keeps_all_of_it() {
        let mut input = Input::default();
        input.push(counted(BACKLOG));
        assert_eq!(input.samples.len(), BACKLOG);
        assert_eq!(input.samples[0], 0.0);
        assert_eq!(input.samples[BACKLOG - 1], (BACKLOG - 1) as f32);
    }
}
