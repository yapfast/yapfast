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
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use yapfast_discord::{
    opus, rtp,
    stream_connection::{ReceivedSound, Sound, SoundSender},
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
/// Printed at most this often: what waits, and what concealment, drops and
/// skips did.
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
        let inputs = Inputs::default();
        let counted = Arc::new(Counted::default());
        let stop = Arc::new(AtomicBool::new(false));
        let capture = listen(inputs.clone(), counted.clone(), stop.clone())?;
        let mut encoder = opus::Encoder::new(2, opus::Application::Audio, BITRATE)?;
        let stopping = stop.clone();
        let counting = counted.clone();
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
                    let mut inputs = inputs
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    // What waits before this turn is mixed, which is the
                    // delay a viewer would hear; the mix then takes a frame
                    // from each application that has one.
                    let waiting = inputs.values().map(Input::waiting).max().unwrap_or(0);
                    let heard = mix(&mut inputs, &mut frame);
                    drop(inputs);
                    counting.held.store(waiting, Ordering::Relaxed);
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
            counted,
            threads: vec![capture, mixer],
        })
    }

    /// What the captured sound did so far, for the stream's report.
    pub fn stats(&self) -> SoundStats {
        self.counted.read()
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
    /// Writes `samples` per channel from [`Playout::pcm`].
    Play { samples: usize, of: Played },
    /// Nothing can play yet: waits at most this long for real sound.
    Wait(Duration),
}

