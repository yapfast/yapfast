//! `rtp.rs`'s packetize.

use super::*;

/// Frame bytes per packet, leaving room for headers, the tag and the nonce
/// within a typical path MTU.
pub(crate) const CHUNK: usize = 1100;
/// The stream id op 12 declares a stream under. Each video packet names it
/// in an extension, which is how Discord ties the packet to the stream.
pub const STREAM_RID: &str = "100";
/// One-byte extension ids in the numbering a Discord client sends with,
/// which isn't the numbering in Discord's WebRTC SDP: there id 5 is playout
/// delay, here it's the transport-wide sequence number, and a packet with a
/// malformed one is dropped. Discord's own clients also send a playout
/// delay of 0/0 as id 6; from Yapfast, Discord's iOS app shows that stream
/// black, so Yapfast leaves it out.
pub(crate) const TRANSPORT_SEQUENCE: u8 = 5;
pub(crate) const CONTENT_TYPE: u8 = 7;
pub(crate) const RID: u8 = 11;
/// The stream id on a retransmission, in place of [`RID`].
pub(crate) const REPAIRED_RID: u8 = 12;
/// The extension block's size: its preamble, then three words of elements.
pub(crate) const EXTENSIONS: usize = 4 + 12;

/// Cuts encoded frames into RTP packets for one SSRC.
pub struct Packetizer {
    pub(crate) ssrc: u32,
    pub(crate) codec: Codec,
    pub(crate) sequence: u16,
    /// Retransmissions' own numbering, on the RTX SSRC.
    pub(crate) rtx_sequence: u16,
    /// Counts every packet sent, for Discord's congestion control.
    pub(crate) transport: u16,
    pub(crate) picture: u16,
}

impl Packetizer {
    pub fn new(ssrc: u32, codec: Codec) -> Self {
        Self {
            ssrc,
            codec,
            sequence: rand::random(),
            rtx_sequence: rand::random(),
            transport: 1,
            picture: rand::random::<u16>() & 0x7fff,
        }
    }

    /// Packetizes frames in `codec` from the next one, keeping the numbering.
    pub fn set_codec(&mut self, codec: Codec) {
        self.codec = codec;
    }

    /// The packets carrying `frame`, unencrypted, with `timestamp` in the
    /// 90 kHz clock. The last one has the marker bit. AV1 marks a keyframe
    /// in its first packet; the other codecs' payloads show it themselves.
    ///
    /// [`Self::plan`] and [`Self::write`] do the same a packet at a time,
    /// into buffers kept from one frame to the next; this gives each packet
    /// a vector of its own, and is for the tests and the odd packet.
    pub fn packets(&mut self, frame: &[u8], timestamp: u32, keyframe: bool) -> Vec<Vec<u8>> {
        let mut plan = self.plan(frame, timestamp, keyframe);
        let mut out = Vec::new();
        let mut packets = Vec::with_capacity(plan.packets.len());
        while self.write(&mut plan, frame, &mut out).is_some() {
            packets.push(out.clone());
        }
        packets
    }

    /// Describes the packets `frame` goes out in, with `timestamp` in the
    /// 90 kHz clock, as where each of their bytes comes from rather than the
    /// bytes: a plan borrows nothing of the frame, so a send loop can hold it
    /// across an await. Empty for a frame nothing goes out for.
    pub fn plan(&self, frame: &[u8], timestamp: u32, keyframe: bool) -> Plan {
        let mut plan = Plan::default();
        self.plan_into(&mut plan, frame, timestamp, keyframe);
        plan
    }

    /// As [`Self::plan`], into the plan the last frame used, which lends this
    /// one's packets the room they need instead of asking for more.
    pub fn plan_into(&self, plan: &mut Plan, frame: &[u8], timestamp: u32, keyframe: bool) {
        plan.clear();
        plan.payload_type = self.codec.payload_types().0;
        plan.timestamp = timestamp;
        match self.codec {
            Codec::Vp8 => plan.vp8(frame, self.picture),
            Codec::H264 => plan.h264(frame),
            Codec::H265 => plan.h265(frame),
            Codec::Av1 => plan.av1(frame, keyframe),
        }
    }

