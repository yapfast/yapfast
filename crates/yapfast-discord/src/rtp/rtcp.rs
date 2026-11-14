//! `rtp.rs`'s rtcp.

use super::*;

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

pub(crate) fn describe_rtcp(kind: u8, format: u8, packet: &[u8]) -> String {
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
pub(crate) fn remb(packet: &[u8]) -> Option<(u64, Vec<u32>)> {
    if packet.get(1) != Some(&206) || packet[0] & 0x1f != 15 || packet.get(12..16)? != b"REMB" {
        return None;
    }
    let count = usize::from(*packet.get(16)?);
    let exponent = packet.get(17)? >> 2;
    let mantissa = u32::from_be_bytes(packet.get(16..20)?.try_into().ok()?) & 0x3_ffff;
    packet.get(..20 + count * 4)?;
    let ssrcs = (0..count)
        .filter_map(|index| {
            let at = 20 + index * 4;
            Some(u32::from_be_bytes(packet.get(at..at + 4)?.try_into().ok()?))
        })
        .collect();
    let bitrate = u64::from(mantissa).saturating_mul(1u64 << exponent);
    Some((bitrate, ssrcs))
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

/// The sequence numbers of `media` packets that generic NACKs (RTCP 205,
/// format 1) in a decrypted compound packet report lost: each entry names
/// one, and a bitmask the 16 after it. A packet named twice is named once:
/// Discord's stream server relays every viewer's report in the same datagram,
/// so two people who lost the same packet ask for it in two blocks, and one
/// resend answers both.
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
    // In place and in order: a report names a few packets, and the order is
    // the first time each was asked for rather than anything a sequence
    // number can be sorted by across its wrap.
    let mut kept = 0;
    for index in 0..lost.len() {
        let sequence = lost[index];
        if lost[..kept].contains(&sequence) {
            continue;
        }
        lost[kept] = sequence;
        kept += 1;
    }
    lost.truncate(kept);
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
pub fn report_blocks(rtcp: &[u8]) -> Vec<(u32, u32, u32)> {
    blocks(rtcp)
        .into_iter()
        .map(|block| (block.ssrc, block.lost, block.highest))
        .collect()
}

/// The last report block about `ssrc` in a decrypted compound RTCP packet.
pub fn reception(rtcp: &[u8], ssrc: u32) -> Option<Reception> {
    blocks(rtcp).into_iter().rfind(|block| block.ssrc == ssrc)
}

pub(crate) fn blocks(mut rtcp: &[u8]) -> Vec<Reception> {
    let mut blocks = vec![];
    while rtcp.len() >= 4 {
        let length = (usize::from(u16::from_be_bytes([rtcp[2], rtcp[3]])) + 1) * 4;
        let packet = &rtcp[..length.min(rtcp.len())];
        let word = |at: usize| {
            packet
                .get(at..at + 4)
                .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        };
        // A sender or receiver report names its own SSRC after the header.
        let reporter = word(4).unwrap_or_default();
        // A sender report's blocks follow its 20 bytes of sender information.
        let first = match packet[1] {
            200 => Some(28),
            201 => Some(8),
            _ => None,
        };
        for at in first.into_iter().flat_map(|first| {
            (0..usize::from(packet[0] & 0x1f)).map(move |block| first + block * 24)
        }) {
            let block = || {
                let lost = word(at + 4)?;
                Some(Reception {
                    reporter,
                    ssrc: word(at)?,
                    fraction_lost: (lost >> 24) as u8,
                    lost: lost & 0x00ff_ffff,
                    highest: word(at + 8)?,
                    jitter: word(at + 12)?,
                    last_report: word(at + 16)?,
                    delay: word(at + 20)?,
                })
            };
            blocks.extend(block());
        }
        rtcp = rtcp.get(length..).unwrap_or_default();
    }
    blocks
}

/// The round trip a report block measured: from `sent`, when this end sent
/// the sender report it echoes, to `received`, less the `delay` the other
/// end held it, in 1/65536 s. None when the delay is longer than the time
/// between, as a report about another sender report would be.
pub fn round_trip(
    sent: std::time::Instant,
    received: std::time::Instant,
    delay: u32,
) -> Option<std::time::Duration> {
    let held = std::time::Duration::from_micros(u64::from(delay) * 1_000_000 / 65_536);
    received.checked_duration_since(sent)?.checked_sub(held)
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
