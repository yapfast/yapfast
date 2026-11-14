//! `stream_worker`'s share.

use super::*;

/// Whether the card's capture paths can take `source` at all: they capture a
/// display, where a window, a camera and a test pattern are the processor's
/// or nobody's.
#[cfg(any(wgc, dup, test))]
pub(crate) fn on_the_card(source: &Source) -> bool {
    match source {
        Source::Screen | Source::Display(_) => true,
        // A window is the card's where Windows Graphics Capture can take one:
        // DXGI duplicates a display and nothing else, and the person chose the
        // window rather than the display it sits on.
        Source::Window(_) => cfg!(wgc),
        Source::Camera(_) | Source::TestPattern => false,
    }
}

pub(crate) fn capture_screen(
    source: Source,
    mut sink: Sink,
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    // Where a share's frames come from is decided here, and nowhere else: from
    // the card, as the texture Windows Graphics Capture fills or DXGI's
    // Desktop Duplication copies, when this build has either and the machine
    // can encode what it fills; from the processor's memory, as the I420
    // libwebrtc's capturer hands over, everywhere else, which is every machine
    // without NVENC. Everything that keeps a share on the processor is named in
    // the line this logs, so a stream that costs more than it should says why.
    //
    // Linux has the same shape and a different door: the DMA-BUFs the desktop's
    // compositor drew, which the window imports and draws where they lie, from
    // the screen sharing portal where one answers and from the X server's DRI3
    // export where it can. A share that leaves that path — a window with no
    // importer, a picture the encoder can't read — carries on from the
    // processor's capturer, which is where every Linux share used to start.
    #[cfg(any(wgc, dup))]
    let source = {
        let mut wanted = source;
        loop {
            match card_capture(&wanted, &sink, (max_width, max_height)) {
                Ok(chosen) => {
                    let (back, after) =
                        capture_textures(chosen, sink, (max_width, max_height), stop, report)?;
                    sink = back;
                    match after {
                        After::Ended => return Ok(()),
                        // Another screen is opened the way this one was, with
                        // the stream going on; a window or a camera is the
                        // processor's, below.
                        After::Processor(next) if on_the_card(&next) => wanted = next,
                        After::Processor(next) => break next,
                    }
                }
                Err(why) => {
                    tracing::info!(why = format!("{why:#}"), "sharing from the processor");
                    sink.diagnosis.fallback = Some("the card's capture wouldn't open".to_owned());
                    break wanted;
                }
            }
        }
    };
    #[cfg(target_os = "linux")]
    let source = match dmabuf_capture(&source, &mut sink) {
        Ok(opened) => {
            let (back, after) =
                capture_dmabufs(opened, source, sink, (max_width, max_height), stop, report)?;
            sink = back;
            match after {
                After::Ended => return Ok(()),
                // A share that left the card mid-stream stays on the processor:
                // the answer that sent it there — a tiled picture, a window that
                // can't sample one — is the same answer the next attempt would
                // get, and a share that opened and fell back over and over would
                // be a spin.
                After::Processor(next) => next,
            }
        }
        Err(why) => {
            tracing::info!(why = format!("{why:#}"), "sharing from the processor");
            sink.diagnosis.fallback = Some(format!("the processor captures this source: {why:#}"));
            source
        }
    };
    sink.capture = "the processor's capturer".to_owned();
    // Every picture this path makes is a copy: the capturer hands over memory
    // the processor wrote, and the window is shown those pixels, whichever
    // process wrote them.
    sink.diagnosis.presentation = Some(Presentation::Copy);
    let fps = sink.fps;
    let sink = Arc::new(Mutex::new(sink));
    let mut capture = Capture::start(source, &sink, max_width, max_height)?;
    // A screen, window or camera to share in its place, until it gives a
    // picture.
    let mut switching: Option<Capture> = None;
    let started = Instant::now();
    // Only the pace of the loop matters here: frames a stall swallowed are
    // skipped, not captured in a burst.
    let mut slot = 0u64;
    let mut reported = None;
    let mut sampler = pressure::Sampler::new();
    while next_frame(started, &mut slot, fps, stop).is_some() {
        run_glib();
        capture.capture_frame();
        if let Some(next) = &mut switching {
            next.capture_frame();
        }
        // The capture's status, read and let go of before anything else
        // runs: a camera writes it from its own thread, and a report isn't
        // its work.
        let (size, failure) = {
            let seen = capture.seen();
            (seen.size, seen.failed.clone())
        };
        if let Some(failure) = failure {
            bail!(if reported.is_none() && portal_picks() {
                "Screen sharing was cancelled, or this desktop has no screen sharing dialog."
                    .to_owned()
            } else {
                failure
            });
        }
        if let Some((width, height)) = size
            && size != reported
        {
            reported = size;
            report(Report::Sharing { width, height });
        }
        if let Some(next) = &switching {
            let seen = next.seen();
            if let Some(failure) = &seen.failed {
                tracing::info!(failure, "sharing the same screen or window");
                drop(seen);
                switching = None;
            } else if seen.size.is_some() {
                drop(seen);
                capture = switching.take().expect("switching");
                // Discord shows what's shared now.
                let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if sink.streaming {
                    sink.preview_due = Some(Instant::now());
                }
            }
        }
        // The machine is read before the sink is locked: the capturer's own
        // thread takes that lock to hand a frame over, and a question about the
        // adapter is not something to hold it for.
        let reading = sampler.read(Instant::now());
        let switch = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .check(reading, report)?;
        if let Some(source) = switch {
            // A switch asked for while another waits replaces it.
            switching = None;
            match Capture::start(source, &sink, max_width, max_height) {
                Ok(next) => switching = Some(next),
                Err(error) => tracing::warn!(
                    error = format!("{error:#}"),
                    "sharing the same screen or window"
                ),
            }
        }
    }
    Ok(())
}
