//! Packets kept to send again when a viewer reports them lost.

use super::*;

/// How long a kept packet is worth sending again. A viewer holds a gap for
/// 300 ms and asks for the packet again every 80 ms in that time, so anything
/// older than this is one it has given up on and gone for a whole frame
/// instead. Kept generous, since a resend nobody uses costs one packet while
/// the keyframe that replaces it costs a frame.
pub(crate) const RESEND_WINDOW: Duration = Duration::from_secs(2);

/// How many packets a history keeps: its slots, and so the most it can hold
/// whatever the stream does.
pub(crate) const SLOTS: usize = 2048;

/// The video packets sent lately, unencrypted, to resend when a viewer
/// reports one lost: about two seconds at 9 Mbit/s, and never older than
/// `RESEND_WINDOW` however slowly they went.
pub struct History {
    pub(crate) packets: Vec<Option<Sent>>,
    /// When the history was made, which a slot's time counts from.
    pub(crate) born: Instant,
}

/// A packet kept for a viewer that reports it lost, and when it went out.
pub(crate) struct Sent {
    /// The sequence it went out with, which its slot's index names only by
    /// its remainder.
    pub(crate) sequence: u16,
    /// The millisecond of the history's `born` it went out at.
    pub(crate) at: u32,
    pub(crate) packet: Vec<u8>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            // Empty slots, made rather than cloned: a slot that held a
            // packet is never copied.
            packets: std::iter::repeat_with(|| None).take(SLOTS).collect(),
            born: Instant::now(),
        }
    }
}

impl History {
    /// Keeps `packet`, sent at `now`, as the slot its sequence names,
    /// overwriting what that slot held in the vector it already has, so a
    /// steady stream copies bytes instead of allocating a packet-sized vector
    /// each time.
    pub fn remember(&mut self, packet: &[u8], now: Instant) {
        let Some(sent) = parse(packet) else {
            return;
        };
        let at = self.millis(now);
        let slot = usize::from(sent.sequence) % self.packets.len();
        match &mut self.packets[slot] {
            Some(kept) => {
                // The index alone decides which sequence a slot may hold, so
                // the one it names has to follow the packet it keeps now.
                kept.sequence = sent.sequence;
                kept.at = at;
                kept.packet.clear();
                kept.packet.extend_from_slice(packet);
            }
            empty => {
                *empty = Some(Sent {
                    sequence: sent.sequence,
                    at,
                    packet: packet.to_vec(),
                });
            }
        }
    }

    /// `sequence`'s packet, if it went out long enough ago for a viewer to
    /// still be waiting for it. A slot that has since taken another packet
    /// answers with nothing rather than with those bytes: only a sequence the
    /// slot names is its own, and only while it is young enough to resend.
    pub fn get(&self, sequence: u16, now: Instant) -> Option<&[u8]> {
        let kept = self.packets[usize::from(sequence) % self.packets.len()].as_ref()?;
        // Wrapping, as only the difference says how old the packet is and the
        // count of milliseconds comes round again after seven weeks.
        let age = self.millis(now).wrapping_sub(kept.at);
        (kept.sequence == sequence && age < RESEND_WINDOW.as_millis() as u32)
            .then_some(kept.packet.as_slice())
    }

    /// `now` counted from when the history was made, in milliseconds.
    pub(crate) fn millis(&self, now: Instant) -> u32 {
        now.duration_since(self.born).as_millis() as u32
    }
}

/// How many resends the record of them keeps: the ones a viewer may still ask
/// for again while their copy is on its way.
const REMEMBERED: usize = 64;

/// The packets viewers asked for again, waiting their turn on the wire.
///
/// A NACK can name a hundred packets, and it names them because a burst just
/// left a queue full: answering it as one burst is the same flood again. So
/// they go out as a frame's packets do, [`BURST`] at once and the rest paced.
/// Only the sequence numbers wait here: the history has each one's bytes until
/// its turn comes.
pub(crate) struct Resends {
    /// Asked for and not yet sent, oldest first.
    waiting: std::collections::VecDeque<u16>,
    /// Sent lately, and when: a viewer's repeat of a request is not answered
    /// with a second copy while the first is still on its way.
    lately: std::collections::VecDeque<(u16, Instant)>,
    /// The pacing the run goes out at, from its first packet.
    pace: Pace,
}

