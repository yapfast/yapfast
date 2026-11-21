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
///
/// Only the name comes before the first space, comma or slash; the rest says
/// what the path does. A slash names the device a picture is read on —
/// "av1_nvenc/d3d11" is NVENC on the GPU reading the capture's own Direct3D 11
/// device — a bracket the device a VA-API session opened on with its driver,
/// and ", encoding DMA-BUFs where they lie" the card's own path. None of that
/// says where the encoding runs, which is what this answers.
pub fn encoder(engine: &str) -> String {
    let (name, place) = engine.split_once(' ').unwrap_or((engine, ""));
    let name = name.split_once('/').map_or(name, |(name, _)| name);
    // "h264_vaapi, scaling and converting DMA-BUFs on the card".
    let name = name.trim_end_matches(',');
    let driver = place
        .strip_prefix('(')
        .and_then(|place| place.split_once(')'))
        .and_then(|(inside, _)| inside.rsplit(", ").next())
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

/// The Stream tab's rows, in the order it lists them: a label and its value.
/// `sharing` is this account's own screen, which has a bitrate and a capture
/// of its own; a watched stream has neither, and its rows say what decodes it
/// and how far its picture runs from its sound instead.
///
/// A row whose value nothing answered reads "unknown" rather than zero, and a
/// row with nothing to say — no subtitle track asked for a keyframe, no
/// fallback to explain — isn't listed at all.
pub fn stream_rows(stream: &StreamStats, sharing: bool) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    let mut row = |label: &'static str, value: &str| {
        if !value.is_empty() {
            rows.push((label, value.to_owned()));
        }
    };
    if sharing {
        row("Encoder:", &encoder(&stream.engine));
    } else if let Some(decoder) = stream.decoder.as_deref() {
        row("Decoded by:", decoder);
    }
    row("Codec:", &stream.codec);
    if sharing {
        row("Settings the encoder dropped:", &stream.settings_dropped);
    }
    row("Resolution:", &picture(stream));
    if sharing {
        row("Capture:", &stream.capture);
        // A watched stream has no bitrate of its own to report, and the rows
        // for one are left out rather than filled with a share's numbers.
        row("Sending:", &rate(stream.sent_kbps));
        row("Aiming for:", &rate(stream.target_kbps));
        row(
            "Keyframes viewers asked for:",
            &stream.keyframe_requests.to_string(),
        );
        row("Packets resent:", &stream.resent.to_string());
        row("Frames a full queue skipped:", &stream.skipped.to_string());
    }
    if sharing {
        let spell = |held: Option<u32>| {
            held.map_or_else(|| "unknown".to_owned(), |held| format!("{held} ms"))
        };
        row("Shared sound waiting:", &spell(stream.sound_held_ms));
        let dropped = stream
            .sound_dropped_ms
            .map_or_else(|| "unknown".to_owned(), |dropped| format!("{dropped} ms"));
        row("Shared sound dropped:", &dropped);
    }
    // A watched stream's picture against its sound, both as it came and as the
    // window was handed it, and the pictures the sound had passed: a watch that
    // stutters or drifts says which here.
    let skew = |skew: Option<i64>| skew.map_or_else(String::new, |skew| format!("{skew:+} ms"));
    let spent = |ms: Option<u32>| ms.map_or_else(String::new, |ms| format!("{ms} ms"));
    if !sharing {
        row("Sound played:", &spent(stream.sound_heard_ms));
        row(
            "Sound queued in the device:",
            &spent(stream.sound_queued_ms),
        );
        row(
            "Picture against the sound, as it arrived:",
            &skew(stream.arrival_skew_ms),
        );
        row(
            "Picture against the sound, shown:",
            &skew(stream.presentation_skew_ms),
        );
        row(
            "Pictures the sound had passed:",
            &stream
                .late_pictures
                .map(|late| late.to_string())
                .unwrap_or_default(),
        );
    }
    row(
        "Shown by:",
        &stream
            .presentation
            .map_or_else(|| "unknown".to_owned(), |shown| shown.name().to_owned()),
    );
    row("The window draws on:", &adapter_name(stream.present_luid));
    row("Frame wait before encoding:", &wait(stream.queue_delay_ms));
    row(
        "Encoder buffers:",
        &buffers(stream.pool_live, stream.pool_peak),
    );
    row("Memory:", stream.memory.as_deref().unwrap_or_default());
    if sharing {
        row(
            "Not on the card's own path:",
            stream
                .fallback
                .as_deref()
                .map(shared_only)
                .unwrap_or_default(),
        );
    }
    rows
}

/// An adapter as a row names one: its LUID in hex, which is the same on every
/// run and is what two adapters are compared by. Empty where nothing asked.
fn adapter_name(luid: Option<u64>) -> String {
    luid.map(|luid| format!("adapter {luid:#x}"))
        .unwrap_or_default()
}

