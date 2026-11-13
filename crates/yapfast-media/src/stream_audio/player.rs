//! `stream_audio.rs`'s player.

use super::*;

/// An application's sound joins the mix once this much waits, which
/// absorbs the system delivering it in chunks.
pub(crate) const PRIMED: usize = FRAME * 2;
/// The most kept for an application; older sound is dropped, so clock
/// drift never grows into delay.
pub(crate) const BACKLOG: usize = FRAME * 10;
/// Where a mixed sample starts bending towards full scale rather than
/// flat-topping, in sample values. Under it every sample is mixed as it came:
/// an application playing quietly is never made quieter for the company it
/// keeps, and one whose sound stays under it sounds exactly as it played. No
/// curve can be the identity all the way to full scale and still round the
/// corner there, so the bend starts a little under it; a single application at
/// full scale gives up 0.45 dB, which is under the step an ear hears as a
/// change in loudness.
pub(crate) const KNEE: f32 = 0.9;
/// The room a sample over [`KNEE`] has to bend in, up to full scale.
pub(crate) const BEND: f32 = 1.0 - KNEE;
/// A player's turn: 20 ms of sound, then the next.
pub(crate) const TURN: Duration = Duration::from_millis(20);

// How long a watched stream's sound waits and how long it gives a packet
// that hasn't come. Starting points to tune on a real link, not guarantees:
// nothing has measured them against Discord's delivery yet.
/// A stream's sound starts once this many frames wait: 60 ms.
pub(crate) const START: usize = 3;
/// With a single frame, playing starts this long after it came.
pub(crate) const PRIME: Duration = Duration::from_millis(100);
/// Frames wait at most. Each takes the room of one Opus frame, so this is
/// what bounds memory, not delay: a music bot's packet can carry six turns.
pub(crate) const MOST: usize = 10;
/// Sound waits at most, in samples per channel: 200 ms, so a stall never
/// turns into lasting delay. Ten 20 ms frames are that much, but a music
/// bot's sixty-millisecond ones are three times it, and they'd wait just as
/// quietly: the time frames hold is what says when the queue is too long.
pub(crate) const MOST_SAMPLES: u32 = RATE / 1000 * 200;
/// A missing packet is waited for this long before its slot is concealed or
/// dropped: 60 ms.
pub(crate) const LATE: Duration = Duration::from_millis(60);
/// Packets missing in a row after which the sender has stopped: 100 ms.
/// Concealing stops there, and silence plays until sound comes again.
pub(crate) const GONE: u8 = 5;
/// A frame that waited in the queue longer than this came during a stall and
/// is dropped rather than played late: 300 ms.
pub(crate) const STALE: Duration = Duration::from_millis(300);
/// The furthest a frame's timestamp may fall from where the last played one
/// left the stream: a second, in samples. Beyond, the sender began again.
pub(crate) const JUMP: i32 = 48_000;
/// The streamer's picture clock: RTP timestamps on a stream's video SSRC
/// count 90 kHz ticks.
pub(crate) const PICTURE_RATE: u32 = 90_000;
/// What the NTP time kept in a sender report spans: its middle 32 bits are
/// seconds in 16.16, so the seconds they count turn over every 18 hours.
pub(crate) const NTP_TURN: f64 = 65_536.0;
/// Printed at most this often: what waits, and how much of what came was
/// lost, trimmed, skipped or stopped short.
pub(crate) const LOG_EVERY: Duration = Duration::from_secs(5);

