//! `stream_audio.rs`'s capture.

use super::*;

pub(crate) const CHANNELS: usize = 2;
/// Samples of every channel in one 20 ms frame.
pub(crate) const FRAME: usize = RATE as usize / 50 * CHANNELS;
/// Samples per channel in one 20 ms frame, a turn of the player.
pub(crate) const SAMPLES: usize = FRAME / CHANNELS;
/// What Discord's client gives a stream's sound.
pub(crate) const BITRATE: i32 = 64_000;

/// Whether an application, by its name or program, is Yapfast or a Discord
/// client, whose sound is a call or a stream: Yapfast's other workers, such as
/// one playing a watched stream, and Discord's own client and those built
/// on it.
pub(crate) fn is_discord_client(name: &str) -> bool {
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
pub(crate) struct Input {
    pub(crate) samples: VecDeque<f32>,
    pub(crate) primed: bool,
    /// The process playing it, when the system says.
    pub(crate) process: Option<u32>,
}

impl Input {
    /// Adds sound to the backlog, keeping only the newest [`BACKLOG`]
    /// samples: the oldest go first, so a long callback's chunk needs no more
    /// room than its last [`BACKLOG`] samples ever take. Gives how many it
    /// dropped, which is a capture running faster than the mix.
    pub(crate) fn push(&mut self, samples: impl IntoIterator<Item = f32>) -> u64 {
        let mut dropped = 0;
        for sample in samples {
            // A driver that lost its place can hand over a number that isn't
            // one, and the mix would carry it: a clamp returns a NaN as it
            // came, and Opus has no way to answer for it. Silence costs one
            // sample of one application, which the rest of the frame covers.
            let sample = if sample.is_finite() { sample } else { 0.0 };
            if self.samples.len() >= BACKLOG {
                self.samples.pop_front();
                dropped += 1;
            }
            self.samples.push_back(sample);
        }
        dropped
    }

    /// The sound waiting to be mixed, in samples of one channel.
    pub(crate) fn waiting(&self) -> u32 {
        u32::try_from(self.samples.len() / CHANNELS).unwrap_or(u32::MAX)
    }
}

/// What a sharing stream's sound did, for the stream's report: the sound a
/// backlog holds, which is delay before viewers hear it, and the sound a full
/// one dropped rather than mix late.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoundStats {
    /// The most any application's sound waits to be mixed, in milliseconds.
    pub held_ms: u32,
    /// Sound a full backlog dropped since the capture started, in
    /// milliseconds of one channel.
    pub dropped_ms: u64,
}

/// [`SoundStats`] while the capture runs, written by the mixer and its
/// listener and read when the stream reports.
#[derive(Default)]
pub(crate) struct Counted {
    /// Samples of every channel waiting: converted on reading, so a short
    /// wait doesn't round away.
    pub(crate) held: AtomicU32,
    pub(crate) dropped: AtomicU64,
}

