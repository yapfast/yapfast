//! The call's connection in detail: the ping as the signal icon's tooltip,
//! and the popout's graph, numbers and voice privacy code, and how a shared
//! screen goes out. The call measures them all along; they're copied out of
//! it only while one of them shows.

use std::fmt::Write;

use yapfast_core::voice::LinkStats;

use super::*;

/// The graph's box, as `CallLinkCard`'s path takes it, and the pings it
/// spans: five minutes of them, as the call keeps.
const GRAPH: (f32, f32) = (300.0, 100.0);
const SPAN: usize = 60;

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window.global::<CallLink>().on_read(move || {
        let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
            return;
        };
        let Ok(adapter) = adapter.try_borrow() else {
            return;
        };
        let voice = &adapter.app.state.voice;
        let stats = voice
            .link
            .as_ref()
            .map(yapfast_core::voice::Link::read)
            .unwrap_or_default();
        show(
            &window.global::<CallLink>(),
            &stats,
            voice.participants.len() > 1,
        );
    });
    // The stream's latest report as text for a bug report, made when it is
    // asked for and not before: the same numbers the tab shows, and nothing
    // about anything else. A watched stream is reported by its own worker, so
    // a viewer has one to copy too.
    let state = Rc::downgrade(adapter);
    window.global::<CallLink>().on_diagnostics(move || {
        let Some(adapter) = state.upgrade() else {
            return Default::default();
        };
        let Ok(adapter) = adapter.try_borrow() else {
            return Default::default();
        };
        let stats = adapter
            .app
            .state
            .voice
            .link
            .as_ref()
            .map(yapfast_core::voice::Link::read)
            .unwrap_or_default();
        stats
            .stream
            .as_ref()
            .or(stats.watching.as_ref())
            .map(diagnostics)
            .unwrap_or_default()
            .into()
    });
}

