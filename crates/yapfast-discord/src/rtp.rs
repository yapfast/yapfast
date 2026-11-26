//! RTP for Go Live video: packetizing and reassembling VP8 (RFC 7741),
//! H264 (RFC 6184), H265 (RFC 7798) and AV1 (AOM's RTP payload format)
//! frames, Discord's "rtpsize" transport encryption, and the
//! RTCP that goes with them.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use aes_gcm::{Aes256Gcm, KeyInit, aead::AeadInOut};
use anyhow::{Result, anyhow, bail};
use chacha20poly1305::XChaCha20Poly1305;

mod audio;
mod pace;
mod packetize;
mod receive;
mod resend;
mod rtcp;
mod timing;
mod transport;

pub use audio::*;
pub(crate) use pace::*;
pub use packetize::*;
pub use receive::*;
pub use resend::*;
pub use rtcp::*;
pub use timing::*;
pub use transport::*;

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

#[cfg(test)]
mod tests;