/// Plays a watched stream's sound until its Opus frames end or it's
/// dropped, on the speakers the call uses.
pub struct Player {
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) thread: Option<JoinHandle<()>>,
    /// Frames the speakers have played and frames they still hold queued, as
    /// the player's own thread last read them from the device: where the
    /// sound really is, which is what A/V timing is measured from.
    pub(crate) played: Arc<std::sync::atomic::AtomicU64>,
    pub(crate) queued: Arc<std::sync::atomic::AtomicU64>,
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
pub(crate) fn scale(pcm: &mut [i16], volume: f32) {
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
pub(crate) enum Landing {
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
pub(crate) enum Played {
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
pub(crate) enum Turn {
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
pub(crate) enum Missing {
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
pub(crate) struct Counts {
    /// Frames that came, and how many of those played.
    pub(crate) came: u64,
    pub(crate) played: u64,
    /// Turns made up for a sequence number the sender skipped while still
    /// sending: packet loss, which a gap in its numbering is.
    pub(crate) lost: u64,
    /// Turns the playhead had nothing at all to play. A slow sender leaves
    /// these at intervals instead of skipping numbers, so for one this grows
    /// and `lost` doesn't.
    pub(crate) starved: u64,
    /// Turns made up for a packet that came and Opus couldn't read, which is
    /// a codec fault rather than the link's.
    pub(crate) broken: u64,
    /// Frames a bound dropped before their turn, for the room they took or
    /// the time they would have added to the delay. A sender running fast
    /// meets the bound with frames still coming; a lossy one meets it never.
    pub(crate) trimmed: u64,
    /// Frames that came after their slot had played, or waited through a
    /// stall: sound that was late, which a slow sender's is.
    pub(crate) late: u64,
    pub(crate) duplicate: u64,
    /// Frames dropped unheard for the room a queue had to spare: the drift a
    /// sender running fast adds, taken a turn at a time.
    pub(crate) skipped: u64,
    /// Times the sender began again.
    pub(crate) restarts: u64,
}

/// Where `timestamp` on `report`'s clock sits on wall-clock time, in seconds
/// on the middle 32 bits of NTP: the report's own instant plus the ticks
/// between its timestamp and this one, at `rate` ticks a second. Read
/// signed, so a timestamp on the far side of the clock's wrap is a small
/// step rather than four billion ticks.
pub(crate) fn at_wall_time(report: SenderReport, timestamp: u32, rate: u32) -> f64 {
    let ticks = timestamp.wrapping_sub(report.rtp) as i32;
    f64::from(report.ntp) / NTP_TURN + f64::from(ticks) / f64::from(rate)
}

/// How far behind the picture the sound at `sound` is, in seconds, each a
/// timestamp on its own clock together with the sender report that says
/// where that clock's timestamps sit on wall-clock time: the two are only
/// comparable once each is placed there, since 48 kHz and 90 kHz counts
/// share no zero and no rate. `None` while either is missing, since which
/// clock an unmapped timestamp ran on is a guess.
pub(crate) fn behind(
    sound: Option<(SenderReport, u32)>,
    picture: Option<(SenderReport, u32)>,
) -> Option<f64> {
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
pub(crate) struct Playout {
    pub(crate) decoder: opus::Decoder,
    /// Frames in sequence order, oldest first, at most [`MOST`].
    pub(crate) waiting: VecDeque<ReceivedSound>,
    /// The sequence number to play next, while playing; none until primed.
    pub(crate) next: Option<u16>,
    /// The timestamp the next packet should hold, from the last one played.
    pub(crate) expected: Option<u32>,
    /// Where sound plays from `pcm`, for packets longer than 20 ms.
    pub(crate) left: std::ops::Range<usize>,
    /// Room for Opus's longest frame, 120 ms.
    pub(crate) pcm: Vec<i16>,
    /// The SSRC the frames came on; another is another stream, or a restart.
    pub(crate) ssrc: Option<u32>,
    /// When the first frame of a (re)start waited, and when the playhead
    /// first found its packet missing.
    pub(crate) prime: Option<Instant>,
    pub(crate) hold: Option<Instant>,
    /// Packets missing in a row.
    pub(crate) missing: u8,
    /// Samples per channel of the last frame decoded: a frame's timestamp says
    /// where it begins, not how long it plays, so what the queue holds counts
    /// the last one's length. Zero until one has decoded.
    pub(crate) packet: u32,
    /// The stretch of sequence numbers a bound emptied, first to last. The
    /// playhead conceals their slots at once rather than waiting [`LATE`] for
    /// sound it knows is gone, and can tell that trimming from the sender's
    /// loss.
    pub(crate) dropped: Option<(u16, u16)>,
    /// Frames taken a little faster than they play.
    pub(crate) surplus: Surplus,
    pub(crate) counts: Counts,
}

impl Playout {
    pub(crate) fn new() -> Result<Self> {
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
    pub(crate) fn push(&mut self, frame: ReceivedSound, now: Instant) -> Landing {
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
    pub(crate) fn restart(&mut self, frame: ReceivedSound, now: Instant) {
        self.reset();
        self.ssrc = Some(frame.ssrc);
        self.counts.restarts += 1;
        self.queue(frame, now);
    }

    /// Inserts a frame in sequence order, then lets go of what's over a bound.
    pub(crate) fn queue(&mut self, frame: ReceivedSound, now: Instant) -> Landing {
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
    pub(crate) fn held(&self) -> u32 {
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
    pub(crate) fn behind(&self, clock: &StreamClock) -> Option<f64> {
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
    pub(crate) fn discharge(&mut self) -> bool {
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
    pub(crate) fn trimmed(&mut self, sequence: u16) -> bool {
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
    pub(crate) fn next(&mut self, now: Instant) -> Turn {
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
    pub(crate) fn play(&mut self, next: u16) -> Option<Turn> {
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
    pub(crate) fn play_nothing(&mut self, next: u16) -> Turn {
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
    pub(crate) fn play_missing(&mut self, next: u16, now: Instant) -> Turn {
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
    pub(crate) fn note(&mut self, missing: Missing) {
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
    pub(crate) fn conceal(&mut self, missing: Missing) -> Turn {
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
    pub(crate) fn silence(&mut self) -> Turn {
        self.pcm[..FRAME].fill(0);
        self.left = 0..FRAME;
        self.turn(Played::Silence)
    }

    /// Takes the next 20 ms of `pcm`, or what's left of it, to play, and
    /// where in `pcm` it starts.
    pub(crate) fn turn(&mut self, of: Played) -> Turn {
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
    pub(crate) fn set_left(&mut self, samples: usize) {
        self.left = 0..samples.min(self.pcm.len() / CHANNELS) * CHANNELS;
    }

    /// How long to wait for sound: until the first frame has waited
    /// [`PRIME`], so one that came alone still plays.
    pub(crate) fn wait_for(&self, now: Instant) -> Duration {
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
    pub(crate) fn skips(&mut self, samples: usize) -> bool {
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
    pub(crate) fn reset(&mut self) {
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
pub(crate) trait Speakers {
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

pub(crate) fn play(
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
pub(crate) fn turns(
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
