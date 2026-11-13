//! `rtp.rs`'s receive.

use super::*;

/// A decrypted RTP packet.
#[derive(Debug)]
pub struct Packet<'a> {
    pub payload_type: u8,
    pub marker: bool,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: &'a [u8],
    /// The video orientation it carries, if any.
    pub orientation: Option<Orientation>,
}

/// Reads a decrypted RTP packet, skipping CSRCs, extensions and padding.
pub fn parse(packet: &[u8]) -> Option<Packet<'_>> {
    let first = *packet.first()?;
    if first >> 6 != 2 || packet.len() < HEADER {
        return None;
    }
    let mut start = HEADER + usize::from(first & 0x0f) * 4;
    let mut turned = None;
    if first & 0x10 != 0 {
        let profile = u16::from_be_bytes([*packet.get(start)?, *packet.get(start + 1)?]);
        let words = u16::from_be_bytes([*packet.get(start + 2)?, *packet.get(start + 3)?]);
        let elements = packet.get(start + 4..start + 4 + usize::from(words) * 4)?;
        turned = orientation(profile, elements);
        start += 4 + usize::from(words) * 4;
    }
    let padding = if first & 0x20 != 0 {
        usize::from(*packet.last()?)
    } else {
        0
    };
    let end = packet.len().checked_sub(padding)?;
    Some(Packet {
        payload_type: packet[1] & 0x7f,
        marker: packet[1] & 0x80 != 0,
        sequence: u16::from_be_bytes([packet[2], packet[3]]),
        timestamp: u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
        ssrc: u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]),
        payload: packet.get(start..end)?,
        orientation: turned,
    })
}

/// Whether RTP sequence number `a` comes before `b`, across the wrap: of two
/// numbers, the one less than half the sequence space behind the other is
/// the older.
pub fn before(a: u16, b: u16) -> bool {
    a != b && b.wrapping_sub(a) < 0x8000
}

/// A whole received frame, as the sender encoded it.
#[derive(Debug, PartialEq, Eq)]
pub struct Assembled {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub timestamp: u32,
}

/// The largest frame put together, far above a 4K keyframe.
pub(crate) const MAX_FRAME: usize = 8 << 20;

/// Puts one SSRC's packets back together into frames. A frame missing a
/// packet is dropped, and frames are dropped until the next keyframe.
pub struct Assembler {
    pub(crate) codec: Codec,
    pub(crate) frame: Vec<u8>,
    pub(crate) timestamp: Option<u32>,
    pub(crate) expected: Option<u16>,
    pub(crate) damaged: bool,
    pub(crate) keyframe: bool,
    pub(crate) waiting: bool,
    /// AV1: the frame's latest OBU element, written once the next begins
    /// or the frame ends, and whether it carries on in the next packet.
    pub(crate) obu: Vec<u8>,
    pub(crate) obu_open: bool,
    /// A fragmented H264/H265 NAL must end before another unit starts.
    pub(crate) nal_open: Option<u8>,
}

/// What a packet completed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pushed {
    pub frame: Option<Assembled>,
    /// A frame was lost, or none came yet to decode from, so the sender
    /// should send a keyframe.
    pub lost: bool,
}