impl Default for Resends {
    fn default() -> Self {
        Self {
            waiting: std::collections::VecDeque::new(),
            lately: std::collections::VecDeque::new(),
            pace: Pace::new(Instant::now()),
        }
    }
}

impl Resends {
    /// Queues `sequence`, asked for at `now`, and answers whether it's worth
    /// sending: it is while nothing waiting or lately sent names it, and
    /// `in_flight` is the time a copy put on the wire is still assumed to be on
    /// its way. A viewer asks again every 80 ms while it holds a gap, so a
    /// request repeated inside that time is the same lost packet twice.
    pub fn ask(&mut self, sequence: u16, now: Instant, in_flight: Option<Duration>) -> bool {
        self.forget(now, in_flight);
        if self.waiting.contains(&sequence) || self.lately.iter().any(|&(sent, _)| sent == sequence)
        {
            return false;
        }
        if self.waiting.is_empty() {
            // A run of its own, with its burst to go.
            self.pace = Pace::new(now);
        }
        self.waiting.push_back(sequence);
        true
    }

    /// Drops what no longer answers for anything: a repeat comes at most 80 ms
    /// after the request before it, and `in_flight` is how long it waits.
    fn forget(&mut self, now: Instant, in_flight: Option<Duration>) {
        let Some(window) = in_flight else {
            // Nothing measured says how long a copy is on its way, so no
            // request is assumed to be one already answered.
            self.lately.clear();
            return;
        };
        while self
            .lately
            .front()
            .is_some_and(|&(_, sent)| now.duration_since(sent) >= window)
        {
            self.lately.pop_front();
        }
    }

    /// Whether anything waits to go out.
    pub fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// When the next batch may go.
    pub fn due(&self, now: Instant) -> Instant {
        self.pace.due(now)
    }

    /// How many of the waiting packets this turn writes.
    pub fn batch(&mut self, target: u64, factor: u64, now: Instant) -> usize {
        self.pace.batch(target, factor, now)
    }

    /// Notes that this turn's batch is over, `now` being when: the turn after
    /// it is paid for over the time from here.
    pub fn wrote(&mut self, now: Instant) {
        self.pace.wrote(now);
    }

    /// The next packet to send, longest waiting first, noting that it went out
    /// at `now`, which a repeat of its request is measured from.
    pub fn next(&mut self, now: Instant) -> Option<u16> {
        let sequence = self.waiting.pop_front()?;
        self.pace.went();
        self.lately.push_back((sequence, now));
        while self.lately.len() > REMEMBERED {
            self.lately.pop_front();
        }
        Some(sequence)
    }
}

/// A received video packet, kept until those before it arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub payload_type: u8,
    pub marker: bool,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: Vec<u8>,
    pub orientation: Option<Orientation>,
}

impl Held {
    pub fn new(packet: &Packet<'_>) -> Self {
        Self {
            payload_type: packet.payload_type,
            marker: packet.marker,
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            ssrc: packet.ssrc,
            payload: packet.payload.to_vec(),
            orientation: packet.orientation,
        }
    }

    pub fn packet(&self) -> Packet<'_> {
        Packet {
            payload_type: self.payload_type,
            marker: self.marker,
            sequence: self.sequence,
            timestamp: self.timestamp,
            ssrc: self.ssrc,
            payload: &self.payload,
            orientation: self.orientation,
        }
    }

    /// The packet a retransmission on `media`'s RTX SSRC carries (RFC 4588):
    /// its sequence number, then its payload. None for padding, which a
    /// server sends there to probe the path.
    pub fn unwrap_rtx(packet: &Packet<'_>, media: u32) -> Option<Self> {
        let (sequence, payload) = packet.payload.split_first_chunk::<2>()?;
        if payload.is_empty() {
            return None;
        }
        let payload_type = [Codec::Vp8, Codec::H264, Codec::H265, Codec::Av1]
            .into_iter()
            .map(Codec::payload_types)
            .find(|&(_, rtx)| rtx == packet.payload_type)
            .map_or(packet.payload_type, |(media, _)| media);
        Some(Self {
            payload_type,
            marker: packet.marker,
            sequence: u16::from_be_bytes(*sequence),
            timestamp: packet.timestamp,
            ssrc: media,
            payload: payload.to_vec(),
            orientation: packet.orientation,
        })
    }
}
