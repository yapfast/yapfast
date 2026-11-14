//! What a receiver report says about a stream: which end reported, what it
//! lost, and when the streamer's sender report reached it.

/// What one report block of a sender or receiver report says about an SSRC.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reception {
    /// The SSRC of the end that reported: Discord's stream server, or a viewer
    /// whose report the server passed on, so that one end's count of the video
    /// isn't taken for another's.
    pub reporter: u32,
    pub ssrc: u32,
    /// Packets lost since the previous report, out of 256.
    pub fraction_lost: u8,
    /// Packets lost so far.
    pub lost: u32,
    /// The highest sequence number received, extended.
    pub highest: u32,
    /// Interarrival jitter, in the stream's RTP clock.
    pub jitter: u32,
    /// The middle 32 bits of the NTP time of the last sender report heard
    /// from the SSRC, or 0 before one, and the delay since it, in 1/65536 s.
    pub last_report: u32,
    pub delay: u32,
}

/// One of the streamer's sender reports: the RTP timestamp its clock read at
/// the NTP time the report names, and when the report reached Yapfast. The
/// middle 32 bits of the NTP time are kept, as a receiver report echoes them
/// and they are all a relative mapping needs; the whole 64 bits would say
/// the same to the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SenderReport {
    /// The middle 32 bits of the NTP time: seconds since 1900 in 16.16,
    /// keeping the low 16 bits of the seconds and the high 16 of the
    /// fraction, so the seconds turn over every 18 hours.
    pub ntp: u32,
    /// The RTP timestamp its clock carried at that instant.
    pub rtp: u32,
    /// When the report reached Yapfast, which says how fresh the mapping is.
    pub heard: std::time::Instant,
}
