//! `rtp.rs`'s timing.

use super::*;

/// How long a missing packet is waited for, how often it's asked for again,
/// and how many times, before the frames after it go to the assembler
/// without it. Discord's servers are a few tens of milliseconds away.
pub(crate) const GIVE_UP: std::time::Duration = std::time::Duration::from_millis(300);
pub(crate) const ASK_AGAIN: std::time::Duration = std::time::Duration::from_millis(80);
pub(crate) const ASKS: u32 = 3;
/// Packets held at most while one is missing, about a second at 9 Mbit/s.
pub(crate) const HELD: usize = 1000;
/// Missing packets waited for at most, and asked for at once: a sender
/// skipping far ahead again and again would otherwise run up thousands, and
/// a request too large to send.
pub(crate) const MAX_MISSING: usize = HELD;
pub(crate) const MAX_ASKED: usize = 256;

/// The sequence numbers below the highest whose arrival is remembered: a
/// packet that came out of order is counted received once, however late it was,
/// and a duplicate isn't counted twice. About a quarter of a second at the rate
/// a stream sends, and longer than a gap is waited for.
pub(crate) const ARRIVED: u64 = 256;
pub(crate) const ARRIVED_WORDS: usize = (ARRIVED / 64) as usize;

/// Which packets near the highest sequence number have arrived, for the count a
/// receiver report carries.
///
/// A packet that arrives behind the one the frames are being built from has
/// still arrived: Discord's client, and RFC 3550 with it, counts it as received,
/// so a report says the path lost what it lost rather than what arrived out of
/// order. Reading it from the delivery point instead — "the highest less the
/// packets handed on" — called every reordered packet lost, which is a loss a
/// streamer reads as its own uplink's.
#[derive(Default)]
pub(crate) struct Arrived {
    /// The highest sequence number marked.
    pub(crate) newest: u64,
    /// Bit `index * 64 + bit` is the packet `newest - (index * 64 + bit)`.
    pub(crate) bits: [u64; ARRIVED_WORDS],
    /// The packets marked since the stream began.
    pub(crate) count: u64,
}

impl Arrived {
    /// Marks `sequence` as arrived: whether this is its first arrival, which is
    /// what a report counts.
    pub(crate) fn note(&mut self, sequence: u64) -> bool {
        if sequence > self.newest {
            self.advance(sequence - self.newest);
            self.newest = sequence;
        }
        let below = self.newest - sequence;
        // Older than what's remembered: it can't be told from a packet counted
        // when it first came, so it isn't counted again.
        let Some((word, bit)) =
            (below < ARRIVED).then_some(((below / 64) as usize, (below % 64) as u32))
        else {
            return false;
        };
        if self.bits[word] & (1 << bit) != 0 {
            return false;
        }
        self.bits[word] |= 1 << bit;
        self.count += 1;
        true
    }

    /// The highest is `shift` further on, so every packet remembered is that
    /// much older.
    pub(crate) fn advance(&mut self, shift: u64) {
        if shift >= ARRIVED {
            self.bits = [0; ARRIVED_WORDS];
            return;
        }
        let (words, bits) = ((shift / 64) as usize, (shift % 64) as u32);
        let mut moved = [0; ARRIVED_WORDS];
        for (index, word) in moved.iter_mut().enumerate() {
            let Some(from) = index.checked_sub(words) else {
                continue;
            };
            *word = self.bits[from] << bits;
            if bits > 0 && from > 0 {
                *word |= self.bits[from - 1] >> (64 - bits);
            }
        }
        self.bits = moved;
    }
}

/// Puts one SSRC's packets back in sequence for the assembler, as they can
/// arrive out of order, and asks for missing ones again (NACK), as Discord's
/// client does, rather than losing every frame until the next keyframe.
#[derive(Default)]
pub struct Reorder {
    /// The extended sequence number of the next packet to hand on.
    pub(crate) next: Option<u64>,
    pub(crate) highest: Option<u64>,
    pub(crate) held: std::collections::BTreeMap<u64, Held>,
    pub(crate) missing: std::collections::BTreeMap<u64, Missing>,
    /// For receiver reports: the first extended sequence number, and which of
    /// the packets since have come.
    pub(crate) first: Option<u64>,
    pub(crate) arrived: Arrived,
}

