//! Packets kept to send again when a viewer reports them lost.

use super::*;

/// The video packets sent lately, unencrypted, to resend when a viewer
/// reports one lost: about two seconds at 9 Mbit/s.
pub struct History {
    packets: Vec<Option<(u16, Vec<u8>)>>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            packets: vec![None; 2048],
        }
    }
}

impl History {
    pub fn remember(&mut self, packet: &[u8]) {
        if let Some(sent) = parse(packet) {
            let slot = usize::from(sent.sequence) % self.packets.len();
            self.packets[slot] = Some((sent.sequence, packet.to_vec()));
        }
    }

    pub fn get(&self, sequence: u16) -> Option<&[u8]> {
        match &self.packets[usize::from(sequence) % self.packets.len()] {
            Some((kept, packet)) if *kept == sequence => Some(packet),
            _ => None,
        }
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
