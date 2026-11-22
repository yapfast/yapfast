//! The codecs a stream is carried in, and the payload types that name them.
//!
//! Discord's session descriptions name them as these four, and its voice
//! server assigns these payload types; both are RTP's vocabulary rather than
//! a client's, so a streamer's encoder can name a codec without naming one.

pub const H264: u8 = 101;
pub const H264_RTX: u8 = 102;
pub const VP8: u8 = 105;
pub const VP8_RTX: u8 = 106;
pub const H265: u8 = 103;
pub const H265_RTX: u8 = 104;
pub const AV1: u8 = 107;
pub const AV1_RTX: u8 = 108;

/// The video codecs Yapfast sends or receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