/// What a stream's sound did, for the log.
#[derive(Debug, Default)]
struct Counts {
    /// Frames that came, and how many of those played.
    came: u64,
    played: u64,
    /// Turns made up for a packet that didn't come.
    concealed: u64,
    /// Frames dropped: too late to play, twice, or past a bound.
    late: u64,
    duplicate: u64,
    /// Frames a full queue dropped to keep the delay it adds bounded: the
    /// oldest one that wasn't about to play.
    over: u64,
    /// Frames dropped unheard for the room a queue had to spare: the drift a
    /// sender running fast adds, taken a turn at a time.
    skipped: u64,
    /// Times the sender began again.
    restarts: u64,
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
    /// The first frame a bound dropped, whose slot the playhead conceals at
    /// once rather than waiting [`LATE`] for sound already gone.
    dropped: Option<u16>,
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
        self.counts.over += 1;
        if self
            .next
            .is_some_and(|next| rtp::before(next, gone.sequence))
        {
            // The playhead would wait [`LATE`] for a hole a bound made
            // itself: the first one it meets is concealed at once instead,
            // and the ones after it are concealed without waiting anyway.
            self.dropped.get_or_insert(gone.sequence);
        }
        true
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
        let next = self.next.expect("playing");
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
            self.play(next)
        } else if self.waiting.is_empty() {
            self.play_nothing(next)
        } else {
            self.play_missing(next, now)
        }
    }

    /// Plays the playhead's packet, `next`.
    fn play(&mut self, next: u16) -> Turn {
        let frame = self.waiting.pop_front().expect("the playhead");
        self.missing = 0;
        self.hold = None;
        self.next = Some(next.wrapping_add(1));
        if self.dropped == Some(next) {
            // A bound dropped it, and it came after all: it plays.
            self.dropped = None;
        }
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
                return self.conceal();
            }
        };
        self.expected = Some(frame.timestamp.wrapping_add(samples as u32));
        if samples == 0 {
            return self.conceal();
        }
        // How long this one plays, which the next queue bound counts.
        self.packet = samples as u32;
        self.counts.played += 1;
        self.set_left(samples);
        if self.skips(samples) {
            // The queue had frames to spare, so this one goes unheard: that
            // is what keeps the delay from growing.
            self.pcm[..samples * CHANNELS].fill(0);
            return self.turn(Played::Silence);
        }
        self.turn(Played::Frame)
    }

    /// The playhead's packet is missing, and nothing waits after it.
    fn play_nothing(&mut self, next: u16) -> Turn {
        self.hold = None;
        self.missing = self.missing.saturating_add(1);
        self.next = Some(next.wrapping_add(1));
        self.expected = self
            .expected
            .map(|expected| expected.wrapping_add(SAMPLES as u32));
        if self.missing >= GONE {
            // The sender stopped, rather than a packet coming late: silence
            // while waiting for it to start again, not made-up sound for
            // ever.
            self.next = None;
            self.missing = 0;
            self.surplus = Surplus::default();
            // A hole a bound made means nothing to a stream that began again.
            self.dropped = None;
            return self.silence();
        }
        self.conceal()
    }

    /// The playhead's packet is missing, and a later one waits: gives it
    /// [`LATE`], then makes up its 20 ms, and plays that one when its own
    /// turn comes.
    fn play_missing(&mut self, next: u16, now: Instant) -> Turn {
        // A bound dropped this frame, so it's gone for certain: its slot is
        // concealed now rather than waited for.
        let dropped = self.dropped == Some(next);
        if dropped {
            self.dropped = None;
        }
        // Only the first gap of a run waits: the ones after it are concealed
        // at once, so a burst of loss never waits longer than one packet.
        if self.missing == 0 && !dropped {
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
                self.set_left(samples);
                self.turn(Played::Concealed)
            }
            _ => self.conceal(),
        }
    }

    /// 20 ms made up for a packet that didn't come.
    fn conceal(&mut self) -> Turn {
        self.counts.concealed += 1;
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

    /// Takes the next 20 ms of `pcm`, or what's left of it, to play.
    fn turn(&mut self, of: Played) -> Turn {
        let start = self.left.start;
        let end = self.left.end.min(start + FRAME).min(self.pcm.len());
        self.left.start = end;
        Turn::Play {
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
        sounds: std::sync::mpsc::Receiver<ReceivedSound>,
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
    sounds: std::sync::mpsc::Receiver<ReceivedSound>,
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
            let mut logged = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                // What came since the last turn, without waiting for more.
                for frame in sounds.try_iter() {
                    playout.push(frame, Instant::now());
                }
                if !hearing.load(Ordering::Relaxed) {
                    // Deafened: nothing is decoded or written, and nothing
                    // waits to play late once that ends.
                    playout.reset();
                    if let Err(mpsc::RecvTimeoutError::Disconnected) = sounds.recv_timeout(TURN) {
                        break;
                    }
                    continue;
                }
                match playout.next(Instant::now()) {
                    Turn::Play { samples, of } => {
                        let end = (samples * CHANNELS).min(playout.pcm.len());
                        let out = &mut playout.pcm[..end];
                        if of != Played::Silence {
                            scale(out, volume.get());
                        }
                        // The device paces the turn: this waits out the
                        // sound it queued, or, with no device, as long as
                        // that sound would have played.
                        output.write(out);
                    }
                    // At most a turn, so a stop or a deafen is seen soon.
                    Turn::Wait(wait) => {
                        if let Err(mpsc::RecvTimeoutError::Disconnected) =
                            sounds.recv_timeout(wait.min(TURN))
                        {
                            break;
                        }
                    }
                }
                if logged.elapsed() >= LOG_EVERY {
                    logged = Instant::now();
                    // `held` is samples per channel, against [`MOST_SAMPLES`]:
                    // a queue near it is a sender the playout can't keep up
                    // with, whatever the concealments and drops say.
                    tracing::debug!(
                        waiting = playout.waiting.len(),
                        held = playout.held(),
                        counts = ?playout.counts,
                        "stream sound played"
                    );
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
        counted
            .held
            .store(RATE / 1000 * 40 * CHANNELS as u32, Ordering::Relaxed);
        assert_eq!(
            counted.read(),
            SoundStats {
                held_ms: 40,
                dropped_ms: 30
            }
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
            heard,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Volume::new(1.0)),
        )
        .unwrap();
        player.thread.take().unwrap().join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// The player's turn that plays a frame that came.
    const PLAYING: Turn = Turn::Play {
        samples: SAMPLES,
        of: Played::Frame,
    };
    /// The player's turn that makes up for a packet that didn't come.
    const CONCEALED: Turn = Turn::Play {
        samples: SAMPLES,
        of: Played::Concealed,
    };
    /// The player's turn that plays silence.
    const SILENT: Turn = Turn::Play {
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
            assert_eq!(playout.next(now), PLAYING, "turn {turn}");
            assert_eq!(
                playout.waiting.len(),
                START - 1 - turn / 3,
                "a packet plays over three turns; turn {turn}"
            );
        }
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
        assert!(playout.counts.over > 0, "the bound did the dropping");
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
        assert_eq!(playout.counts.over, 5, "the five dropped are counted");
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
        assert!(playout.counts.over > 0, "the bound trimmed the surplus");
        // What's left plays out and empties, so the delay it was held to
        // doesn't stay raised once the sender stops outrunning the player.
        for _ in 0..MOST + usize::from(GONE) {
            playout.next(now);
        }
        assert!(playout.waiting.is_empty(), "the queue drained");
        assert_eq!(playout.held(), 0, "and holds nothing");
    }

    #[test]
    fn a_slow_sender_is_concealed_rather_than_queued() {
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
        assert!(playout.counts.concealed > 0, "the missing sound is made up");
        assert_eq!(playout.counts.over, 0, "nothing arrives too fast to play");
        assert_eq!(playout.counts.restarts, 0, "a slow sender isn't a new one");
        assert!(
            playout.counts.played > (turns * 3 / 4) as u64,
            "and sound plays through it"
        );
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
