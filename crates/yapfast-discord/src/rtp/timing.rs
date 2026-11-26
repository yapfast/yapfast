//! Putting packets back in order and asking again for those that are missing.

use super::*;

/// How long a missing packet is waited for, how often it's asked for again,
/// and how many times, before the frames after it go to the assembler
/// without it. Discord's servers are a few tens of milliseconds away.
pub(super) const GIVE_UP: std::time::Duration = std::time::Duration::from_millis(300);
pub(super) const ASK_AGAIN: std::time::Duration = std::time::Duration::from_millis(80);
pub(super) const ASKS: u32 = 3;
/// Packets held at most while one is missing, about a second at 9 Mbit/s.
pub(super) const HELD: usize = 1000;
/// Missing packets waited for at most, and asked for at once: a sender
/// skipping far ahead again and again would otherwise run up thousands, and
/// a request too large to send.
pub(super) const MAX_MISSING: usize = HELD;
pub(super) const MAX_ASKED: usize = 256;

/// Puts one SSRC's packets back in sequence for the assembler, as they can
/// arrive out of order, and asks for missing ones again (NACK), as Discord's
/// client does, rather than losing every frame until the next keyframe.
#[derive(Default)]
pub struct Reorder {
    /// The extended sequence number of the next packet to hand on.
    next: Option<u64>,
    highest: Option<u64>,
    held: std::collections::BTreeMap<u64, Held>,
    pub(super) missing: std::collections::BTreeMap<u64, Missing>,
    /// For receiver reports: the first extended sequence number, and the
    /// packets received since.
    first: Option<u64>,
    received: u64,
}

pub(super) struct Missing {
    since: std::time::Instant,
    asked: Option<std::time::Instant>,
    asks: u32,
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
    fn extend(&self, sequence: u16) -> u64 {
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
        if sequence < next || self.held.contains_key(&sequence) {
            return Arrival::Late;
        }
        self.received += 1;
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
            expected.saturating_sub(self.received) as u32,
        ))
    }
}