impl Counted {
    /// Records how much one application's sound waits, in samples of one
    /// channel as [`Input::waiting`] gives it, which every channel's is
    /// converted from on reading.
    pub(crate) fn hold(&self, waiting: u32) {
        self.held
            .store(waiting * CHANNELS as u32, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> SoundStats {
        let milliseconds = |samples: u64| samples * 1000 / (RATE as u64 * CHANNELS as u64);
        SoundStats {
            held_ms: u32::try_from(milliseconds(u64::from(self.held.load(Ordering::Relaxed))))
                .unwrap_or(u32::MAX),
            dropped_ms: milliseconds(self.dropped.load(Ordering::Relaxed)),
        }
    }
}

/// Each monitored application's sound, by the system's number for it.
pub(crate) type Inputs = Arc<Mutex<HashMap<u32, Input>>>;

/// A mixed sample, bent rather than clipped at full scale: up to [`KNEE`]
/// exactly as it summed, and above it towards full scale without reaching it.
/// The bend is what several loud applications adding up need, since their sum
/// would otherwise flat-top at the first sample past full scale and tear the
/// sound; 1.4 comes out at 0.98 of full scale instead. Slope is one where the
/// bend starts, so nothing jumps where it begins, and the curve carries
/// nothing from frame to frame: a loud passage can't pull down the sound after
/// it, and the bend is not a gain heard as the mix pumping.
pub(crate) fn bend(sample: f32) -> f32 {
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
pub(crate) fn mix(inputs: &mut HashMap<u32, Input>, out: &mut [f32]) -> bool {
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
pub(crate) fn listen(
    inputs: Inputs,
    counted: Arc<Counted>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
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
                        let dropped = input.push(samples.iter().copied());
                        counted.dropped.fetch_add(dropped, Ordering::Relaxed);
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
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) counted: Arc<Counted>,
    pub(crate) threads: Vec<JoinHandle<()>>,
}

impl Capture {
    /// Starts capturing other applications' sound and sending it as Opus.
    pub fn start(sound: SoundSender) -> Result<Self> {
        capture(
            Inputs::default(),
            Arc::new(Counted::default()),
            || opus::Encoder::new(2, opus::Application::Audio, BITRATE),
            listen,
            move |encoder, stopping, counting, inputs| {
                spawn_mixer(encoder, stopping, counting, inputs, sound)
            },
        )
    }

    /// What the captured sound did so far, for the stream's report.
    pub fn stats(&self) -> SoundStats {
        self.counted.read()
    }
}

/// A capture built a step at a time, each fallible step handed in so a test
/// can make one fail and watch what the ones before it leave behind: `open`
/// makes the codec, `listen` hears the machine into `inputs`, and `mix`
/// starts sending what it heard. A step that fails after another started a
/// thread must stop it before the error goes back — a join handle dropped on
/// its own only detaches its thread, and the listener's holds the operating
/// system's capture and its buffers for as long as the stream's worker lives.
pub(crate) fn capture(
    inputs: Inputs,
    counted: Arc<Counted>,
    open: impl FnOnce() -> Result<opus::Encoder>,
    listen: impl FnOnce(Inputs, Arc<Counted>, Arc<AtomicBool>) -> Result<JoinHandle<()>>,
    mix: impl FnOnce(
        opus::Encoder,
        Arc<AtomicBool>,
        Arc<Counted>,
        Inputs,
    ) -> std::io::Result<JoinHandle<()>>,
) -> Result<Capture> {
    // The codec first: building it starts nothing, so it is the one step
    // whose failure has nothing to undo. It used to run after the capture had
    // already begun hearing the machine.
    let encoder = open()?;
    let mut started = Started::new();
    started.begin(|stop| listen(inputs.clone(), Arc::clone(&counted), stop))?;
    let counting = Arc::clone(&counted);
    started.begin(move |stop| {
        mix(encoder, stop, counting, inputs).context("could not start mixing the stream's sound")
    })?;
    let (stop, threads) = started.keep();
    Ok(Capture {
        stop,
        counted,
        threads,
    })
}

/// The threads a [`Capture`]'s start has begun, which stop if it never
/// arrives: without a [`Capture`] to own them, a failed start would leave
/// them running with nothing left to stop them.
pub(crate) struct Started {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) threads: Vec<JoinHandle<()>>,
}

impl Started {
    pub(crate) fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::with_capacity(2),
        }
    }

    /// Starts the one thread `begin` makes, which runs while the stop isn't
    /// set, and keeps it for the capture this becomes.
    pub(crate) fn begin(
        &mut self,
        begin: impl FnOnce(Arc<AtomicBool>) -> Result<JoinHandle<()>>,
    ) -> Result<()> {
        self.threads.push(begin(Arc::clone(&self.stop))?);
        Ok(())
    }

    /// Gives the threads and the stop that ends them to the [`Capture`] that
    /// now owns stopping them.
    pub(crate) fn keep(mut self) -> (Arc<AtomicBool>, Vec<JoinHandle<()>>) {
        (
            std::mem::take(&mut self.stop),
            std::mem::take(&mut self.threads),
        )
    }
}

/// A start that didn't finish stops every thread it began, and waits for each
/// to let go of what it held, before its error reaches the caller.
impl Drop for Started {
    fn drop(&mut self) {
        if self.threads.is_empty() {
            return;
        }
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Spawns the mixer: what the applications played, mixed and encoded into
/// Opus every 20 ms until `stopping`.
pub(crate) fn spawn_mixer(
    encoder: opus::Encoder,
    stopping: Arc<AtomicBool>,
    counting: Arc<Counted>,
    inputs: Inputs,
    sound: SoundSender,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("yapfast-stream-sound".into())
        .spawn(move || {
            let mut encoder = encoder;
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
                let mut inputs = inputs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                // What waits before this turn is mixed, which is the delay a
                // viewer would hear; the mix then takes a frame from each
                // application that has one.
                let waiting = inputs.values().map(Input::waiting).max().unwrap_or(0);
                let heard = mix(&mut inputs, &mut frame);
                drop(inputs);
                counting.hold(waiting);
                if heard {
                    match encoder.encode_float(&frame, &mut opus) {
                        Ok(size) => {
                            // The frame goes into a buffer the connection
                            // hands back once it's on the wire: the mixer runs
                            // every 20 ms, and copying a frame into a buffer it
                            // keeps is cheaper here than allocating one for each
                            // of them.
                            let mut bytes = sound.buffer();
                            bytes.extend_from_slice(&opus[..size]);
                            sound.send(Sound {
                                opus: bytes,
                                timestamp,
                            });
                        }
                        Err(error) => {
                            tracing::warn!(%error, "could not encode the stream's sound")
                        }
                    }
                }
                timestamp = timestamp.wrapping_add((FRAME / CHANNELS) as u32);
            }
        })
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
