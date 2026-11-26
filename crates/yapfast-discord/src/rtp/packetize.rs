//! Cutting a frame into RTP packets: the payload formats and their header extensions.

use super::*;

/// Frame bytes per packet, leaving room for headers, the tag and the nonce
/// within a typical path MTU.
pub(super) const CHUNK: usize = 1100;
/// RTP's fixed header.
pub(super) const HEADER: usize = 12;
/// The stream id op 12 declares a stream under. Each video packet names it
/// in an extension, which is how Discord ties the packet to the stream.
pub const STREAM_RID: &str = "100";
/// One-byte extension ids in the numbering a Discord client sends with,
/// which isn't the numbering in Discord's WebRTC SDP: there id 5 is playout
/// delay, here it's the transport-wide sequence number, and a packet with a
/// malformed one is dropped. Discord's own clients also send a playout
/// delay of 0/0 as id 6; from Yapfast, Discord's iOS app shows that stream
/// black, so Yapfast leaves it out.
pub(super) const TRANSPORT_SEQUENCE: u8 = 5;
pub(super) const CONTENT_TYPE: u8 = 7;
pub(super) const RID: u8 = 11;
/// The stream id on a retransmission, in place of [`RID`].
pub(super) const REPAIRED_RID: u8 = 12;
/// `urn:3gpp:video-orientation` (CVO), which Discord's native clients
/// number 4 (docs.discord.food, Voice Connections, "Native RTP Header
/// Extensions"; its WebRTC SDP says 13). Phones film in the sensor's
/// orientation and say here how to turn each frame upright.
pub(super) const VIDEO_ORIENTATION: u8 = 4;
/// The extension block's size: its preamble, then three words of elements.
pub(super) const EXTENSIONS: usize = 4 + 12;

/// Cuts encoded frames into RTP packets for one SSRC.
pub struct Packetizer {
    ssrc: u32,
    codec: Codec,
    pub(super) sequence: u16,
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
    pub fn packets(&mut self, frame: &[u8], timestamp: u32, keyframe: bool) -> Vec<Vec<u8>> {
        let payloads = match self.codec {
            Codec::Vp8 => self.vp8_payloads(frame),
            Codec::H264 => h264_payloads(frame),
            Codec::H265 => h265_payloads(frame),
            Codec::Av1 => av1_payloads(frame, keyframe),
        };
        let count = payloads.len();
        let payload_type = self.codec.payload_types().0;
        payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| {
                let packet = self.header(
                    payload_type,
                    index + 1 == count,
                    self.sequence,
                    timestamp,
                    self.ssrc,
                    RID,
                    &payload,
                );
                self.sequence = self.sequence.wrapping_add(1);
                packet
            })
            .collect()
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
    #[expect(clippy::too_many_arguments, reason = "each is a field of the header")]
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
        packet.extend_from_slice(payload);
        self.transport = self.transport.wrapping_add(1);
        packet
    }

    /// VP8 payloads: each chunk after the extended payload descriptor with
    /// a 15-bit picture id; S marks the start of the frame's first partition.
    fn vp8_payloads(&mut self, frame: &[u8]) -> Vec<Vec<u8>> {
        let payloads = frame
            .chunks(CHUNK)
            .enumerate()
            .map(|(index, chunk)| {
                let start = if index == 0 { 0x10 } else { 0 };
                let mut payload = vec![0x80 | start, 0x80];
                payload.extend_from_slice(&(0x8000 | self.picture).to_be_bytes());
                payload.extend_from_slice(chunk);
                payload
            })
            .collect();
        self.picture = (self.picture + 1) & 0x7fff;
        payloads
    }
}

/// H264 payloads in packetization mode 1: a NAL unit that fits goes whole,
/// a bigger one in fragmentation units (FU-A).
pub(super) fn h264_payloads(frame: &[u8]) -> Vec<Vec<u8>> {
    let mut payloads = vec![];
    for unit in nal_units(frame) {
        let Some((&header, body)) = unit.split_first() else {
            continue;
        };
        if unit.len() <= CHUNK {
            payloads.push(unit.to_vec());
            continue;
        }
        let chunks: Vec<&[u8]> = body.chunks(CHUNK - 2).collect();
        let last = chunks.len() - 1;
        for (index, chunk) in chunks.into_iter().enumerate() {
            // The indicator keeps the unit's F and NRI bits; the FU header
            // its type, with start and end flags.
            let mut fragment = header & 0x1f;
            if index == 0 {
                fragment |= 0x80;
            }
            if index == last {
                fragment |= 0x40;
            }
            let mut payload = vec![(header & 0xe0) | 28, fragment];
            payload.extend_from_slice(chunk);
            payloads.push(payload);
        }
    }
    payloads
}