pub(crate) struct Missing {
    pub(crate) since: std::time::Instant,
    pub(crate) asked: Option<std::time::Instant>,
    pub(crate) asks: u32,
}

/// What a packet was to the reorder buffer.
#[derive(Debug, PartialEq, Eq)]
pub enum Arrival {
    New,
    /// One it was waiting for, probably resent.
    Recovered,
    /// A duplicate, or too late: the packets after it went on without it.
    Late,
}

impl Reorder {
    /// `sequence` extended to 64 bits, nearest the highest seen so far.
    pub(crate) fn extend(&self, sequence: u16) -> u64 {
        match self.highest {
            None => (1 << 32) + u64::from(sequence),
            Some(highest) => {
                let step = sequence.wrapping_sub(highest as u16) as i16;
                highest.wrapping_add_signed(i64::from(step))
            }
        }
    }

    pub fn insert(&mut self, packet: Held, now: std::time::Instant) -> Arrival {
        let sequence = self.extend(packet.sequence);
        let next = *self.next.get_or_insert(sequence);
        self.first.get_or_insert(sequence);
        // A copy of one still waiting is a copy, not a packet that arrived.
        if self.held.contains_key(&sequence) {
            return Arrival::Late;
        }
        let fresh = self.arrived.note(sequence);
        if sequence < next {
            // Behind what the frames were built from: the packets after it went
            // on without it, so it can't join one, but it did arrive and the
            // report says so.
            return if fresh {
                Arrival::Recovered
            } else {
                Arrival::Late
            };
        }
        let recovered = self.missing.remove(&sequence).is_some();
        match self.highest {
            // Far ahead: the sender started again, so start with it.
            Some(highest) if sequence > highest + HELD as u64 => {
                self.held.clear();
                self.missing.clear();
                self.next = Some(sequence);
            }
            Some(highest)
                if sequence > highest + 1
                    && self.missing.len() + (sequence - highest - 1) as usize <= MAX_MISSING =>
            {
                for gap in highest + 1..sequence {
                    self.missing.insert(
                        gap,
                        Missing {
                            since: now,
                            asked: None,
                            asks: 0,
                        },
                    );
                }
            }
            _ => {}
        }
        if self.highest.is_none_or(|highest| sequence > highest) {
            self.highest = Some(sequence);
        }
        self.held.insert(sequence, packet);
        if recovered {
            Arrival::Recovered
        } else {
            Arrival::New
        }
    }

    /// The packets ready in sequence, and how many were given up on.
    pub fn ready(&mut self, now: std::time::Instant) -> (Vec<Held>, u64) {
        let mut ready = vec![];
        let mut skipped = 0;
        while let Some(next) = self.next {
            if let Some(packet) = self.held.remove(&next) {
                ready.push(packet);
                self.next = Some(next + 1);
                continue;
            }
            let Some(&after) = self.held.keys().next() else {
                break;
            };
            let waited = self
                .missing
                .get(&next)
                .is_none_or(|missing| now.duration_since(missing.since) >= GIVE_UP);
            if !waited && self.held.len() < HELD {
                break;
            }
            skipped += after - next;
            self.missing.retain(|&sequence, _| sequence > after);
            self.next = Some(after);
        }
        (ready, skipped)
    }

    /// The missing packets to ask for now: newly missed ones, and those
    /// still missing a while after the last ask.
    pub fn to_ask(&mut self, now: std::time::Instant) -> Vec<u16> {
        let mut asking = vec![];
        for (&sequence, missing) in &mut self.missing {
            let due = missing
                .asked
                .is_none_or(|asked| now.duration_since(asked) >= ASK_AGAIN);
            if asking.len() >= MAX_ASKED {
                break;
            }
            if missing.asks < ASKS && due {
                missing.asked = Some(now);
                missing.asks += 1;
                asking.push(sequence as u16);
            }
        }
        asking
    }

    /// The highest extended sequence number received, and the packets
    /// expected so far that never came, for a receiver report.
    pub fn counts(&self) -> Option<(u32, u32)> {
        let (first, highest) = (self.first?, self.highest?);
        let expected = highest + 1 - first;
        Some((
            highest as u32,
            expected.saturating_sub(self.arrived.count) as u32,
        ))
    }
}
