//! The call's connection in words and lines, as the voice details popout
//! shows it: the pings, the loss, the graph's steps, the privacy code's
//! groups, how a shared screen goes out and how a watched one comes in,
//! and all of it as text to paste into a bug report. No toolkit in it.

use crate::voice::{LinkStats, StreamStats, WatchStats};

/// The graph's box, and the pings it spans: five minutes of them, as the
/// call keeps.
pub const GRAPH: (f32, f32) = (300.0, 100.0);
pub const SPAN: usize = 60;

/// A ping as people read it: "42 ms".
pub fn milliseconds(ping: u32) -> String {
    format!("{ping} ms")
}

/// The last ping, as the signal's tooltip and the popout say it.
pub fn last_ping(stats: &LinkStats) -> Option<String> {
    stats.pings.last().map(|&ping| milliseconds(ping.into()))
}

/// The share of packets lost, as "1.2%".
pub fn loss(stats: &LinkStats) -> Option<String> {
    stats.loss.map(|loss| format!("{loss:.1}%"))
}

pub fn average(pings: &[u16]) -> Option<u32> {
    let sum: u32 = pings.iter().copied().map(u32::from).sum();
    (!pings.is_empty()).then(|| sum / pings.len() as u32)
}

/// One step of the graph in [`GRAPH`]'s box: from `x0` to `x1` at `y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

/// The pings as steps, the newest at the right edge, and the ping at the
/// top: the smallest 1, 2 or 5 times a power of ten above them all. Each
/// ping holds for the step before it, so even one shows.
pub fn graph(pings: &[u16]) -> (Vec<Step>, u32) {
    let highest = pings.iter().copied().max().map_or(0, u32::from);
    let top = [1, 2, 5]
        .into_iter()
        .flat_map(|step| (1..6).map(move |power| step * 10u32.pow(power)))
        .filter(|&top| top >= highest)
        .min()
        .unwrap_or(highest);
    let step = GRAPH.0 / SPAN as f32;
    let y = |ping: u16| GRAPH.1 - f32::from(ping) / top as f32 * GRAPH.1;
    let first = SPAN.saturating_sub(pings.len());
    let steps = pings
        .iter()
        .rev()
        .take(SPAN)
        .rev()
        .enumerate()
        .map(|(index, &ping)| {
            let x1 = (first + index + 1) as f32 * step;
            Step {
                x0: x1 - step,
                x1,
                y: y(ping),
            }
        })
        .collect();
    (steps, top)
}

/// A privacy code in the groups of five digits it's read in.
pub fn groups(code: &str) -> Vec<String> {
    code.as_bytes()
        .chunks(5)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect()
}

/// Where the stream's encoder runs, by its name: "h264_nvenc" is NVENC on
/// the GPU, "h264_amf" AMD's AMF, "hevc_vaapi (renderD128, radeonsi)"
/// VA-API through Mesa's radeonsi driver, "x264" on the CPU.
pub fn encoder(engine: &str) -> String {
    let (name, place) = engine.split_once(' ').unwrap_or((engine, ""));
    let driver = place
        .trim_matches(['(', ')'])
        .rsplit(", ")
        .next()
        .filter(|driver| !driver.is_empty());
    match name.rsplit_once('_') {
        Some((_, "nvenc")) => "NVENC on the GPU".to_owned(),
        Some((_, "amf")) => "AMF on the GPU".to_owned(),
        Some((_, "vaapi")) => match driver {
            Some(driver) => format!("VA-API on the GPU ({driver})"),
            None => "VA-API on the GPU".to_owned(),
        },
        _ => format!("{name} on the CPU"),
    }
}

/// A bitrate as people read it: "3.8 Mbit/s", or "800 kbit/s" below one.
pub fn rate(kbps: u32) -> String {
    if kbps >= 1000 {
        format!("{:.1} Mbit/s", f64::from(kbps) / 1000.0)
    } else {
        format!("{kbps} kbit/s")
    }
}

/// The stream's size and frame rate: "1920×1080 at 60 fps".
pub fn picture(stream: &StreamStats) -> String {
    format!("{}×{} at {} fps", stream.width, stream.height, stream.fps)
}

/// A count, and what share of `of` it is when there's any: "12 (0.4%)".
fn share(count: u64, of: u64) -> String {
    if count == 0 || of == 0 {
        count.to_string()
    } else {
        format!("{count} ({:.1}%)", count as f64 * 100.0 / of as f64)
    }
}

/// The Stream tab's rows for the screen this account shares.
pub fn sharing_rows(stream: &StreamStats) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Encoder:", encoder(&stream.engine)),
        ("Codec:", stream.codec.clone()),
    ];
    if !stream.captured.is_empty() {
        rows.push(("Captured:", stream.captured.clone()));
    }
    rows.extend([
        ("Resolution:", picture(stream)),
        ("Sending:", rate(stream.sent_kbps)),
        ("Aiming for:", rate(stream.target_kbps)),
        (
            "Keyframes viewers asked for:",
            stream.keyframe_requests.to_string(),
        ),
        ("Packets sent:", stream.packets.to_string()),
        ("Packets resent:", share(stream.resent, stream.packets)),
    ]);
    rows
}

