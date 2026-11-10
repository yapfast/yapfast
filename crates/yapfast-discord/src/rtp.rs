//! RTP for Go Live video: packetizing and reassembling VP8 (RFC 7741),
//! H264 (RFC 6184), H265 (RFC 7798) and AV1 (AOM's RTP payload format)
//! frames, Discord's "rtpsize" transport encryption, and the
//! RTCP that goes with them.

use std::time::{Duration, Instant};

use aes_gcm::{Aes256Gcm, KeyInit, aead::AeadInPlace};
use anyhow::{Result, anyhow, bail};
use chacha20poly1305::XChaCha20Poly1305;

/// The payload types Yapfast offers Discord, as Discord's clients number them.
pub const OPUS: u8 = 120;
pub const H264: u8 = 101;
pub const H264_RTX: u8 = 102;
pub const VP8: u8 = 105;
pub const VP8_RTX: u8 = 106;
pub const H265: u8 = 103;
pub const H265_RTX: u8 = 104;
pub const AV1: u8 = 107;
pub const AV1_RTX: u8 = 108;

/// The video codecs Yapfast sends or receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Codec {
    Vp8,
    H264,
    H265,
    Av1,
}

impl Codec {
    pub fn from_payload_type(payload_type: u8) -> Option<Self> {
        match payload_type {
            VP8 => Some(Self::Vp8),
            H264 => Some(Self::H264),
            H265 => Some(Self::H265),
            AV1 => Some(Self::Av1),
            _ => None,
        }
    }

    /// As Discord names it in session descriptions.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "VP8" => Some(Self::Vp8),
            "H264" => Some(Self::H264),
            "H265" => Some(Self::H265),
            "AV1" => Some(Self::Av1),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Vp8 => "VP8",
            Self::H264 => "H264",
            Self::H265 => "H265",
            Self::Av1 => "AV1",
        }
    }

    /// The payload types of the codec and of its retransmissions.
    pub fn payload_types(self) -> (u8, u8) {
        match self {
            Self::Vp8 => (VP8, VP8_RTX),
            Self::H264 => (H264, H264_RTX),
            Self::H265 => (H265, H265_RTX),
            Self::Av1 => (AV1, AV1_RTX),
        }
    }
}

/// Frame bytes per packet, leaving room for headers, the tag and the nonce
/// within a typical path MTU.
const CHUNK: usize = 1100;
/// RTP's fixed header.
const HEADER: usize = 12;
/// The stream id op 12 declares a stream under. Each video packet names it
/// in an extension, which is how Discord ties the packet to the stream.
pub const STREAM_RID: &str = "100";
/// One-byte extension ids in the numbering a Discord client sends with,
/// which isn't the numbering in Discord's WebRTC SDP: there id 5 is playout
/// delay, here it's the transport-wide sequence number, and a packet with a
/// malformed one is dropped. Discord's own clients also send a playout
/// delay of 0/0 as id 6; from Yapfast, Discord's iOS app shows that stream
/// black, so Yapfast leaves it out.
const TRANSPORT_SEQUENCE: u8 = 5;
const CONTENT_TYPE: u8 = 7;
const RID: u8 = 11;
/// The stream id on a retransmission, in place of [`RID`].
const REPAIRED_RID: u8 = 12;
/// `urn:3gpp:video-orientation` (CVO), which Discord's native clients
/// number 4 (docs.discord.food, Voice Connections, "Native RTP Header
/// Extensions"; its WebRTC SDP says 13). Phones film in the sensor's
/// orientation and say here how to turn each frame upright.
const VIDEO_ORIENTATION: u8 = 4;
/// The extension block's size: its preamble, then three words of elements.
const EXTENSIONS: usize = 4 + 12;

/// An RTP packet carrying one Opus frame from `ssrc`, with `timestamp` in
/// 48 kHz samples and no extensions, as a call's audio goes. The marker
/// starts a burst of sound.
pub fn opus_packet(ssrc: u32, sequence: u16, timestamp: u32, marker: bool, opus: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();
    write_audio_into(&mut packet, ssrc, sequence, timestamp, marker, opus);
    packet
}

/// Writes one Opus frame's packet into `packet`, as [`opus_packet`] makes it,
/// reusing the room the buffer already has: a call and a stream send a frame
/// every 20 ms for as long as they last, and the buffer they send from keeps
/// its capacity from one frame to the next, so after the first few they
/// allocate for none of them. The room it reserves past the payload is for
/// [`Transport::seal`]'s tag and counter, which follow the frame.
pub fn write_audio_into(
    packet: &mut Vec<u8>,
    ssrc: u32,
    sequence: u16,
    timestamp: u32,
    marker: bool,
    opus: &[u8],
) {
    packet.clear();
    packet.reserve(HEADER + opus.len() + 20);
    packet.extend_from_slice(&[0x80, u8::from(marker) << 7 | OPUS]);
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());
    packet.extend_from_slice(opus);
}

