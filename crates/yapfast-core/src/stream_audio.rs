//! Go Live's sound. Sharing: what every other application plays, mixed
//! into 20 ms Opus frames for the stream. Yapfast's own sound is left out, so
//! viewers never hear the call echoed back, as Discord's client and Vesktop
//! do, and so are other Discord clients': their sound is a call too, which a
//! stream would echo back. [`crate::audio_io::others`] hears the applications.
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
    stream_connection::{Sound, SoundSender},
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
    fn push(&mut self, samples: impl IntoIterator<Item = f32>) {
        self.samples.extend(samples);
        let excess = self.samples.len().saturating_sub(BACKLOG);
        self.samples.drain(..excess);
    }
}

/// Each monitored application's sound, by the system's number for it.
type Inputs = Arc<Mutex<HashMap<u32, Input>>>;

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
        *sample = sample.clamp(-1.0, 1.0);
    }
    heard
}

/// Hears the other applications into `inputs` until `stop`.
fn listen(inputs: Inputs, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    crate::audio_io::others(
        is_discord_client,
        move |heard| {
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
        },
        stop,
    )
}

/// Captures and sends the stream's sound until dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Capture {
    /// Starts capturing other applications' sound and sending it as Opus.
    pub fn start(sound: SoundSender) -> Result<Self> {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), stop.clone())?;
        let mut encoder = opus::Encoder::new(2, opus::Application::Audio, BITRATE)?;
        let stopping = stop.clone();
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
                                tracing::warn!(%error, "could not encode the stream's sound")
                            }
                        }
                    }
                    timestamp = timestamp.wrapping_add((FRAME / CHANNELS) as u32);
                }
            })
            .context("could not start mixing the stream's sound")?;
        Ok(Self {
            stop,
            threads: vec![capture, mixer],
        })
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

/// Plays a watched stream's sound until its Opus frames end or it's
/// dropped, on the speakers the call uses.
pub struct Player {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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

impl Player {
    /// Plays `sounds` on `speakers`, or the default output, at `volume`, while
    /// `hearing` is set: being deafened silences streams too.
    pub fn start(
        sounds: std::sync::mpsc::Receiver<Vec<u8>>,
        speakers: Option<String>,
        hearing: Arc<AtomicBool>,
        volume: Arc<Volume>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = play(sounds, speakers, hearing, volume, stop.clone())?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn play(
    sounds: std::sync::mpsc::Receiver<Vec<u8>>,
    speakers: Option<String>,
    hearing: Arc<AtomicBool>,
    volume: Arc<Volume>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    use std::sync::mpsc;
    let (ready, started) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("yapfast-stream-player".into())
        .spawn(move || {
            // About 100 ms queued, which rides out the network's jitter.
            let output = LastingOutput::<i16>::open(
                speakers.as_deref(),
                "Stream sound",
                RATE,
                CHANNELS as u16,
                Some(Duration::from_millis(100)),
            );
            let mut output = match output {
                Ok(output) => {
                    let _ = ready.send(Ok(()));
                    output
                }
                Err(error) => {
                    let _ = ready.send(Err(error.context("could not open the speakers")));
                    return;
                }
            };
            let mut decoder = match opus::Decoder::new(2) {
                Ok(decoder) => decoder,
                Err(error) => {
                    tracing::warn!(%error, "could not start the Opus decoder");
                    return;
                }
            };
            // Opus's longest frame, 120 ms.
            let mut pcm = vec![0i16; RATE as usize / 1000 * 120 * CHANNELS];
            while !stop.load(Ordering::Relaxed) {
                let opus = match sounds.recv_timeout(Duration::from_millis(100)) {
                    Ok(opus) => opus,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                if !hearing.load(Ordering::Relaxed) {
                    continue;
                }
                match decoder.decode(&opus, &mut pcm, false) {
                    Ok(samples) => {
                        let pcm = &mut pcm[..samples * CHANNELS];
                        scale(pcm, volume.get());
                        output.write(pcm);
                    }
                    Err(error) => tracing::debug!(%error, "a stream sound frame didn't decode"),
                }
            }
        })
        .context("could not start playing the stream's sound")?;
    started
        .recv()
        .context("the stream's sound stopped before it started")??;
    Ok(thread)
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
        // The first ran dry: it waits again, and loud sums clip.
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
        assert_eq!(out[0], 1.0);
        inputs.clear();
        assert!(!mix(&mut inputs, &mut out), "silence sends nothing");
    }

    /// Plays a quiet tone with paplay and hears it through the monitor:
    /// `YAPFAST_TONE=/path/to/tone.wav cargo test -- --ignored captures_`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn captures_what_another_application_plays() {
        let tone = std::env::var("YAPFAST_TONE").expect("YAPFAST_TONE names a WAV file");
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), stop.clone()).unwrap();
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
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn leaves_out_yapfasts_own_sound() {
        let inputs = Inputs::default();
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), stop.clone()).unwrap();
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

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn plays_a_streams_sound_until_it_ends() {
        let (sounds, heard) = std::sync::mpsc::sync_channel(32);
        for _ in 0..20 {
            sounds.send(vec![0xf8, 0xff, 0xfe]).unwrap();
        }
        drop(sounds);
        let started = Instant::now();
        let mut player = Player::start(
            heard,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Volume::new(1.0)),
        )
        .unwrap();
        player.thread.take().unwrap().join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn keeps_a_bounded_backlog() {
        let mut input = Input::default();
        input.push(std::iter::repeat_n(0.1, BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
    }
}