/// The Stream tab's rows for the stream this account watches.
pub fn watching_rows(watching: &WatchStats) -> Vec<(&'static str, String)> {
    let decoder = if watching.decoder.is_empty() {
        "Starting".to_owned()
    } else {
        format!("{} on the CPU", watching.decoder)
    };
    vec![
        ("Decoder:", decoder),
        ("Codec:", watching.codec.clone()),
        (
            "Resolution:",
            format!(
                "{}×{} at {} fps",
                watching.width, watching.height, watching.fps
            ),
        ),
        ("Receiving:", rate(watching.received_kbps)),
        ("Packets received:", watching.packets.to_string()),
        ("Packets lost:", share(watching.lost, watching.packets)),
        ("Packets resent to you:", watching.repaired.to_string()),
        (
            "Keyframes asked for:",
            watching.keyframe_requests.to_string(),
        ),
        ("Frames dropped:", watching.dropped.to_string()),
    ]
}

/// Everything the details show, as text for a bug report: `build`, the
/// build's one-line summary, then the connection and each stream's rows.
pub fn diagnostics(build: &str, stats: &LinkStats) -> String {
    use std::fmt::Write as _;
    // Writing to a String can't fail.
    let mut text = String::new();
    let _ = writeln!(text, "{build}\n{}\n\nConnection", std::env::consts::OS);
    let average = average(&stats.pings).map_or_else(|| "Unknown".to_owned(), milliseconds);
    let _ = writeln!(text, "Average ping: {average}");
    if let Some(ping) = last_ping(stats) {
        let _ = writeln!(text, "Last ping: {ping}");
    }
    if let Some(loss) = loss(stats) {
        let _ = writeln!(text, "Outbound packet loss rate: {loss}");
    }
    let sections = [
        ("Sharing", stats.stream.as_ref().map(sharing_rows)),
        ("Watching", stats.watching.as_ref().map(watching_rows)),
    ];
    for (title, rows) in sections {
        let Some(rows) = rows else { continue };
        let _ = writeln!(text, "\n{title}");
        for (label, value) in rows {
            let _ = writeln!(text, "{label} {value}");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphs_the_pings_up_to_a_round_top() {
        assert_eq!(graph(&[]), (Vec::new(), 10));
        let (steps, top) = graph(&[40, 80]);
        assert_eq!(top, 100);
        let rounded: Vec<_> = steps
            .iter()
            .map(|step| (step.x0.round(), step.x1.round(), step.y.round()))
            .collect();
        assert_eq!(rounded, [(290.0, 295.0, 60.0), (295.0, 300.0, 20.0)]);
        assert_eq!(graph(&[260]).1, 500);
        assert_eq!(graph(&[1500]).1, 2000);
    }

    /// The copied report holds the connection and each stream's rows, as
    /// the tab words them, under the build it came from.
    #[test]
    fn diagnostics_say_everything_the_details_show() {
        let stats = LinkStats {
            pings: vec![40, 80],
            loss: Some(1.5),
            privacy_code: Some("123456789012345678901234567890".into()),
            stream: None,
            watching: Some(WatchStats {
                decoder: "FFmpeg".into(),
                codec: "H264".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                received_kbps: 5200,
                packets: 2000,
                lost: 0,
                repaired: 4,
                keyframe_requests: 1,
                dropped: 2,
            }),
        };
        let text = diagnostics("Yapfast 0.5.0 (abc1234)", &stats);
        assert!(text.starts_with("Yapfast 0.5.0 (abc1234)\n"), "{text}");
        for line in [
            "Average ping: 60 ms",
            "Last ping: 80 ms",
            "Outbound packet loss rate: 1.5%",
            "Watching",
            "Decoder: FFmpeg on the CPU",
            "Resolution: 1920×1080 at 30 fps",
            "Receiving: 5.2 Mbit/s",
            "Packets lost: 0",
            "Packets resent to you: 4",
            "Frames dropped: 2",
        ] {
            assert!(text.contains(line), "{line} in {text}");
        }
        assert!(!text.contains("Sharing"), "nothing shared");
        assert!(!text.contains("12345"), "never the privacy code");
    }

    #[test]
    fn says_where_the_stream_is_encoded() {
        assert_eq!(encoder("h264_nvenc"), "NVENC on the GPU");
        assert_eq!(encoder("av1_amf"), "AMF on the GPU");
        assert_eq!(
            encoder("hevc_vaapi (renderD128, radeonsi)"),
            "VA-API on the GPU (radeonsi)"
        );
        assert_eq!(encoder("x264"), "x264 on the CPU");
        assert_eq!(rate(3812), "3.8 Mbit/s");
        assert_eq!(rate(800), "800 kbit/s");
    }

    #[test]
    fn reads_the_code_in_groups_of_five() {
        let groups = groups("123456789012345678901234567890");
        assert_eq!(groups.len(), 6);
        assert_eq!(groups[1], "67890");
        assert_eq!(average(&[10, 20, 31]), Some(20));
        assert_eq!(average(&[]), None);
    }
}