/// Shows `stats`, with the privacy code only while `others` are in the call
/// to compare it with, as Discord's client does.
fn show(link: &CallLink<'_>, stats: &LinkStats, others: bool) {
    let milliseconds = |ping: u32| format!("{ping} ms");
    let last = stats.pings.last().map(|&ping| milliseconds(ping.into()));
    link.set_ping(last.unwrap_or_default().into());
    let average = average(&stats.pings).map(milliseconds);
    link.set_average(average.unwrap_or_default().into());
    let loss = stats.loss.map(|loss| format!("{loss:.1}%"));
    link.set_loss(loss.unwrap_or_default().into());
    let (graph, top) = graph(&stats.pings);
    link.set_graph(graph.into());
    link.set_graph_top(top as i32);
    let code = stats.privacy_code.as_deref().filter(|_| others);
    let groups = code.map(groups).unwrap_or_default();
    let shown = link.get_code();
    if !shown.iter().eq(groups.iter().cloned()) {
        link.set_code(slint::ModelRc::new(slint::VecModel::from(groups)));
    }
    // Whichever stream there is to talk about: the one this account shares,
    // whose worker says how it goes out, or the one it watches, whose worker
    // says how it is decoded and shown. A watched stream has no bitrate of its
    // own to report, and the rows for one are left empty rather than filled
    // with a share's numbers.
    let shown = stats.stream.as_ref().or(stats.watching.as_ref());
    let sharing = stats.stream.is_some();
    link.set_streaming(shown.is_some());
    if let Some(stream) = shown {
        link.set_encoder(
            if sharing {
                encoder(&stream.engine)
            } else {
                String::new()
            }
            .into(),
        );
        link.set_decoder(stream.decoder.as_deref().unwrap_or_default().into());
        link.set_codec(stream.codec.as_str().into());
        link.set_capture(if sharing { stream.capture.as_str() } else { "" }.into());
        link.set_settings_dropped(
            if sharing {
                stream.settings_dropped.as_str()
            } else {
                ""
            }
            .into(),
        );
        link.set_picture(
            format!("{}×{} at {} fps", stream.width, stream.height, stream.fps).into(),
        );
        link.set_sent(
            if sharing {
                rate(stream.sent_kbps)
            } else {
                String::new()
            }
            .into(),
        );
        link.set_target(
            if sharing {
                rate(stream.target_kbps)
            } else {
                String::new()
            }
            .into(),
        );
        link.set_keyframes(
            if sharing {
                stream.keyframe_requests.to_string()
            } else {
                String::new()
            }
            .into(),
        );
        link.set_resent(
            if sharing {
                stream.resent.to_string()
            } else {
                String::new()
            }
            .into(),
        );
        link.set_skipped(
            if sharing {
                stream.skipped.to_string()
            } else {
                String::new()
            }
            .into(),
        );
        link.set_sound_waiting(
            if sharing {
                format!("{} ms", stream.sound_held_ms)
            } else {
                String::new()
            }
            .into(),
        );
        link.set_sound_dropped(
            if sharing {
                format!("{} ms", stream.sound_dropped_ms)
            } else {
                String::new()
            }
            .into(),
        );
        link.set_presentation(
            stream
                .presentation
                .map_or_else(|| "unknown".to_owned(), |shown| shown.name().to_owned())
                .into(),
        );
        link.set_presented_on(adapter_name(stream.present_luid).into());
        link.set_heard(
            stream
                .sound_heard_ms
                .map_or_else(String::new, |heard| format!("{heard} ms"))
                .into(),
        );
        link.set_sound_queued(
            stream
                .sound_queued_ms
                .map_or_else(String::new, |queued| format!("{queued} ms"))
                .into(),
        );
        link.set_queue_delay(wait(stream.queue_delay_ms).into());
        link.set_buffers(buffers(stream.pool_live, stream.pool_peak).into());
        link.set_memory(stream.memory.clone().unwrap_or_default().into());
        link.set_fallback(
            stream
                .fallback
                .as_deref()
                .map(shared_only)
                .unwrap_or_default()
                .to_owned()
                .into(),
        );
    }
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
fn diagnostics(stream: &voice::StreamStats) -> String {
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
    put("preview_shown", stream.preview.into());
    put("width", stream.width.into());
    put("height", stream.height.into());
    put("fps", stream.fps.into());
    put("target_kbps", stream.target_kbps.into());
    put("sent_kbps", stream.sent_kbps.into());
    put("keyframe_requests", stream.keyframe_requests.into());
    put("resent", stream.resent.into());
    put("skipped", stream.skipped.into());
    put("sound_held_ms", stream.sound_held_ms.into());
    put("sound_dropped_ms", stream.sound_dropped_ms.into());
    for (key, value) in [
        ("queue_delay_ms", stream.queue_delay_ms.map(u64::from)),
        ("pool_live_bytes", stream.pool_live),
        ("pool_peak_bytes", stream.pool_peak),
    ] {
        put(key, value.into());
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(report))
        .unwrap_or_else(|_| String::new())
}

/// Where the stream's encoder runs, by its name: "h264_nvenc" is NVENC on
/// the GPU, "h264_amf" AMD's AMF, "hevc_vaapi (renderD128, radeonsi)"
/// VA-API through Mesa's radeonsi driver, "x264" on the CPU.
///
/// After a slash comes the device a picture is read on — "av1_nvenc/d3d11" is
/// NVENC on the GPU reading the capture's own Direct3D 11 device — which says
/// where the frames come from rather than where the encoding runs.
fn encoder(engine: &str) -> String {
    let engine = engine.split_once('/').map_or(engine, |(engine, _)| engine);
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
fn rate(kbps: u32) -> String {
    if kbps >= 1000 {
        format!("{:.1} Mbit/s", f64::from(kbps) / 1000.0)
    } else {
        format!("{kbps} kbit/s")
    }
}

fn average(pings: &[u16]) -> Option<u32> {
    let sum: u32 = pings.iter().copied().map(u32::from).sum();
    (!pings.is_empty()).then(|| sum / pings.len() as u32)
}

/// The pings as steps, the newest at the right edge, and the ping at the
/// top: the smallest 1, 2 or 5 times a power of ten above them all.
fn graph(pings: &[u16]) -> (String, u32) {
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
    let mut path = String::new();
    // Each ping holds for the step before it, so even one shows.
    for (index, &ping) in pings.iter().rev().take(SPAN).rev().enumerate() {
        let x = (first + index + 1) as f32 * step;
        let _ = if path.is_empty() {
            write!(path, "M {:.1} {:.1} H {x:.1}", x - step, y(ping))
        } else {
            write!(path, " V {:.1} H {x:.1}", y(ping))
        };
    }
    (path, top)
}

/// A privacy code in the groups of five digits it's read in.
fn groups(code: &str) -> Vec<slint::SharedString> {
    code.as_bytes()
        .chunks(5)
        .map(|group| String::from_utf8_lossy(group).as_ref().into())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_names_no_window_and_says_unknown_rather_than_zero() {
        let stats = voice::StreamStats {
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
    fn a_wait_and_a_pool_read_as_people_read_them() {
        assert_eq!(wait(Some(12)), "12 ms");
        assert_eq!(wait(None), "unknown");
        assert_eq!(
            buffers(Some(512 * 1024), Some(1024 * 1024)),
            "512 KiB, at most 1.0 MiB"
        );
        assert_eq!(buffers(None, None), "");
        assert_eq!(adapter_name(Some(0x1488a)), "adapter 0x1488a");
        assert_eq!(adapter_name(None), "");
    }

    #[test]
    fn graphs_the_pings_up_to_a_round_top() {
        assert_eq!(graph(&[]), (String::new(), 10));
        let (path, top) = graph(&[40, 80]);
        assert_eq!(top, 100);
        assert_eq!(path, "M 290.0 60.0 H 295.0 V 20.0 H 300.0");
        assert_eq!(graph(&[260]).1, 500);
        assert_eq!(graph(&[1500]).1, 2000);
    }

    #[test]
    fn says_where_the_stream_is_encoded() {
        assert_eq!(encoder("h264_nvenc"), "NVENC on the GPU");
        // The device the pictures are read on isn't where the encoding runs.
        assert_eq!(encoder("av1_nvenc/d3d11"), "NVENC on the GPU");
        assert_eq!(encoder("h264_nvenc/d3d11"), "NVENC on the GPU");
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