    /// Writes the plan's next packet into `out`, which it empties, and says
    /// whether that was the frame's last. None once the frame has no packets
    /// left to write.
    ///
    /// The packet's header and extensions go around its payload, which is
    /// copied once; `out` is given room for all of them and for the tag and
    /// counter [`Transport::seal`] adds, so a packet is sealed where it was
    /// written and a buffer that carried one frame carries the next one too.
    pub fn write(&mut self, plan: &mut Plan, frame: &[u8], out: &mut Vec<u8>) -> Option<bool> {
        let first = plan.packets.get(plan.next).copied()?;
        let next = plan.next + 1;
        plan.next = next;
        let last = next == plan.packets.len();
        let end = plan.packets.get(next).copied().unwrap_or(plan.chunks.len());
        let chunks = &plan.chunks[first..end];
        let body: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        out.clear();
        out.reserve(HEADER + EXTENSIONS + body + TAG + 4);
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        self.write_header(
            out,
            plan.payload_type,
            last,
            sequence,
            plan.timestamp,
            self.ssrc,
            RID,
        );
        for chunk in chunks {
            match *chunk {
                Chunk::Written { at, len } => out.extend_from_slice(&plan.written[at..at + len]),
                Chunk::Copied { at, len } => out.extend_from_slice(&frame[at..at + len]),
            }
        }
        // VP8 numbers a frame's chunks with one picture id, moving on for
        // the next frame.
        if last && self.codec == Codec::Vp8 {
            self.picture = (self.picture + 1) & 0x7fff;
        }
        Some(last)
    }

    /// A packet sent before, again: on `rtx_ssrc` with the RTX payload type
    /// and numbering, the original sequence number before its payload
    /// (RFC 4588), and the repaired stream id. None for a packet that isn't
    /// this packetizer's video.
    pub fn retransmit(&mut self, original: &[u8], rtx_ssrc: u32) -> Option<Vec<u8>> {
        let sent = parse(original)?;
        // In the codec the packet went out in, which may not be today's.
        let codec = Codec::from_payload_type(sent.payload_type)?;
        let rtx_payload_type = codec.payload_types().1;
        if sent.ssrc != self.ssrc {
            return None;
        }
        let payload = [&sent.sequence.to_be_bytes()[..], sent.payload].concat();
        let packet = self.header(
            rtx_payload_type,
            sent.marker,
            self.rtx_sequence,
            sent.timestamp,
            rtx_ssrc,
            REPAIRED_RID,
            &payload,
        );
        self.rtx_sequence = self.rtx_sequence.wrapping_add(1);
        Some(packet)
    }

    /// An RTP packet with the extensions a Discord client sends, naming
    /// the stream with `rid`, and the next transport sequence number.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn header(
        &mut self,
        payload_type: u8,
        marker: bool,
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        rid: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(HEADER + EXTENSIONS + payload.len() + 20);
        self.write_header(
            &mut packet,
            payload_type,
            marker,
            sequence,
            timestamp,
            ssrc,
            rid,
        );
        packet.extend_from_slice(payload);
        packet
    }

    /// A packet's header and extensions, after which its payload goes, and
    /// the next transport sequence number.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_header(
        &mut self,
        packet: &mut Vec<u8>,
        payload_type: u8,
        marker: bool,
        sequence: u16,
        timestamp: u32,
        ssrc: u32,
        rid: u8,
    ) {
        // Version 2 with the extension bit.
        packet.extend_from_slice(&[0x90, u8::from(marker) << 7 | payload_type]);
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&ssrc.to_be_bytes());
        packet.extend_from_slice(&[0xbe, 0xde, 0, ((EXTENSIONS - 4) / 4) as u8]);
        let mut extension = |id: u8, value: &[u8]| {
            packet.push(id << 4 | (value.len() - 1) as u8);
            packet.extend_from_slice(value);
        };
        extension(TRANSPORT_SEQUENCE, &self.transport.to_be_bytes());
        // Screen content.
        extension(CONTENT_TYPE, &[1]);
        extension(rid, STREAM_RID.as_bytes());
        packet.resize(HEADER + EXTENSIONS, 0);
        self.transport = self.transport.wrapping_add(1);
    }
}

/// The packets one frame goes out in, for [`Packetizer::write`] to write one
/// at a time.
///
/// A plan holds where each packet's bytes come from rather than the bytes, so
/// that it borrows nothing of the frame and can wait across an await, and it
/// keeps the room its last frame needed for the next frame's packets.
#[derive(Default)]
pub struct Plan {
    /// The payload type and timestamp every packet of the frame carries.
    pub(crate) payload_type: u8,
    pub(crate) timestamp: u32,
    /// Where each packet's chunks begin in `chunks`: a packet runs to the
    /// next one's start, or to the end.
    pub(crate) packets: Vec<usize>,
    /// What each packet's payload is made of, one packet's after another's.
    pub(crate) chunks: Vec<Chunk>,
    /// The packets' own bytes, which their chunks point into: payload
    /// descriptors, fragmentation headers, aggregation headers and lengths.
    pub(crate) written: Vec<u8>,
    /// The packet `write` gives out next.
    pub(crate) next: usize,
    /// The pieces of the AV1 packet under way.
    pub(crate) pieces: Vec<Piece>,
}

impl Plan {
    pub(crate) fn clear(&mut self) {
        self.packets.clear();
        self.chunks.clear();
        self.written.clear();
        self.pieces.clear();
        self.next = 0;
    }