/// A wait in milliseconds, or "unknown" where nothing measured one.
fn wait(value: Option<u32>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |ms| format!("{ms} ms"))
}

/// What the encoder's output buffers hold now and the most they have held, as
/// people read bytes: "512 KiB, at most 1.0 MiB". Empty where unknown.
fn buffers(live: Option<u64>, peak: Option<u64>) -> String {
    let (Some(live), Some(peak)) = (live, peak) else {
        return String::new();
    };
    let bytes = |bytes: u64| {
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        if bytes >= MIB {
            format!("{:.1} MiB", bytes as f64 / MIB as f64)
        } else {
            format!("{} KiB", bytes / KIB)
        }
    };
    format!("{}, at most {}", bytes(live), bytes(peak))
}

/// The part of a report's line that names what ran, never what was captured:
/// a window's title or a screen's name is the person's own business, and a
/// capture's or a fallback's line can carry it.
fn shared_only(line: &str) -> &str {
    line.split_once(" · ").map_or(line, |(what, _)| what)
}

/// The stream's latest report as one block of text to paste into a bug
/// report: what makes a picture and what shows it, and the numbers that say
/// why it looks or behaves as it does.
///
/// A value the worker or the window didn't answer is written as "unknown"
/// rather than as zero, so a report never claims a machine said something it
/// didn't, and nothing that isn't about the stream goes in: no token, no
/// server, no window title, no media.
pub fn diagnostics(stream: &StreamStats) -> String {
    let mut report = serde_json::Map::new();
    let mut put = |key: &str, value: serde_json::Value| {
        report.insert(key.to_owned(), value);
    };
    put("encoder", stream.engine.as_str().into());
    put("codec", stream.codec.as_str().into());
    put("capture", shared_only(&stream.capture).into());
    put("decoder", stream.decoder.as_deref().into());
    put(
        "presentation",
        stream
            .presentation
            .map_or_else(|| "unknown".to_owned(), |shown| shown.name().to_owned())
            .into(),
    );
    for (key, luid) in [
        ("capture_adapter", stream.capture_luid),
        ("encoder_adapter", stream.encode_luid),
        ("window_adapter", stream.present_luid),
    ] {
        put(
            key,
            luid.map_or_else(|| "unknown".to_owned(), |luid| format!("{luid:#x}"))
                .into(),
        );
    }
    put(
        "fallback",
        stream.fallback.as_deref().map(shared_only).into(),
    );
    put("memory", stream.memory.as_deref().into());
    // Signed, and as text: which way the picture runs from the sound is the
    // whole point of the number.
    put(
        "arrival_skew_ms",
        stream
            .arrival_skew_ms
            .map_or_else(String::new, |skew| format!("{skew:+}"))
            .into(),
    );
    put(
        "presentation_skew_ms",
        stream
            .presentation_skew_ms
            .map_or_else(String::new, |skew| format!("{skew:+}"))
            .into(),
    );
    put("preview_shown", stream.preview.into());
    put("width", stream.width.into());
    put("height", stream.height.into());
    put("fps", stream.fps.into());
    put("target_kbps", stream.target_kbps.into());
    put("sent_kbps", stream.sent_kbps.into());
    put("keyframe_requests", stream.keyframe_requests.into());
    put("resent", stream.resent.into());
    put("skipped", stream.skipped.into());
    for (key, value) in [
        ("sound_held_ms", stream.sound_held_ms.map(u64::from)),
        ("sound_dropped_ms", stream.sound_dropped_ms),
        ("late_pictures", stream.late_pictures),
        ("queue_delay_ms", stream.queue_delay_ms.map(u64::from)),
        ("pool_live_bytes", stream.pool_live),
        ("pool_peak_bytes", stream.pool_peak),
    ] {
        put(key, value.into());
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(report))
        .unwrap_or_else(|_| String::new())
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
        // The path beside the name says where a picture is read, not where the
        // encoding runs.
        assert_eq!(encoder("av1_nvenc/d3d11"), "NVENC on the GPU");
        assert_eq!(
            encoder("h264_vaapi, scaling and converting DMA-BUFs on the card"),
            "VA-API on the GPU"
        );
        assert_eq!(
            encoder("hevc_vaapi (renderD128, radeonsi), encoding DMA-BUFs where they lie"),
            "VA-API on the GPU (radeonsi)"
        );
        assert_eq!(encoder("x264"), "x264 on the CPU");
        assert_eq!(rate(3812), "3.8 Mbit/s");
        assert_eq!(rate(800), "800 kbit/s");
    }

    #[test]
    fn the_streams_rows_say_what_a_share_has_and_a_watch_hasnt() {
        let share = StreamStats {
            engine: "av1_nvenc/d3d11".into(),
            codec: "Av1".into(),
            capture: "Windows Graphics Capture · 3440×1440 screen".into(),
            width: 3440,
            height: 1440,
            fps: 60,
            target_kbps: 35_000,
            sent_kbps: 7_110,
            keyframe_requests: 2,
            resent: 3,
            skipped: 1,
            sound_held_ms: Some(40),
            sound_dropped_ms: None,
            presentation: Some(crate::voice::Presentation::Shared),
            present_luid: Some(0x1488a),
            queue_delay_ms: Some(12),
            pool_live: Some(512 * 1024),
            pool_peak: Some(1024 * 1024),
            fallback: Some("sharing from the processor · a card refused".into()),
            ..Default::default()
        };
        let rows = stream_rows(&share, true);
        let find = |label: &str| {
            rows.iter()
                .find(|(name, _)| *name == label)
                .map(|(_, value)| value.clone())
        };
        // The encoder's name carries the path it reads a picture on, which
        // doesn't change where the encoding runs.
        assert_eq!(find("Encoder:").as_deref(), Some("NVENC on the GPU"));
        assert_eq!(find("Sending:").as_deref(), Some("7.1 Mbit/s"));
        assert_eq!(find("Aiming for:").as_deref(), Some("35.0 Mbit/s"));
        assert_eq!(find("Shared sound waiting:").as_deref(), Some("40 ms"));
        assert_eq!(
            find("Shared sound dropped:").as_deref(),
            Some("unknown"),
            "nothing answered, so not zero"
        );
        assert_eq!(
            find("Shown by:").as_deref(),
            Some("the worker's own texture")
        );
        assert_eq!(
            find("The window draws on:").as_deref(),
            Some("adapter 0x1488a")
        );
        assert_eq!(
            find("Encoder buffers:").as_deref(),
            Some("512 KiB, at most 1.0 MiB")
        );
        // A window's title in a capture's or a fallback's line is the
        // person's own business.
        assert_eq!(
            find("Not on the card's own path:").as_deref(),
            Some("sharing from the processor")
        );
        // A share has no decoding, sound or skew of a watched stream.
        assert_eq!(find("Decoded by:"), None);
        assert_eq!(find("Sound played:"), None);
        // A watched stream has the decoder and the skews, and no bitrate.
        let watched = StreamStats {
            engine: String::new(),
            codec: "H265".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            decoder: Some("h265 (d3d11va)".into()),
            presentation: Some(crate::voice::Presentation::Copy),
            sound_heard_ms: Some(2_500),
            sound_queued_ms: Some(60),
            arrival_skew_ms: Some(-12),
            presentation_skew_ms: Some(8),
            late_pictures: Some(2),
            ..Default::default()
        };
        let rows = stream_rows(&watched, false);
        let find = |label: &str| {
            rows.iter()
                .find(|(name, _)| *name == label)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(find("Decoded by:").as_deref(), Some("h265 (d3d11va)"));
        assert_eq!(find("Encoder:"), None);
        assert_eq!(find("Sending:"), None);
        assert_eq!(find("Capture:"), None);
        assert_eq!(find("Sound played:").as_deref(), Some("2500 ms"));
        assert_eq!(
            find("Picture against the sound, as it arrived:").as_deref(),
            Some("-12 ms")
        );
        assert_eq!(
            find("Picture against the sound, shown:").as_deref(),
            Some("+8 ms")
        );
        assert_eq!(find("Pictures the sound had passed:").as_deref(), Some("2"));
        assert_eq!(find("Shown by:").as_deref(), Some("a copy of the picture"));
        assert_eq!(
            find("Frame wait before encoding:").as_deref(),
            Some("unknown")
        );
    }

    #[test]
    fn the_export_names_no_window_and_says_unknown_rather_than_zero() {
        let stats = StreamStats {
            // A capture's line can carry a window's title, which is the
            // person's own business and nobody else's.
            capture: "Windows Graphics Capture · 3440×1440 · Notepad — untitled".to_owned(),
            engine: "h264_nvenc/d3d11".to_owned(),
            codec: "H264".to_owned(),
            width: 1920,
            height: 1080,
            fps: 30,
            ..Default::default()
        };
        let exported = diagnostics(&stats);
        assert!(!exported.contains("Notepad"), "{exported}");
        assert!(
            exported.contains("\"Windows Graphics Capture\""),
            "{exported}"
        );
        // What nothing answered is unknown, not zero.
        assert!(
            exported.contains("\"presentation\": \"unknown\""),
            "{exported}"
        );
        assert!(
            exported.contains("\"window_adapter\": \"unknown\""),
            "{exported}"
        );
        assert!(exported.contains("\"pool_live_bytes\": null"), "{exported}");
        // What it does know is there, as the number it is.
        assert!(exported.contains("\"width\": 1920"), "{exported}");
        assert!(
            exported.contains("\"encoder\": \"h264_nvenc/d3d11\""),
            "{exported}"
        );
        // A stream with nothing to explain exports no reason at all.
        assert!(exported.contains("\"fallback\": null"), "{exported}");
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
