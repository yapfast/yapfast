//! Opus packets for a call's voice.

use super::*;

/// An RTP packet carrying one Opus frame from `ssrc`, with `timestamp` in
/// 48 kHz samples and no extensions, as a call's audio goes. The marker
/// starts a burst of sound.
pub fn opus_packet(ssrc: u32, sequence: u16, timestamp: u32, marker: bool, opus: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(HEADER + opus.len() + 20);
    packet.extend_from_slice(&[0x80, u8::from(marker) << 7 | OPUS]);
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());
    packet.extend_from_slice(opus);
    packet
}
