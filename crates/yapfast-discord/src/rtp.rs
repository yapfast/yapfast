//! RTP for Go Live video: packetizing and reassembling VP8 (RFC 7741),
//! H264 (RFC 6184), H265 (RFC 7798) and AV1 (AOM's RTP payload format)
//! frames, Discord's "rtpsize" transport encryption, and the
//! RTCP that goes with them.

use std::time::{Duration, Instant};

use aes_gcm::{Aes256Gcm, KeyInit, aead::AeadInPlace};
use anyhow::{Result, anyhow, bail};
use chacha20poly1305::XChaCha20Poly1305;

pub use yapfast_media::codec::{AV1, AV1_RTX, Codec, H264, H264_RTX, H265, H265_RTX, VP8, VP8_RTX};
pub use yapfast_media::rtcp::Reception;

mod audio;
mod packetize;
mod receive;
mod resend;
mod rtcp;
mod timing;
mod transport;

#[allow(unused_imports)]
pub(crate) use audio::*;
#[allow(unused_imports)]
pub(crate) use packetize::*;
#[allow(unused_imports)]
pub(crate) use receive::*;
#[allow(unused_imports)]
pub(crate) use resend::*;
#[allow(unused_imports)]
pub(crate) use rtcp::*;
#[allow(unused_imports)]
pub(crate) use timing::*;
#[allow(unused_imports)]
pub(crate) use transport::*;

pub use audio::OPUS;
pub use audio::opus_packet;
pub use audio::write_audio_into;
pub use packetize::Orientation;
pub use packetize::Packetizer;
pub use packetize::Plan;
pub use packetize::STREAM_RID;
pub use packetize::nal_units;
pub use packetize::obu_elements;
pub use receive::Assembled;
pub use receive::Assembler;
pub use receive::Packet;
pub use receive::Pushed;
pub use receive::before;
pub use receive::parse;
pub use receive::picture_loss;
pub use resend::Held;
pub use resend::History;
pub use rtcp::bandwidth_estimate;
pub use rtcp::is_rtcp;
pub use rtcp::nack;
pub use rtcp::nacked;
pub use rtcp::receiver_report;
pub use rtcp::reception;
pub use rtcp::report_blocks;
pub use rtcp::round_trip;
pub use rtcp::rtcp_kinds;
pub use rtcp::sender_report;
pub use rtcp::sender_reports;
pub use rtcp::wants_keyframe;
pub use timing::Arrival;
pub use timing::Reorder;
pub use transport::RTCP_CLEAR;
pub use transport::Transport;
pub use transport::rtp_clear_len;

#[cfg(test)]
mod tests;
