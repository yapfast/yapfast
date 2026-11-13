//! `rtp.rs`'s audio.

use super::*;

/// The payload types Yapfast offers Discord, as Discord's clients number them.
pub const OPUS: u8 = 120;

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