/// Cuts encoded frames into RTP packets for one SSRC.
pub struct Packetizer {
    ssrc: u32,
    codec: Codec,
    sequence: u16,
    /// Retransmissions' own numbering, on the RTX SSRC.
    rtx_sequence: u16,
    /// Counts every packet sent, for Discord's congestion control.
    transport: u16,
    picture: u16,
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
    fn header(
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
    fn write_header(
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
    payload_type: u8,
    timestamp: u32,
    /// Where each packet's chunks begin in `chunks`: a packet runs to the
    /// next one's start, or to the end.
    packets: Vec<usize>,
    /// What each packet's payload is made of, one packet's after another's.
    chunks: Vec<Chunk>,
    /// The packets' own bytes, which their chunks point into: payload
    /// descriptors, fragmentation headers, aggregation headers and lengths.
    written: Vec<u8>,
    /// The packet `write` gives out next.
    next: usize,
    /// The pieces of the AV1 packet under way.
    pieces: Vec<Piece>,
}

impl Plan {
    fn clear(&mut self) {
        self.packets.clear();
        self.chunks.clear();
        self.written.clear();
        self.pieces.clear();
        self.next = 0;
    }

    /// Starts a packet whose payload begins with `bytes`.
    fn begin(&mut self, bytes: &[u8]) {
        self.packets.push(self.chunks.len());
        self.literal(bytes);
    }

    /// The plan's own bytes as the packet's next run.
    fn literal(&mut self, bytes: &[u8]) {
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
    fn copied(&mut self, at: usize, len: usize) {
        if len > 0 {
            self.chunks.push(Chunk::Copied { at, len });
        }
    }

    /// VP8: each packet after the extended payload descriptor with a 15-bit
    /// picture id; S marks the start of the frame's first partition.
    fn vp8(&mut self, frame: &[u8], picture: u16) {
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
    fn h264(&mut self, frame: &[u8]) {
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
    fn h265(&mut self, frame: &[u8]) {
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
    fn av1(&mut self, frame: &[u8], keyframe: bool) {
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
    fn end_packet(
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
    fn piece(&mut self, frame: &[u8], piece: Piece) {
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
enum Chunk {
    /// Bytes of the plan's own: a payload descriptor, a fragmentation header,
    /// an aggregation header or a length.
    Written { at: usize, len: usize },
    /// A run of the frame, as it is.
    Copied { at: usize, len: usize },
}

impl Chunk {
    /// How many bytes of the packet's payload the run takes.
    fn len(self) -> usize {
        match self {
            Self::Written { len, .. } | Self::Copied { len, .. } => len,
        }
    }
}

/// A run of one AV1 element in the packet under way, numbered in the bytes
/// the element makes: its header first, then its payload.
#[derive(Clone, Copy)]
struct Piece {
    element: Element,
    from: usize,
    to: usize,
}

/// The NAL units of an Annex B access unit, without their start codes. As
/// libwebrtc finds them: a start code is 0 0 1, with at most one more zero
/// before it, so bytes of a unit that happen to be zero stay in it.
pub fn nal_units(frame: &[u8]) -> Vec<&[u8]> {
    NalUnits::new(frame).map(|unit| &frame[unit]).collect()
}

/// Those units one at a time, as the ranges of the frame they take, so the
/// packetizer plans a frame without a vector of its units.
struct NalUnits<'a> {
    frame: &'a [u8],
    /// Where to look for the next start code.
    at: usize,
    /// The start code found but not yet a unit's: where it began, and where
    /// the bytes after it begin.
    found: Option<(usize, usize)>,
}

impl<'a> NalUnits<'a> {
    fn new(frame: &'a [u8]) -> Self {
        Self {
            frame,
            at: 0,
            found: None,
        }
    }

    /// The next start code from `at`, which moves past it.
    fn find(&mut self) -> Option<(usize, usize)> {
        while self.at + 3 <= self.frame.len() {
            let at = self.at;
            if self.frame[at..at + 3] == [0, 0, 1] {
                let code = if at > 0 && self.frame[at - 1] == 0 {
                    at - 1
                } else {
                    at
                };
                self.at = at + 3;
                return Some((code, at + 3));
            }
            self.at = at + 1;
        }
        None
    }
}

impl Iterator for NalUnits<'_> {
    type Item = std::ops::Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // The code that opens the unit, and the one that ends it.
            let (_, start) = match self.found.take() {
                Some(found) => found,
                None => self.find()?,
            };
            let end = match self.find() {
                Some(found) => {
                    self.found = Some(found);
                    found.0
                }
                None => self.frame.len(),
            };
            // Two start codes in a row make a unit of no bytes, which isn't
            // one.
            if end > start {
                return Some(start..end);
            }
        }
    }
}

/// AV1 OBU types the packetizer and DAVE handle apart.
const OBU_SEQUENCE_HEADER: u8 = 1;
const OBU_TEMPORAL_DELIMITER: u8 = 2;
const OBU_TILE_LIST: u8 = 8;
const OBU_PADDING: u8 = 15;
/// An OBU header's extension and size flags.
const OBU_EXTENSION: u8 = 0x04;
const OBU_HAS_SIZE: u8 = 0x02;

/// One OBU of an AV1 temporal unit, as the packetizer hands it on: its header
/// without the size flag, then its payload, which the frame keeps apart from
/// it behind the size.
#[derive(Clone, Copy)]
struct Element {
    /// Where the OBU's header is in the frame, and how long the header is.
    at: usize,
    header: usize,
    /// Where its payload is, and how long it is.
    body: usize,
    len: usize,
}

impl Element {
    /// The element's bytes: its header without the size flag, then its
    /// payload.
    fn bytes(self, frame: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.header + self.len);
        bytes.push(frame[self.at] & !OBU_HAS_SIZE);
        bytes.extend_from_slice(&frame[self.at + 1..self.at + self.header]);
        bytes.extend_from_slice(&frame[self.body..self.body + self.len]);
        bytes
    }
}

/// Reads the OBU at `from` in `frame`: the element it makes, unless its type
/// is one the packetizer drops, and where the OBU after it begins. None for
/// an OBU that runs past the frame or sets a bit its header keeps clear.
fn read_obu(frame: &[u8], from: usize) -> Option<(Option<Element>, usize)> {
    let rest = &frame[from..];
    let &header = rest.first()?;
    // The forbidden bit and the reserved one.
    if header & 0x81 != 0 {
        return None;
    }
    let header_size = if header & OBU_EXTENSION != 0 { 2 } else { 1 };
    let headers = rest.get(..header_size)?;
    // The extension's reserved bits, after the temporal and spatial ids.
    if headers
        .get(1)
        .is_some_and(|extension| extension & 0x07 != 0)
    {
        return None;
    }
    let rest = &rest[header_size..];
    let (size, used) = if header & OBU_HAS_SIZE != 0 {
        read_leb128(rest)?
    } else {
        (rest.len(), 0)
    };
    let body = from + header_size + used;
    if size > frame.len() - body {
        return None;
    }
    let dropped = matches!(
        (header >> 3) & 0x0f,
        OBU_TEMPORAL_DELIMITER | OBU_TILE_LIST | OBU_PADDING
    );
    let element = (!dropped).then_some(Element {
        at: from,
        header: header_size,
        body,
        len: size,
    });
    Some((element, body + size))
}

/// The OBUs of an AV1 temporal unit, each as an OBU element: its header
/// (and extension) without the size flag, then its payload. As libwebrtc's
/// packetizer reads them: an OBU without a size runs to the end of the
/// frame, as DAVE leaves the last one with its trailer after it; temporal
/// delimiters, tile lists and padding are dropped. None when an OBU runs
/// past the frame, or sets a bit its header keeps clear.
pub fn obu_elements(frame: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut elements = vec![];
    let mut at = 0;
    while at < frame.len() {
        let (element, next) = read_obu(frame, at)?;
        at = next;
        if let Some(element) = element {
            elements.push(element.bytes(frame));
        }
    }
    Some(elements)
}

/// An unsigned LEB128 number and the bytes it took: at most eight, as AV1
/// allows.
fn read_leb128(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0u64;
    for (index, &byte) in bytes.iter().enumerate().take(8) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((usize::try_from(value).ok()?, index + 1));
        }
    }
    None
}

fn write_leb128(mut value: usize, out: &mut Vec<u8>) {
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

fn leb128_size(value: usize) -> usize {
    (usize::BITS - value.max(1).leading_zeros()).div_ceil(7) as usize
}

/// How to turn a camera's frame to show it upright, from the sender's
/// video orientation (3GPP TS 26.114, 7.4.5): a byte `0000CFRR`, the
/// quarter turns clockwise in `RR` and a left-right flip in `F`. The camera
/// bit `C` (front or back) changes nothing shown. libwebrtc, which Discord's
/// apps are built on, sends it on a frame's last packet whenever it isn't
/// upright, and once more when it turns back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Orientation {
    /// Quarter turns clockwise, 0 to 3.
    pub turns: u8,
    /// Flipped left to right, after turning.
    pub flip: bool,
}

impl Orientation {
    pub fn from_cvo(byte: u8) -> Self {
        Self {
            turns: byte & 0x03,
            flip: byte & 0x04 != 0,
        }
    }
}

/// The video orientation among a packet's header extensions, `profile`
/// being the extension block's first word: one-byte elements (`0xBEDE`,
/// RFC 8285), as Discord sends, or two-byte ones (`0x100x`).
fn orientation(profile: u16, mut elements: &[u8]) -> Option<Orientation> {
    let two_byte = profile & 0xfff0 == 0x1000;
    if profile != 0xbede && !two_byte {
        return None;
    }
    while let Some((&first, rest)) = elements.split_first() {
        // A zero byte is padding in either form.
        if first == 0 {
            elements = rest;
            continue;
        }
        let (id, length, rest) = if two_byte {
            let (&length, rest) = rest.split_first()?;
            (first, usize::from(length), rest)
        } else if first >> 4 == 15 {
            // Reserved: the block ends.
            return None;
        } else {
            (first >> 4, usize::from(first & 0x0f) + 1, rest)
        };
        let value = rest.get(..length)?;
        if id == VIDEO_ORIENTATION
            && let Some(&byte) = value.first()
        {
            return Some(Orientation::from_cvo(byte));
        }
        elements = &rest[length..];
    }
    None
}

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
const MAX_FRAME: usize = 8 << 20;

/// Puts one SSRC's packets back together into frames. A frame missing a
/// packet is dropped, and frames are dropped until the next keyframe.
pub struct Assembler {
    codec: Codec,
    frame: Vec<u8>,
    timestamp: Option<u32>,
    expected: Option<u16>,
    damaged: bool,
    keyframe: bool,
    waiting: bool,
    /// AV1: the frame's latest OBU element, written once the next begins
    /// or the frame ends, and whether it carries on in the next packet.
    obu: Vec<u8>,
    obu_open: bool,
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
            lost = !self.frame.is_empty() || !self.obu.is_empty();
            self.frame.clear();
            self.obu.clear();
            self.obu_open = false;
            self.timestamp = Some(packet.timestamp);
            self.damaged = gap;
            self.keyframe = false;
        } else {
            self.damaged |= gap;
        }
        let first = self.frame.is_empty();
        match self.codec {
            Codec::Vp8 => self.vp8(packet.payload, first),
            Codec::H264 => self.h264(packet.payload),
            Codec::H265 => self.h265(packet.payload),
            Codec::Av1 => self.av1(packet.payload),
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
        let data = std::mem::take(&mut self.frame);
        self.timestamp = None;
        if self.damaged || data.is_empty() {
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

    fn vp8(&mut self, payload: &[u8], first: bool) {
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
    fn av1(&mut self, payload: &[u8]) {
        let Some((&header, mut rest)) = payload.split_first() else {
            self.damaged = true;
            return;
        };
        let (continues, carries_on) = (header & 0x80 != 0, header & 0x40 != 0);
        let count = usize::from((header >> 4) & 0x03);
        self.keyframe |= header & 0x08 != 0;
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
        self.obu_open = carries_on;
    }

    /// Writes the OBU element held as an OBU, with its size or without.
    fn write_obu(&mut self, sized: bool) {
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
    fn h265(&mut self, payload: &[u8]) {
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
            0..=47 => unit(&mut self.frame, &mut self.keyframe, payload),
            48 => {
                let mut rest = &payload[2..];
                while rest.len() >= 2 {
                    let size = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
                    let Some(nal) = rest.get(2..2 + size) else {
                        self.damaged = true;
                        return;
                    };
                    unit(&mut self.frame, &mut self.keyframe, nal);
                    rest = &rest[2 + size..];
                }
            }
            49 => {
                let Some(&fragment) = payload.get(2) else {
                    self.damaged = true;
                    return;
                };
                if fragment & 0x80 != 0 {
                    let kind = fragment & 0x3f;
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

    fn h264(&mut self, payload: &[u8]) {
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
            1..=23 => unit(&mut self.frame, &mut self.keyframe, payload),
            // STAP-A: sizes and units after the indicator.
            24 => {
                let mut rest = &payload[1..];
                while rest.len() >= 2 {
                    let size = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
                    let Some(nal) = rest.get(2..2 + size) else {
                        self.damaged = true;
                        return;
                    };
                    unit(&mut self.frame, &mut self.keyframe, nal);
                    rest = &rest[2 + size..];
                }
            }
            // FU-A: one unit across packets, its header rebuilt from the
            // indicator and the fragment header.
            28 => {
                let Some(&header) = payload.get(1) else {
                    self.damaged = true;
                    return;
                };
                let data = &payload[2..];
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

/// An RTCP sender report from `ssrc`: that `timestamp` in the RTP clock
/// is `now` in wall-clock time, after `packets` packets carrying `octets`
/// bytes of payload. Receivers use it to place the stream in time.
pub fn sender_report(
    ssrc: u32,
    now: std::time::SystemTime,
    timestamp: u32,
    packets: u32,
    octets: u32,
) -> Vec<u8> {
    // NTP counts from 1900, the Unix clock from 1970.
    const NTP_UNIX: u64 = 2_208_988_800;
    let since = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = (since.as_secs() + NTP_UNIX) as u32;
    let fraction = ((u64::from(since.subsec_nanos()) << 32) / 1_000_000_000) as u32;
    let mut packet = vec![0x80, 200, 0, 6];
    for word in [ssrc, seconds, fraction, timestamp, packets, octets] {
        packet.extend_from_slice(&word.to_be_bytes());
    }
    packet
}

/// The bytes authenticated but not encrypted at the start of an RTP packet:
/// the fixed header, CSRCs, and the extension preamble but not its body.
pub fn rtp_clear_len(packet: &[u8]) -> Option<usize> {
    let first = *packet.first()?;
    let csrcs = usize::from(first & 0x0f) * 4;
    let extension = if first & 0x10 != 0 { 4 } else { 0 };
    let clear = HEADER + csrcs + extension;
    (packet.len() >= clear).then_some(clear)
}

/// RTCP's clear part: its header and the sender's SSRC.
pub const RTCP_CLEAR: usize = 8;

enum Cipher {
    Aes(Box<Aes256Gcm>),
    XChaCha(XChaCha20Poly1305),
}

/// Discord's transport encryption: the clear part is authenticated data,
/// the rest is encrypted, then the tag and a 32-bit counter, which is the
/// start of the nonce, follow.
pub struct Transport {
    cipher: Cipher,
    counter: u32,
}

const TAG: usize = 16;

impl Transport {
    /// The modes Yapfast asks for, best first.
    pub const MODES: [&str; 2] = ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"];

    pub fn new(mode: &str, key: &[u8]) -> Result<Self> {
        let cipher = match mode {
            "aead_aes256_gcm_rtpsize" => Cipher::Aes(Box::new(
                Aes256Gcm::new_from_slice(key)
                    .map_err(|_| anyhow!("the stream key isn't 32 bytes"))?,
            )),
            "aead_xchacha20_poly1305_rtpsize" => Cipher::XChaCha(
                XChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| anyhow!("the stream key isn't 32 bytes"))?,
            ),
            _ => bail!("Discord chose an encryption mode Yapfast doesn't know: {mode}"),
        };
        Ok(Self {
            cipher,
            counter: rand::random(),
        })
    }

    /// Encrypts `packet` after its first `clear` bytes, in place.
    pub fn seal(&mut self, packet: &mut Vec<u8>, clear: usize) -> Result<()> {
        let counter = self.counter.to_be_bytes();
        self.counter = self.counter.wrapping_add(1);
        let (header, body) = packet.split_at_mut(clear);
        let sealed = match &self.cipher {
            Cipher::Aes(cipher) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(&counter);
                cipher.encrypt_in_place_detached(&nonce, header, body)
            }
            Cipher::XChaCha(cipher) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(&counter);
                cipher.encrypt_in_place_detached(&nonce, header, body)
            }
        };
        let sealed = sealed.map_err(|_| anyhow!("could not encrypt a stream packet"))?;
        // The tag comes back on the stack, so a packet sent allocates only
        // for the bytes of the packet itself.
        let mut tag = [0u8; TAG];
        tag.copy_from_slice(&sealed);
        packet.extend_from_slice(&tag);
        packet.extend_from_slice(&counter);
        Ok(())
    }

    /// Decrypts a sealed packet where it lies, returning its clear part and
    /// plaintext: everything but the tag and the counter, which close it.
    /// The caller's own buffer is the plaintext, which is what makes a call's
    /// hundreds of received packets a second cost no allocation each.
    pub fn open<'a>(&self, packet: &'a mut [u8], clear: usize) -> Option<&'a [u8]> {
        if packet.len() < clear + TAG + 4 {
            return None;
        }
        let sealed = packet.len() - TAG - 4;
        let (header, rest) = packet.split_at_mut(clear);
        let (body, tail) = rest.split_at_mut(sealed - clear);
        let (tag, counter) = tail.split_at(TAG);
        let tag: &[u8] = tag;
        let opened = match &self.cipher {
            Cipher::Aes(cipher) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(counter);
                cipher.decrypt_in_place_detached(&nonce, header, body, tag.into())
            }
            Cipher::XChaCha(cipher) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(counter);
                cipher.decrypt_in_place_detached(&nonce, header, body, tag.into())
            }
        };
        opened.ok()?;
        Some(&packet[..sealed])
    }
}

/// Whether a decrypted compound RTCP packet asks for a keyframe, with a
/// picture loss indication or a full intra request.
pub fn wants_keyframe(mut rtcp: &[u8]) -> bool {
    while rtcp.len() >= 4 {
        let format = rtcp[0] & 0x1f;
        let kind = rtcp[1];
        // Payload-specific feedback: 1 is PLI, 4 is FIR.
        if kind == 206 && (format == 1 || format == 4) {
            return true;
        }
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    false
}

/// The packets in a decrypted compound RTCP packet, as `type/format` and
/// what each says, for the log: 200 is a sender report, 201 a receiver
/// report, 204 an application-defined packet, 206/1 a picture loss
/// indication and 206/15 a receiver's bitrate estimate (REMB).
pub fn rtcp_kinds(mut rtcp: &[u8]) -> Vec<(String, String)> {
    let mut kinds = vec![];
    while rtcp.len() >= 4 {
        let (kind, format) = (rtcp[1], rtcp[0] & 0x1f);
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        let packet = &rtcp[..length.min(rtcp.len())];
        kinds.push((
            format!("{kind}/{format}"),
            describe_rtcp(kind, format, packet),
        ));
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    kinds
}

fn describe_rtcp(kind: u8, format: u8, packet: &[u8]) -> String {
    let word = |at: usize| {
        packet
            .get(at..at + 4)
            .map(|bytes| u32::from_be_bytes(bytes.try_into().unwrap_or_default()))
    };
    match (kind, format) {
        // A sender report's blocks follow its 20 bytes of sender information.
        (200 | 201, _) => {
            let first = if kind == 200 { 28 } else { 8 };
            let blocks = (0..usize::from(format))
                .filter_map(|block| {
                    let at = first + block * 24;
                    let lost = word(at + 4)?;
                    Some(format!(
                        "ssrc {} lost {}/256 total {} highest {} jitter {} lsr {} dlsr {}",
                        word(at)?,
                        lost >> 24,
                        lost & 0x00ff_ffff,
                        word(at + 8)?,
                        word(at + 12)?,
                        word(at + 16)?,
                        word(at + 20)?
                    ))
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!("from {}: {blocks}", word(4).unwrap_or_default())
        }
        (206, 15) => remb(packet)
            .map(|(bitrate, ssrcs)| format!("{bitrate} bit/s for {ssrcs:?}"))
            .unwrap_or_default(),
        // Discord's own; unknown, so shown whole.
        (204, _) => {
            let data: String = packet
                .iter()
                .take(40)
                .map(|byte| format!("{byte:02x}"))
                .collect();
            format!("subtype {format}: {data}")
        }
        _ => String::new(),
    }
}

/// A receiver's bitrate estimate (REMB) in one RTCP packet: the bit/s it
/// thinks the path carries, and the SSRCs that covers.
fn remb(packet: &[u8]) -> Option<(u64, Vec<u32>)> {
    if packet.get(1) != Some(&206) || packet[0] & 0x1f != 15 || packet.get(12..16)? != b"REMB" {
        return None;
    }
    let count = usize::from(*packet.get(16)?);
    let exponent = packet.get(17)? >> 2;
    let mantissa = u32::from_be_bytes(packet.get(16..20)?.try_into().ok()?) & 0x3_ffff;
    let ssrcs = (0..count)
        .filter_map(|index| {
            let at = 20 + index * 4;
            Some(u32::from_be_bytes(packet.get(at..at + 4)?.try_into().ok()?))
        })
        .collect();
    Some((u64::from(mantissa) << exponent, ssrcs))
}

/// The lowest bitrate a decrypted compound RTCP packet estimates for `ssrc`.
pub fn bandwidth_estimate(mut rtcp: &[u8], ssrc: u32) -> Option<u64> {
    let mut lowest = None;
    while rtcp.len() >= 4 {
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        if let Some((bitrate, ssrcs)) = remb(&rtcp[..length.min(rtcp.len())])
            && ssrcs.contains(&ssrc)
        {
            lowest = Some(lowest.map_or(bitrate, |lowest: u64| lowest.min(bitrate)));
        }
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    lowest
}

/// How long a kept packet is worth sending again. A viewer holds a gap for
/// 300 ms and asks for the packet again every 80 ms in that time, so anything
/// older than this is one it has given up on and gone for a whole frame
/// instead. Kept generous, since a resend nobody uses costs one packet while
/// the keyframe that replaces it costs a frame.
const RESEND_WINDOW: Duration = Duration::from_secs(2);

/// How many packets a history keeps: its slots, and so the most it can hold
/// whatever the stream does.
const SLOTS: usize = 2048;

/// The video packets sent lately, unencrypted, to resend when a viewer
/// reports one lost: about two seconds at 9 Mbit/s, and never older than
/// `RESEND_WINDOW` however slowly they went.
pub struct History {
    packets: Vec<Option<Sent>>,
    /// When the history was made, which a slot's time counts from.
    born: Instant,
}

/// A packet kept for a viewer that reports it lost, and when it went out.
struct Sent {
    /// The sequence it went out with, which its slot's index names only by
    /// its remainder.
    sequence: u16,
    /// The millisecond of the history's `born` it went out at.
    at: u32,
    packet: Vec<u8>,
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
    fn millis(&self, now: Instant) -> u32 {
        now.duration_since(self.born).as_millis() as u32
    }
}

/// The sequence numbers of `media` packets that generic NACKs (RTCP 205,
/// format 1) in a decrypted compound packet report lost: each entry names
/// one, and a bitmask the 16 after it.
pub fn nacked(mut rtcp: &[u8], media: u32) -> Vec<u16> {
    let mut lost = vec![];
    while rtcp.len() >= 4 {
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        let packet = &rtcp[..length.min(rtcp.len())];
        if packet[1] == 205
            && packet[0] & 0x1f == 1
            && packet.get(8..12) == Some(&media.to_be_bytes()[..])
        {
            for entry in packet[12..].chunks_exact(4) {
                let first = u16::from_be_bytes([entry[0], entry[1]]);
                let mask = u16::from_be_bytes([entry[2], entry[3]]);
                lost.push(first);
                lost.extend(
                    (0..16)
                        .filter(|bit| mask & (1 << bit) != 0)
                        .map(|bit| first.wrapping_add(bit + 1)),
                );
            }
        }
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    lost
}

/// Whether a datagram is RTCP rather than RTP, by its packet type.
pub fn is_rtcp(packet: &[u8]) -> bool {
    packet.get(1).is_some_and(|kind| (192..=223).contains(kind))
}

/// A generic NACK (RTCP 205, format 1) from `sender` asking for `media`'s
/// packets `lost` again, in the order given: each entry names one, and a
/// bitmask the 16 after it.
pub fn nack(sender: u32, media: u32, lost: &[u16]) -> Vec<u8> {
    let mut entries: Vec<(u16, u16)> = vec![];
    for &sequence in lost {
        match entries.last_mut() {
            Some((first, mask)) if (1..=16).contains(&sequence.wrapping_sub(*first)) => {
                *mask |= 1 << (sequence.wrapping_sub(*first) - 1);
            }
            _ => entries.push((sequence, 0)),
        }
    }
    let words = 2 + entries.len() as u16;
    let mut packet = vec![0x81, 205];
    packet.extend_from_slice(&words.to_be_bytes());
    packet.extend_from_slice(&sender.to_be_bytes());
    packet.extend_from_slice(&media.to_be_bytes());
    for (first, mask) in entries {
        packet.extend_from_slice(&first.to_be_bytes());
        packet.extend_from_slice(&mask.to_be_bytes());
    }
    packet
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
    pub fn new(packet: &Packet) -> Self {
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
    pub fn unwrap_rtx(packet: &Packet, media: u32) -> Option<Self> {
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

/// How long a missing packet is waited for, how often it's asked for again,
/// and how many times, before the frames after it go to the assembler
/// without it. Discord's servers are a few tens of milliseconds away.
const GIVE_UP: std::time::Duration = std::time::Duration::from_millis(300);
const ASK_AGAIN: std::time::Duration = std::time::Duration::from_millis(80);
const ASKS: u32 = 3;
/// Packets held at most while one is missing, about a second at 9 Mbit/s.
const HELD: usize = 1000;
/// Missing packets waited for at most, and asked for at once: a sender
/// skipping far ahead again and again would otherwise run up thousands, and
/// a request too large to send.
const MAX_MISSING: usize = HELD;
const MAX_ASKED: usize = 256;

/// Puts one SSRC's packets back in sequence for the assembler, as they can
/// arrive out of order, and asks for missing ones again (NACK), as Discord's
/// client does, rather than losing every frame until the next keyframe.
#[derive(Default)]
pub struct Reorder {
    /// The extended sequence number of the next packet to hand on.
    next: Option<u64>,
    highest: Option<u64>,
    held: std::collections::BTreeMap<u64, Held>,
    missing: std::collections::BTreeMap<u64, Missing>,
    /// For receiver reports: the first extended sequence number, and the
    /// packets received since.
    first: Option<u64>,
    received: u64,
}

struct Missing {
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

/// An RTCP receiver report from `sender` about `media`: the highest
/// sequence number received, extended, packets lost, the fraction lost
/// since the last report (out of 256), and the last sender report heard
/// (the middle of its NTP time) with the delay since, in 1/65536 s.
pub fn receiver_report(
    sender: u32,
    media: u32,
    (highest, lost, fraction): (u32, u32, u8),
    (last_report, delay): (u32, u32),
) -> Vec<u8> {
    let mut packet = vec![0x81, 201, 0, 7];
    packet.extend_from_slice(&sender.to_be_bytes());
    packet.extend_from_slice(&media.to_be_bytes());
    packet.extend_from_slice(&((u32::from(fraction) << 24) | lost.min(0x7f_ffff)).to_be_bytes());
    for word in [highest, 0, last_report, delay] {
        packet.extend_from_slice(&word.to_be_bytes());
    }
    packet
}

/// The report blocks of the sender and receiver reports in a decrypted
/// compound RTCP packet: each about one SSRC, with the packets lost from it
/// so far and the highest sequence number received, extended.
pub fn report_blocks(mut rtcp: &[u8]) -> Vec<(u32, u32, u32)> {
    let mut blocks = vec![];
    while rtcp.len() >= 4 {
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        let packet = &rtcp[..length.min(rtcp.len())];
        let word = |at: usize| {
            packet
                .get(at..at + 4)
                .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        };
        // A sender report's blocks follow its 20 bytes of sender information.
        let first = match packet[1] {
            200 => Some(28),
            201 => Some(8),
            _ => None,
        };
        for block in first.into_iter().flat_map(|first| {
            (0..usize::from(packet[0] & 0x1f)).map(move |block| first + block * 24)
        }) {
            if let (Some(ssrc), Some(lost), Some(highest)) =
                (word(block), word(block + 4), word(block + 8))
            {
                blocks.push((ssrc, lost & 0x00ff_ffff, highest));
            }
        }
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    blocks
}

/// The sender reports in a decrypted compound RTCP packet: each sender's
/// SSRC, the middle 32 bits of its NTP time, and the RTP timestamp its own
/// clock read at that instant. A receiver report echoes the middle word for
/// the round trip; the two words together are what places a sender's RTP
/// clock on wall-clock time.
pub fn sender_reports(mut rtcp: &[u8]) -> Vec<(u32, u32, u32)> {
    let mut reports = vec![];
    while rtcp.len() >= 4 {
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        if rtcp[1] == 200
            && let Some(header) = rtcp.get(4..20)
        {
            let sender = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
            let middle = u32::from_be_bytes([header[6], header[7], header[8], header[9]]);
            let rtp = u32::from_be_bytes([header[12], header[13], header[14], header[15]]);
            reports.push((sender, middle, rtp));
        }
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_sequence_numbers_across_the_wrap() {
        assert!(before(1, 2));
        assert!(!before(2, 1));
        assert!(!before(7, 7), "a packet is not before itself");
        assert!(before(65_535, 0), "the wrap is a step, not a jump");
        assert!(before(65_534, 1));
        assert!(
            !before(1, 65_534),
            "and the other way round: that came later"
        );
        // Half the sequence space apart is too far apart either way to say
        // which came first, so neither is before the other.
        assert!(!before(0, 0x8000));
        assert!(!before(0x8000, 0));
    }

    #[test]
    fn reads_report_blocks() {
        let report = receiver_report(1, 77, (1000, 12, 3), (0, 0));
        let both = [report.clone(), receiver_report(1, 78, (40, 0, 0), (0, 0))].concat();
        assert_eq!(report_blocks(&report), [(77, 12, 1000)]);
        assert_eq!(report_blocks(&both), [(77, 12, 1000), (78, 0, 40)]);
        assert!(report_blocks(&picture_loss(1, 77)).is_empty());
    }

    #[test]
    fn packetizes_frames_with_vp8_descriptors() {
        let mut packetizer = Packetizer::new(77, Codec::Vp8);
        let frame: Vec<u8> = (0..2500u32).map(|index| index as u8).collect();
        let packets = packetizer.packets(&frame, 9000, false);
        assert_eq!(packets.len(), 3);
        let mut joined = vec![];
        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet[0], 0x90, "version 2, with extensions");
            assert_eq!(packet[1] & 0x7f, VP8);
            assert_eq!(packet[1] & 0x80 != 0, index == 2, "marker on the last");
            assert_eq!(&packet[4..8], &9000u32.to_be_bytes());
            assert_eq!(&packet[8..12], &77u32.to_be_bytes());
            assert_eq!(rtp_clear_len(packet), Some(16));
            // Transport sequence, screen content, then the stream id,
            // padded to whole words.
            let transport = (index + 1) as u8;
            assert_eq!(
                &packet[12..28],
                &[
                    0xbe, 0xde, 0, 3, 0x51, 0, transport, 0x70, 1, 0xb2, b'1', b'0', b'0', 0, 0, 0
                ]
            );
            let descriptor = &packet[28..32];
            assert_eq!(descriptor[0], if index == 0 { 0x90 } else { 0x80 });
            joined.extend_from_slice(&packet[32..]);
        }
        assert_eq!(joined, frame);
        let sequence = |packet: &Vec<u8>| u16::from_be_bytes([packet[2], packet[3]]);
        assert_eq!(sequence(&packets[1]), sequence(&packets[0]).wrapping_add(1));
        let picture = |packet: &Vec<u8>| u16::from_be_bytes([packet[30], packet[31]]) & 0x7fff;
        let next = packetizer.packets(&[1], 12000, false);
        assert_eq!(picture(&next[0]), (picture(&packets[0]) + 1) & 0x7fff);
    }

    /// A frame of each codec: whole units, units in fragments, and an AV1
    /// frame of two elements in three packets.
    fn sample_frames() -> [(Codec, Vec<u8>, bool); 4] {
        let vp8 = (0..2500u32).map(|index| index as u8).collect();
        let h264 = [
            &[0, 0, 0, 1, 0x67, 0x42][..],
            &[3; 40],
            &[0, 0, 0, 1, 0x41],
            &[7; 2500],
        ]
        .concat();
        let h265 = [&[0, 0, 0, 1, 19 << 1, 0x01][..], &[5; 2400]].concat();
        let av1 = [obu(OBU_SEQUENCE_HEADER, &[1; 11]), obu(6, &[9; 2500])].concat();
        [
            (Codec::Vp8, vp8, false),
            (Codec::H264, h264, false),
            (Codec::H265, h265, true),
            (Codec::Av1, av1, true),
        ]
    }

    /// What a plan writes is what `packets` gives, byte for byte, and leaves
    /// the numbering where `packets` left it.
    #[test]
    fn writes_what_packets_gives_packet_for_packet() {
        for (codec, frame, keyframe) in sample_frames() {
            let mut collecting = Packetizer::new(5, codec);
            let mut writing = Packetizer::new(5, codec);
            // The same numbering from the same place, at the wrap of each.
            for packetizer in [&mut collecting, &mut writing] {
                packetizer.sequence = 65_532;
                packetizer.transport = 65_533;
                packetizer.picture = 0x7ffd;
            }
            let expected = collecting.packets(&frame, 9000, keyframe);
            assert!(expected.len() > 1, "{codec:?}");
            let mut plan = Plan::default();
            let mut out = Vec::new();
            writing.plan_into(&mut plan, &frame, 9000, keyframe);
            let mut written = vec![];
            while writing.write(&mut plan, &frame, &mut out).is_some() {
                written.push(out.clone());
            }
            assert_eq!(written, expected, "{codec:?}");
            assert_eq!(
                (writing.sequence, writing.transport, writing.picture),
                (
                    collecting.sequence,
                    collecting.transport,
                    collecting.picture
                ),
                "{codec:?} numbered on as it went"
            );
        }
    }

    /// The buffer one frame's packets were written into carries the next
    /// frame's too, with room for the tag and counter sealing adds, so
    /// neither asks for a buffer of its own.
    #[test]
    fn keeps_the_packet_buffer_from_one_frame_to_the_next() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut plan = Plan::default();
        let mut out = Vec::new();
        let keyframe = vec![0x10; 3 * CHUNK];
        packetizer.plan_into(&mut plan, &keyframe, 0, false);
        while packetizer.write(&mut plan, &keyframe, &mut out).is_some() {}
        let (capacity, buffer) = (out.capacity(), out.as_ptr());
        assert!(
            capacity >= out.len() + TAG + 4,
            "room for the tag and counter a sealed packet adds"
        );
        let delta = vec![0x11; 200];
        packetizer.plan_into(&mut plan, &delta, 3000, false);
        while packetizer.write(&mut plan, &delta, &mut out).is_some() {}
        assert_eq!(out.capacity(), capacity, "a smaller frame asks for nothing");
        assert_eq!(out.as_ptr(), buffer, "it went into the same buffer");
    }

    /// A malformed AV1 frame is planned as no packets, so none of it goes out
    /// and the numbering stays where it was.
    #[test]
    fn writes_nothing_of_a_malformed_av1_frame() {
        let malformed: [&[u8]; 6] = [
            &[0x80, 0x00],       // the forbidden bit
            &[0x0a],             // a size flag without its size
            &[0x0a, 0x80],       // a size that never ends
            &[0x0a, 0x02, 0xaa], // a payload past the frame
            &[0x0c],             // an extension flag without the extension
            &[0x0c, 0x01],       // the extension's reserved bits
        ];
        let mut packetizer = Packetizer::new(77, Codec::Av1);
        let mut plan = Plan::default();
        let mut out = Vec::new();
        let numbered = (
            packetizer.sequence,
            packetizer.transport,
            packetizer.picture,
        );
        let mut frames: Vec<&[u8]> = malformed.to_vec();
        let oversized = vec![0; MAX_FRAME + 1];
        frames.push(&oversized);
        for frame in frames {
            packetizer.plan_into(&mut plan, frame, 9000, true);
            assert!(plan.packets.is_empty(), "{} bytes", frame.len());
            assert!(
                packetizer.write(&mut plan, frame, &mut out).is_none(),
                "{} bytes",
                frame.len()
            );
        }
        assert!(out.is_empty(), "nothing of the frame went out");
        assert_eq!(
            (
                packetizer.sequence,
                packetizer.transport,
                packetizer.picture
            ),
            numbered,
            "the numbering stayed put"
        );
        // The frame after one of them goes out under its own time, from the
        // numbering the malformed ones left.
        let frame = obu(OBU_SEQUENCE_HEADER, &[0xaa]);
        packetizer.plan_into(&mut plan, &frame, 12_000, true);
        assert!(packetizer.write(&mut plan, &frame, &mut out).is_some());
        let packet = parse(&out).unwrap();
        assert_eq!((packet.sequence, packet.timestamp), (numbered.0, 12_000));
        assert_eq!(packetizer.sequence, numbered.0.wrapping_add(1));
    }

    /// The marker is how a receiver knows a frame ended, so only the frame's
    /// last packet carries it, whether the frame took one packet or many.
    #[test]
    fn marks_only_the_last_packet_of_a_frame() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut plan = Plan::default();
        let mut out = Vec::new();
        for frame in [vec![1; 100], vec![2; 3 * CHUNK]] {
            let expected = frame.chunks(CHUNK).count();
            packetizer.plan_into(&mut plan, &frame, 0, false);
            let mut seen = 0;
            while let Some(last) = packetizer.write(&mut plan, &frame, &mut out) {
                seen += 1;
                let packet = parse(&out).unwrap();
                assert_eq!(packet.marker, last, "the marker is the plan's word");
                assert_eq!(last, seen == expected, "packet {seen} of {expected}");
            }
            assert_eq!(seen, expected);
        }
    }

    /// A keyframe far past a datagram's worth of bytes goes in packets that
    /// fit one, the frame bytes and the encryption Discord adds with them.
    #[test]
    fn splits_a_large_keyframe_into_datagrams_worth() {
        let tile: Vec<u8> = (0..64 * 1024u32)
            .map(|index| (index % 251) as u8 + 1)
            .collect();
        let frames: [(Codec, Vec<u8>); 4] = [
            (Codec::Vp8, tile.clone()),
            (Codec::H264, [&[0, 0, 0, 1, 0x65][..], &tile].concat()),
            (
                Codec::H265,
                [&[0, 0, 0, 1, 19 << 1, 0x01][..], &tile].concat(),
            ),
            (Codec::Av1, obu(6, &tile)),
        ];
        for (codec, frame) in frames {
            let mut packetizer = Packetizer::new(5, codec);
            let mut plan = Plan::default();
            let mut out = Vec::new();
            let mut packets = 0;
            packetizer.plan_into(&mut plan, &frame, 0, true);
            while packetizer.write(&mut plan, &frame, &mut out).is_some() {
                packets += 1;
                assert!(out.len() + TAG + 4 <= 1500, "{codec:?} fills a datagram");
            }
            assert!(packets >= 64 * 1024 / CHUNK, "{codec:?} splits it");
            assert!(packets <= 64 * 1024 / CHUNK + 4, "{codec:?} needs no more");
        }
    }

    #[test]
    fn an_audio_packet_takes_the_room_the_last_one_left() {
        // A call and a stream write a frame every 20 ms into the same buffer:
        // the room it has is room the allocator isn't asked for again.
        let mut packet = Vec::new();
        write_audio_into(&mut packet, 1, 0, 0, true, b"the first frame");
        let room = packet.capacity();
        write_audio_into(&mut packet, 1, 1, 960, false, b"the next frame");
        assert_eq!(packet.capacity(), room, "the room carries over");
        assert_eq!(packet.len(), HEADER + b"the next frame".len());
        assert_eq!(&packet[HEADER..], b"the next frame", "the frame it holds");
        assert_eq!(packet[1] & 0x7f, OPUS);
        assert_eq!(packet[1] & 0x80, 0, "only the first frame marks a burst");
        assert_eq!(packet, opus_packet(1, 1, 960, false, b"the next frame"));
        // The room past the frame is for sealing it where it lies.
        assert!(packet.capacity() >= packet.len() + TAG + 4);
    }

    #[test]
    fn seals_and_opens_both_modes() {
        for mode in Transport::MODES {
            let mut transport = Transport::new(mode, &[7; 32]).unwrap();
            let mut packet = Packetizer::new(1, Codec::Vp8)
                .packets(b"frame", 0, false)
                .remove(0);
            let plain = packet.clone();
            let clear = rtp_clear_len(&packet).unwrap();
            transport.seal(&mut packet, clear).unwrap();
            assert_eq!(packet.len(), plain.len() + 20);
            assert_eq!(packet[..clear], plain[..clear], "the header stays readable");
            assert_ne!(packet[clear..plain.len()], plain[clear..]);
            // Opening decrypts the packet where it lies: the buffer handed
            // over is the plaintext itself.
            let mut opening = packet.clone();
            assert_eq!(transport.open(&mut opening, clear), Some(&plain[..]));
            let mut tampered = packet.clone();
            tampered[clear] ^= 1;
            assert_eq!(
                transport.open(&mut tampered, clear),
                None,
                "tampering fails"
            );
        }
        assert!(Transport::new("xsalsa20_poly1305", &[0; 32]).is_err());
    }

    fn packet(sequence: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
        let mut packet = vec![0x80, if marker { 0x80 } else { 0 } | H264];
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&9u32.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    #[test]
    fn reassembles_what_the_packetizer_cut() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut assembler = Assembler::new(Codec::Vp8);
        let keyframe: Vec<u8> = [0x10]
            .into_iter()
            .chain((0..3000).map(|index| index as u8))
            .collect();
        let delta = vec![0x11; 1500];
        let mut frames = vec![];
        for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
            for sent in packetizer.packets(frame, index as u32 * 3000, false) {
                let packet = parse(&sent).unwrap();
                assert_eq!((packet.ssrc, packet.payload_type), (5, VP8));
                frames.extend(assembler.push(&packet).frame);
            }
        }
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
        assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
    }

    #[test]
    fn drops_frames_after_a_loss_until_a_keyframe() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut assembler = Assembler::new(Codec::Vp8);
        let delta = vec![0x11; 2500];
        // A delta first: nothing to decode it against.
        let mut pushed: Vec<Pushed> = packetizer
            .packets(&delta, 0, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .collect();
        assert!(pushed.iter().all(|pushed| pushed.frame.is_none()));
        let keyframe = vec![0x10; 2500];
        let mut sent = packetizer.packets(&keyframe, 3000, false);
        sent.remove(1);
        pushed = sent
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .collect();
        assert!(
            pushed.last().unwrap().lost,
            "a missing packet asks for a keyframe"
        );
        let whole = packetizer.packets(&keyframe, 6000, false);
        let last = whole
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert_eq!(last.frame.map(|frame| frame.keyframe), Some(true));
    }

    #[test]
    fn asks_for_a_keyframe_until_one_comes() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut assembler = Assembler::new(Codec::Vp8);
        let delta = vec![0x11; 500];
        let pushed = packetizer
            .packets(&delta, 0, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert!(pushed.lost, "nothing to decode yet");
        // Padding between frames isn't a loss.
        let padding = Packet {
            payload_type: VP8,
            marker: false,
            sequence: packetizer.sequence,
            timestamp: 0,
            ssrc: 5,
            payload: &[],
            orientation: None,
        };
        packetizer.sequence = packetizer.sequence.wrapping_add(1);
        assert_eq!(assembler.push(&padding), Pushed::default());
        let keyframe = vec![0x10; 500];
        let pushed = packetizer
            .packets(&keyframe, 3000, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert_eq!(pushed.frame.map(|frame| frame.data), Some(keyframe));
        assembler.wait_for_keyframe();
        let pushed = packetizer
            .packets(&delta, 6000, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert!(
            pushed.frame.is_none() && pushed.lost,
            "the decoder missed a frame"
        );
    }

    #[test]
    fn puts_packets_back_in_order_and_asks_for_lost_ones() {
        let start = std::time::Instant::now();
        let at = |ms: u64| start + std::time::Duration::from_millis(ms);
        let held = |sequence: u16| Held {
            payload_type: VP8,
            marker: false,
            sequence,
            timestamp: 0,
            ssrc: 5,
            payload: vec![sequence as u8],
            orientation: None,
        };
        let sequences =
            |ready: Vec<Held>| -> Vec<u16> { ready.iter().map(|held| held.sequence).collect() };
        let mut reorder = Reorder::default();
        // Across the wrap: 65534, 65535, then 1 before 0.
        for sequence in [65534, 65535, 1] {
            assert_eq!(reorder.insert(held(sequence), at(0)), Arrival::New);
        }
        assert_eq!(sequences(reorder.ready(at(0)).0), [65534, 65535]);
        assert_eq!(reorder.to_ask(at(0)), [0], "asked for at once");
        assert!(reorder.to_ask(at(10)).is_empty(), "not again so soon");
        assert_eq!(reorder.insert(held(0), at(20)), Arrival::Recovered);
        assert_eq!(sequences(reorder.ready(at(20)).0), [0, 1]);
        assert_eq!(reorder.insert(held(0), at(30)), Arrival::Late);

        // 2 and 3 never come: asked for three times, then given up on.
        reorder.insert(held(4), at(100));
        assert_eq!(reorder.to_ask(at(100)), [2, 3]);
        assert_eq!(reorder.to_ask(at(180)), [2, 3]);
        assert_eq!(reorder.to_ask(at(260)), [2, 3]);
        assert!(reorder.to_ask(at(340)).is_empty());
        assert_eq!(reorder.ready(at(399)), (vec![], 0));
        let (ready, skipped) = reorder.ready(at(400));
        assert_eq!((sequences(ready), skipped), (vec![4], 2));
        assert_eq!(reorder.insert(held(3), at(410)), Arrival::Late);
        assert_eq!(reorder.counts(), Some((65536 + 4, 2)), "one wrap, two lost");
    }

    #[test]
    fn bounds_what_a_sender_skipping_ahead_runs_up() {
        let at = |ms: u64| std::time::Instant::now() + std::time::Duration::from_millis(ms);
        let held = |sequence: u16| Held {
            payload_type: VP8,
            marker: false,
            sequence,
            timestamp: 0,
            ssrc: 5,
            payload: vec![0],
            orientation: None,
        };
        let mut reorder = Reorder::default();
        let mut sequence = 0u16;
        for _ in 0..2000 {
            reorder.insert(held(sequence), at(0));
            sequence = sequence.wrapping_add(HELD as u16 - 1);
        }
        assert!(reorder.missing.len() <= MAX_MISSING);
        assert!(reorder.to_ask(at(0)).len() <= MAX_ASKED);
    }

    #[test]
    fn drops_a_frame_that_never_ends() {
        let mut packetizer = Packetizer::new(5, Codec::Vp8);
        let mut assembler = Assembler::new(Codec::Vp8);
        let huge = vec![0x10; MAX_FRAME + 4096];
        let last = packetizer
            .packets(&huge, 0, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert_eq!(last.frame, None);
        assert!(assembler.frame.len() <= MAX_FRAME);
        let keyframe = vec![0x10; 2500];
        let last = packetizer
            .packets(&keyframe, 3000, false)
            .iter()
            .map(|sent| assembler.push(&parse(sent).unwrap()))
            .last()
            .unwrap();
        assert_eq!(last.frame.map(|frame| frame.data), Some(keyframe));
    }

    /// An AV1 element that carries on in every packet, never reaching the
    /// frame, is held to the same limit.
    #[test]
    fn drops_an_av1_element_that_never_ends() {
        let mut assembler = Assembler::new(Codec::Av1);
        let mut payload = vec![0x50, 0x32];
        payload.resize(CHUNK, 7);
        for sequence in 0..(2 * MAX_FRAME / CHUNK) as u16 {
            assembler.push(&Packet {
                payload_type: AV1,
                marker: false,
                sequence,
                timestamp: 3000,
                ssrc: 5,
                payload: &payload,
                orientation: None,
            });
            // W=1, Z and Y: the one element continues the last packet's
            // and carries on in the next.
            payload[..2].copy_from_slice(&[0xd0, 7]);
        }
        assert!(assembler.obu.len() <= MAX_FRAME);
    }

    /// A packet with header extensions `profile` and `elements`, padded to
    /// whole words, around a one-byte payload.
    fn extended(profile: u16, elements: &[u8]) -> Vec<u8> {
        let mut packet = vec![0x90, VP8, 0, 1, 0, 0, 0, 9, 0, 0, 0, 5];
        let words = elements.len().div_ceil(4);
        packet.extend_from_slice(&profile.to_be_bytes());
        packet.extend_from_slice(&(words as u16).to_be_bytes());
        packet.extend_from_slice(elements);
        packet.resize(HEADER + 4 + words * 4, 0);
        packet.push(0x42);
        packet
    }

    #[test]
    fn reads_the_video_orientation_at_discords_id() {
        // Among the transport sequence and the stream id, as Discord sends.
        for (byte, turns) in [(0, 0), (1, 1), (2, 2), (3, 3)] {
            let packet = extended(0xbede, &[0x51, 0, 7, 0x40, byte, 0xb2, b'1', b'0', b'0']);
            let parsed = parse(&packet).unwrap();
            assert_eq!(parsed.payload, &[0x42]);
            assert_eq!(
                parsed.orientation,
                Some(Orientation { turns, flip: false }),
                "{} degrees",
                u32::from(turns) * 90
            );
        }
        let turned =
            |profile, elements: &[u8]| parse(&extended(profile, elements)).unwrap().orientation;
        // The flip, with the front camera's bit, which changes nothing.
        assert_eq!(
            turned(0xbede, &[0x40, 0x0d]),
            Some(Orientation {
                turns: 1,
                flip: true
            })
        );
        // After padding, and in the two-byte form.
        assert_eq!(turned(0xbede, &[0, 0, 0x40, 2]).map(|o| o.turns), Some(2));
        assert_eq!(
            turned(0x1000, &[5, 2, 0, 9, 0, 4, 1, 3]).map(|o| o.turns),
            Some(3)
        );
        // Other ids, a one-byte value at another id, and none at all.
        assert_eq!(turned(0xbede, &[0x51, 0, 7, 0x70, 1]), None);
        assert_eq!(turned(0xbede, &[]), None);
        // A value running past the block is no orientation, not a panic.
        assert_eq!(turned(0xbede, &[0x5f, 0]), None);
        // Yapfast's own packets carry none.
        let mut packetizer = Packetizer::new(5, Codec::H264);
        let own = packetizer.packets(&[0, 0, 0, 1, 0x65, 1, 2], 0, true);
        assert_eq!(parse(&own[0]).unwrap().orientation, None);
        // A resent packet keeps it.
        let sent = extended(0xbede, &[0x40, 1, 0, 0, 0, 0]);
        let resent = parse(&sent).unwrap();
        let resent = Packet {
            payload: &[0, 7, 1],
            ..resent
        };
        assert_eq!(
            Held::unwrap_rtx(&resent, 4)
                .unwrap()
                .orientation
                .map(|o| o.turns),
            Some(1)
        );
    }

    #[test]
    fn unwraps_resent_packets() {
        let mut packetizer = Packetizer::new(5, Codec::H264);
        let frame = [&[0, 0, 0, 1, 0x41][..], &[7; 20]].concat();
        let original = packetizer.packets(&frame, 3000, false).remove(0);
        let again = packetizer.retransmit(&original, 6).unwrap();
        let resent = parse(&again).unwrap();
        assert_eq!(resent.ssrc, 6);
        let unwrapped = Held::unwrap_rtx(&resent, 5).unwrap();
        let original = parse(&original).unwrap();
        assert_eq!(unwrapped, Held::new(&original));
        let padding = Packet {
            payload: &[],
            ..resent
        };
        assert_eq!(Held::unwrap_rtx(&padding, 5), None);
    }

    #[test]
    fn writes_nacks_and_receiver_reports_that_read_back() {
        let lost = [65530, 65531, 65535, 4, 40];
        let packet = nack(1, 2, &lost);
        assert_eq!(
            packet.len(),
            12 + 2 * 4,
            "65531 to 4 ride on 65530's bitmask"
        );
        assert_eq!(nacked(&packet, 2), lost);
        assert!(nacked(&packet, 3).is_empty());
        let report = receiver_report(1, 2, (70000, 3, 25), (0x1234_5678, 655));
        assert_eq!(report.len(), 32);
        let described = &rtcp_kinds(&report)[0];
        assert_eq!(described.0, "201/1");
        assert_eq!(
            described.1,
            "from 1: ssrc 2 lost 25/256 total 3 highest 70000 jitter 0 lsr 305419896 dlsr 655"
        );
        let sent = sender_report(9, std::time::SystemTime::UNIX_EPOCH, 0, 0, 0);
        // 1970 in NTP time: 2208988800 seconds, no fraction.
        assert_eq!(sender_reports(&sent), [(9, 2_208_988_800u32 << 16, 0)]);
    }

    #[test]
    fn packetizes_and_reassembles_h265() {
        let mut packetizer = Packetizer::new(5, Codec::H265);
        let mut assembler = Assembler::new(Codec::H265);
        // An IDR slice (type 19), its header's second byte the temporal id.
        let idr: Vec<u8> = [19 << 1, 0x01]
            .into_iter()
            .chain((0..3000u32).map(|index| (index % 251) as u8 + 1))
            .collect();
        let keyframe = [
            &[0, 0, 0, 1, 32 << 1, 0x01, 0x0c][..],
            &[0, 0, 0, 1, 33 << 1, 0x01, 0x01],
            &[0, 0, 0, 1, 34 << 1, 0x01, 0xc1],
            &[0, 0, 0, 1],
            &idr,
        ]
        .concat();
        let delta = [&[0, 0, 0, 1, 1 << 1, 0x01][..], &[7; 200]].concat();
        let mut frames = vec![];
        for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
            let sent = packetizer.packets(frame, index as u32 * 3000, false);
            if index == 0 {
                // The parameter sets alone, then the slice in three fragments.
                assert_eq!(sent.len(), 6);
                assert_eq!(
                    &sent[3][28..31],
                    &[49 << 1, 0x01, 0x80 | 19],
                    "starting the IDR slice"
                );
                assert_eq!(sent[5][30], 0x40 | 19, "ending it");
            }
            for packet in &sent {
                let packet = parse(packet).unwrap();
                assert_eq!(packet.payload_type, H265);
                frames.extend(assembler.push(&packet).frame);
            }
        }
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
        assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
    }

    /// An OBU of `kind` that gives its size.
    fn obu(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut obu = vec![kind << 3 | OBU_HAS_SIZE];
        write_leb128(payload.len(), &mut obu);
        obu.extend_from_slice(payload);
        obu
    }

    /// libaom's AV1 of the H264 recording, cut into temporal units at their
    /// delimiters.
    fn av1_recording() -> Vec<Vec<u8>> {
        let file = include_bytes!("../../../testing/fixtures/stream-av1.obu");
        let mut units: Vec<Vec<u8>> = vec![];
        let mut rest = &file[..];
        while let Some(&header) = rest.first() {
            let (size, used) = read_leb128(&rest[1..]).unwrap();
            let length = 1 + used + size;
            if (header >> 3) & 0x0f == OBU_TEMPORAL_DELIMITER {
                units.push(vec![]);
            }
            units.last_mut().unwrap().extend_from_slice(&rest[..length]);
            rest = &rest[length..];
        }
        units
    }

    #[test]
    fn packetizes_and_reassembles_av1() {
        let mut packetizer = Packetizer::new(5, Codec::Av1);
        let mut assembler = Assembler::new(Codec::Av1);
        let units = av1_recording();
        assert_eq!(units.len(), 30);
        let mut frames = vec![];
        let mut keyframes = vec![];
        for (index, unit) in units.iter().enumerate() {
            let keyframe = obu_elements(unit)
                .unwrap()
                .iter()
                .any(|element| (element[0] >> 3) & 0x0f == OBU_SEQUENCE_HEADER);
            keyframes.push(keyframe);
            let sent = packetizer.packets(unit, index as u32 * 3000, keyframe);
            for (at, packet) in sent.iter().enumerate() {
                let packet = parse(packet).unwrap();
                assert_eq!(packet.payload_type, AV1);
                assert!(packet.payload.len() <= CHUNK);
                assert_eq!(packet.marker, at + 1 == sent.len());
                // N on a keyframe's first packet only, which starts with
                // its sequence header.
                assert_eq!(packet.payload[0] & 0x08 != 0, keyframe && at == 0);
                frames.extend(assembler.push(&packet).frame);
            }
        }
        assert_eq!(keyframes.iter().filter(|&&key| key).count(), 2);
        assert_eq!(frames.len(), 30);
        for ((frame, unit), keyframe) in frames.iter().zip(&units).zip(keyframes) {
            assert_eq!(frame.keyframe, keyframe);
            // The same OBUs, less the temporal delimiter.
            assert_eq!(obu_elements(&frame.data), obu_elements(unit));
            assert_ne!((frame.data[0] >> 3) & 0x0f, OBU_TEMPORAL_DELIMITER);
        }
    }

    /// A frame as DAVE leaves it: its last OBU without a size, running into
    /// DAVE's trailer, which goes out with it; and an element across three
    /// packets, then more than three in one packet.
    #[test]
    fn packetizes_av1_as_dave_leaves_it() {
        let mut packetizer = Packetizer::new(5, Codec::Av1);
        let mut assembler = Assembler::new(Codec::Av1);
        let tile: Vec<u8> = (0..2500u32).map(|index| (index % 251) as u8).collect();
        let last = [&[6 << 3][..], &[9; 40], b"trailer\xfa\xfa"].concat();
        let frame = [
            obu(OBU_TEMPORAL_DELIMITER, &[]),
            obu(OBU_SEQUENCE_HEADER, &[1; 11]),
            obu(6, &tile),
            obu(OBU_PADDING, &[0; 8]),
            last.clone(),
        ]
        .concat();
        let sent = packetizer.packets(&frame, 0, true);
        assert_eq!(sent.len(), 3);
        let payloads: Vec<&[u8]> = sent
            .iter()
            .map(|packet| parse(packet).unwrap().payload)
            .collect();
        // Z, Y, W and N: the sequence header and the tile's start, the
        // tile's middle alone, its end and the last OBU.
        let headers: Vec<u8> = payloads.iter().map(|payload| payload[0]).collect();
        assert_eq!(
            headers,
            [0x40 | 2 << 4 | 0x08, 0xc0 | 1 << 4, 0x80 | 2 << 4]
        );
        // Elements lose their size flag: the sequence header's is 1 << 3.
        assert_eq!(&payloads[0][1..3], &[12, OBU_SEQUENCE_HEADER << 3]);
        let mut frames = vec![];
        for packet in &sent {
            frames.extend(assembler.push(&parse(packet).unwrap()).frame);
        }
        // The last OBU stays without its size, as DAVE's decryptor needs.
        let expected = [obu(OBU_SEQUENCE_HEADER, &[1; 11]), obu(6, &tile), last].concat();
        assert_eq!(frames.len(), 1);
        assert_eq!((frames[0].keyframe, &frames[0].data), (true, &expected));

        let small: Vec<u8> = (0..5).flat_map(|index| obu(6, &[index; 20])).collect();
        let sent = packetizer.packets(&small, 3000, false);
        assert_eq!(sent.len(), 1);
        assert_eq!(
            parse(&sent[0]).unwrap().payload[0],
            0,
            "five elements, each sized"
        );
        let frame = assembler.push(&parse(&sent[0]).unwrap()).frame.unwrap();
        assert!(!frame.keyframe);
        assert_eq!(obu_elements(&frame.data), obu_elements(&small));
        assert_eq!(
            frame.data.len(),
            small.len() - 1,
            "the last without its size"
        );

        // The tile's middle lost: the frame is dropped, and frames after it
        // until a keyframe.
        let sent = packetizer.packets(&obu(6, &tile), 6000, false);
        assert!(assembler.push(&parse(&sent[0]).unwrap()).frame.is_none());
        let pushed = assembler.push(&parse(&sent[2]).unwrap());
        assert_eq!((pushed.frame, pushed.lost), (None, true));
        let sent = packetizer.packets(&small, 9000, false);
        assert!(assembler.push(&parse(&sent[0]).unwrap()).frame.is_none());
    }

    #[test]
    fn sends_nothing_of_a_malformed_av1_frame() {
        let malformed: [&[u8]; 6] = [
            &[0x80, 0x00],       // the forbidden bit
            &[0x0a],             // a size flag without its size
            &[0x0a, 0x80],       // a size that never ends
            &[0x0a, 0x02, 0xaa], // a payload past the frame
            &[0x0c],             // an extension flag without the extension
            &[0x0c, 0x01],       // the extension's reserved bits
        ];
        let mut packetizer = Packetizer::new(77, Codec::Av1);
        for frame in malformed {
            assert!(
                packetizer.packets(frame, 9000, true).is_empty(),
                "{frame:?}"
            );
        }
        assert!(
            packetizer
                .packets(&vec![0; MAX_FRAME + 1], 9000, false)
                .is_empty()
        );
    }

    /// OBUs whose extensions name different temporal or spatial layers go
    /// in packets of their own; those without one go along with either.
    #[test]
    fn keeps_av1_layers_apart() {
        let mut packetizer = Packetizer::new(77, Codec::Av1);
        let layered = |id: u8| vec![6 << 3 | OBU_EXTENSION | OBU_HAS_SIZE, id << 5, 3, 1, 2, 3];
        let frame = [
            obu(OBU_SEQUENCE_HEADER, &[1; 4]),
            layered(0),
            layered(0),
            layered(1),
        ]
        .concat();
        let sent = packetizer.packets(&frame, 0, true);
        let counts: Vec<u8> = sent
            .iter()
            .map(|packet| (parse(packet).unwrap().payload[0] >> 4) & 0x03)
            .collect();
        assert_eq!(counts, [3, 1]);
    }

    #[test]
    fn retransmits_av1_with_its_rtx_payload_type() {
        let mut packetizer = Packetizer::new(5, Codec::Av1);
        let original = packetizer
            .packets(&obu(OBU_SEQUENCE_HEADER, &[0xaa]), 3000, true)
            .remove(0);
        let resent = packetizer.retransmit(&original, 6).unwrap();
        let resent = parse(&resent).unwrap();
        assert_eq!(resent.payload_type, AV1_RTX);
        let restored = Held::unwrap_rtx(&resent, 5).unwrap();
        assert_eq!((restored.payload_type, restored.timestamp), (AV1, 3000));
        assert_eq!(restored.payload, parse(&original).unwrap().payload);
    }

    #[test]
    fn reads_and_writes_leb128() {
        for value in [0, 1, 127, 128, 1100, 16383, 16384, 1 << 30] {
            let mut bytes = vec![];
            write_leb128(value, &mut bytes);
            assert_eq!(bytes.len(), leb128_size(value));
            assert_eq!(read_leb128(&bytes), Some((value, bytes.len())));
        }
        // An encoder may pad a size to more bytes than it needs.
        assert_eq!(read_leb128(&[0x85, 0x80, 0x00]), Some((5, 3)));
        assert_eq!(read_leb128(&[0x80]), None);
    }

    #[test]
    fn packetizes_and_reassembles_h264() {
        let mut packetizer = Packetizer::new(5, Codec::H264);
        let mut assembler = Assembler::new(Codec::H264);
        let idr: Vec<u8> = [0x65]
            .into_iter()
            .chain((0..3000u32).map(|index| (index % 251) as u8 + 1))
            .collect();
        let keyframe = [
            &[0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f][..],
            &[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80],
            &[0, 0, 0, 1],
            &idr,
        ]
        .concat();
        let delta = [&[0, 0, 0, 1, 0x41][..], &[7; 200]].concat();
        let mut frames = vec![];
        for (index, frame) in [&keyframe, &delta].into_iter().enumerate() {
            let sent = packetizer.packets(frame, index as u32 * 3000, false);
            if index == 0 {
                // The parameter sets alone, then the slice in three fragments.
                assert_eq!(sent.len(), 5);
                assert_eq!(sent[2][28], (0x65 & 0xe0) | 28, "an FU-A indicator");
                assert_eq!(sent[2][29], 0x80 | 5, "starting an IDR slice");
                assert_eq!(sent[4][29], 0x40 | 5, "ending it");
            }
            for packet in &sent {
                let packet = parse(packet).unwrap();
                assert_eq!(packet.payload_type, H264);
                frames.extend(assembler.push(&packet).frame);
            }
        }
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].keyframe, &frames[0].data), (true, &keyframe));
        assert_eq!((frames[1].keyframe, &frames[1].data), (false, &delta));
    }

    #[test]
    fn finds_nal_units_as_libwebrtc_does() {
        // A unit ending in a zero before a long start code keeps it; a
        // short start code works too.
        let frame = [
            0, 0, 0, 1, 0x41, 9, 0, 0, 0, 0, 1, 0x41, 8, 0, 0, 1, 0x68, 7,
        ];
        assert_eq!(
            nal_units(&frame),
            [&[0x41, 9, 0][..], &[0x41, 8], &[0x68, 7]]
        );
        assert!(nal_units(&[1, 2, 3]).is_empty());
    }

    #[test]
    fn reassembles_h264_units() {
        let mut assembler = Assembler::new(Codec::H264);
        // An SPS and PPS together, then an IDR slice in two fragments.
        let stap = [24, 0, 2, 0x67, 1, 0, 2, 0x68, 2];
        let first = [0x7c, 0x85, 10, 11];
        let last = [0x7c, 0x45, 12];
        assert_eq!(
            assembler.push(&parse(&packet(1, 90, false, &stap)).unwrap()),
            Pushed::default()
        );
        assembler.push(&parse(&packet(2, 90, false, &first)).unwrap());
        let frame = assembler
            .push(&parse(&packet(3, 90, true, &last)).unwrap())
            .frame
            .unwrap();
        assert!(frame.keyframe);
        assert_eq!(
            frame.data,
            [
                0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 10, 11, 12
            ]
        );
        // A slice alone is a delta frame.
        let delta = assembler
            .push(&parse(&packet(4, 180, true, &[0x41, 7])).unwrap())
            .frame
            .unwrap();
        assert_eq!(
            (delta.keyframe, delta.data),
            (false, vec![0, 0, 0, 1, 0x41, 7])
        );
        assert_eq!(
            picture_loss(1, 2),
            [0x81, 206, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2]
        );
    }

    #[test]
    fn sender_reports_say_when_a_timestamp_was() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_500);
        let report = sender_report(7, at, 90_000, 12, 3400);
        assert_eq!(report.len(), 28, "a header and 20 bytes of sender info");
        assert_eq!(&report[..4], &[0x80, 200, 0, 6]);
        assert!(is_rtcp(&report));
        let word = |at: usize| u32::from_be_bytes(report[at..at + 4].try_into().unwrap());
        assert_eq!(word(4), 7);
        assert_eq!(word(8), 2_208_988_801);
        assert_eq!(word(12), 1 << 31, "half a second");
        assert_eq!((word(16), word(20), word(24)), (90_000, 12, 3400));
        // The middle word and the RTP timestamp it was read at, together.
        assert_eq!(
            sender_reports(&report),
            [(7, (2_208_988_801u32 << 16) | (1 << 15), 90_000)]
        );
    }

    #[test]
    fn describes_receiver_reports_and_remb() {
        let mut report = vec![0x81, 201, 0, 7, 0, 0, 0, 1];
        for word in [55u32, (12 << 24) | 3, 70_000, 9, 0, 0] {
            report.extend_from_slice(&word.to_be_bytes());
        }
        let mut remb = vec![0x8f, 206, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0];
        remb.extend_from_slice(b"REMB");
        // One SSRC, 2^2 * 250_000.
        remb.extend_from_slice(&[1, 2 << 2 | 3, 0xd0, 0x90]);
        remb.extend_from_slice(&55u32.to_be_bytes());
        let kinds = rtcp_kinds(&[report.clone(), remb.clone()].concat());
        assert_eq!(
            kinds,
            [
                (
                    "201/1".to_owned(),
                    "from 1: ssrc 55 lost 12/256 total 3 highest 70000 jitter 9 lsr 0 dlsr 0"
                        .to_owned()
                ),
                ("206/15".to_owned(), "1000000 bit/s for [55]".to_owned()),
            ]
        );
        let compound = [report, remb].concat();
        assert_eq!(bandwidth_estimate(&compound, 55), Some(1_000_000));
        assert_eq!(
            bandwidth_estimate(&compound, 56),
            None,
            "about another stream"
        );
    }

    #[test]
    fn retransmits_what_viewers_report_lost() {
        let mut packetizer = Packetizer::new(77, Codec::H264);
        let mut history = History::default();
        let sent_at = Instant::now();
        let frame = [&[0, 0, 0, 1, 0x65][..], &[9; 3000]].concat();
        let sent = packetizer.packets(&frame, 9000, false);
        for packet in &sent {
            history.remember(packet, sent_at);
        }
        let first = parse(&sent[0]).unwrap().sequence;
        // A NACK for the first packet and, by its bitmask, the third.
        let mut nack = vec![0x81, 205, 0, 3, 0, 0, 0, 1];
        nack.extend_from_slice(&77u32.to_be_bytes());
        nack.extend_from_slice(&first.to_be_bytes());
        nack.extend_from_slice(&0b10u16.to_be_bytes());
        let lost = nacked(&nack, 77);
        assert_eq!(lost, [first, first.wrapping_add(2)]);
        assert!(nacked(&nack, 78).is_empty(), "about another stream");
        let again = packetizer
            .retransmit(history.get(lost[1], sent_at).unwrap(), 79)
            .unwrap();
        let original = parse(&sent[2]).unwrap();
        let resent = parse(&again).unwrap();
        assert_eq!(
            (
                resent.payload_type,
                resent.ssrc,
                resent.timestamp,
                resent.marker
            ),
            (H264_RTX, 79, 9000, original.marker)
        );
        assert_eq!(&resent.payload[..2], &lost[1].to_be_bytes());
        assert_eq!(&resent.payload[2..], original.payload);
        assert_eq!(again[21], 0xc2, "the repaired stream id, not the stream id");
        assert!(
            history.get(first.wrapping_sub(1), sent_at).is_none(),
            "never sent"
        );
        assert!(packetizer.retransmit(&again, 79).is_none(), "not video");
    }

    /// Two sequences a history apart: they land on the same slot.
    fn same_slot(sequence: u16, slots: usize) -> u16 {
        sequence + slots as u16
    }

    #[test]
    fn reuses_the_vector_in_an_rtx_history_slot() {
        let mut history = History::default();
        let slots = history.packets.len();
        let first = 1000u16;
        let second = same_slot(first, slots);
        assert_eq!(
            usize::from(first) % slots,
            usize::from(second) % slots,
            "the same slot"
        );
        let before = packet(first, 500, false, &[1; 400]);
        let after = packet(second, 900, true, &[2; 300]);

        history.remember(&before, Instant::now());
        let slot = history.packets[usize::from(first) % slots]
            .as_ref()
            .expect("a slot filled");
        assert_eq!(
            (slot.sequence, slot.packet.as_slice()),
            (first, before.as_slice())
        );
        let storage = (slot.packet.as_ptr(), slot.packet.capacity());

        let replaced_at = Instant::now();
        history.remember(&after, replaced_at);
        let slot = history.packets[usize::from(second) % slots]
            .as_ref()
            .expect("the slot holds a packet still");
        assert_eq!(
            (slot.packet.as_ptr(), slot.packet.capacity()),
            storage,
            "the slot's own vector, not a new one"
        );
        assert_eq!(
            (slot.sequence, slot.packet.as_slice()),
            (second, after.as_slice()),
            "the packet just kept, with no bytes left over from the last"
        );
        assert_eq!(
            history.get(first, replaced_at),
            None,
            "replaced in that slot"
        );
        assert_eq!(history.get(second, replaced_at), Some(after.as_slice()));
    }

    #[test]
    fn an_rtx_history_slot_holds_packets_of_changing_length() {
        let mut history = History::default();
        let slots = history.packets.len();
        let sequence = 7u16;
        let long_sequence = same_slot(sequence, slots);
        let shortest_sequence = sequence + 2 * (slots as u16);
        let short = packet(sequence, 1, false, &[3; 100]);
        let long = packet(long_sequence, 2, false, &[4; 2000]);
        let shortest = packet(shortest_sequence, 3, true, &[5; 20]);
        for other in [long_sequence, shortest_sequence] {
            assert_eq!(
                usize::from(other) % slots,
                usize::from(sequence) % slots,
                "the same slot"
            );
        }
        let slot = usize::from(sequence) % slots;
        let sent_at = Instant::now();

        history.remember(&short, sent_at);
        history.remember(&long, sent_at);
        let stored = &history.packets[slot]
            .as_ref()
            .expect("a slot filled")
            .packet;
        assert_eq!(stored.as_slice(), long.as_slice(), "grown to fit");
        assert!(stored.capacity() >= long.len(), "room for the whole packet");
        let grown = stored.capacity();

        history.remember(&shortest, sent_at);
        let stored = &history.packets[slot]
            .as_ref()
            .expect("a slot filled")
            .packet;
        assert_eq!(
            stored.as_slice(),
            shortest.as_slice(),
            "cut to the shorter packet"
        );
        assert_eq!(stored.capacity(), grown, "the larger vector kept");
        assert_eq!(history.get(long_sequence, sent_at), None, "replaced");
        assert_eq!(
            history.get(shortest_sequence, sent_at),
            Some(shortest.as_slice())
        );
    }

    #[test]
    fn an_rtx_history_slot_validates_its_sequence_across_wraparound() {
        let mut history = History::default();
        let slots = history.packets.len();
        let before_wrap = 65_500u16;
        let after_wrap = before_wrap.wrapping_add(slots as u16);
        assert!(
            after_wrap < before_wrap,
            "across the end of the sequence numbers"
        );
        assert_eq!(
            usize::from(after_wrap) % slots,
            usize::from(before_wrap) % slots,
            "the same slot"
        );
        let tail = packet(before_wrap, 1, false, &[6; 200]);
        let head = packet(after_wrap, 2, true, &[7; 200]);

        let sent_at = Instant::now();
        history.remember(&tail, sent_at);
        assert_eq!(history.get(before_wrap, sent_at), Some(tail.as_slice()));

        history.remember(&head, sent_at);
        assert_eq!(history.get(after_wrap, sent_at), Some(head.as_slice()));
        assert_eq!(
            history.get(before_wrap, sent_at),
            None,
            "replaced in that slot"
        );
        assert!(
            history.get(1234, sent_at).is_none(),
            "a slot nothing has filled"
        );
    }

    #[test]
    fn rtx_history_remembers_only_parsable_packets() {
        let mut history = History::default();
        let sent_at = Instant::now();
        history.remember(&[], sent_at);
        history.remember(&[0x80, H264], sent_at);
        history.remember(&[0x40; 20], sent_at);
        assert!(
            history.packets.iter().all(|slot| slot.is_none()),
            "no slot filled, nor partly"
        );
    }

    #[test]
    fn an_rtx_slot_ages_out_rather_than_lasting_through_a_stall() {
        // Nothing ages a slot but the packets that follow it, so a stream that
        // stops sending leaves the history as it was: minutes later a viewer's
        // NACK for one of those packets is about a packet no viewer can still
        // be waiting for, and the uplink a recovering stream needs is not
        // spent on it. What the slot holds is never handed out in its place.
        let mut history = History::default();
        let sent_at = Instant::now();
        let stalled = sent_at + RESEND_WINDOW + Duration::from_millis(1);
        let old = packet(200, 1, false, &[8; 300]);
        let new = packet(same_slot(200, history.packets.len()), 2, true, &[9; 300]);

        history.remember(&old, sent_at);
        assert_eq!(history.get(200, sent_at), Some(old.as_slice()));
        assert_eq!(
            history.get(200, stalled),
            None,
            "in the history still, and too old to resend"
        );

        // The slot takes the packet the sequence numbers put in it next, with
        // their own bytes: neither the old sequence nor the new one is ever
        // answered with the other's packet.
        let replaced_at = Instant::now();
        history.remember(&new, replaced_at);
        let kept = parse(&new).unwrap().sequence;
        assert_eq!(history.get(200, replaced_at), None, "gone from that slot");
        assert_eq!(history.get(kept, replaced_at), Some(new.as_slice()));
        assert_eq!(
            history.get(kept, replaced_at + RESEND_WINDOW),
            None,
            "and ages out in its turn"
        );
    }

    #[test]
    fn an_rtx_slot_costs_no_more_than_the_packet_alone() {
        // What a slot keeps for ageing it is the room the tuple had spare:
        // the store is bounded by its 2048 slots whatever the stream does.
        assert_eq!(
            std::mem::size_of::<Option<Sent>>(),
            std::mem::size_of::<Option<(u16, Vec<u8>)>>(),
            "a kept packet, its sequence and its age in a packet's worth of slot"
        );
    }

    #[test]
    fn finds_keyframe_requests() {
        // A receiver report, then a PLI.
        let report = [0x80, 201, 0, 1, 0, 0, 0, 1];
        let pli = [0x81, 206, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2];
        assert!(!wants_keyframe(&report));
        assert!(wants_keyframe(&[&report[..], &pli[..]].concat()));
        assert!(is_rtcp(&pli) && !is_rtcp(&[0x90, VP8]));
    }
}