/// RFC 7798's payloads for an H265 access unit: each NAL unit whole when it
/// fits, and a larger one as fragmentation units (type 49), whose payload
/// header keeps the unit's F bit, layer and temporal id, and whose FU header
/// carries its type with start and end flags.
pub(super) fn h265_payloads(frame: &[u8]) -> Vec<Vec<u8>> {
    let mut payloads = vec![];
    for unit in nal_units(frame) {
        if unit.len() <= CHUNK {
            payloads.push(unit.to_vec());
            continue;
        }
        let ([first, second], body) = (unit.first_chunk().copied().unwrap_or_default(), &unit[2..]);
        let kind = (first >> 1) & 0x3f;
        let chunks: Vec<&[u8]> = body.chunks(CHUNK - 3).collect();
        let last = chunks.len() - 1;
        for (index, chunk) in chunks.into_iter().enumerate() {
            let mut fragment = kind;
            if index == 0 {
                fragment |= 0x80;
            }
            if index == last {
                fragment |= 0x40;
            }
            let mut payload = vec![(first & 0x81) | (49 << 1), second, fragment];
            payload.extend_from_slice(chunk);
            payloads.push(payload);
        }
    }
    payloads
}

/// AV1 OBU types the packetizer and DAVE handle apart.
pub(super) const OBU_SEQUENCE_HEADER: u8 = 1;
pub(super) const OBU_TEMPORAL_DELIMITER: u8 = 2;
pub(super) const OBU_TILE_LIST: u8 = 8;
pub(super) const OBU_PADDING: u8 = 15;
/// An OBU header's extension and size flags.
pub(super) const OBU_EXTENSION: u8 = 0x04;
pub(super) const OBU_HAS_SIZE: u8 = 0x02;

/// The OBUs of an AV1 temporal unit, each as an OBU element: its header
/// (and extension) without the size flag, then its payload. As libwebrtc's
/// packetizer reads them: an OBU without a size runs to the end of the
/// frame, as DAVE leaves the last one with its trailer after it; temporal
/// delimiters, tile lists and padding are dropped. None when an OBU runs
/// past the frame, or sets a bit its header keeps clear.
pub fn obu_elements(frame: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut elements = vec![];
    let mut rest = frame;
    while let Some(&header) = rest.first() {
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
        rest = &rest[header_size..];
        let payload_size = if header & OBU_HAS_SIZE != 0 {
            let (size, used) = read_leb128(rest)?;
            rest = &rest[used..];
            size
        } else {
            rest.len()
        };
        let payload = rest.get(..payload_size)?;
        rest = &rest[payload_size..];
        if matches!(
            (header >> 3) & 0x0f,
            OBU_TEMPORAL_DELIMITER | OBU_TILE_LIST | OBU_PADDING
        ) {
            continue;
        }
        let mut element = Vec::with_capacity(header_size + payload.len());
        element.push(header & !OBU_HAS_SIZE);
        element.extend_from_slice(&headers[1..]);
        element.extend_from_slice(payload);
        elements.push(element);
    }
    Some(elements)
}