    /// Starts a packet whose payload begins with `bytes`.
    pub(crate) fn begin(&mut self, bytes: &[u8]) {
        self.packets.push(self.chunks.len());
        self.literal(bytes);
    }

    /// The plan's own bytes as the packet's next run.
    pub(crate) fn literal(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let at = self.written.len();
        self.written.extend_from_slice(bytes);
        self.chunks.push(Chunk::Written {
            at,
            len: bytes.len(),
        });
    }

    /// A run of the frame as the packet's next.
    pub(crate) fn copied(&mut self, at: usize, len: usize) {
        if len > 0 {
            self.chunks.push(Chunk::Copied { at, len });
        }
    }

    /// VP8: each packet after the extended payload descriptor with a 15-bit
    /// picture id; S marks the start of the frame's first partition.
    pub(crate) fn vp8(&mut self, frame: &[u8], picture: u16) {
        let mut at = 0;
        while at < frame.len() {
            let len = CHUNK.min(frame.len() - at);
            let start = if at == 0 { 0x10 } else { 0 };
            let mut descriptor = [0x80 | start, 0x80, 0, 0];
            descriptor[2..].copy_from_slice(&(0x8000 | picture).to_be_bytes());
            self.begin(&descriptor);
            self.copied(at, len);
            at += len;
        }
    }

    /// H264 in packetization mode 1: a NAL unit that fits goes whole, a
    /// bigger one in fragmentation units (FU-A).
    pub(crate) fn h264(&mut self, frame: &[u8]) {
        for unit in NalUnits::new(frame) {
            if unit.len() <= CHUNK {
                self.begin(&[]);
                self.copied(unit.start, unit.len());
                continue;
            }
            let (header, body) = (frame[unit.start], unit.start + 1);
            let mut at = body;
            while at < unit.end {
                let len = (CHUNK - 2).min(unit.end - at);
                // The indicator keeps the unit's F and NRI bits; the FU
                // header its type, with start and end flags.
                let mut fragment = header & 0x1f;
                if at == body {
                    fragment |= 0x80;
                }
                if at + len == unit.end {
                    fragment |= 0x40;
                }
                self.begin(&[(header & 0xe0) | 28, fragment]);
                self.copied(at, len);
                at += len;
            }
        }
    }

    /// RFC 7798's payloads for an H265 access unit: each NAL unit whole when
    /// it fits, and a larger one as fragmentation units (type 49), whose
    /// payload header keeps the unit's F bit, layer and temporal id, and
    /// whose FU header carries its type with start and end flags.
    pub(crate) fn h265(&mut self, frame: &[u8]) {
        for unit in NalUnits::new(frame) {
            if unit.len() <= CHUNK {
                self.begin(&[]);
                self.copied(unit.start, unit.len());
                continue;
            }
            let (first, second) = (frame[unit.start], frame[unit.start + 1]);
            let kind = (first >> 1) & 0x3f;
            let body = unit.start + 2;
            let mut at = body;
            while at < unit.end {
                let len = (CHUNK - 3).min(unit.end - at);
                let mut fragment = kind;
                if at == body {
                    fragment |= 0x80;
                }
                if at + len == unit.end {
                    fragment |= 0x40;
                }
                self.begin(&[(first & 0x81) | (49 << 1), second, fragment]);
                self.copied(at, len);
                at += len;
            }
        }
    }

    /// AV1 payloads as AOM's RTP payload format lays them out: an aggregation
    /// header, then OBU elements, each after its LEB128 length but for the
    /// last when the packet holds three or fewer (W). An element too large
    /// for a packet carries on in the next: Y marks the packet it continues
    /// from, Z the one it continues in. N marks a keyframe's first packet
    /// when it starts with the sequence header, as libwebrtc's packetizer
    /// marks it, and receivers take a frame starting so for a keyframe. OBUs
    /// of different temporal or spatial layers never share a packet. A
    /// malformed frame gives no packets, so none of it goes out.
    pub(crate) fn av1(&mut self, frame: &[u8], keyframe: bool) {
        if frame.len() > MAX_FRAME {
            tracing::warn!(size = frame.len(), "a malformed AV1 frame");
            return;
        }
        // Whether the first packet starts a coded video sequence: a
        // keyframe's does when it begins with the sequence header.
        let (mut starts_sequence, mut first) = (keyframe, true);
        // Each packet's pieces of elements, whether its first piece
        // continues the last packet's and its last carries on in the next.
        let (mut used, mut continues) = (1, false);
        // The temporal and spatial ids of the packet's OBUs that give them.
        let mut packet_layer = None;
        let mut at = 0;
        while at < frame.len() {
            let Some((element, next)) = read_obu(frame, at) else {
                tracing::warn!(size = frame.len(), "a malformed AV1 frame");
                self.clear();
                return;
            };
            at = next;
            let Some(element) = element else {
                continue;
            };
            if first {
                starts_sequence &= (frame[element.at] >> 3) & 0x0f == OBU_SEQUENCE_HEADER;
                first = false;
            }
            let extension = frame[element.at] & OBU_EXTENSION != 0;
            let layer = extension.then(|| frame[element.at + 1] & 0xf8);
            if layer.is_some() && packet_layer.is_some() && layer != packet_layer {
                self.end_packet(frame, continues, false, starts_sequence);
                (used, continues, packet_layer) = (1, false, None);
            }
            packet_layer = packet_layer.or(layer);
            let (mut from, to) = (0, element.header + element.len);
            loop {
                // Room for a piece after a length of up to two bytes, which
                // covers anything up to CHUNK.
                if CHUNK.saturating_sub(used) < 3 {
                    self.end_packet(frame, continues, false, starts_sequence);
                    (used, continues, packet_layer) = (1, false, layer);
                }
                let take = (to - from).min(CHUNK - used - 2);
                self.pieces.push(Piece {
                    element,
                    from,
                    to: from + take,
                });
                used += leb128_size(take) + take;
                from += take;
                if from == to {
                    break;
                }
                self.end_packet(frame, continues, true, starts_sequence);
                (used, continues, packet_layer) = (1, true, layer);
            }
        }
        if !self.pieces.is_empty() {
            self.end_packet(frame, continues, false, starts_sequence);
        }
    }

