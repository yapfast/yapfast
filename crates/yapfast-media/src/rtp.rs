//! What a packet of a stream's sound or picture is, in RTP's terms: its
//! header's numbering and timing, the payload types a session offers, and the
//! bytes that say how a picture is to be turned upright.

/// RTP's fixed header.
pub const HEADER: usize = 12;

/// `urn:3gpp:video-orientation` (CVO), which a native client numbers 4
/// (docs.discord.food, Voice Connections, "Native RTP Header Extensions"; its
/// WebRTC SDP says 13). Phones film in the sensor's orientation and say here
/// how to turn each frame upright.
pub const VIDEO_ORIENTATION: u8 = 4;

/// The payload types Yapfast offers Discord, as Discord's clients number them.
pub const OPUS: u8 = 120;

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
pub(crate) fn orientation(profile: u16, mut elements: &[u8]) -> Option<Orientation> {
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
