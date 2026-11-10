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
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use yapfast_discord::{
    opus, rtp,
    stream_connection::{
        ReceivedSound, SenderReport, Sound, SoundReceiver, SoundSender, StreamClock,
    },
    voice::{QUIET, Surplus},
};

use crate::audio_io::{Heard, LastingOutput, RATE};

const CHANNELS: usize = 2;
/// Samples of every channel in one 20 ms frame.
const FRAME: usize = RATE as usize / 50 * CHANNELS;
/// Samples per channel in one 20 ms frame, a turn of the player.
const SAMPLES: usize = FRAME / CHANNELS;
/// An application's sound joins the mix once this much waits, which
/// absorbs the system delivering it in chunks.
const PRIMED: usize = FRAME * 2;
/// The most kept for an application; older sound is dropped, so clock
/// drift never grows into delay.
const BACKLOG: usize = FRAME * 10;
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
/// What Discord's client gives a stream's sound.
const BITRATE: i32 = 64_000;
/// A player's turn: 20 ms of sound, then the next.
const TURN: Duration = Duration::from_millis(20);

// How long a watched stream's sound waits and how long it gives a packet
// that hasn't come. Starting points to tune on a real link, not guarantees:
// nothing has measured them against Discord's delivery yet.
/// A stream's sound starts once this many frames wait: 60 ms.
const START: usize = 3;
/// With a single frame, playing starts this long after it came.
const PRIME: Duration = Duration::from_millis(100);
/// Frames wait at most. Each takes the room of one Opus frame, so this is
/// what bounds memory, not delay: a music bot's packet can carry six turns.
const MOST: usize = 10;
/// Sound waits at most, in samples per channel: 200 ms, so a stall never
/// turns into lasting delay. Ten 20 ms frames are that much, but a music
/// bot's sixty-millisecond ones are three times it, and they'd wait just as
/// quietly: the time frames hold is what says when the queue is too long.
const MOST_SAMPLES: u32 = RATE / 1000 * 200;
/// A missing packet is waited for this long before its slot is concealed or
/// dropped: 60 ms.
const LATE: Duration = Duration::from_millis(60);
/// Packets missing in a row after which the sender has stopped: 100 ms.
/// Concealing stops there, and silence plays until sound comes again.
const GONE: u8 = 5;
/// A frame that waited in the queue longer than this came during a stall and
/// is dropped rather than played late: 300 ms.
const STALE: Duration = Duration::from_millis(300);
/// The furthest a frame's timestamp may fall from where the last played one
/// left the stream: a second, in samples. Beyond, the sender began again.
const JUMP: i32 = 48_000;
/// The streamer's picture clock: RTP timestamps on a stream's video SSRC
/// count 90 kHz ticks.
const PICTURE_RATE: u32 = 90_000;
/// What the NTP time kept in a sender report spans: its middle 32 bits are
/// seconds in 16.16, so the seconds they count turn over every 18 hours.
const NTP_TURN: f64 = 65_536.0;
/// Printed at most this often: what waits, and how much of what came was
/// lost, trimmed, skipped or stopped short.
const LOG_EVERY: Duration = Duration::from_secs(5);

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
    /// room than its last [`BACKLOG`] samples ever take. Gives how many it
    /// dropped, which is a capture running faster than the mix.
    fn push(&mut self, samples: impl IntoIterator<Item = f32>) -> u64 {
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
    fn waiting(&self) -> u32 {
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
struct Counted {
    /// Samples of every channel waiting: converted on reading, so a short
    /// wait doesn't round away.
    held: AtomicU32,
    dropped: AtomicU64,
}

impl Counted {
    /// Records how much one application's sound waits, in samples of one
    /// channel as [`Input::waiting`] gives it, which every channel's is
    /// converted from on reading.
    fn hold(&self, waiting: u32) {
        self.held
            .store(waiting * CHANNELS as u32, Ordering::Relaxed);
    }

    fn read(&self) -> SoundStats {
        let milliseconds = |samples: u64| samples * 1000 / (RATE as u64 * CHANNELS as u64);
        SoundStats {
            held_ms: u32::try_from(milliseconds(u64::from(self.held.load(Ordering::Relaxed))))
                .unwrap_or(u32::MAX),
            dropped_ms: milliseconds(self.dropped.load(Ordering::Relaxed)),
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
fn listen(inputs: Inputs, counted: Arc<Counted>, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
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
    stop: Arc<AtomicBool>,
    counted: Arc<Counted>,
    threads: Vec<JoinHandle<()>>,
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
fn capture(
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
struct Started {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Started {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::with_capacity(2),
        }
    }

    /// Starts the one thread `begin` makes, which runs while the stop isn't
    /// set, and keeps it for the capture this becomes.
    fn begin(
        &mut self,
        begin: impl FnOnce(Arc<AtomicBool>) -> Result<JoinHandle<()>>,
    ) -> Result<()> {
        self.threads.push(begin(Arc::clone(&self.stop))?);
        Ok(())
    }

    /// Gives the threads and the stop that ends them to the [`Capture`] that
    /// now owns stopping them.
    fn keep(mut self) -> (Arc<AtomicBool>, Vec<JoinHandle<()>>) {
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
fn spawn_mixer(
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

/// Plays a watched stream's sound until its Opus frames end or it's
/// dropped, on the speakers the call uses.
pub struct Player {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Frames the speakers have played and frames they still hold queued, as
    /// the player's own thread last read them from the device: where the
    /// sound really is, which is what A/V timing is measured from.
    played: Arc<std::sync::atomic::AtomicU64>,
    queued: Arc<std::sync::atomic::AtomicU64>,
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

/// What happened to a frame that came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Landing {
    InOrder,
    /// It filled a gap the playhead hadn't reached yet.
    Reordered,
    /// Its slot played already, or it waited in the queue through a stall.
    Late,
    /// The same packet twice.
    Duplicate,
    /// The sender began again: another SSRC, or a timestamp far off.
    Restart,
}

/// What made the sound a [`Turn::Play`] plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Played {
    /// A packet that came.
    Frame,
    /// Made up for a packet that didn't come: by the next packet's forward
    /// error correction, or by Opus's own concealment.
    Concealed,
    /// Nothing came for long enough: silence, rather than made-up sound.
    Silence,
}

/// What the player does with its next turn.
#[derive(Debug, PartialEq, Eq)]
enum Turn {
    /// Writes `samples` per channel, starting `at` samples into
    /// [`Playout::pcm`]: a packet longer than a turn plays from further in on
    /// each of its turns, so what plays is the next 20 ms and not the first
    /// one again.
    Play {
        at: usize,
        samples: usize,
        of: Played,
    },
    /// Nothing can play yet: waits at most this long for real sound.
    Wait(Duration),
}

/// Why the playhead had no packet of its own to play.
#[derive(Clone, Copy)]
enum Missing {
    /// The sender's numbering skipped it while still sending: packet loss.
    Lost,
    /// Nothing waited at all: the playhead outran the sender, which stalled
    /// or whose clock runs slow. Sound comes next, just after its slot.
    Starved,
    /// It came, and Opus couldn't read it.
    Broken,
    /// The queue's bound dropped it before its turn: too much sound came for
    /// the time the queue may hold, which a sender running fast does.
    Trimmed,
}

/// What a stream's sound did, for the log. Loss and the several ways sound
/// goes unheard without being lost are separate, since a gap in a sender's
/// numbering and a queue it overruns mean opposite things.
#[derive(Debug, Default)]
struct Counts {
    /// Frames that came, and how many of those played.
    came: u64,
    played: u64,
    /// Turns made up for a sequence number the sender skipped while still
    /// sending: packet loss, which a gap in its numbering is.
    lost: u64,
    /// Turns the playhead had nothing at all to play. A slow sender leaves
    /// these at intervals instead of skipping numbers, so for one this grows
    /// and `lost` doesn't.
    starved: u64,
    /// Turns made up for a packet that came and Opus couldn't read, which is
    /// a codec fault rather than the link's.
    broken: u64,
    /// Frames a bound dropped before their turn, for the room they took or
    /// the time they would have added to the delay. A sender running fast
    /// meets the bound with frames still coming; a lossy one meets it never.
    trimmed: u64,
    /// Frames that came after their slot had played, or waited through a
    /// stall: sound that was late, which a slow sender's is.
    late: u64,
    duplicate: u64,
    /// Frames dropped unheard for the room a queue had to spare: the drift a
    /// sender running fast adds, taken a turn at a time.
    skipped: u64,
    /// Times the sender began again.
    restarts: u64,
}

/// Where `timestamp` on `report`'s clock sits on wall-clock time, in seconds
/// on the middle 32 bits of NTP: the report's own instant plus the ticks
/// between its timestamp and this one, at `rate` ticks a second. Read
/// signed, so a timestamp on the far side of the clock's wrap is a small
/// step rather than four billion ticks.
fn at_wall_time(report: SenderReport, timestamp: u32, rate: u32) -> f64 {
    let ticks = timestamp.wrapping_sub(report.rtp) as i32;
    f64::from(report.ntp) / NTP_TURN + f64::from(ticks) / f64::from(rate)
}

/// How far behind the picture the sound at `sound` is, in seconds, each a
/// timestamp on its own clock together with the sender report that says
/// where that clock's timestamps sit on wall-clock time: the two are only
/// comparable once each is placed there, since 48 kHz and 90 kHz counts
/// share no zero and no rate. `None` while either is missing, since which
/// clock an unmapped timestamp ran on is a guess.
fn behind(sound: Option<(SenderReport, u32)>, picture: Option<(SenderReport, u32)>) -> Option<f64> {
    let (sound_report, sound_timestamp) = sound?;
    let (picture_report, picture_timestamp) = picture?;
    let sound = at_wall_time(sound_report, sound_timestamp, RATE);
    let picture = at_wall_time(picture_report, picture_timestamp, PICTURE_RATE);
    let mut behind = picture - sound;
    // The NTP time kept is its middle 32 bits, so its seconds turn over
    // every 18 hours: two reports seconds apart can sit either side of that
    // turn. Half the turn is far more than any real skew, so a difference
    // wider than that is the turn, not the clocks.
    if behind > NTP_TURN / 2.0 {
        behind -= NTP_TURN;
    } else if behind < -NTP_TURN / 2.0 {
        behind += NTP_TURN;
    }
    Some(behind)
}

/// A watched stream's sound on its way to the speakers: frames in sequence
/// order, held a little for the ones that come late, played 20 ms a turn, and
/// concealed while a packet is missing. It's given its clock, so a test drives
/// it without waiting.
///
/// The frames it holds are network jitter buffering, separate from the ~100 ms
/// of device buffer [`LastingOutput`] keeps.
struct Playout {
    decoder: opus::Decoder,
    /// Frames in sequence order, oldest first, at most [`MOST`].
    waiting: VecDeque<ReceivedSound>,
    /// The sequence number to play next, while playing; none until primed.
    next: Option<u16>,
    /// The timestamp the next packet should hold, from the last one played.
    expected: Option<u32>,
    /// Where sound plays from `pcm`, for packets longer than 20 ms.
    left: std::ops::Range<usize>,
    /// Room for Opus's longest frame, 120 ms.
    pcm: Vec<i16>,
    /// The SSRC the frames came on; another is another stream, or a restart.
    ssrc: Option<u32>,
    /// When the first frame of a (re)start waited, and when the playhead
    /// first found its packet missing.
    prime: Option<Instant>,
    hold: Option<Instant>,
    /// Packets missing in a row.
    missing: u8,
    /// Samples per channel of the last frame decoded: a frame's timestamp says
    /// where it begins, not how long it plays, so what the queue holds counts
    /// the last one's length. Zero until one has decoded.
    packet: u32,
    /// The stretch of sequence numbers a bound emptied, first to last. The
    /// playhead conceals their slots at once rather than waiting [`LATE`] for
    /// sound it knows is gone, and can tell that trimming from the sender's
    /// loss.
    dropped: Option<(u16, u16)>,
    /// Frames taken a little faster than they play.
    surplus: Surplus,
    counts: Counts,
}

impl Playout {
    fn new() -> Result<Self> {
        Ok(Self {
            decoder: opus::Decoder::new(CHANNELS as u16)?,
            waiting: VecDeque::with_capacity(MOST + 1),
            next: None,
            expected: None,
            left: 0..0,
            pcm: vec![0; RATE as usize / 1000 * 120 * CHANNELS],
            ssrc: None,
            prime: None,
            hold: None,
            missing: 0,
            packet: 0,
            dropped: None,
            surplus: Surplus::default(),
            counts: Counts::default(),
        })
    }

    /// Queues a frame that came at `now`.
    fn push(&mut self, frame: ReceivedSound, now: Instant) -> Landing {
        self.counts.came += 1;
        // Another SSRC is another stream, or the sender starting again: its
        // sequence numbers say nothing about the old ones', so this comes
        // before any comparison below.
        if self.ssrc.is_some_and(|ssrc| ssrc != frame.ssrc) {
            self.restart(frame, now);
            return Landing::Restart;
        }
        self.ssrc = Some(frame.ssrc);
        // A timestamp a second from where the last played one left the
        // stream is a sender that began again. One that wrapped is not: read
        // signed, its difference is small.
        if let Some(expected) = self.expected {
            let jump = frame.timestamp.wrapping_sub(expected) as i32;
            if jump.unsigned_abs() > JUMP.unsigned_abs() {
                self.restart(frame, now);
                return Landing::Restart;
            }
        }
        // It waited in the queue through a stall: playing it now would only
        // add to the delay.
        if now.saturating_duration_since(frame.arrived) > STALE {
            if self.next.is_some() {
                // A stalled player starts again rather than concealing the
                // gap it fell behind by.
                self.reset();
            }
            self.counts.late += 1;
            return Landing::Late;
        }
        // Its slot played already: it can never play now. One for the slot
        // the playhead is missing is the packet the hold is waiting for, so
        // it's queued.
        if self
            .next
            .is_some_and(|next| rtp::before(frame.sequence, next))
        {
            self.counts.late += 1;
            return Landing::Late;
        }
        self.queue(frame, now)
    }

    /// Starts the playout again with `frame`: a new SSRC, or a timestamp that
    /// began again, leaves nothing waiting that could still play.
    fn restart(&mut self, frame: ReceivedSound, now: Instant) {
        self.reset();
        self.ssrc = Some(frame.ssrc);
        self.counts.restarts += 1;
        self.queue(frame, now);
    }

    /// Inserts a frame in sequence order, then lets go of what's over a bound.
    fn queue(&mut self, frame: ReceivedSound, now: Instant) -> Landing {
        let at = self
            .waiting
            .iter()
            .rposition(|waiting| !rtp::before(frame.sequence, waiting.sequence))
            .map_or(0, |at| at + 1);
        if at > 0 && self.waiting[at - 1].sequence == frame.sequence {
            self.counts.duplicate += 1;
            return Landing::Duplicate;
        }
        let landing = if at == self.waiting.len() {
            Landing::InOrder
        } else {
            Landing::Reordered
        };
        self.waiting.insert(at, frame);
        if self.next.is_none() {
            // A (re)start: the first frame begins the wait to play.
            self.prime.get_or_insert(now);
        }
        // Two bounds, because one is about memory and the other about delay:
        // frames for the room they take, sound for the time it takes to play.
        // A 20 ms stream meets both at the same depth; a music bot's packets
        // meet the second far sooner, which is what keeps its delay from
        // being three times as long.
        while self.waiting.len() > MOST && self.discharge() {}
        while self.held() > MOST_SAMPLES && self.discharge() {}
        landing
    }

    /// The sound waiting, in samples per channel: from where the next turn's
    /// sound begins to the end of the newest frame. Timestamps say where
    /// frames belong, so a packet that plays over three turns is counted as
    /// the three turns it takes, and the rest of one playing now is already
    /// in `expected`.
    fn held(&self) -> u32 {
        let (Some(front), Some(back)) = (self.waiting.front(), self.waiting.back()) else {
            return 0;
        };
        let from = match (self.next, self.expected) {
            // Playing: what's held comes after the sound already accounted
            // for, this turn's and the next turns' of a long packet.
            (Some(_), Some(expected)) => expected,
            // Not playing: nothing has played, so every frame that waits is
            // held.
            _ => front.timestamp,
        };
        // What the last decode made says how long a frame plays. Before
        // anything has decoded, the queue's own spacing does: a stream's
        // packets are as long as the one before them.
        let length = match self.packet {
            0 => self
                .waiting
                .iter()
                .rev()
                .nth(1)
                .map_or(0, |before| back.timestamp.wrapping_sub(before.timestamp)),
            packet => packet,
        };
        let held = back.timestamp.wrapping_add(length).wrapping_sub(from) as i32;
        held.max(0) as u32
    }

    /// How far the sound at the playhead sits behind the newest picture that
    /// came, in seconds, both placed on wall-clock time by the streamer's
    /// sender reports: `None` while either report, the picture, or the
    /// playhead's own timestamp is missing.
    fn behind(&self, clock: &StreamClock) -> Option<f64> {
        // Not playing: the playhead's timestamp says nothing about what a
        // viewer hears, and silence is not the sound falling behind.
        self.next?;
        behind(
            clock.audio().zip(self.expected),
            clock.video().zip(clock.picture()),
        )
    }

    /// Drops one frame from the end the queue is too long at, and says
    /// whether it could. While sound plays, the newest goes: what the sound
    /// waits through is the slots after the playhead's own, and a hole
    /// further in costs the turn its frame would have taken, so dropping a
    /// frame there shortens nothing — while the playhead's own packet going
    /// is the jump cut this is here to avoid. Before it starts, the oldest
    /// go instead: everything waiting will play, and the newest is what came
    /// closest to now.
    fn discharge(&mut self) -> bool {
        let at = if self.next.is_some() {
            match self.waiting.len() {
                0 => return false,
                // Alone, the playhead's own packet is what plays next, and
                // dropping it is the jump cut this is here to avoid.
                1 if self.next == self.waiting.front().map(|front| front.sequence) => return false,
                // The newest goes, including the single frame that came too
                // early for a playhead still missing its own.
                _ => self.waiting.len() - 1,
            }
        } else {
            // Nothing has played: the oldest goes, since everything waiting
            // will play and the newest is what came closest to now.
            if self.waiting.is_empty() {
                return false;
            }
            0
        };
        let Some(gone) = self.waiting.remove(at) else {
            return false;
        };
        self.counts.trimmed += 1;
        if self
            .next
            .is_some_and(|next| rtp::before(next, gone.sequence))
        {
            // Widen the stretch of dropped sequence numbers: the playhead
            // would wait [`LATE`] for a hole a bound made itself, and their
            // slots are the queue's trimming rather than the sender's loss.
            self.dropped = Some(match self.dropped {
                Some((first, last)) if rtp::before(gone.sequence, first) => (gone.sequence, last),
                Some((first, last)) if rtp::before(last, gone.sequence) => (first, gone.sequence),
                // A burst drops the newest in order, which a stretch already
                // covers; one inside it changes nothing.
                Some(stretch) => stretch,
                None => (gone.sequence, gone.sequence),
            });
        }
        true
    }

    /// Whether the playhead's missing packet, `sequence`, is one a bound
    /// emptied rather than one the sender lost: the queue's own doing, not
    /// the link's. Forgets a stretch once the playhead has played past it, so
    /// a run of loss can't hide behind an old trim.
    fn trimmed(&mut self, sequence: u16) -> bool {
        let Some((first, last)) = self.dropped else {
            return false;
        };
        if rtp::before(last, sequence) {
            self.dropped = None;
            return false;
        }
        !rtp::before(sequence, first)
    }

    /// The next 20 ms into `self.pcm`, or how long to wait for something to
    /// play.
    fn next(&mut self, now: Instant) -> Turn {
        // A packet longer than 20 ms, as music bots send, plays over as many
        // turns as it takes.
        if !self.left.is_empty() {
            return self.turn(Played::Frame);
        }
        if self.next.is_none() {
            // Not playing: starts once a few frames wait, or one has waited
            // long enough that nothing more is coming.
            let primed = self.waiting.len() >= START
                || self
                    .prime
                    .is_some_and(|prime| now.saturating_duration_since(prime) >= PRIME);
            let Some(front) = self.waiting.front().filter(|_| primed) else {
                return Turn::Wait(self.wait_for(now));
            };
            self.next = Some(front.sequence);
            self.prime = None;
        }
        let mut next = self.next.expect("playing");
        loop {
            // Packets whose slot played already can never play: they came too
            // late to be worth the delay of waiting for them.
            while self
                .waiting
                .front()
                .is_some_and(|frame| rtp::before(frame.sequence, next))
            {
                self.waiting.pop_front();
                self.counts.late += 1;
            }
            if self
                .waiting
                .front()
                .is_some_and(|frame| frame.sequence == next)
            {
                if let Some(turn) = self.play(next) {
                    return turn;
                }
                // The drift control took that frame whole, and no turn is
                // spent on it: the packet after it plays in this turn instead,
                // which is what shortens playback by the interval the sender
                // ran ahead by. Each drop takes a frame out of the queue and
                // puts none back, so this ends with a turn to play.
                next = self.next.expect("still playing");
                continue;
            }
            return if self.waiting.is_empty() {
                self.play_nothing(next)
            } else {
                self.play_missing(next, now)
            };
        }
    }

    /// Plays the playhead's packet, `next`: `None` when the drift control
    /// dropped it whole, so that the packet after it plays in this turn.
    fn play(&mut self, next: u16) -> Option<Turn> {
        let frame = self.waiting.pop_front().expect("the playhead");
        self.missing = 0;
        self.hold = None;
        self.next = Some(next.wrapping_add(1));
        let samples = match self.decoder.decode(&frame.opus, &mut self.pcm, false) {
            Ok(samples) => samples,
            Err(error) => {
                tracing::debug!(
                    %error,
                    sequence = frame.sequence,
                    length = frame.opus.len(),
                    "a stream sound frame didn't decode"
                );
                self.packet = SAMPLES as u32;
                self.expected = Some(frame.timestamp.wrapping_add(SAMPLES as u32));
                return Some(self.conceal(Missing::Broken));
            }
        };
        self.expected = Some(frame.timestamp.wrapping_add(samples as u32));
        if samples == 0 {
            return Some(self.conceal(Missing::Broken));
        }
        // How long this one plays, which the next queue bound counts.
        self.packet = samples as u32;
        self.counts.played += 1;
        self.set_left(samples);
        if self.skips(samples) {
            // The queue had frames to spare, so this one goes unheard: that is
            // what keeps the delay from growing. Nothing is left of it to
            // play, rather than a turn of silence in its place, which would
            // spend the very interval the skip is here to save and leave the
            // delay where it was.
            self.left = 0..0;
            return None;
        }
        Some(self.turn(Played::Frame))
    }

    /// The playhead's packet is missing, and nothing waits after it.
    fn play_nothing(&mut self, next: u16) -> Turn {
        self.hold = None;
        let trimmed = self.trimmed(next);
        self.missing = self.missing.saturating_add(1);
        self.next = Some(next.wrapping_add(1));
        self.expected = self
            .expected
            .map(|expected| expected.wrapping_add(SAMPLES as u32));
        if self.missing >= GONE {
            // The sender stopped, rather than a packet coming late: silence
            // while waiting for it to start again, not made-up sound for
            // ever. A stretch of slots the queue's own bound emptied ends the
            // same way, which is how a playhead far behind a fast sender gets
            // back to the sound that's here instead of chasing what it
            // dropped.
            self.next = None;
            self.missing = 0;
            self.surplus = Surplus::default();
            self.dropped = None;
            return self.silence();
        }
        self.conceal(if trimmed {
            Missing::Trimmed
        } else {
            Missing::Starved
        })
    }

    /// The playhead's packet is missing, and a later one waits: gives it
    /// [`LATE`], then makes up its 20 ms, and plays that one when its own
    /// turn comes.
    fn play_missing(&mut self, next: u16, now: Instant) -> Turn {
        // A bound emptied this slot, so it's gone for certain: it's concealed
        // now rather than waited for.
        let trimmed = self.trimmed(next);
        let missing = if trimmed {
            Missing::Trimmed
        } else {
            Missing::Lost
        };
        // Only the first gap of a run waits: the ones after it are concealed
        // at once, so a burst of loss never waits longer than one packet.
        if self.missing == 0 && !trimmed {
            let hold = *self.hold.get_or_insert(now);
            let waited = now.saturating_duration_since(hold);
            if waited < LATE {
                return Turn::Wait(LATE.saturating_sub(waited));
            }
        }
        self.hold = None;
        self.missing = self.missing.saturating_add(1);
        self.next = Some(next.wrapping_add(1));
        self.expected = self
            .expected
            .map(|expected| expected.wrapping_add(SAMPLES as u32));
        // The packet after the missing one may carry its forward error
        // correction. A stream's encoder needn't send any, and libopus says
        // nothing that tells them apart, so this is opportunistic: whatever
        // the decode leaves, that packet stays queued and plays as itself on
        // its own turn. When the decode fails, conceal instead.
        let mut recovered = None;
        if let Some(frame) = self.waiting.front()
            && frame.sequence == next.wrapping_add(1)
        {
            recovered = self
                .decoder
                .decode(&frame.opus, &mut self.pcm[..FRAME], true)
                .ok();
        }
        match recovered {
            Some(samples) if samples > 0 => {
                // The next packet's error correction made the sound up, but
                // the packet is still the one that didn't come.
                self.note(missing);
                self.set_left(samples);
                self.turn(Played::Concealed)
            }
            _ => self.conceal(missing),
        }
    }

    /// Counts a turn the playhead filled without the packet it wanted,
    /// `missing` saying why.
    fn note(&mut self, missing: Missing) {
        match missing {
            Missing::Lost => self.counts.lost += 1,
            Missing::Starved => self.counts.starved += 1,
            Missing::Broken => self.counts.broken += 1,
            // The bound counted the frame it dropped, and its slot is what
            // that count is of.
            Missing::Trimmed => {}
        }
    }

    /// 20 ms made up for a packet that didn't play, `missing` saying why.
    fn conceal(&mut self, missing: Missing) -> Turn {
        self.note(missing);
        match self.decoder.conceal(&mut self.pcm[..FRAME]) {
            Ok(samples) if samples > 0 => {
                self.set_left(samples);
                self.turn(Played::Concealed)
            }
            Ok(_) => self.silence(),
            Err(error) => {
                tracing::debug!(%error, "couldn't conceal a stream sound frame");
                self.silence()
            }
        }
    }

    /// 20 ms of silence: nothing can play.
    fn silence(&mut self) -> Turn {
        self.pcm[..FRAME].fill(0);
        self.left = 0..FRAME;
        self.turn(Played::Silence)
    }

    /// Takes the next 20 ms of `pcm`, or what's left of it, to play, and
    /// where in `pcm` it starts.
    fn turn(&mut self, of: Played) -> Turn {
        let start = self.left.start;
        let end = self.left.end.min(start + FRAME).min(self.pcm.len());
        self.left.start = end;
        Turn::Play {
            at: start,
            samples: (end - start) / CHANNELS,
            of,
        }
    }

    /// Limits what plays from `pcm` to the `samples` per channel the decoder
    /// made, clamped to the room `pcm` has.
    fn set_left(&mut self, samples: usize) {
        self.left = 0..samples.min(self.pcm.len() / CHANNELS) * CHANNELS;
    }

    /// How long to wait for sound: until the first frame has waited
    /// [`PRIME`], so one that came alone still plays.
    fn wait_for(&self, now: Instant) -> Duration {
        match self.prime {
            Some(prime) => PRIME.saturating_sub(now.saturating_duration_since(prime)),
            None => PRIME,
        }
    }

    /// Counts what waits after the playhead's packet was taken, and says
    /// whether to drop the frame it decoded to, `samples` long, for the one
    /// after: the queue had frames to spare for a whole window.
    ///
    /// This is the drift control there is: a window is 50 frames taken, a
    /// second of 20 ms ones, and a skip spends the window's spare, so no more
    /// than a turn a second goes unheard — 2 % — and only while the queue
    /// keeps frames waiting. A long packet declines it, since a skip would
    /// take more than a turn: the time bound drops whole frames of those
    /// instead, and conceals their slots.
    fn skips(&mut self, samples: usize) -> bool {
        self.surplus.note(self.waiting.len());
        let following = self.next.is_some_and(|next| {
            self.waiting
                .front()
                .is_some_and(|frame| frame.sequence == next)
        });
        // A long packet would skip more than a turn.
        if !following || samples != SAMPLES {
            return false;
        }
        let played = &self.pcm[..samples * CHANNELS];
        let power = played
            .iter()
            .map(|sample| (f32::from(*sample) / 32768.0).powi(2))
            .sum::<f32>()
            / played.len() as f32;
        let skips = self.surplus.skips(|| power < QUIET);
        if skips {
            self.counts.skipped += 1;
        }
        skips
    }

    /// Forgets what waited: deafened, or a stalled player starting again.
    fn reset(&mut self) {
        self.waiting.clear();
        self.next = None;
        self.expected = None;
        self.left = 0..0;
        self.ssrc = None;
        self.prime = None;
        self.hold = None;
        self.missing = 0;
        self.packet = 0;
        self.dropped = None;
        self.surplus = Surplus::default();
    }
}

impl Player {
    /// Plays `sounds` on `speakers`, or the default output, at `volume`, while
    /// `hearing` is set: being deafened silences streams too.
    pub fn start(
        sounds: SoundReceiver,
        speakers: Option<String>,
        hearing: Arc<AtomicBool>,
        volume: Arc<Volume>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        // Unknown until the device says: the largest value there is stands for
        // "it hasn't told us", which is not the same as playing nothing.
        let played = Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX));
        let queued = Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX));
        let thread = play(
            sounds,
            speakers,
            hearing,
            volume,
            stop.clone(),
            played.clone(),
            queued.clone(),
        )?;
        Ok(Self {
            stop,
            thread: Some(thread),
            played,
            queued,
        })
    }
}

impl Player {
    /// Where the sound is, once the device has said: the milliseconds it has
    /// played since the player started and the milliseconds it still holds
    /// queued. `None` while no device can say, which is not the same as zero.
    pub fn heard(&self) -> Option<(u32, u32)> {
        let load = |value: &std::sync::atomic::AtomicU64| {
            let frames = value.load(Ordering::Relaxed);
            (frames != u64::MAX).then_some(frames)
        };
        let (played, queued) = (load(&self.played)?, load(&self.queued)?);
        let milliseconds = |frames: u64| (frames * 1_000 / u64::from(RATE)) as u32;
        Some((milliseconds(played), milliseconds(queued)))
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

/// Where a player's turns go: the speakers, which pace them, or, in a test,
/// whatever takes them in its hands instead.
trait Speakers {
    /// Takes one turn of interleaved samples, and takes as long to play it as
    /// a device would.
    fn write(&mut self, samples: &[i16]);

    /// The frames the device has played since it opened and the frames it
    /// still holds queued, or `None` where it cannot say — which is not the
    /// same as playing nothing.
    fn position(&mut self) -> Option<(u64, u64)> {
        None
    }
}

impl Speakers for LastingOutput<i16> {
    fn write(&mut self, samples: &[i16]) {
        LastingOutput::write(self, samples);
    }

    fn position(&mut self) -> Option<(u64, u64)> {
        LastingOutput::position(self)
    }
}

fn play(
    sounds: SoundReceiver,
    speakers: Option<String>,
    hearing: Arc<AtomicBool>,
    volume: Arc<Volume>,
    stop: Arc<AtomicBool>,
    played: Arc<std::sync::atomic::AtomicU64>,
    queued: Arc<std::sync::atomic::AtomicU64>,
) -> Result<JoinHandle<()>> {
    let (ready, started) = mpsc::sync_channel(1);
    let clock = sounds.clock();
    let thread = std::thread::Builder::new()
        .name("yapfast-stream-player".into())
        .spawn(move || {
            // About 100 ms queued: the device's own buffer, which the
            // playout's priming and holding are separate from.
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
            // A stop ends the device's waits too, rather than at the next
            // turn of the loop, and never counts as the device going away.
            output.stop_with(stop.clone());
            let mut playout = match Playout::new() {
                Ok(playout) => playout,
                Err(error) => {
                    tracing::warn!(%error, "could not start the Opus decoder");
                    return;
                }
            };
            turns(
                &sounds,
                &mut playout,
                &mut output,
                &hearing,
                &volume,
                &stop,
                &clock,
                &played,
                &queued,
            );
        })
        .context("could not start playing the stream's sound")?;
    started
        .recv()
        .context("the stream's sound stopped before it started")??;
    Ok(thread)
}

/// Plays the sound that comes on `sounds` into `output`, a turn at a time,
/// until `stop` or the sound ends. This is the player's turn of events, split
/// from [`play`] only so a test can run this same loop into speakers of its
/// own rather than a device's: every frame that comes is conserved, and every
/// turn it plays is written once.
#[allow(clippy::too_many_arguments)]
fn turns(
    sounds: &SoundReceiver,
    playout: &mut Playout,
    output: &mut impl Speakers,
    hearing: &AtomicBool,
    volume: &Volume,
    stop: &AtomicBool,
    clock: &StreamClock,
    played: &std::sync::atomic::AtomicU64,
    queued: &std::sync::atomic::AtomicU64,
) {
    let mut logged = Instant::now();
    // Whether the log has said that nothing maps the sound's clock to the
    // picture's: said once, since a report that never comes would otherwise
    // repeat it every five seconds.
    let mut told_unknown = false;
    while !stop.load(Ordering::Relaxed) {
        // What came since the last turn, without waiting for more.
        for frame in sounds.try_iter() {
            playout.push(frame, Instant::now());
        }
        if !hearing.load(Ordering::Relaxed) {
            // Deafened: nothing is decoded or written, and nothing waits to
            // play late once that ends. What comes is dropped, which is the
            // point of being deafened.
            playout.reset();
            if let Err(mpsc::RecvTimeoutError::Disconnected) = sounds.recv_timeout(TURN) {
                break;
            }
            continue;
        }
        match playout.next(Instant::now()) {
            Turn::Play { at, samples, of } => {
                // `turn` keeps the range inside `pcm`.
                let out = &mut playout.pcm[at..at + samples * CHANNELS];
                if of != Played::Silence {
                    scale(out, volume.get());
                }
                // The device paces the turn: this waits out the sound it
                // queued, or, with no device, as long as that sound would
                // have played.
                output.write(out);
                // Where the sound really is, once a turn: the frames the device
                // has played, and what it still holds. What a device cannot say
                // stays unknown rather than counting as nothing played.
                if let Some((heard, held)) = output.position() {
                    played.store(heard, Ordering::Relaxed);
                    queued.store(held, Ordering::Relaxed);
                }
            }
            // At most a turn, so a stop or a deafen is seen soon.
            Turn::Wait(wait) => match sounds.recv_timeout(wait.min(TURN)) {
                // It was waited for, so it is sound that came: the queue was
                // empty because it was needed, and dropping it here would be
                // this player losing audio the network delivered on time.
                Ok(frame) => {
                    playout.push(frame, Instant::now());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
        }
        if logged.elapsed() >= LOG_EVERY {
            logged = Instant::now();
            // How far the sound at the playhead sits behind the newest
            // picture that came, on the wall clock the streamer's sender
            // reports put both on. The playhead is not the speaker: the
            // device holds about 100 ms more, and the picture shows only once
            // decoded, so this is the stream's own skew rather than a
            // viewer's, and the plan's 80 ms is a bound to sanity-check it
            // against, not a claim. Unknown while a report, a picture or the
            // playhead is missing. The queue's delay in milliseconds is
            // against the 200 [`MOST_SAMPLES`] allows: a queue near it is a
            // sender the playout can't keep up with. Which counter grew says
            // why — `lost` is the link dropping packets, `trimmed` and
            // `skipped` a sender running fast, and `starved` one running
            // slow — so a gap in a sender's numbering is never read as a
            // clock that disagrees.
            let behind = playout.behind(clock);
            // The older of the two mappings, since the skew can be no fresher
            // than it.
            let age = [clock.audio(), clock.video()]
                .into_iter()
                .flatten()
                .map(|report| report.heard.elapsed())
                .max();
            tracing::debug!(
                waiting = playout.waiting.len(),
                held_ms = u64::from(playout.held()) * 1_000 / u64::from(RATE),
                skew_ms = ?behind.map(|behind| (behind * 1_000.0).round() as i64),
                map_age_ms = ?age.map(|age| age.as_millis() as u64),
                came = playout.counts.came,
                played = playout.counts.played,
                lost = playout.counts.lost,
                starved = playout.counts.starved,
                broken = playout.counts.broken,
                trimmed = playout.counts.trimmed,
                late = playout.counts.late,
                duplicate = playout.counts.duplicate,
                skipped = playout.counts.skipped,
                restarts = playout.counts.restarts,
                "stream sound played"
            );
            if behind.is_none() && playout.next.is_some() && !told_unknown {
                told_unknown = true;
                tracing::debug!(
                    audio_report = clock.audio().is_some(),
                    video_report = clock.video().is_some(),
                    picture = clock.picture().is_some(),
                    "no sender report maps this stream's sound and picture onto one clock: A/V skew is unknown"
                );
            }
        }
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

    #[test]
    fn a_flooding_application_starves_no_other_and_grows_nothing() {
        let mut inputs = HashMap::new();
        let mut flood = Input::default();
        flood.push(std::iter::repeat_n(0.5, BACKLOG));
        inputs.insert(1, flood);
        let mut steady = Input::default();
        steady.push(std::iter::repeat_n(0.25, PRIMED));
        inputs.insert(2, steady);
        let mut out = vec![0.0; FRAME];
        for turn in 0..50 {
            // Each callback from the flooder carries a whole backlog, ten
            // turns of sound; the other delivers a frame a turn, as a system
            // handing over every 20 ms would.
            inputs
                .get_mut(&1)
                .unwrap()
                .push(std::iter::repeat_n(0.5, BACKLOG));
            inputs
                .get_mut(&2)
                .unwrap()
                .push(std::iter::repeat_n(0.25, FRAME));
            assert!(mix(&mut inputs, &mut out));
            assert_eq!(out[0], 0.75, "turn {turn}: the steady one still plays");
            assert_eq!(out[FRAME - 1], 0.75, "turn {turn}: for its whole frame");
            assert!(
                inputs[&1].samples.len() <= BACKLOG,
                "turn {turn}: the flooder's backlog stays bounded"
            );
        }
    }

    #[test]
    fn the_frame_kept_for_mixing_holds_the_new_mix_not_the_old() {
        let mut inputs = HashMap::new();
        let mut loud = Input::default();
        loud.push(std::iter::repeat_n(0.5, PRIMED));
        inputs.insert(1, loud);
        let mut frame = vec![0.0; FRAME];
        assert!(mix(&mut inputs, &mut frame));
        assert_eq!(frame[0], 0.5);
        assert_eq!(frame[FRAME - 1], 0.5);
        // The same buffer, another turn, with an application playing silence.
        inputs.clear();
        let mut quiet = Input::default();
        quiet.push(std::iter::repeat_n(0.0, PRIMED));
        inputs.insert(2, quiet);
        assert!(mix(&mut inputs, &mut frame), "silence still plays");
        assert!(
            frame.iter().all(|&sample| sample == 0.0),
            "the old mix is gone from the buffer it was mixed in"
        );
    }

    #[test]
    fn counts_the_sound_a_backlog_holds_and_drops() {
        let mut music = Input::default();
        assert_eq!(
            music.push(std::iter::repeat_n(0.25, PRIMED)),
            0,
            "room to spare"
        );
        assert_eq!(music.waiting(), (PRIMED / CHANNELS) as u32);
        // Past the bound, the oldest goes, and it's counted.
        let overflow = BACKLOG + 2 * CHANNELS;
        assert_eq!(
            music.push(std::iter::repeat_n(0.25, overflow)),
            (overflow - (BACKLOG - PRIMED)) as u64,
            "what the bound dropped"
        );
        assert_eq!(music.waiting(), (BACKLOG / CHANNELS) as u32, "bounded");
        let counted = Counted::default();
        counted
            .dropped
            .store(RATE as u64 / 1000 * 30 * CHANNELS as u64, Ordering::Relaxed);
        // Held comes from the mixer in samples of one channel, as every
        // application's backlog is measured, and the report counts every
        // channel's.
        counted.hold(RATE / 1000 * 40);
        assert_eq!(
            counted.read(),
            SoundStats {
                held_ms: 40,
                dropped_ms: 30
            }
        );
    }

    #[test]
    fn a_full_backlog_waits_two_hundred_milliseconds() {
        let mut input = Input::default();
        input.push(counted(BACKLOG));
        let stats = Counted::default();
        stats.hold(input.waiting());
        assert_eq!(
            stats.read().held_ms,
            200,
            "the delay a viewer would hear at the bound"
        );
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
        let capture = listen(inputs.clone(), Arc::default(), stop.clone()).unwrap();
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
        let capture = listen(inputs.clone(), Arc::default(), stop.clone()).unwrap();
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
        let now = Instant::now();
        for sequence in 0..20u16 {
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            let opus = vec![0xf8, 0xff, 0xfe];
            sounds
                .send(sound(sequence, timestamp, 7, opus, now))
                .unwrap();
        }
        drop(sounds);
        let started = Instant::now();
        let mut player = Player::start(
            SoundReceiver::new(heard, Arc::new(StreamClock::default())),
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Volume::new(1.0)),
        )
        .unwrap();
        player.thread.take().unwrap().join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// A sender report whose clock read `rtp` at `seconds` of NTP time.
    fn reported(seconds: u32, rtp: u32) -> SenderReport {
        SenderReport {
            ntp: seconds << 16,
            rtp,
            heard: Instant::now(),
        }
    }

    #[test]
    fn sender_reports_say_how_far_the_sound_lags_the_picture() {
        // The streamer's sound read 4_800_000 at 100 s and its picture
        // 9_000_000 at that same instant. A second later the sound is at
        // 101 s; half a second after that the newest picture is at 101.5 s,
        // so the sound plays half a second behind the picture.
        let sound = (reported(100, 4_800_000), 4_800_000 + 48_000);
        let picture = (reported(100, 9_000_000), 9_000_000 + 90_000 + 45_000);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.5).abs() < 1e-9, "{skew}");
        // The plan's 80 ms as a sanity bound: a picture 80 ms newer than the
        // sound on the same wall clock reads as 0.08 s behind.
        let picture = (reported(100, 9_000_000), 9_000_000 + 90_000 + 7_200);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.08).abs() < 1e-9, "{skew}");
    }

    #[test]
    fn sender_reports_hold_across_both_wraps() {
        // The sound's RTP timestamp turns over between the report and the
        // playhead: from 24_000 samples before the wrap to 24_000 samples
        // past it is a second, not a jump of four billion ticks.
        let sound = (
            reported(100, u32::MAX - 23_999),
            (u32::MAX - 23_999).wrapping_add(48_000),
        );
        let picture = (reported(100, 0), 90_000 + 7_200);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 0.08).abs() < 1e-9, "{skew}");
        // The NTP time kept is the middle 32 bits, whose seconds turn over
        // every 18 hours: a sound at 65_535 s and a picture whose report sits
        // just past that turn are two seconds apart, not 18 hours.
        let sound = (reported(65_535, 4_800_000), 4_800_000);
        let picture = (reported(0, 0), 90_000);
        let skew = behind(Some(sound), Some(picture)).unwrap();
        assert!((skew - 2.0).abs() < 1e-9, "{skew}");
    }

    #[test]
    fn missing_sender_reports_leave_the_skew_unknown() {
        let sound = (reported(100, 4_800_000), 4_800_000);
        let picture = (reported(100, 9_000_000), 9_000_000);
        assert_eq!(behind(None, Some(picture)), None, "no report for the sound");
        assert_eq!(behind(Some(sound), None), None, "none for the picture");
        assert_eq!(behind(None, None), None);
    }

    /// The player's turn that plays a frame that came, from the start of what
    /// it decoded.
    const PLAYING: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Frame,
    };
    /// The player's turn that makes up for a packet that didn't come.
    const CONCEALED: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Concealed,
    };
    /// The player's turn that plays silence.
    const SILENT: Turn = Turn::Play {
        at: 0,
        samples: SAMPLES,
        of: Played::Silence,
    };

    /// An encoder for a player's sound, as a stream's is sent: music mode.
    fn music() -> opus::Encoder {
        opus::Encoder::new(CHANNELS as u16, opus::Application::Audio, BITRATE).unwrap()
    }

    /// One 20 ms frame of a tone, `at` frames in, encoded.
    fn tone(encoder: &mut opus::Encoder, at: usize) -> Vec<u8> {
        let pcm: Vec<f32> = (0..FRAME)
            .map(|sample| {
                let when = (at * FRAME + sample) as f32;
                (when * 330.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5
            })
            .collect();
        let mut opus = vec![0; 4000];
        let length = encoder.encode_float(&pcm, &mut opus).unwrap();
        opus.truncate(length);
        opus
    }

    /// Opus frames of a tone, 20 ms each, in order.
    fn tones(encoder: &mut opus::Encoder, count: usize) -> Vec<Vec<u8>> {
        (0..count).map(|at| tone(encoder, at)).collect()
    }

    /// One 60 ms frame of a tone, `at` frames in, encoded: what a music bot
    /// passing a source's Opus through sends.
    fn long(encoder: &mut opus::Encoder, at: usize) -> Vec<u8> {
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| {
                let when = (at * FRAME * 3 + sample) as f32;
                (when * 330.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.25
            })
            .collect();
        let mut opus = vec![0; 4000];
        let length = encoder.encode_float(&pcm, &mut opus).unwrap();
        opus.truncate(length);
        opus
    }

    /// A frame as the connection hands it to a player.
    fn sound(
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        opus: Vec<u8>,
        arrived: Instant,
    ) -> ReceivedSound {
        let packet = rtp::Packet {
            payload_type: rtp::OPUS,
            marker: false,
            sequence,
            timestamp,
            ssrc,
            payload: &[],
            // A test's packet says nothing about which way the camera was.
            orientation: None,
        };
        ReceivedSound::new(&packet, opus, arrived)
    }

    /// Pushes `count` frames of a tone, from `sequence` on, in order.
    fn push_tones(playout: &mut Playout, opus: &[Vec<u8>], first: u16, ssrc: u32, now: Instant) {
        for (at, frame) in opus.iter().enumerate() {
            let sequence = first.wrapping_add(at as u16);
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            playout.push(sound(sequence, timestamp, ssrc, frame.clone(), now), now);
        }
    }

    /// Speakers a test plays into: every turn kept, and taken as long to play
    /// as a device would, so the player runs on a clock of its own.
    #[derive(Default)]
    struct Recorded {
        /// Every turn written, in order.
        chunks: Arc<Mutex<Vec<Vec<i16>>>>,
        /// When the first one went out, which is when sound is heard.
        first: Arc<Mutex<Option<Instant>>>,
    }

    impl Recorded {
        fn lock(chunks: &Mutex<Vec<Vec<i16>>>) -> std::sync::MutexGuard<'_, Vec<Vec<i16>>> {
            chunks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        fn written(&self) -> usize {
            Self::lock(&self.chunks).len()
        }
    }

    impl Speakers for Recorded {
        fn write(&mut self, samples: &[i16]) {
            self.first
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_or_insert_with(Instant::now);
            Self::lock(&self.chunks).push(samples.to_vec());
            // The device paces the player through its buffer, so a turn here
            // takes as long as its sound would.
            std::thread::sleep(TURN);
        }
    }

    #[test]
    fn sound_that_comes_while_the_player_waits_is_played_and_not_thrown_away() {
        let sent = START + 5;
        let (sending, arriving) = mpsc::sync_channel(sent);
        let mut encoder = music();
        // None is waiting when the player starts, and they come one at a time
        // after it begins to wait: every one arrives during a blocking wait,
        // which is where a packet used to be taken off the channel and
        // dropped. Three start it; five more play after them.
        let sender = std::thread::spawn(move || {
            for at in 0..sent {
                std::thread::sleep(Duration::from_millis(15));
                let packet = tone(&mut encoder, at);
                let timestamp = (at * SAMPLES) as u32;
                let sound = sound(at as u16, timestamp, 9, packet, Instant::now());
                if sending.send(sound).is_err() {
                    break;
                }
            }
        });
        let mut playout = Playout::new().unwrap();
        let mut output = Recorded::default();
        let hearing = AtomicBool::new(true);
        let volume = Volume::new(1.0);
        let stop = AtomicBool::new(false);
        let receiver = SoundReceiver::new(arriving, Arc::new(StreamClock::default()));
        let clock = receiver.clock();
        let started = Instant::now();
        let played = std::sync::atomic::AtomicU64::new(u64::MAX);
        let queued = std::sync::atomic::AtomicU64::new(u64::MAX);
        turns(
            &receiver,
            &mut playout,
            &mut output,
            &hearing,
            &volume,
            &stop,
            &clock,
            &played,
            &queued,
        );
        sender.join().unwrap();
        assert_eq!(
            playout.counts.came, sent as u64,
            "every packet that came was queued"
        );
        assert_eq!(
            playout.counts.played, sent as u64,
            "and every one of them reached the playhead"
        );
        assert_eq!(playout.counts.late, 0, "none came after its slot played");
        assert_eq!(playout.counts.trimmed, 0, "and none waited too long");
        assert_eq!(playout.counts.lost, 0, "nothing was skipped by the sender");
        assert!(
            output.written() >= sent,
            "{} turns reached the speakers",
            output.written()
        );
        let first = *output
            .first
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let first = first.expect("sound played");
        assert!(
            first.saturating_duration_since(started) < Duration::from_millis(400),
            "first sound {:?} in",
            first.saturating_duration_since(started)
        );
    }

    #[test]
    fn plays_frames_in_order_and_drops_what_comes_twice() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus, 0, 7, now);
        assert_eq!(playout.counts.came, 4, "all four came in order");
        assert_eq!(playout.next(now), PLAYING, "three waiting start it");
        // The packet it just played, again: its slot has gone.
        assert_eq!(
            playout.push(sound(0, 0, 7, opus[0].clone(), now), now),
            Landing::Late
        );
        // And one still waiting, again: it can only play once.
        let timestamp = 2 * SAMPLES as u32;
        assert_eq!(
            playout.push(sound(2, timestamp, 7, opus[2].clone(), now), now),
            Landing::Duplicate
        );
        for at in 1..4 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn holds_a_gap_for_a_late_packet_then_conceals_it() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 never came, and 3 and 4 did.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        // Its slot waits 60 ms for it, then makes it up.
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE / 2), Turn::Wait(LATE / 2));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        // Packet 3, which carried the concealment, still plays as itself.
        assert_eq!(playout.next(primed + LATE), PLAYING);
        assert_eq!(playout.waiting.len(), 1, "packet 4 waits its turn");
    }

    #[test]
    fn an_out_of_order_packet_fills_the_gap_it_belongs_in() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING);
        // 4 comes before 3 does.
        let landing = playout.push(sound(4, 4 * SAMPLES as u32, 7, opus[4].clone(), now), now);
        assert_eq!(landing, Landing::InOrder);
        let landing = playout.push(sound(3, 3 * SAMPLES as u32, 7, opus[3].clone(), now), now);
        assert_eq!(landing, Landing::Reordered, "it goes in front of 4");
        for at in 1..5 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn a_packet_that_comes_before_its_slot_is_made_up_plays() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // 2 hasn't come, and 3 and 4 have.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        assert_eq!(
            playout.next(primed),
            Turn::Wait(LATE),
            "its slot waits for it"
        );
        // It comes within that wait, so it plays, and 3 and 4 after it.
        let later = primed + LATE / 2;
        let landing = playout.push(
            sound(2, 2 * SAMPLES as u32, 7, opus[2].clone(), later),
            later,
        );
        assert_eq!(landing, Landing::Reordered);
        for at in 2..5 {
            assert_eq!(playout.next(primed + LATE), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn the_packet_after_a_gap_still_plays_after_carrying_its_concealment() {
        // Voice mode is what turns Opus's forward error correction on, which
        // a stream's own encoder needn't send.
        let mut encoder =
            opus::Encoder::new(CHANNELS as u16, opus::Application::Voice, BITRATE).unwrap();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 is lost, and 3 and 4 came.
        push_tones(&mut playout, &opus[3..], 3, 7, primed);
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(3),
            "the packet that carried it isn't counted as played"
        );
        assert_eq!(playout.next(primed + LATE), PLAYING, "packet 3, as itself");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(4));
    }

    #[test]
    fn an_undecodable_packet_still_gets_its_own_concealed_turn() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..2], 0, 7, now);
        // Two are short of the three that start it, so they play because the
        // first waited out PRIME.
        let primed = now + PRIME;
        assert_eq!(playout.next(primed), PLAYING);
        assert_eq!(playout.next(primed), PLAYING);
        // Packet 2 is lost, and packet 3 is one libopus can't read: the try
        // at its forward error correction fails, so 2's slot is concealed
        // and 3 is left to play, and fail, on its own turn.
        let garbage = vec![0xff];
        playout.push(sound(3, 3 * SAMPLES as u32, 7, garbage, primed), primed);
        assert_eq!(playout.next(primed), Turn::Wait(LATE));
        assert_eq!(playout.next(primed + LATE), CONCEALED);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(3),
            "a failed decode is not swallowed"
        );
        assert!(
            matches!(playout.next(primed + LATE), Turn::Play { .. }),
            "it plays, and fails, on its own turn"
        );
        assert!(playout.waiting.is_empty());
        assert_eq!(playout.next, Some(4));
        // Sound after it plays as usual.
        let landing = playout.push(
            sound(4, 4 * SAMPLES as u32, 7, opus[4].clone(), primed),
            primed,
        );
        assert_eq!(landing, Landing::InOrder);
        assert_eq!(playout.next(primed + LATE), PLAYING);
    }

    #[test]
    fn wraps_sequences_and_timestamps_without_starting_again() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // The sequences wrap the u16 as the timestamps wrap the u32.
        let sequences = [65_534u16, 65_535, 0, 1];
        let mut timestamp = u32::MAX - 2 * SAMPLES as u32;
        for (at, sequence) in sequences.iter().enumerate() {
            let frame = sound(*sequence, timestamp, 7, opus[at].clone(), now);
            assert_eq!(playout.push(frame, now), Landing::InOrder, "{sequence}");
            timestamp = timestamp.wrapping_add(SAMPLES as u32);
        }
        for at in 0..4 {
            assert_eq!(playout.next(now), PLAYING, "frame {at}");
        }
        assert!(playout.waiting.is_empty());
        assert_eq!(playout.counts.restarts, 0, "a wrap is not a restart");
    }

    #[test]
    fn starts_again_on_another_ssrc_or_a_timestamp_jump() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 8);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // Another SSRC is another stream: what waited means nothing to it.
        let landing = playout.push(sound(0, 0, 8, opus[3].clone(), now), now);
        assert_eq!(landing, Landing::Restart);
        assert_eq!(playout.next, None, "it waits to start again");
        assert_eq!(playout.waiting.len(), 1);
        // A sender that began again may keep its SSRC: a timestamp a second
        // away is what says so.
        push_tones(&mut playout, &opus[4..6], 1, 8, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        let jump = 3 * SAMPLES as u32 + 48_000 + SAMPLES as u32;
        let landing = playout.push(sound(3, jump, 8, opus[6].clone(), now), now);
        assert_eq!(landing, Landing::Restart);
        assert_eq!(playout.next, None, "it waits to start again");
    }

    #[test]
    fn goes_quiet_after_a_long_gap_and_starts_again_when_sound_comes() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 9);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // Nothing more comes: its slot is made up for a while, then the
        // sender has stopped, and silence plays rather than sound that isn't
        // there.
        for missing in 1..GONE {
            assert_eq!(playout.next(now), CONCEALED, "{missing} packets missing");
        }
        assert_eq!(playout.next(now), SILENT);
        assert_eq!(playout.next, None, "it waits to start again");
        assert_eq!(playout.next(now), Turn::Wait(PRIME), "no sound to wait for");
        // The sender comes back where the silence left the stream: no jump.
        push_tones(&mut playout, &opus[3..6], 8, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three frames start it again");
        assert_eq!(playout.counts.restarts, 0, "silence isn't a restart");
    }

    #[test]
    fn drops_a_frame_that_waited_too_long_and_starts_again() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 4);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        for _ in 0..3 {
            assert_eq!(playout.next(now), PLAYING);
        }
        // A frame that waited in the queue through a stall of 300 ms.
        let stalled = now.checked_sub(STALE + Duration::from_millis(20)).unwrap();
        let landing = playout.push(
            sound(3, 3 * SAMPLES as u32, 7, opus[3].clone(), stalled),
            now,
        );
        assert_eq!(landing, Landing::Late);
        assert_eq!(playout.next, None, "a stalled player starts again");
        assert!(playout.waiting.is_empty(), "nothing old waits to replay");
    }

    #[test]
    fn primes_on_three_frames_or_a_hundred_ms_of_one() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        assert_eq!(
            playout.push(sound(0, 0, 7, opus[0].clone(), now), now),
            Landing::InOrder
        );
        assert_eq!(
            playout.next(now),
            Turn::Wait(PRIME),
            "one frame isn't enough"
        );
        push_tones(&mut playout, &opus[1..3], 1, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three are");
        // One that came alone starts once it has waited 100 ms.
        playout.reset();
        assert_eq!(
            playout.push(sound(5, 0, 7, opus[0].clone(), now), now),
            Landing::InOrder
        );
        assert_eq!(playout.next(now), Turn::Wait(PRIME));
        assert_eq!(playout.next(now + PRIME / 2), Turn::Wait(PRIME / 2));
        assert_eq!(playout.next(now + PRIME), PLAYING);
    }

    #[test]
    fn keeps_at_most_most_frames_and_plays_the_newest() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST + 5);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus, 0, 7, now);
        assert_eq!(playout.waiting.len(), MOST, "the oldest went");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(5));
        assert_eq!(playout.next(now), PLAYING);
        assert_eq!(
            playout.waiting.front().map(|frame| frame.sequence),
            Some(6),
            "and what plays is the newest ten"
        );
    }

    #[test]
    fn plays_a_sixty_ms_packet_over_three_turns() {
        let mut encoder = music();
        // A music bot passing a source's Opus through can send 60 ms packets.
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| (sample as f32 * 0.01).sin() * 0.25)
            .collect();
        let opus: Vec<Vec<u8>> = (0..START)
            .map(|_| {
                let mut packet = vec![0; 4000];
                let length = encoder.encode_float(&pcm, &mut packet).unwrap();
                packet.truncate(length);
                packet
            })
            .collect();
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        for (at, frame) in opus.into_iter().enumerate() {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, frame, now), now);
        }
        for turn in 0..START * 3 {
            assert_eq!(
                playout.next(now),
                Turn::Play {
                    at: (turn % 3) * FRAME,
                    samples: SAMPLES,
                    of: Played::Frame,
                },
                "turn {turn}: each 20 ms plays from where the one before left off"
            );
            assert_eq!(
                playout.waiting.len(),
                START - 1 - turn / 3,
                "a packet plays over three turns; turn {turn}"
            );
        }
    }

    #[test]
    fn a_sixty_ms_packet_plays_each_of_its_parts_once_at_any_volume() {
        let mut encoder = music();
        // Three 20 ms stretches that can't be mistaken for each other, so a
        // turn that played the start of the packet again says so in the PCM
        // itself and not only in the count of turns.
        let pcm: Vec<f32> = (0..FRAME * 3)
            .map(|sample| match sample / FRAME {
                0 => 0.5,
                1 => -0.25,
                _ => 0.125,
            })
            .collect();
        let packet = {
            let mut opus = vec![0; 4000];
            let length = encoder.encode_float(&pcm, &mut opus).unwrap();
            opus.truncate(length);
            opus
        };
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        for at in 0..START {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, packet.clone(), now), now);
        }
        // What the packet holds, decoded in the same order the player decodes
        // it, so that every sample emitted can be compared with the one it
        // should have played: a whole packet is three turns of three different
        // 20 ms stretches, each scaled once.
        let mut reference = opus::Decoder::new(CHANNELS as u16).unwrap();
        let mut expected = vec![0; FRAME * 3 * START];
        for at in 0..START {
            let decoded = reference
                .decode(&packet, &mut expected[at * FRAME * 3..], false)
                .unwrap();
            assert_eq!(decoded, SAMPLES * 3, "a packet of three turns");
        }
        let volume = 0.5;
        let mut played = Vec::new();
        for turn in 0..START * 3 {
            let Turn::Play { at, samples, of } = playout.next(now) else {
                panic!("turn {turn}: a primed playout plays");
            };
            assert_eq!(
                (at, samples, of),
                ((turn % 3) * FRAME, SAMPLES, Played::Frame)
            );
            let mut chunk = playout.pcm[at..at + samples * CHANNELS].to_vec();
            scale(&mut chunk, volume);
            played.push(chunk);
        }
        for (turn, chunk) in played.iter().enumerate() {
            let mut once = expected[turn * FRAME..][..FRAME].to_vec();
            scale(&mut once, volume);
            assert_eq!(chunk, &once, "turn {turn}: exactly this 20 ms, scaled once");
        }
        // The three turns of one packet are three different stretches: not
        // its first 20 ms three times.
        assert_ne!(played[0], played[1], "the second turn is not the first");
        assert_ne!(played[1], played[2], "and the third is not the second");
    }

    #[test]
    fn a_sixty_ms_packet_stream_is_held_by_time_not_frame_count() {
        let mut encoder = music();
        let opus: Vec<Vec<u8>> = (0..MOST).map(|at| long(&mut encoder, at)).collect();
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Ten 60 ms frames are 600 ms of sound. A queue that only counted
        // frames would hold all of them, three times the delay a 20 ms
        // stream is held to, and play it all a second late.
        for (at, frame) in opus.into_iter().enumerate() {
            let timestamp = (at * SAMPLES * 3) as u32;
            playout.push(sound(at as u16, timestamp, 7, frame, now), now);
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held after {} frames",
                playout.held(),
                at + 1
            );
            assert!(playout.waiting.len() <= MOST, "room for them all");
        }
        // Three of the ten are the little under 200 ms that's left, all at
        // the newest end: the oldest went to make room.
        assert_eq!(playout.waiting.len(), 3);
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(7));
        assert!(
            playout.counts.trimmed > 0,
            "the time bound did the dropping"
        );
    }

    #[test]
    fn a_bound_drops_the_sound_that_came_early_not_the_playhead() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST + 6);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..MOST], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "ten frames start it");
        assert_eq!(playout.next, Some(1), "and the playhead is at the second");
        // Six more come before its turn: the ones furthest ahead are the
        // sound that came too early to play in time, so they're the ones to
        // go, and the playhead's own packet is never one of them.
        push_tones(&mut playout, &opus[MOST..], MOST as u16, 7, now);
        assert_eq!(playout.next, Some(1), "the playhead didn't skip");
        assert_eq!(playout.waiting.front().map(|frame| frame.sequence), Some(1));
        assert_eq!(
            playout.waiting.back().map(|frame| frame.sequence),
            Some(MOST as u16),
            "the newest went, and what's left is still in order"
        );
        assert!(playout.held() <= MOST_SAMPLES);
        assert!(playout.waiting.len() <= MOST);
        assert_eq!(playout.counts.trimmed, 5, "the five dropped are counted");
        // What's left plays through without a gap or a wait: nothing was
        // taken out of the middle of it.
        for sequence in 1..=MOST {
            assert_eq!(playout.next(now), PLAYING, "frame {sequence}");
        }
        assert!(playout.waiting.is_empty());
    }

    #[test]
    fn a_fast_sender_has_its_delay_trimmed_rather_than_raised() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST * 3 * 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Three frames arrive a turn where one plays, as a sender whose clock
        // runs at three times the playout's: the queue, and the delay with it,
        // would grow two turns a turn without a bound.
        for (at, frame) in opus.into_iter().enumerate() {
            let sequence = at as u16;
            let timestamp = u32::from(sequence) * SAMPLES as u32;
            playout.push(sound(sequence, timestamp, 7, frame, now), now);
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held after {} frames",
                playout.held(),
                at + 1
            );
            assert!(
                playout.waiting.len() <= MOST,
                "{} frames after {}",
                playout.waiting.len(),
                at + 1
            );
            if at % 3 == 2 {
                let turn = playout.next(now);
                assert!(matches!(turn, Turn::Play { .. }), "{at}: {turn:?}");
            }
        }
        assert!(playout.counts.trimmed > 0, "the bound trimmed the surplus");
        assert_eq!(
            playout.counts.lost, 0,
            "every number came, so none of this is loss"
        );
        // What's left plays out and empties, so the delay it was held to
        // doesn't stay raised once the sender stops outrunning the player.
        for _ in 0..MOST + usize::from(GONE) {
            playout.next(now);
        }
        assert!(playout.waiting.is_empty(), "the queue drained");
        assert_eq!(playout.held(), 0, "and holds nothing");
    }

    #[test]
    fn a_fast_sender_is_trimmed_and_skipped_but_not_lost() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns * 20 / 19 + 2);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % fast: a frame every 19 ms of the 20 ms it should be.
        // The queue fills a little each turn until its bound trims it, and
        // the turns the queue has to spare take a quiet one unheard.
        let mut sent = 0;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 19 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            playout.next(now);
        }
        assert!(playout.counts.trimmed > 0, "the bound kept the delay down");
        assert!(playout.counts.skipped > 0, "and a spare turn went unheard");
        assert_eq!(
            playout.counts.lost, 0,
            "every number came, so none of this is loss"
        );
        assert_eq!(playout.counts.starved, 0, "and the queue never ran dry");
        assert_eq!(playout.counts.restarts, 0, "the sender kept its numbering");
    }

    #[test]
    fn a_skipped_frame_shortens_playback_instead_of_playing_silence() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns * 20 / 19 + 2);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % fast, its packets and the player's turns on clocks the
        // test keeps: a frame every 19 ms of the 20 ms a turn plays, so the
        // queue fills until its bound trims it and the drift control takes a
        // frame on the turns the queue has to spare.
        let mut sent = 0;
        let mut played = 0u64;
        let mut skips = 0u64;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 19 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            let skipped = playout.counts.skipped;
            if let Turn::Play { at, samples, of } = playout.next(now) {
                assert_eq!(samples, SAMPLES, "a 20 ms packet is a turn; turn {turn}");
                if of == Played::Frame {
                    played += 1;
                    assert!(
                        playout.pcm[at..at + samples * CHANNELS]
                            .iter()
                            .any(|&sample| sample != 0),
                        "turn {turn}: a frame played is the frame that came"
                    );
                }
                if playout.counts.skipped > skipped {
                    skips += 1;
                    // The frame the drift control took is gone from the
                    // output: this turn goes to the sound after it, not to
                    // silence standing in for what was dropped.
                    assert_ne!(
                        of,
                        Played::Silence,
                        "turn {turn}: a skipped frame must not cost a turn of silence"
                    );
                }
            }
            assert!(
                playout.held() <= MOST_SAMPLES,
                "turn {turn}: {} samples held, not a queue growing with the mismatch",
                playout.held()
            );
        }
        assert!(skips > 0, "the drift control took a frame");
        assert_eq!(
            skips, playout.counts.skipped,
            "each skip cost one turn at most"
        );
        // What input took and output spent, in turns: every frame taken is one
        // turn of sound, and a skipped one no turn at all, so the output is
        // shorter than the input by exactly what was skipped. That shorter
        // output is the drift caught up with.
        assert_eq!(
            played + skips,
            playout.counts.played,
            "{} turns played and {} skipped, of {} frames taken",
            played,
            skips,
            playout.counts.played
        );
    }

    #[test]
    fn a_slow_sender_starves_the_playhead_rather_than_losing_packets() {
        let mut encoder = music();
        let turns = 200;
        let opus = tones(&mut encoder, turns);
        let mut playout = Playout::new().unwrap();
        let start = Instant::now();
        // A sender 5 % slow: a frame every 21 ms of the 20 ms it should be,
        // so a turn every so often has nothing to play.
        let mut sent = 0;
        for turn in 0..turns {
            let now = start + Duration::from_millis(20) * turn as u32;
            while sent * 21 <= turn * 20 {
                let sequence = sent as u16;
                let timestamp = u32::from(sequence) * SAMPLES as u32;
                playout.push(sound(sequence, timestamp, 7, opus[sent].clone(), now), now);
                sent += 1;
            }
            playout.next(now);
            assert!(playout.waiting.len() <= MOST, "the queue doesn't grow");
            assert!(
                playout.held() <= MOST_SAMPLES,
                "{} samples held at turn {turn}",
                playout.held()
            );
        }
        assert!(playout.counts.starved > 0, "the playhead ran dry");
        assert!(
            playout.counts.late > 0,
            "and the sound it outran came after its slot"
        );
        assert_eq!(
            playout.counts.lost, 0,
            "the sender skipped no number: it's slow, not lossy"
        );
        assert_eq!(
            playout.counts.trimmed, 0,
            "nothing arrives too fast to play"
        );
        assert_eq!(playout.counts.restarts, 0, "a slow sender isn't a new one");
        assert!(
            playout.counts.played > (turns * 3 / 4) as u64,
            "and sound plays through it"
        );
    }

    #[test]
    fn a_lossy_stream_reports_loss_and_no_trimming() {
        let mut encoder = music();
        let opus = tones(&mut encoder, 8);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        // Three start it, and four more play on their own turns, so the
        // playhead is at 5 with the queue empty behind it.
        push_tones(&mut playout, &opus[..3], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three start it");
        push_tones(&mut playout, &opus[3..5], 3, 7, now);
        for frame in 1..5 {
            assert_eq!(playout.next(now), PLAYING, "frame {frame}");
        }
        // 5 and 6 never come and 7 does: a gap in the sender's numbering
        // with sound after it, which is what loss looks like.
        push_tones(&mut playout, &opus[7..], 7, 7, now);
        assert_eq!(playout.next(now), Turn::Wait(LATE), "its slot waits for it");
        assert_eq!(playout.next(now + LATE), CONCEALED, "5 is made up");
        assert_eq!(playout.next(now + LATE), CONCEALED, "and 6");
        assert_eq!(playout.next(now + LATE), PLAYING, "then 7 plays");
        assert_eq!(playout.counts.lost, 2, "both gaps are loss");
        assert_eq!(playout.counts.trimmed, 0, "nothing came too fast to hold");
        assert_eq!(playout.counts.starved, 0, "the playhead never ran dry");
        assert_eq!(playout.counts.skipped, 0, "and nothing went unheard");
        assert_eq!(playout.counts.restarts, 0, "the sender kept sending");
    }

    #[test]
    fn a_bound_emptied_slot_is_trimming_not_loss() {
        let mut encoder = music();
        let opus = tones(&mut encoder, MOST * 3);
        let mut playout = Playout::new().unwrap();
        let now = Instant::now();
        push_tones(&mut playout, &opus[..START], 0, 7, now);
        assert_eq!(playout.next(now), PLAYING, "three start it");
        // A burst from a sender that outran the playout: everything past the
        // queue's bounds goes, and those numbers can never come again.
        push_tones(&mut playout, &opus[START..], START as u16, 7, now);
        assert!(playout.counts.trimmed > 0, "the bounds dropped the far end");
        // What's left plays, and then the emptied slots are made up at
        // once: none of them is counted as a number the sender skipped, and
        // the run of them is what says the playout should wait for sound it
        // can play rather than chase what its own bound threw away.
        for frame in 1..=MOST as u16 {
            assert_eq!(playout.next(now), PLAYING, "frame {frame}");
        }
        for slot in MOST + 1..MOST + usize::from(GONE) {
            assert_eq!(playout.next(now), CONCEALED, "trimmed slot {slot}");
        }
        assert_eq!(playout.next(now), SILENT, "then it waits for sound");
        assert_eq!(playout.next, None);
        assert_eq!(
            playout.counts.trimmed,
            (MOST * 2 - 1) as u64,
            "each dropped frame is counted once"
        );
        assert_eq!(playout.counts.lost, 0, "not one of them is the link's");
        assert_eq!(playout.counts.starved, 0, "nor a playhead outrun");
    }

    #[test]
    fn a_frame_longer_than_the_room_is_clamped_to_it() {
        let mut playout = Playout::new().unwrap();
        let room = playout.pcm.len();
        // More to play than `pcm` holds, which a decoder that overran its
        // buffer would leave: the slices stop at the room there is.
        let start = room - CHANNELS * 100;
        playout.left = start..room + CHANNELS * 100;
        assert_eq!(
            playout.next(Instant::now()),
            Turn::Play {
                at: start,
                samples: 100,
                of: Played::Frame
            }
        );
    }

    /// Samples numbered in the order they were played, so what a backlog
    /// keeps says which ones did.
    fn counted(count: usize) -> impl Iterator<Item = f32> {
        (0..count).map(|sample| sample as f32)
    }

    #[test]
    fn keeps_a_bounded_backlog() {
        let mut input = Input::default();
        input.push(std::iter::repeat_n(0.1, BACKLOG * 3));
        assert_eq!(input.samples.len(), BACKLOG);
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

    #[test]
    fn a_start_that_fails_stops_the_threads_it_already_began() {
        // A listener standing in for the operating system's capture: it says
        // when it has begun, and when it has let go of what it held.
        let listening = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        let began = Arc::clone(&listening);
        let going = Arc::clone(&running);
        let heard = move |_inputs: Inputs, _counted: Arc<Counted>, stop: Arc<AtomicBool>| {
            Ok(std::thread::spawn(move || {
                began.store(true, Ordering::Relaxed);
                while !stop.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                going.store(false, Ordering::Relaxed);
            }))
        };
        let codec = Arc::new(AtomicBool::new(false));
        let opened = Arc::clone(&codec);
        let started = capture(
            Inputs::default(),
            Arc::new(Counted::default()),
            move || {
                opened.store(true, Ordering::Relaxed);
                opus::Encoder::new(2, opus::Application::Audio, BITRATE)
            },
            heard,
            // The step after the listener: a mixer whose thread won't start.
            |_encoder, _stop, _counted, _inputs| {
                Err(std::io::Error::other("no thread for the mixer"))
            },
        );
        assert!(started.is_err(), "the mixer's thread wouldn't start");
        assert!(
            !running.load(Ordering::Relaxed),
            "the listener stopped before the error went back"
        );
        assert!(
            listening.load(Ordering::Relaxed),
            "it had begun before the step that failed"
        );
        assert!(codec.load(Ordering::Relaxed), "the codec came first");

        // A codec that won't open leaves nothing to undo: it runs before the
        // capture begins hearing the machine, so no thread was ever started.
        let anything = Arc::new(AtomicBool::new(false));
        let begun = Arc::clone(&anything);
        let none = move |_inputs: Inputs, _counted: Arc<Counted>, _stop: Arc<AtomicBool>| {
            begun.store(true, Ordering::Relaxed);
            Ok(std::thread::spawn(|| {}))
        };
        let refused = capture(
            Inputs::default(),
            Arc::new(Counted::default()),
            || Err(anyhow::anyhow!("no codec")),
            none,
            |_encoder, _stop, _counted, _inputs| Ok(std::thread::spawn(|| {})),
        );
        assert!(refused.is_err(), "the codec wouldn't open");
        assert!(
            !anything.load(Ordering::Relaxed),
            "the listener never began, so there was nothing to stop"
        );
    }
}