/// AV1 payloads as AOM's RTP payload format lays them out: an aggregation
/// header, then OBU elements, each after its LEB128 length but for the last
/// when the packet holds three or fewer (W). An element too large for a
/// packet carries on in the next: Y marks the packet it continues from, Z
/// the one it continues in. N marks a keyframe's first packet when it
/// starts with the sequence header, as libwebrtc's packetizer marks it, and
/// receivers take a frame starting so for a keyframe. OBUs of different
/// temporal or spatial layers never share a packet. A malformed frame gives
/// no packets.
pub(super) fn av1_payloads(frame: &[u8], keyframe: bool) -> Vec<Vec<u8>> {
    let elements = (frame.len() <= MAX_FRAME)
        .then(|| obu_elements(frame))
        .flatten();
    let Some(elements) = elements else {
        tracing::warn!(size = frame.len(), "a malformed AV1 frame");
        return vec![];
    };
    // Each packet's pieces of elements, whether its first piece continues
    // the last packet's and its last carries on in the next.
    let mut packets: Vec<(Vec<&[u8]>, bool, bool)> = vec![];
    let mut pieces: Vec<&[u8]> = vec![];
    let (mut used, mut continues) = (1, false);
    // The temporal and spatial ids of the packet's OBUs that give them.
    let mut packet_layer = None;
    for element in &elements {
        let layer = (element[0] & OBU_EXTENSION != 0).then(|| element[1] & 0xf8);
        if layer.is_some() && packet_layer.is_some() && layer != packet_layer {
            packets.push((std::mem::take(&mut pieces), continues, false));
            (used, continues, packet_layer) = (1, false, None);
        }
        packet_layer = packet_layer.or(layer);
        let mut rest = element.as_slice();
        loop {
            // Room for a piece after a length of up to two bytes, which
            // covers anything up to CHUNK.
            if CHUNK.saturating_sub(used) < 3 {
                packets.push((std::mem::take(&mut pieces), continues, false));
                (used, continues, packet_layer) = (1, false, layer);
            }
            let take = rest.len().min(CHUNK - used - 2);
            pieces.push(&rest[..take]);
            used += leb128_size(take) + take;
            rest = &rest[take..];
            if rest.is_empty() {
                break;
            }
            packets.push((std::mem::take(&mut pieces), continues, true));
            (used, continues, packet_layer) = (1, true, layer);
        }
    }
    if !pieces.is_empty() {
        packets.push((pieces, continues, false));
    }
    let starts_sequence = keyframe
        && elements
            .first()
            .is_some_and(|element| (element[0] >> 3) & 0x0f == OBU_SEQUENCE_HEADER);
    packets
        .into_iter()
        .enumerate()
        .map(|(index, (pieces, continues, carries_on))| {
            let count = if pieces.len() <= 3 { pieces.len() } else { 0 };
            let mut header = (count as u8) << 4;
            if continues {
                header |= 0x80;
            }
            if carries_on {
                header |= 0x40;
            }
            if index == 0 && starts_sequence {
                header |= 0x08;
            }
            let mut payload = vec![header];
            for (at, piece) in pieces.iter().enumerate() {
                if count == 0 || at + 1 < count {
                    write_leb128(piece.len(), &mut payload);
                }
                payload.extend_from_slice(piece);
            }
            payload
        })
        .collect()
}

/// An unsigned LEB128 number and the bytes it took: at most eight, as AV1
/// allows.
pub(super) fn read_leb128(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0u64;
    for (index, &byte) in bytes.iter().enumerate().take(8) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((usize::try_from(value).ok()?, index + 1));
        }
    }
    None
}

pub(super) fn write_leb128(mut value: usize, out: &mut Vec<u8>) {
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

pub(super) fn leb128_size(value: usize) -> usize {
    (usize::BITS - value.max(1).leading_zeros()).div_ceil(7) as usize
}

/// The NAL units of an Annex B access unit, without their start codes. As
/// libwebrtc finds them: a start code is 0 0 1, with at most one more zero
/// before it, so bytes of a unit that happen to be zero stay in it.
pub fn nal_units(frame: &[u8]) -> Vec<&[u8]> {
    let mut starts = vec![];
    let mut at = 0;
    while at + 3 <= frame.len() {
        if frame[at..at + 3] == [0, 0, 1] {
            let code = if at > 0 && frame[at - 1] == 0 {
                at - 1
            } else {
                at
            };
            starts.push((code, at + 3));
            at += 3;
        } else {
            at += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, &(_, start))| {
            let end = starts.get(index + 1).map_or(frame.len(), |&(code, _)| code);
            &frame[start..end.max(start)]
        })
        .filter(|unit| !unit.is_empty())
        .collect()
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
pub(super) fn orientation(profile: u16, mut elements: &[u8]) -> Option<Orientation> {
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