impl Assembler {
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            frame: vec![],
            timestamp: None,
            expected: None,
            damaged: false,
            keyframe: false,
            waiting: true,
            obu: vec![],
            obu_open: false,
            nal_open: None,
        }
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Drops frames until the next keyframe, as after one the decoder never
    /// got.
    pub fn wait_for_keyframe(&mut self) {
        self.waiting = true;
    }

    pub fn push(&mut self, packet: &Packet) -> Pushed {
        self.push_with_codec(packet, self.codec)
    }

    /// Uses the negotiated codec only for an unrecognized payload type.
    pub fn push_with_codec(&mut self, packet: &Packet, negotiated: Codec) -> Pushed {
        // Select from packets in sequence, after reordering. Arrival order can
        // interleave an old codec's retransmissions with the new codec.
        let codec = Codec::from_payload_type(packet.payload_type).unwrap_or(negotiated);
        if codec != self.codec {
            *self = Self::new(codec);
        }
        let gap = self
            .expected
            .is_some_and(|expected| expected != packet.sequence);
        if packet.payload.is_empty() {
            // Padding, sent to probe the path: no part of a frame, but a gap
            // before it still counts against the next packet.
            if !gap {
                self.expected = Some(packet.sequence.wrapping_add(1));
            }
            return Pushed::default();
        }
        self.expected = Some(packet.sequence.wrapping_add(1));
        let mut lost = false;
        if self.timestamp != Some(packet.timestamp) {
            // The last frame never saw its marker.
            lost = self.timestamp.is_some();
            self.waiting |= lost;
            self.frame.clear();
            self.obu.clear();
            self.obu_open = false;
            self.nal_open = None;
            self.timestamp = Some(packet.timestamp);
            self.damaged = gap;
            self.keyframe = false;
        } else {
            self.damaged |= gap;
        }
        // Once damaged, no byte of this timestamp can be decoded. Keep the
        // storage for reuse, but stop parsing/copying every remaining packet.
        let first = self.frame.is_empty();
        if !self.damaged {
            match self.codec {
                Codec::Vp8 => self.vp8(packet.payload, first),
                Codec::H264 => self.h264(packet.payload),
                Codec::H265 => self.h265(packet.payload),
                Codec::Av1 => self.av1(packet.payload),
            }
        }
        // Packets that never end their frame would grow it without end, as
        // would AV1 packets whose element always carries on.
        if self.frame.len() + self.obu.len() > MAX_FRAME {
            self.frame = Vec::new();
            self.obu = Vec::new();
            self.damaged = true;
        }
        if !packet.marker {
            return Pushed { frame: None, lost };
        }
        if self.codec == Codec::Av1 {
            // An element still carrying on when the frame ended.
            self.damaged |= std::mem::take(&mut self.obu_open);
            self.write_obu(false);
        }
        self.damaged |= self.nal_open.is_some();
        self.timestamp = None;
        if self.damaged || self.frame.is_empty() {
            self.waiting = true;
            return Pushed {
                frame: None,
                lost: true,
            };
        }
        if self.waiting && !self.keyframe {
            return Pushed {
                frame: None,
                lost: true,
            };
        }
        let data = std::mem::take(&mut self.frame);
        self.waiting = false;
        Pushed {
            frame: Some(Assembled {
                data,
                keyframe: self.keyframe,
                timestamp: packet.timestamp,
            }),
            lost,
        }
    }

    pub(crate) fn vp8(&mut self, payload: &[u8], first: bool) {
        let Some(&descriptor) = payload.first() else {
            self.damaged = true;
            return;
        };
        let mut at = 1;
        if descriptor & 0x80 != 0 {
            let Some(&extended) = payload.get(1) else {
                self.damaged = true;
                return;
            };
            at = 2;
            if extended & 0x80 != 0 {
                at += if payload.get(2).is_some_and(|byte| byte & 0x80 != 0) {
                    2
                } else {
                    1
                };
            }
            if extended & 0x40 != 0 {
                at += 1;
            }
            if extended & 0x30 != 0 {
                at += 1;
            }
        }
        let start = descriptor & 0x10 != 0 && descriptor & 0x07 == 0;
        if first != start {
            self.damaged = true;
        }
        let Some(data) = payload.get(at..) else {
            self.damaged = true;
            return;
        };
        if first {
            // A keyframe's frame tag has bit 0 clear.
            self.keyframe = data.first().is_some_and(|tag| tag & 1 == 0);
        }
        self.frame.extend_from_slice(data);
    }

    /// AV1's OBU elements, written as OBUs that give their size but for
    /// the last, as DAVE left it: its decryptor reads the frame as the
    /// sender sealed it. N in a packet's aggregation header makes the frame
    /// a keyframe.
    pub(crate) fn av1(&mut self, payload: &[u8]) {
        let Some((&header, mut rest)) = payload.split_first() else {
            self.damaged = true;
            return;
        };
        let (continues, carries_on) = (header & 0x80 != 0, header & 0x40 != 0);
        let count = usize::from((header >> 4) & 0x03);
        self.keyframe |= header & 0x08 != 0;
        if continues && header & 0x08 != 0 {
            self.damaged = true;
            return;
        }
        if continues != self.obu_open {
            // A continuation without its start, or a start while the last
            // element waits for its end.
            self.damaged = true;
            self.obu.clear();
            if continues {
                return;
            }
        }
        let mut index = 0;
        while !rest.is_empty() {
            index += 1;
            let size = if index == count {
                rest.len()
            } else {
                let Some((size, used)) = read_leb128(rest) else {
                    self.damaged = true;
                    return;
                };
                rest = &rest[used..];
                size
            };
            if size == 0 {
                self.damaged = true;
                return;
            }
            let Some(element) = rest.get(..size) else {
                self.damaged = true;
                return;
            };
            rest = &rest[size..];
            if !(index == 1 && continues) {
                self.write_obu(true);
            }
            self.obu.extend_from_slice(element);
        }
        self.damaged |= index == 0 || (count != 0 && index != count);
        self.obu_open = carries_on;
    }

    /// Writes the OBU element held as an OBU, with its size or without.
    pub(crate) fn write_obu(&mut self, sized: bool) {
        let Some(&header) = self.obu.first() else {
            return;
        };
        let header_size = if header & OBU_EXTENSION != 0 { 2 } else { 1 };
        let Some(extension) = self.obu.get(1..header_size) else {
            self.damaged = true;
            self.obu.clear();
            return;
        };
        if sized {
            self.frame.push(header | OBU_HAS_SIZE);
            self.frame.extend_from_slice(extension);
            write_leb128(self.obu.len() - header_size, &mut self.frame);
        } else {
            self.frame.push(header & !OBU_HAS_SIZE);
            self.frame.extend_from_slice(extension);
        }
        self.frame.extend_from_slice(&self.obu[header_size..]);
        self.obu.clear();
    }

    /// Writes NAL units in Annex B form with four-byte start codes, the form
    /// DAVE expects.
    /// RFC 7798: whole units, aggregation packets (48) and fragmentation
    /// units (49). Random access units (types 16 to 21) make a keyframe.
    pub(crate) fn h265(&mut self, payload: &[u8]) {
        let [first, second] = match payload.first_chunk() {
            Some(header) => *header,
            None => {
                self.damaged = true;
                return;
            }
        };
        let random_access = |kind: u8| (16..=21).contains(&kind);
        let unit = |frame: &mut Vec<u8>, keyframe: &mut bool, nal: &[u8]| {
            *keyframe |= nal
                .first()
                .is_some_and(|header| random_access((header >> 1) & 0x3f));
            frame.extend_from_slice(&[0, 0, 0, 1]);
            frame.extend_from_slice(nal);
        };
        match (first >> 1) & 0x3f {
            0..=47 => {
                self.damaged |= self.nal_open.is_some();
                unit(&mut self.frame, &mut self.keyframe, payload);
            }
            48 => {
                self.damaged |= self.nal_open.is_some();
                let mut rest = &payload[2..];
                while rest.len() >= 2 {
                    let size = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
                    let Some(nal) = rest.get(2..2 + size) else {
                        self.damaged = true;
                        return;
                    };
                    if size < 2 {
                        self.damaged = true;
                        return;
                    }
                    unit(&mut self.frame, &mut self.keyframe, nal);
                    rest = &rest[2 + size..];
                }
                self.damaged |= !rest.is_empty();
            }
            49 => {
                let Some(&fragment) = payload.get(2) else {
                    self.damaged = true;
                    return;
                };
                let kind = fragment & 0x3f;
                self.fragment(kind, fragment, payload.len() > 3);
                if fragment & 0x80 != 0 {
                    self.keyframe |= random_access(kind);
                    let header = (first & 0x81) | (kind << 1);
                    self.frame.extend_from_slice(&[0, 0, 0, 1, header, second]);
                } else if self.frame.is_empty() {
                    self.damaged = true;
                }
                self.frame.extend_from_slice(&payload[3..]);
            }
            _ => {}
        }
    }

    pub(crate) fn fragment(&mut self, kind: u8, flags: u8, has_data: bool) {
        let start = flags & 0x80 != 0;
        let end = flags & 0x40 != 0;
        let valid_kind = match self.codec {
            Codec::H264 => (1..=23).contains(&kind),
            Codec::H265 => kind <= 47,
            _ => false,
        };
        self.damaged |= (self.codec == Codec::H265 && !has_data) || !valid_kind || (start && end);
        if start {
            self.damaged |= self.nal_open.is_some();
            self.nal_open = Some(kind);
        } else {
            self.damaged |= self.nal_open != Some(kind);
        }
        if end {
            self.nal_open = None;
        }
    }

    pub(crate) fn h264(&mut self, payload: &[u8]) {
        let Some(&indicator) = payload.first() else {
            self.damaged = true;
            return;
        };
        let unit = |frame: &mut Vec<u8>, keyframe: &mut bool, nal: &[u8]| {
            *keyframe |= nal.first().is_some_and(|header| header & 0x1f == 5);
            frame.extend_from_slice(&[0, 0, 0, 1]);
            frame.extend_from_slice(nal);
        };
        match indicator & 0x1f {
            1..=23 => {
                self.damaged |= self.nal_open.is_some();
                unit(&mut self.frame, &mut self.keyframe, payload);
            }
            // STAP-A: sizes and units after the indicator.
            24 => {
                self.damaged |= self.nal_open.is_some();
                let mut rest = &payload[1..];
                while rest.len() >= 2 {
                    let size = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
                    let Some(nal) = rest.get(2..2 + size) else {
                        self.damaged = true;
                        return;
                    };
                    if size == 0 {
                        self.damaged = true;
                        return;
                    }
                    unit(&mut self.frame, &mut self.keyframe, nal);
                    rest = &rest[2 + size..];
                }
                self.damaged |= !rest.is_empty();
            }
            // FU-A: one unit across packets, its header rebuilt from the
            // indicator and the fragment header.
            28 => {
                let Some(&header) = payload.get(1) else {
                    self.damaged = true;
                    return;
                };
                let data = &payload[2..];
                self.fragment(header & 0x1f, header, !data.is_empty());
                if header & 0x80 != 0 {
                    let nal_header = (indicator & 0xe0) | (header & 0x1f);
                    self.keyframe |= header & 0x1f == 5;
                    self.frame.extend_from_slice(&[0, 0, 0, 1, nal_header]);
                } else if self.frame.is_empty() {
                    // A continuation without its start.
                    self.damaged = true;
                }
                self.frame.extend_from_slice(data);
            }
            _ => {}
        }
    }
}

/// A picture loss indication from `sender` about `media`: RTCP asking for a keyframe.
pub fn picture_loss(sender: u32, media: u32) -> Vec<u8> {
    let mut packet = vec![0x81, 206, 0, 2];
    packet.extend_from_slice(&sender.to_be_bytes());
    packet.extend_from_slice(&media.to_be_bytes());
    packet
}
