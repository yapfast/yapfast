//! The call's connection in words and lines, as the voice details popout
//! shows it: the pings, the loss, the graph's steps, the privacy code's
//! groups and how a shared screen goes out. No toolkit in it.

use crate::voice::{LinkStats, StreamStats};

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