    /// Ends the packet under way: its aggregation header, then each of its
    /// pieces after its length, the last one's left out when the packet is of
    /// three elements or fewer.
    pub(crate) fn end_packet(
        &mut self,
        frame: &[u8],
        continues: bool,
        carries_on: bool,
        starts_sequence: bool,
    ) {
        let count = if self.pieces.len() <= 3 {
            self.pieces.len()
        } else {
            0
        };
        let mut header = (count as u8) << 4;
        if continues {
            header |= 0x80;
        }
        if carries_on {
            header |= 0x40;
        }
        if self.packets.is_empty() && starts_sequence {
            header |= 0x08;
        }
        self.begin(&[header]);
        for index in 0..self.pieces.len() {
            let piece = self.pieces[index];
            if count == 0 || index + 1 < count {
                let at = self.written.len();
                write_leb128(piece.to - piece.from, &mut self.written);
                self.chunks.push(Chunk::Written {
                    at,
                    len: self.written.len() - at,
                });
            }
            self.piece(frame, piece);
        }
        self.pieces.clear();
    }

    /// One piece of an element: its header's first byte without the size flag
    /// the OBU gave, the rest of its header, then the payload it reaches.
    pub(crate) fn piece(&mut self, frame: &[u8], piece: Piece) {
        let Element {
            at, header, body, ..
        } = piece.element;
        let mut from = piece.from;
        if from == 0 {
            // Laid out as if the OBU never gave its size, so its first byte
            // loses the flag that said it did.
            let cleared = self.written.len();
            self.written.push(frame[at] & !OBU_HAS_SIZE);
            self.chunks.push(Chunk::Written {
                at: cleared,
                len: 1,
            });
            from = 1;
        }
        if from < header {
            let end = piece.to.min(header);
            self.copied(at + from, end - from);
            from = end;
        }
        if from < piece.to {
            self.copied(body + (from - header), piece.to - from);
        }
    }
}

/// One run of a packet's payload.
#[derive(Clone, Copy)]
pub(crate) enum Chunk {
    /// Bytes of the plan's own: a payload descriptor, a fragmentation header,
    /// an aggregation header or a length.
    Written { at: usize, len: usize },
    /// A run of the frame, as it is.
    Copied { at: usize, len: usize },
}

impl Chunk {
    /// How many bytes of the packet's payload the run takes.
    pub(crate) fn len(self) -> usize {
        match self {
            Self::Written { len, .. } | Self::Copied { len, .. } => len,
        }
    }
}

/// A run of one AV1 element in the packet under way, numbered in the bytes
/// the element makes: its header first, then its payload.
#[derive(Clone, Copy)]
pub(crate) struct Piece {
    pub(crate) element: Element,
    pub(crate) from: usize,
    pub(crate) to: usize,
}

// The shapes a frame's bytes come in, and the header the packetizer writes,
// are the media crate's: a decoder there reads the same units.
pub use yapfast_media::bitstream::{nal_units, obu_elements};
pub(crate) use yapfast_media::bitstream::{NalUnits, read_leb128, read_obu};
pub(crate) use yapfast_media::rtp::{HEADER, VIDEO_ORIENTATION, orientation};
pub use yapfast_media::rtp::Orientation;

pub(crate) fn write_leb128(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn leb128_size(value: usize) -> usize {
    (usize::BITS - value.max(1).leading_zeros()).div_ceil(7) as usize
}
