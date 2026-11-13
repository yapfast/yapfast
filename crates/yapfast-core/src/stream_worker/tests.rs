//! The worker's tests, reaching the modules through the
//! root's re-exports.

use super::*;

/// A worker that dies without a [`Report::Ended`] is one the UI hears about
/// anyway, with the job it had and how it went — and the warning that says
/// so is the same sentence, which is checked here without a logger to catch
/// it through: a scoped logger is hearsay under a test binary whose other
/// tests may have started the logging first.
#[cfg(unix)]
#[test]
fn a_worker_that_dies_says_how() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let died = |script: &str| {
        let worker = directory.path().join("worker");
        std::fs::write(&worker, format!("#!/bin/sh\nread job\n{script}\n")).unwrap();
        std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Only this test starts a worker.
        unsafe { std::env::set_var("YAPFAST_STREAM_WORKER", &worker) };
        let (worker, mut reports) = Worker::start(
            &Job::Glance {
                screens: vec![],
                width: 320,
                height: 180,
            },
            None,
        )
        .unwrap();
        assert_eq!(
            reports.blocking_recv(),
            Some(Report::Ended {
                error: Some("Screen sharing stopped unexpectedly.".into())
            })
        );
        // The image the popup shows, and the exit the warning carries.
        let exit = exited(&worker.child);
        drop(worker);
        (
            exit,
            Last::new(&Job::Glance {
                screens: vec![],
                width: 320,
                height: 180,
            }),
        )
    };

    // No core file: the shell's would be in the way.
    let (exit, last) = died("ulimit -c 0; kill -SEGV $$");
    assert!(exit.contains("signal=11 (SIGSEGV"), "{exit}");
    let log = last.said(&exit);
    assert!(log.contains("ended without saying why"), "{log}");
    assert!(log.contains("job=\"glance\""), "{log}");
    assert!(log.contains("size=\"up to 320x180\""), "{log}");
    assert!(log.contains("engine=\"unknown\""), "{log}");

    let (exit, last) = died("exit 3");
    assert!(exit.contains("code=3"), "{exit}");
    assert!(last.said(&exit).contains("exit=\"code=3\""), "{exit}");
}

#[cfg(unix)]
#[test]
fn says_which_signal_ended_it() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(how(ExitStatus::from_raw(11)), "signal=11 (SIGSEGV)");
    assert_eq!(
        how(ExitStatus::from_raw(6 | 0x80)),
        "signal=6 (SIGABRT, core dumped)"
    );
    assert_eq!(how(ExitStatus::from_raw(1 << 8)), "code=1");
}

#[test]
fn the_worker_is_the_running_yapfast() {
    // Here that's the test binary, which lists its tests and exits.
    let program = running_yapfast().unwrap();
    let status = std::process::Command::new(program)
        .args(["--list", "--format", "terse"])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn a_full_queue_drops_a_status_and_keeps_the_end() {
    let (reports, mut ui) = mpsc::channel(1);
    let first = Report::Sending(StreamStats::default());
    assert!(matches!(deliver(&reports, first), Delivery::Taken));
    let status = deliver(&reports, Report::Sending(StreamStats::default()));
    assert!(
        matches!(status, Delivery::Taken),
        "a status is dropped rather than wait: a later one says the same"
    );
    let ended = deliver(&reports, Report::Ended { error: None });
    assert!(
        matches!(ended, Delivery::Waiting(Report::Ended { .. })),
        "the end waits for room, so a full queue can't hide it"
    );
    assert!(matches!(ui.try_recv(), Ok(Report::Sending(_))), "the first");
    assert!(ui.try_recv().is_err(), "and only the first");
    drop(ui);
    assert!(
        matches!(deliver(&reports, Report::Streaming), Delivery::Gone),
        "and nothing is left to report to"
    );
}

/// A window is the card's only where Windows Graphics Capture can take
/// one: DXGI duplicates a display and nothing else, and a person who chose
/// a window never gets the desktop it happens to sit on.
#[test]
fn a_window_is_the_cards_only_where_capture_takes_one() {
    assert_eq!(on_the_card(&Source::Window(0x1234)), cfg!(wgc));
    assert!(on_the_card(&Source::Screen));
    assert!(on_the_card(&Source::Display(0)), "the primary display");
    assert!(!on_the_card(&Source::Camera("cam".into())));
    assert!(!on_the_card(&Source::TestPattern));
}

/// What the card asks Windows Graphics Capture for is what the person chose:
/// a window's own handle goes to `CreateForWindow`, a display's to
/// `CreateForMonitor`, and there is no source that is captured as the other
/// kind. Nothing here needs a screen or a window, so the rule is checked on
/// every platform this crate builds for, including the one where the rung
/// isn't carried at all.
#[cfg(any(wgc, test))]
#[test]
fn the_card_captures_a_window_as_itself_and_never_as_a_display() {
    // The handle `Source::Window` carries is the one the window is captured
    // by, unchanged: nothing reinterprets it as a monitor's.
    assert_eq!(
        card_target(&Source::Window(0x1234)).ok(),
        Some(CardTarget::Window(0x1234))
    );
    // A screen and a display are displays, and the primary display is the
    // one with no handle of its own.
    assert_eq!(
        card_target(&Source::Screen).ok(),
        Some(CardTarget::Display(None))
    );
    assert_eq!(
        card_target(&Source::Display(3)).ok(),
        Some(CardTarget::Display(Some(3)))
    );
    // Whichever source is asked about, the two kinds of target are never
    // each other's.
    for source in [
        Source::Window(7),
        Source::Screen,
        Source::Display(1),
        Source::Camera("cam".into()),
        Source::TestPattern,
    ] {
        let kind = card_target(&source).ok();
        assert_eq!(
            matches!(kind, Some(CardTarget::Window(_))),
            matches!(source, Source::Window(_)),
            "{source:?} is captured as {kind:?}"
        );
    }
    // A camera and a test pattern are no screen, and are refused by name
    // rather than captured as the desktop behind them.
    for source in [Source::Camera("cam".into()), Source::TestPattern] {
        let refused = card_target(&source).expect_err("not a screen");
        assert!(!format!("{refused:#}").is_empty(), "{source:?}");
    }
}

/// The fallback keeps the window it was given: what the card's paths can't
/// take is the processor's, and the processor's capturer is asked for a
/// window of its own rather than for a screen — the same window, by the same
/// handle, and never the display it sits on.
#[test]
fn a_window_the_card_cannot_take_goes_on_as_that_window() {
    for source in [
        Source::Window(0x1234),
        Source::Screen,
        Source::Display(2),
        Source::Camera("cam".into()),
        Source::TestPattern,
    ] {
        assert_eq!(
            capturer_takes_a_window(&source),
            matches!(source, Source::Window(_)),
            "{source:?}"
        );
    }
    // A build with Windows Graphics Capture keeps a window on the card, so a
    // window reaches the processor only where the card has no way to take
    // one — and there it arrives as the window it was.
    assert_eq!(on_the_card(&Source::Window(0x1234)), cfg!(wgc));
    // Where the system's own dialog picks, there is no kind to ask for: it
    // picks a screen or a window, and what it picks is what the fallback
    // shares. Everywhere else the kind carries the choice.
    if portal_picks() {
        // A window is captured as a window: the capturer is told its id, and
        // the answer is not the whole screen.
        assert_ne!(
            capturer_kind(&Source::Window(0x1234)),
            capturer_kind(&Source::Screen)
        );
    } else {
        assert_eq!(
            capturer_kind(&Source::Window(0x1234)),
            livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Window
        );
        assert_eq!(
            capturer_kind(&Source::Display(2)),
            livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen
        );
    }
}

/// A duplication asked for a window says so rather than capturing the
/// display behind it: the person chose the window.
#[cfg(dup)]
#[test]
fn a_duplication_never_takes_a_window_for_a_display() {
    let why = duplication_target(&Source::Window(0x1234)).unwrap_err();
    assert!(format!("{why:#}").contains("window"), "{why:#}");
}

#[test]
fn fits_frames_with_even_sizes() {
    assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
    assert_eq!(fit(3440, 1440, 1280, 720), (1280, 534));
    assert_eq!(fit(801, 601, 1280, 720), (800, 600), "never larger");
}

/// The box a stream fits in turns with the picture, so a screen on its side
/// keeps its own shape at the resolution asked for instead of being shrunk
/// to a landscape frame's height.
#[test]
fn a_portrait_screen_fits_the_box_stood_up() {
    // 1080x1920 on a 1080p share: every pixel of it, not 606x1080.
    assert_eq!(fit(1080, 1920, SHARE_WIDTH, SHARE_HEIGHT), (1080, 1920));
    assert_eq!(fit(1080, 1920, 1280, 720), (720, 1280));
    assert_eq!(fit(1080, 1920, 3840, 2160), (1080, 1920), "never larger");
    // A landscape screen is fitted exactly as it was.
    assert_eq!(fit(1920, 1080, SHARE_WIDTH, SHARE_HEIGHT), (1920, 1080));
    assert_eq!(fit(3440, 1440, SHARE_WIDTH, SHARE_HEIGHT), (1920, 802));
    // A square box fits either way round.
    assert_eq!(fit(1080, 1920, 1920, 1920), (1080, 1920));
}

/// A preview drawn on the card is the stream's own picture, up to the 1920
/// its longest edge is allowed; the shape of the screen decides which edge
/// that is.
#[cfg(any(wgc, dup))]
#[test]
fn a_preview_on_the_card_keeps_the_streams_pixels() {
    assert_eq!(preview_size(3440, 1440, false), (1920, 802));
    // A machine short of memory takes the smaller box, and never upscales.
    assert_eq!(preview_size(3440, 1440, true), (640, 266));
    assert_eq!(preview_size(1080, 1920, false), (1080, 1920));
    assert_eq!(preview_size(1080, 1920, true), (360, 640));
    assert_eq!(preview_size(1280, 720, false), (1280, 720), "never larger");
    assert_eq!(
        preview_size(320, 180, true),
        (320, 180),
        "and never larger here"
    );
}

/// Only a worker that decodes pictures takes a share of the machine's
/// decoder threads: a watched stream and a played video do, where a shared
/// screen only encodes and a glance only takes a picture.
#[test]
fn only_a_worker_that_decodes_takes_a_share() {
    let frames = PathBuf::from("frames");
    let server = StreamServer {
        stream_key: "key".into(),
        user_id: "1".into(),
        session_id: "2".into(),
        rtc_server_id: "3".into(),
        token: "token".into(),
        endpoint: "wss://discord.test".into(),
    };
    let watch = Job::Watch {
        server,
        frames: frames.clone(),
        width: WATCH_WIDTH,
        height: WATCH_HEIGHT,
        speakers: None,
        deafened: false,
        volume: 1.0,
    };
    assert!(watch.decodes(), "a watched stream decodes");
    let played = Job::Play {
        file: PathBuf::from("video"),
        size: None,
        frames: frames.clone(),
        width: WATCH_WIDTH,
        height: WATCH_HEIGHT,
        speakers: None,
        volume: 1.0,
    };
    assert!(played.decodes(), "a played video decodes");
    let shared = Job::Share {
        source: Source::Screen,
        frames: frames.clone(),
        width: SHARE_WIDTH,
        height: SHARE_HEIGHT,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    assert!(!shared.decodes(), "a shared screen only encodes");
    let glance = Job::Glance {
        screens: vec![1],
        width: 320,
        height: 180,
    };
    assert!(!glance.decodes(), "a glance only takes a picture");
}

/// A picture a newer one replaces before the screen could draw it isn't
/// converted, scaled or shown: the watch drains what queued and shows the
/// newest of it. A watch that keeps up shows every frame, and a watch
/// that hasn't shown anything yet always shows the first.
#[test]
fn a_picture_a_newer_one_replaces_isnt_shown() {
    let millis = Duration::from_millis;
    let shown = Instant::now();
    // Nothing shown yet: the first picture reaches the screen whatever
    // came in behind it, or a stream that outruns its decoder would show
    // nothing at all.
    assert_eq!(worth_showing(true, None, shown, false), Worth::Show);
    assert_eq!(worth_showing(true, None, shown, true), Worth::Show);
    // Pictures decoded back to back — what queued while the decoder was
    // slow — are pictures no screen draws: the last of them is shown.
    assert_eq!(
        worth_showing(true, Some(shown), shown + millis(1), true),
        Worth::Replaced
    );
    assert_eq!(
        worth_showing(true, Some(shown), shown + millis(7), true),
        Worth::Replaced
    );
    assert_eq!(
        worth_showing(true, Some(shown), shown + millis(8), true),
        Worth::Show
    );
    // A frame period later — 30 frames a second — it's the newest
    // picture there could be, so it's shown.
    assert_eq!(
        worth_showing(true, Some(shown), shown + millis(33), true),
        Worth::Show
    );
    // With nothing newer in hand the decoder is keeping up: every frame
    // is shown, however fast they come.
    assert_eq!(worth_showing(true, Some(shown), shown, false), Worth::Show);
}

/// A hidden or minimised window shows no picture, so none is converted or
/// written while it's hidden, however long the decoder has been running;
/// the frames themselves are decoded all the same. When the window comes
/// back, the picture decoded next is the one shown, at once: the last one
/// that reached the screen was long enough ago to be replaced by any.
#[test]
fn a_hidden_window_shows_no_picture() {
    let millis = Duration::from_millis;
    let shown = Instant::now();
    // Hidden, nothing at all is made of a picture: not the first, not one
    // with nothing behind it, and not the newest of a queue either.
    assert_eq!(worth_showing(false, None, shown, false), Worth::Unseen);
    assert_eq!(
        worth_showing(false, Some(shown), shown + millis(33), false),
        Worth::Unseen,
    );
    assert_eq!(
        worth_showing(false, Some(shown), shown + millis(33), true),
        Worth::Unseen,
    );
    // The frame is decoded before this is asked, in the watch's loop: what
    // a hidden window skips is the conversion and the write, not the
    // decode. So the pictures after the window comes back are made from a
    // chain the decoder kept, and the next one decoded is shown without
    // waiting for anything: the screen drew its last a minute ago.
    assert_eq!(
        worth_showing(true, Some(shown), shown + Duration::from_secs(60), true),
        Worth::Show,
    );
}

/// What a watched stream is written at: a picture larger than the tile
/// showing it is scaled down to the tile, and one smaller keeps its own
/// size, so a stream shown in a small tile costs a small frame and a
/// small stream no more than it is.
#[test]
fn a_stream_is_written_at_the_size_its_tile_shows_it() {
    let frames = Frames::create(32, 18).unwrap();
    let mut writer = Writer::open(&frames.path().unwrap()).unwrap();
    frames.unlink();
    let mut scratch = vec![];
    let picture = |width, height| {
        let mut picture = I420Buffer::new(width, height);
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
        picture
    };
    // A 64 by 36 picture in a 32 by 18 tile is scaled down to the tile.
    let large = picture(64, 36);
    let shown = planes(&large);
    let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18), 0);
    assert_eq!(size, (32, 18));
    assert_eq!(
        frames.take(|width, height, pixels| (width, height, pixels.len())),
        Some((32, 18, 32 * 18 * 4)),
        "the tile's RGBA, row for row"
    );
    // A 16 by 10 picture in that tile keeps its own size: nothing scales
    // a stream up to the tile it shows in.
    let small = picture(16, 10);
    let shown = planes(&small);
    let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18), 0);
    assert_eq!(size, (16, 10));
    assert_eq!(
        frames.take(|width, height, pixels| (width, height, pixels.len())),
        Some((16, 10, 16 * 10 * 4))
    );
}

/// A tile's size says what size a stream shows at: none while no tile
/// shows it, and never more than a stream is shown at, however large the
/// window is, so a 4K tile doesn't grow the frames for a 1080p stream.
#[test]
fn a_tile_says_how_large_a_stream_shows() {
    assert_eq!(tile_size((0, 0)), None, "no tile shows it");
    assert_eq!(tile_size((0, 270)), None, "half a size says nothing");
    assert_eq!(tile_size((480, 270)), Some((480, 270)));
    assert_eq!(
        tile_size((3840, 2160)),
        Some((WATCH_WIDTH, WATCH_HEIGHT)),
        "no stream is shown larger than it is"
    );
}

/// Frames at 30 fps are due every 33⅓ ms: on time, a loop waits for the
/// frame in line; behind, it makes the latest frame due and skips the rest.
#[test]
fn skips_the_frames_a_stall_swallowed() {
    let started = Instant::now();
    let millis = Duration::from_millis;
    // The first frame goes at once, so a stream shows without the wait.
    assert_eq!(pace(started, 0, 30, started), (0, Some(Duration::ZERO)));
    // On time, it waits out the rest of the frame's interval.
    assert_eq!(
        pace(started, 2, 30, started + millis(40)),
        (2, Some(Duration::from_secs(2) / 30 - millis(40)))
    );
    // A little late still makes the frame that was due.
    assert_eq!(pace(started, 2, 30, started + millis(70)), (2, None));
    // A quarter of a second late makes frame 7, not frames 1 to 7.
    assert_eq!(pace(started, 1, 30, started + millis(250)), (7, None));
    // After a skipped burst the cadence carries on from the clock.
    assert_eq!(
        pace(started, 8, 30, started + millis(250)),
        (8, Some(Duration::from_secs(8) / 30 - millis(250)))
    );
}

/// 60 fps is a frame every 16⅔ ms, which no whole millisecond marks.
#[test]
fn paces_a_frame_rate_that_no_millisecond_marks() {
    let started = Instant::now();
    let millis = Duration::from_millis;
    assert_eq!(pace(started, 0, 60, started + millis(16)), (0, None));
    assert_eq!(
        pace(started, 1, 60, started + millis(16)),
        (1, Some(Duration::from_secs(1) / 60 - millis(16)))
    );
    // Frame 4 came due at 66⅔ ms; frames 1 to 3 were missed.
    assert_eq!(pace(started, 1, 60, started + millis(67)), (4, None));
}

/// Whatever the clock says, a paced loop never makes a frame early, never
/// goes back to one it skipped, and hands out the whole cadence in order.
#[test]
fn frames_follow_the_clock() {
    let started = Instant::now();
    for fps in [1u32, 30, 60] {
        let mut slot = 0u64;
        for millis in (0..1_100u64).step_by(7) {
            let now = started + Duration::from_millis(millis);
            let (due, wait) = pace(started, slot, fps, now);
            let deadline = started + Duration::from_secs(due) / fps;
            assert!(due >= slot, "{fps} fps went back to {due} at {millis} ms");
            match wait {
                // On time, it waits for the frame in line to be due.
                Some(wait) => {
                    assert_eq!(due, slot);
                    assert_eq!(now + wait, deadline, "{fps} fps at {millis} ms");
                }
                // Behind, it makes the latest frame due, and not before.
                None => {
                    assert!(deadline <= now, "{fps} fps early at {millis} ms");
                    // Deadlines are whole nanoseconds, so rounding can leave
                    // a frame due at this very instant for the next call.
                    assert!(
                        started + Duration::from_secs(due + 2) / fps > now,
                        "{fps} fps left two frames due at {millis} ms"
                    );
                }
            }
            slot = due + 1;
        }
    }
}

#[test]
fn keeps_the_edges_of_a_frame_rate() {
    let started = Instant::now();
    // A job's rate is clamped to 1–60; without one there's no cadence.
    assert_eq!(pace(started, 3, 0, started), (3, None));
    // One frame a second starts at once and waits its second after.
    assert_eq!(pace(started, 0, 1, started), (0, Some(Duration::ZERO)));
    assert_eq!(
        pace(started, 1, 1, started + Duration::from_millis(200)),
        (1, Some(Duration::from_millis(800)))
    );
}

#[test]
fn stops_a_paced_loop() {
    // Begun two seconds ago, so every frame it should have made is due and
    // the loop has no waiting to do.
    let started = Instant::now() - Duration::from_secs(2);
    let stop = AtomicBool::new(true);
    let mut slot = 0u64;
    assert_eq!(next_frame(started, &mut slot, 30, &stop), None);
    assert_eq!(slot, 0, "a stopped loop keeps its place");
    stop.store(false, Ordering::Relaxed);
    let frame = next_frame(started, &mut slot, 30, &stop).expect("not stopped");
    assert!(frame >= 60, "the 2 seconds it missed are skipped");
    assert_eq!(slot, frame + 1, "and the loop goes on from there");
}

/// A share is declared what its picture needs: bits per pixel per frame,
/// so the same quality at every size, under the safe ceiling.
#[test]
fn aims_for_what_the_picture_needs() {
    // 0.07 bits a pixel a frame, under the 9 Mbit/s a share declares by
    // default: a 1080p60 share asks for less than that and gets it.
    assert_eq!(kbps(1920, 1080, 60), 8_709);
    assert_eq!(kbps(1280, 720, 30), 3_500, "the floor a screen is given");
    // Above that the declaration is cut to the ceiling, which is what
    // Settings' experimental switch raises.
    assert_eq!(kbps(3440, 1440, 60), 9_000, "cut to the safe cap");
    assert_eq!(kbps(3840, 2160, 60), 9_000);
    assert_eq!(kbps(7680, 4320, 60), 9_000, "whatever the size");
    // A frame rate a person picks takes its share of the bits with it,
    // under the cap either way.
    assert_eq!(kbps(3840, 2160, 30), 9_000);
    assert_eq!(kbps(1920, 1080, 30), 4_354);
    // A screen on its side is the same picture, so it is worth the same.
    assert_eq!(kbps(1080, 1920, 30), kbps(1920, 1080, 30));
}

/// With the GPU, AV1 and H265 before H264 where they open, and an
/// encoder for the first; without it, H264 alone, on x264.
#[test]
fn offers_the_gpus_codecs_first() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: true,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let (offers, encoder) = sink.offers(320, 180, None, false).unwrap();
    eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
    assert_eq!(offers.last(), Some(&Codec::H264));
    let mut order = [Codec::Av1, Codec::H265, Codec::H264].iter();
    assert!(
        offers.iter().all(|codec| order.any(|next| next == codec)),
        "in order"
    );
    assert_eq!(encoder.codec(), offers[0]);
    sink.hardware = false;
    let (offers, encoder) = sink.offers(320, 180, None, false).unwrap();
    assert_eq!((offers, encoder.engine()), (vec![Codec::H264], "x264"));
}

/// The codecs to offer Discord, most wanted first, with the settings' own
/// codec in front where this machine encodes it.
#[test]
fn the_settings_codec_is_offered_first() {
    let card = vec![Codec::Av1, Codec::H265, Codec::H264];
    assert_eq!(
        preferred(card.clone(), StreamCodec::Automatic),
        card,
        "automatic is the order the card's codecs are in"
    );
    assert_eq!(
        preferred(card.clone(), StreamCodec::H264),
        vec![Codec::H264, Codec::Av1, Codec::H265]
    );
    assert_eq!(
        preferred(card, StreamCodec::H265),
        vec![Codec::H265, Codec::Av1, Codec::H264]
    );
    // A codec this machine has no encoder for isn't offered at all, so
    // there is nothing to put first: the order is what it was.
    assert_eq!(
        preferred(vec![Codec::H264], StreamCodec::Av1),
        vec![Codec::H264]
    );
}

/// A share whose frames come from the card encodes with the card's encoder,
/// on the device the capture fills its textures on: what reads a texture is
/// NVENC on one D3D11 device, which is another encoder of FFmpeg's, reached
/// another way, and the device it reads on is the capture's own.
#[cfg(wgc)]
#[test]
fn a_share_from_the_card_encodes_on_the_card() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: true,
        footage: Footage::OnTheCard,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    // The device the stream's own session opens on is the capture's, which
    // a test can only ask for by capturing something. A machine with no
    // screen has no card's device to check, and says so.
    let Ok(capture) = wgc::Capture::open(wgc::Monitor::PRIMARY) else {
        eprintln!("no screen here, so there is no capture to read from");
        return;
    };
    // A machine without NVENC on that device has nothing to check here,
    // and says so the way a stream on it would: this is the error that
    // keeps that machine sharing from the processor.
    let Ok((offers, encoder)) = sink.offers(320, 180, Some(capture.device()), capture.hdr()) else {
        eprintln!("no NVENC here, so there is no card's encoder to check");
        return;
    };
    eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
    let Encoding::Texture(encoder) = encoder else {
        panic!("{} doesn't read textures", encoder.engine());
    };
    assert!(encoder.engine().ends_with("/d3d11"), "{}", encoder.engine());
    assert_eq!(encoder.size(), (320, 180));
    assert_eq!(
        encoder.device(),
        capture.device(),
        "the session reads on the device the capture fills its textures on"
    );
}

/// Everything that keeps a share on the processor says so, and names which
/// one it was: a window and a camera aren't screens, a test pattern isn't
/// captured at all, and a stream set to encode on the processor doesn't look
/// at the card.
#[cfg(any(wgc, dup))]
#[test]
fn a_share_the_card_cannot_capture_says_why() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: true,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let bounds = (SHARE_WIDTH, SHARE_HEIGHT);
    for (source, why) in [
        (Source::Window(7), "a window"),
        (Source::Camera("none".into()), "a camera"),
        (Source::TestPattern, "a test pattern"),
    ] {
        let Err(error) = card_capture(&source, &sink, bounds) else {
            panic!("{source:?} isn't captured from the card");
        };
        let said = format!("{error:#}");
        assert!(said.contains(why), "{said}");
    }
    sink.hardware = false;
    let Err(error) = card_capture(&Source::Display(0), &sink, bounds) else {
        panic!("a stream set to the processor still captured from the card");
    };
    assert!(
        format!("{error:#}").contains("processor"),
        "the setting itself keeps it there"
    );
}

/// The one log line says which rung ran and why: the setting's choice where
/// that decided, the platform's answer under Auto, and every refusal before
/// the rung that opened, in the order they were tried. A bordered Graphics
/// Capture that was let go for Duplication therefore says so in the same
/// line, in the shim's own words.
#[cfg(any(wgc, dup))]
#[test]
fn the_chosen_rung_says_why_it_is_the_one() {
    use crate::capture::{Available, Platform, Preference, Rung};

    let both = Available {
        graphics: true,
        duplication: true,
    };

    // The setting's own answer, on a Windows where the other way would be
    // first under Auto.
    assert_eq!(
        chosen_why(
            Preference::GraphicsCapture,
            Platform::new(19045),
            both,
            Rung::Graphics,
            ""
        ),
        "the setting asks for Windows Graphics Capture"
    );
    assert_eq!(
        chosen_why(
            Preference::Duplication,
            Platform::new(22621),
            both,
            Rung::Duplication,
            ""
        ),
        "the setting asks for Desktop Duplication"
    );
    // Auto reads the platform: Windows 10's Graphics Capture can't take
    // its border off, so Duplication comes first and the line says so; on
    // Windows 11, Graphics Capture can and does.
    let windows_10 = chosen_why(
        Preference::Auto,
        Platform::new(19045),
        both,
        Rung::Duplication,
        "",
    );
    assert!(
        windows_10.contains("border") && windows_10.contains("Duplication"),
        "{windows_10}"
    );
    let windows_11 = chosen_why(
        Preference::Auto,
        Platform::new(22621),
        both,
        Rung::Graphics,
        "",
    );
    assert!(
        windows_11.contains("border") && windows_11.contains("Graphics"),
        "{windows_11}"
    );
    // A build with no Graphics Capture rung says that, rather than blaming
    // the border on a capture it doesn't carry.
    let duplication_only = chosen_why(
        Preference::Auto,
        Platform::new(22621),
        Available {
            graphics: false,
            duplication: true,
        },
        Rung::Duplication,
        "",
    );
    assert!(
        duplication_only.contains("no Windows Graphics Capture"),
        "{duplication_only}"
    );

    // Each refusal carries its rung's name and what it said, in the order
    // tried, and the rung that opened is named after them.
    let mut refused = String::new();
    refuse(
        &mut refused,
        Rung::Graphics,
        "the border is on: nothing took",
    );
    let said = chosen_why(
        Preference::Auto,
        Platform::new(22621),
        both,
        Rung::Duplication,
        &refused,
    );
    let graphics = said
        .find("Windows Graphics Capture: the border is on")
        .unwrap_or_else(|| panic!("the rung that refused is named with its reason: {said}"));
    let duplication = said
        .find("Desktop Duplication came next")
        .unwrap_or_else(|| panic!("the rung that opened is named: {said}"));
    assert!(
        graphics < duplication,
        "in the order they were tried: {said}"
    );

    let mut both = String::new();
    refuse(&mut both, Rung::Graphics, "no");
    refuse(&mut both, Rung::Duplication, "no either");
    assert_eq!(
        both,
        "Windows Graphics Capture: no; Desktop Duplication: no either"
    );
}

/// A duplication takes the same displays the first rung does — the primary
/// one when nothing was picked, and the one the shell listed otherwise — and
/// refuses everything that isn't a display by its own name, rather than
/// duplicating the primary display and calling it the chosen one.
#[cfg(dup)]
#[test]
fn a_duplication_names_the_display_it_captures_and_what_it_cannot() {
    for (source, why) in [
        (Source::Window(7), "window"),
        (Source::Camera("none".into()), "camera"),
        (Source::TestPattern, "drawn"),
    ] {
        let Err(error) = duplication_target(&source) else {
            panic!("{source:?} isn't a display to duplicate");
        };
        let said = format!("{error:#}");
        assert!(said.contains(why), "{said}");
    }
    // A screen is named by its place in the system's enumeration, so a
    // place no display is listed at names nothing.
    let Err(error) = duplication_target(&Source::Display(u64::MAX)) else {
        panic!("a place no display is listed at named one");
    };
    assert!(
        format!("{error:#}").contains("no display is listed"),
        "{error:#}"
    );

    // And the place that does have one is turned into that display's own
    // handle, which is what DXGI is asked for: screen 1 on this machine is
    // not `HMONITOR 0x1`, which is what a share of it used to look for.
    let listed = sources(livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen);
    for source in listed {
        let Some(monitor) = monitor_of(source.id()).ok() else {
            panic!("a listed screen is a display: {:?}", source.id());
        };
        assert_ne!(monitor, source.id(), "a place is not a handle");
        if let Err(error) = target_of_monitor(monitor) {
            panic!("a listed display is one DXGI enumerates: {error:#}");
        }
    }
}

/// Whether a screen's texture and an NVENC session can be on one device, as
/// the share from the card needs them to be. It is a fact about the machine
/// and not about this code, so it is asked here and printed: a session reads
/// a texture only on the device it was opened on, and this one is opened on
/// the capture's.
///
/// The session is opened at the size the stream sends rather than at the
/// screen's, since that is the size the stream's own session opens at, and
/// the point is the device rather than the size.
#[cfg(wgc)]
#[test]
#[ignore = "needs a screen and an NVIDIA card"]
fn the_capture_and_the_encoder_are_on_one_device() {
    let capture = wgc::Capture::open(wgc::Monitor::PRIMARY).expect("a capture of the screen");
    eprintln!("{}", capture.describe());
    let (width, height) = capture.content_size();
    eprintln!("the screen is {width}x{height}");
    let (sent_width, sent_height) = fit(width, height, SHARE_WIDTH, SHARE_HEIGHT);
    eprintln!("a stream of it sends {sent_width}x{sent_height}");
    let Ok(encoder) = d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        30,
        kbps(sent_width, sent_height, 30),
        d3d11::Surfaces::Ring,
        d3d11::Light::of(capture.hdr()),
    ) else {
        eprintln!(
            "and this machine has no H264 NVENC session on the device the capture fills its \
                 textures on"
        );
        return;
    };
    eprintln!(
        "capture fills textures on {:p}, the session reads them on {:p}",
        capture.device(),
        encoder.device()
    );
    assert_eq!(
        capture.device(),
        encoder.device(),
        "a share from the card needs the capture's device to be the session's: NVENC reads a \
             texture only on the one it opened on"
    );
    assert_eq!(encoder.size(), (sent_width, sent_height));
}

/// The fake Discord's fixture: Alice, in the guild, in the Lounge. The ids
/// are `testing/fake_discord.ts`'s, which is where a stream's key and the
/// account watching it come from.
#[cfg(wgc)]
const GUILD: &str = "1400000000000000100";
#[cfg(wgc)]
const LOUNGE: &str = "1400000000000000103";
#[cfg(wgc)]
const ALICE: &str = "1400000000000000001";

/// What a real share costs the processor a frame, measured on the machine
/// that runs it: the primary display captured, encoded at 1080p30 and sent
/// to the fake Discord, once from the card and once through libwebrtc's
/// capturer and x264. The process's own kernel and user time is counted
/// across each leg and divided by the frames the fake says arrived, so
/// waiting on the compositor costs nothing and nothing that didn't arrive is
/// counted.
///
/// It prints the numbers rather than asserting them: what a card or a
/// processor costs is the machine's answer, and this is here to be run on
/// the machine whose answer it is.
#[cfg(wgc)]
#[test]
#[ignore = "needs a screen, an NVIDIA card, bun and a python3 with tkinter"]
fn a_real_share_costs_what_it_costs_a_frame() {
    // The worker's own log line says which path a share took and why, which
    // is the half of this a number can't say.
    crate::logging::init_to_stderr();
    let fake = Fake::start();
    let _drawing = Animated::start();
    // The window takes a moment to appear, and a leg measured before it does
    // sees a screen that isn't drawing.
    std::thread::sleep(Duration::from_millis(1500));
    for (hardware, path) in [(true, "the card"), (false, "the processor")] {
        let (cpu, frames, stats) = share_for(&fake, hardware, Duration::from_secs(20));
        assert!(
            frames >= 100,
            "{path} sent {frames} frames while the screen was measured: the screen wasn't \
                 drawing, so there is no number for this path here"
        );
        // The card's path is the one whose encoder reads the capture's
        // texture, which it says by naming the D3D11 backend; anything else
        // is the processor's path, and isn't the number this test is for.
        if hardware {
            assert!(
                stats
                    .as_ref()
                    .is_some_and(|stats| stats.engine.ends_with("/d3d11")),
                "the card's path didn't engage: the stream says {:?}, and the log above says \
                     why",
                stats.map(|stats| stats.engine)
            );
        }
        eprintln!(
            "{path}: {:.3} ms of processor a frame over {frames} frames{}",
            cpu / frames.max(1) as f64,
            stats.map_or(String::new(), |stats| format!(
                ", by {} ({} {}x{} at {} fps, {} kbit/s sent)",
                stats.engine, stats.codec, stats.width, stats.height, stats.fps, stats.sent_kbps
            ))
        );
    }
}

/// Runs one share of the primary display against `fake` for `measure`, and
/// answers the processor time it took in milliseconds, the frames the fake
/// says arrived in that time, and the last statistics the worker reported.
#[cfg(wgc)]
fn share_for(fake: &Fake, hardware: bool, measure: Duration) -> (f64, u64, Option<StreamStats>) {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (input, mut feed) = std::io::pipe().unwrap();
    let (reports, heard) = channel::channel();
    let serving = std::thread::spawn(move || {
        serve(Box::new(BufReader::new(input)), &mut |report| {
            let _ = reports.send(report);
        });
    });
    let job = Job::Share {
        source: Source::Screen,
        frames: frames.path().unwrap(),
        width: SHARE_WIDTH,
        height: SHARE_HEIGHT,
        fps: 30,
        effort: Effort::Light,
        hardware,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
    assert_eq!(
        heard
            .recv_timeout(Duration::from_secs(10))
            .expect("the worker's first report"),
        Report::Ready
    );
    // The UI shows the share's preview from the moment sharing starts, which
    // is the card's own session for a tile, so a measurement has it on
    // rather than measuring a share nobody is looking at.
    writeln!(
        feed,
        "{}",
        serde_json::to_string(&Control::Preview(true)).unwrap()
    )
    .unwrap();
    let key = format!("guild:{GUILD}:{LOUNGE}:{ALICE}");
    let server = StreamServer {
        stream_key: key.clone(),
        user_id: ALICE.to_owned(),
        session_id: "measure".to_owned(),
        rtc_server_id: "fake-rtc".to_owned(),
        token: format!("stream-token-{key}"),
        endpoint: fake.voice(),
    };
    writeln!(
        feed,
        "{}",
        serde_json::to_string(&Control::Send(server)).unwrap()
    )
    .unwrap();
    // Nothing is encoded before the stream is live, which the worker reports
    // once the sender has connected: only the measuring waits for it.
    loop {
        match heard.recv_timeout(Duration::from_secs(30)) {
            Ok(Report::Streaming) => break,
            Ok(Report::Ended { error }) => {
                panic!("the share ended before it went live: {error:?}")
            }
            Ok(_) => {}
            Err(error) => panic!("the stream never went live: {error}"),
        }
    }
    let started = (cpu_millis(), fake.frames(&key));
    std::thread::sleep(measure);
    let (cpu, sent) = (cpu_millis(), fake.frames(&key));
    // What the worker says about the leg it just ran: the engine that made
    // the picture is what says which path engaged.
    let mut stats = None;
    while let Ok(report) = heard.try_recv() {
        if let Report::Sending(sending) = report {
            stats = Some(sending);
        }
    }
    writeln!(feed, "stop").unwrap();
    serving.join().unwrap();
    (cpu - started.0, sent.saturating_sub(started.1), stats)
}

/// The fake Discord, on a port of its own, with the counters a measurement
/// reads: the only Discord a measurement here may talk to.
#[cfg(wgc)]
struct Fake {
    child: std::process::Child,
    address: String,
}

#[cfg(wgc)]
impl Fake {
    fn start() -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("a free port")
            .local_addr()
            .unwrap()
            .port();
        let log = std::fs::File::create(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/fake-discord-measure.log"
        ))
        .unwrap();
        let child = std::process::Command::new("bun")
            .arg("testing/fake_discord.ts")
            .args(["--port", &port.to_string()])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .expect("bun, which runs the fake Discord");
        let fake = Self {
            child,
            address: format!("http://127.0.0.1:{port}"),
        };
        for _ in 0..100 {
            if get(&format!("{}/_control/health", fake.address)).is_some() {
                return fake;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the fake Discord didn't answer; see target/fake-discord-measure.log");
    }

    /// Where a stream's connection reaches the fake's voice server.
    fn voice(&self) -> String {
        format!("{}/voice", self.address.replacen("http", "ws", 1))
    }

    /// How many frames reached the fake on `key` since it started.
    fn frames(&self, key: &str) -> u64 {
        let answer =
            get(&format!("{}/_control/video", self.address)).expect("the fake's video counters");
        let body = answer
            .split_once("\r\n\r\n")
            .map_or(answer.as_str(), |(_, body)| body);
        let counters: serde_json::Value =
            serde_json::from_str(body).expect("what the fake received");
        counters["received"]
            .as_array()
            .into_iter()
            .flatten()
            // The fake keeps a record for every connection, and a leg's
            // own is the last one on its key: an earlier leg's is still
            // here.
            .rev()
            .find(|received| received["stream"].as_str() == Some(key))
            .map_or(0, |received| received["frames"].as_u64().unwrap_or(0))
    }
}

#[cfg(wgc)]
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A GET of an `http://` URL, answered whole: what the fake's control
/// endpoints need, without an HTTP client in a test's dependencies.
#[cfg(wgc)]
fn get(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_once('/')?;
    let mut stream = std::net::TcpStream::connect(host).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut answer = String::new();
    std::io::Read::read_to_string(&mut stream, &mut answer).ok()?;
    // A connection refused while the fake starts is an answer without a
    // status, which isn't one.
    answer.starts_with("HTTP/1.1 200").then_some(answer)
}

/// A window that keeps drawing while a leg runs: Windows Graphics Capture
/// and libwebrtc's capturer both hand over a frame only when the screen
/// changed, so a still desktop would measure how often nothing happened.
#[cfg(wgc)]
struct Animated {
    child: std::process::Child,
}

#[cfg(wgc)]
impl Animated {
    fn start() -> Self {
        let script = r##"
import tkinter as tk
root = tk.Tk()
root.overrideredirect(True)
root.attributes("-topmost", True)
root.geometry("640x360+20+20")
canvas = tk.Canvas(root, bg="black", highlightthickness=0)
canvas.pack(fill="both", expand=True)
step = [0]
def tick():
    step[0] = (step[0] + 9) % 580
    canvas.delete("all")
    canvas.create_rectangle(step[0], 20, step[0] + 60, 340, fill="#ff00ff", outline="")
    canvas.create_rectangle(580 - step[0], 20, 640 - step[0], 340, fill="#00ff88", outline="")
    root.after(16, tick)
tick()
root.mainloop()
"##;
        Self {
            child: std::process::Command::new("python3")
                .args(["-c", script])
                .spawn()
                .expect("python3 with tkinter, which draws while a leg is measured"),
        }
    }
}

#[cfg(wgc)]
impl Drop for Animated {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The process's own kernel and user time, in milliseconds, as the
/// measurements in `docs/measurements` count it: Windows charges it in
/// 15.6 ms ticks, so a leg has to run long enough for the ticks to add up.
#[cfg(wgc)]
fn cpu_millis() -> f64 {
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let blank = || FileTime { low: 0, high: 0 };
    let (mut created, mut exited, mut kernel, mut user) = (blank(), blank(), blank(), blank());
    // SAFETY: plain out-parameters for the call, and the process is this one.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    let ticks = |time: FileTime| f64::from(time.high) * 4_294_967_296.0 + f64::from(time.low);
    (ticks(kernel) + ticks(user)) / 10_000.0
}

#[test]
fn previews_what_it_sends_at_a_smaller_size() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: true,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    // A 1080p picture, mid-grey: luma and chroma halfway.
    let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
    let (y, u, v) = picture.data_mut();
    y.fill(126);
    u.fill(128);
    v.fill(128);
    sink.frame(&picture).unwrap();
    let (width, height, first) = frames
        .take(|width, height, pixels| (width, height, pixels[..4].to_vec()))
        .unwrap();
    assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
    assert!(
        first[..3].iter().all(|&level| level.abs_diff(128) < 4),
        "{first:?}"
    );
    assert_eq!(first[3], 255);
    // A tile's size, once the window says which tile shows it, is what the
    // preview is written at: this picture in a 480x270 tile is 480x270, not
    // the box above.
    sink.preview_tile = Some((480, 270));
    sink.frame(&picture).unwrap();
    let shown = frames.take(|width, height, _| (width, height)).unwrap();
    assert_eq!(shown, (480, 270));
}

/// A frame is scaled and converted for the UI only while the UI shows the
/// preview, and the first frame after it says so has one.
#[test]
fn previews_only_while_the_ui_shows_one() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
    let (y, u, v) = picture.data_mut();
    y.fill(126);
    u.fill(128);
    v.fill(128);
    sink.frame(&picture).unwrap();
    assert_eq!(frames.take(|_, _, _| ()), None, "nothing shows it");
    let mut report = |_| {};
    // A share started while the worker thought nothing watched it: the
    // control comes, and the next frame has a picture for the UI.
    controls.send(Control::Preview(true)).unwrap();
    sink.check(pressure::Reading::default(), &mut report)
        .unwrap();
    sink.frame(&picture).unwrap();
    let (width, height) = frames
        .take(|width, height, _| (width, height))
        .expect("the frame after the control");
    assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
    controls.send(Control::Preview(false)).unwrap();
    sink.check(pressure::Reading::default(), &mut report)
        .unwrap();
    sink.frame(&picture).unwrap();
    assert_eq!(
        frames.take(|_, _, _| ()),
        None,
        "and stops once nothing does"
    );
}

/// Discord's preview of the stream is still made, and its clock kept, while
/// the UI shows no preview of it.
#[test]
fn makes_discords_preview_while_the_ui_shows_none() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: Some(Instant::now()),
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
    let (y, u, v) = picture.data_mut();
    y.fill(126);
    u.fill(128);
    v.fill(128);
    sink.frame(&picture).unwrap();
    assert!(sink.thumbnail.take().is_some(), "a preview for Discord");
    assert!(
        sink.preview_due.is_some_and(|due| due > Instant::now()),
        "the next one isn't due for minutes"
    );
}

#[test]
fn switches_to_the_last_source_asked_for() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let mut report = |_| {};
    assert_eq!(
        sink.check(pressure::Reading::default(), &mut report)
            .unwrap(),
        None
    );
    controls.send(Control::Switch(Source::Display(1))).unwrap();
    controls.send(Control::Switch(Source::Window(2))).unwrap();
    assert_eq!(
        sink.check(pressure::Reading::default(), &mut report)
            .unwrap(),
        Some(Source::Window(2))
    );
    assert_eq!(
        sink.check(pressure::Reading::default(), &mut report)
            .unwrap(),
        None
    );
}

#[test]
fn previews_a_stream_once_its_picture_shows() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: Some(Instant::now()),
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
    let (y, u, v) = picture.data_mut();
    y.fill(16);
    u.fill(128);
    v.fill(128);
    sink.frame(&picture).unwrap();
    assert_eq!(sink.thumbnail, None, "not while the screen is black");
    picture.data_mut().0.fill(126);
    sink.frame(&picture).unwrap();
    let thumbnail = sink.thumbnail.take().expect("a preview");
    let jpeg = thumbnail
        .strip_prefix("data:image/jpeg;base64,")
        .map(|data| {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap()
        })
        .unwrap();
    let shown = image::load_from_memory(&jpeg).unwrap();
    assert_eq!((shown.width(), shown.height()), (512, 288));
    sink.frame(&picture).unwrap();
    assert_eq!(sink.thumbnail, None, "the next one comes minutes later");
}

/// A capture's kept buffer holds the frame just converted, not the one
/// before it, and is the same buffer: a frame costs no picture of its own.
#[test]
fn a_kept_picture_holds_the_new_frame() {
    // Two pixels by two, a row every eight bytes: black, then white.
    let mut argb = vec![0; 2 * 2 * 4];
    let mut kept = None;
    let picture = buffer(&mut kept, 2, 2);
    convert(picture, &argb, 8, &Tone::SDR);
    let (dark, address) = {
        let held = kept.as_ref().expect("made above");
        (held.data().0[0], held.data().0.as_ptr())
    };
    argb.fill(255);
    let picture = buffer(&mut kept, 2, 2);
    convert(picture, &argb, 8, &Tone::SDR);
    let held = kept.as_ref().expect("made above");
    assert!(dark <= 20, "black's luma is {dark}");
    assert!(
        held.data().0.iter().all(|&luma| luma >= 230),
        "white's luma is {:?}",
        held.data().0
    );
    assert_eq!(address, held.data().0.as_ptr(), "the same buffer");
}

/// A frame from a screen showing SDR reaches the encoder as libyuv made
/// it, byte for byte: a share of an SDR screen costs what it did, whatever
/// the curve for an HDR screen's frames would have been.
#[test]
fn an_sdr_frame_is_converted_as_libyuv_makes_it() {
    // Eight pixels by four, a row every thirty-two bytes, of every colour.
    let argb: Vec<u8> = (0..8 * 4 * 4)
        .map(|index| (index * 7 % 251) as u8)
        .collect();
    let mut expected = I420Buffer::new(8, 4);
    {
        let (y_stride, u_stride, v_stride) = expected.strides();
        let (y, u, v) = expected.data_mut();
        yuv_helper::argb_to_i420(&argb, 32, y, y_stride, u, u_stride, v, v_stride, 8, 4);
    }
    let mut converted = I420Buffer::new(8, 4);
    convert(&mut converted, &argb, 32, &Tone::SDR);
    assert_eq!(converted.data().0, expected.data().0, "luma");
    assert_eq!(converted.data().1, expected.data().1, "chroma");
    assert_eq!(converted.data().2, expected.data().2, "chroma");
}

/// A frame from a screen showing HDR comes through the curve into the same
/// buffer and planes, with no picture of its own: the code a screen showing
/// HDR writes SDR's white as shows as the white an SDR screen has.
#[test]
fn an_hdr_frame_comes_through_the_curve() {
    // Four pixels by two, a row every sixteen bytes.
    let argb = [149u8; 4 * 2 * 4];
    let mut picture = I420Buffer::new(4, 2);
    convert(&mut picture, &argb, 16, &Tone::pq());
    let (y, u, v) = picture.data();
    assert!(
        y.iter().all(|&luma| luma.abs_diff(209) <= 6),
        "white's luma is {y:?}"
    );
    assert!(
        u.iter().chain(v).all(|&level| level.abs_diff(128) <= 2),
        "white has no tint: {u:?}, {v:?}"
    );
}

/// A picture scaled down for sharing lands in the buffer kept for it, at
/// the size shared, frame after frame, and a stream of another size gets a
/// buffer of its own.
#[test]
fn a_kept_scale_holds_each_new_frame() {
    // Five by three, so both chroma planes are odd too.
    let mut source = I420Buffer::new(5, 3);
    let fill = |source: &mut I420Buffer, luma: u8| {
        let (y, u, v) = source.data_mut();
        y.fill(luma);
        u.fill(128);
        v.fill(128);
    };
    fill(&mut source, 16);
    let mut kept = None;
    let into = buffer(&mut kept, 4, 2);
    scale_into(&planes(&source), into, 4, 2);
    assert_eq!((into.width(), into.height()), (4, 2));
    assert!(into.data().0.iter().all(|&luma| luma <= 20), "black");
    let address = into.data().0.as_ptr();
    fill(&mut source, 235);
    let into = buffer(&mut kept, 4, 2);
    scale_into(&planes(&source), into, 4, 2);
    assert!(into.data().0.iter().all(|&luma| luma >= 230), "white");
    assert_eq!(address, into.data().0.as_ptr(), "the same buffer");
    let into = buffer(&mut kept, 6, 2);
    assert_eq!((into.width(), into.height()), (6, 2));
    assert_ne!(address, into.data().0.as_ptr(), "another buffer");
}

/// The preview is made anew for each frame, in the buffer kept for it: the
/// second frame shows, not the first.
#[test]
fn previews_each_frame_as_it_comes() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let (_controls, received) = channel::channel();
    let mut sink = Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: true,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    };
    let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
    {
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
    }
    sink.frame(&picture).unwrap();
    let first = frames.take(|_, _, pixels| pixels[..4].to_vec()).unwrap();
    picture.data_mut().0.fill(200);
    sink.frame(&picture).unwrap();
    let second = frames.take(|_, _, pixels| pixels[..4].to_vec()).unwrap();
    assert!(
        first[..3].iter().all(|&level| level.abs_diff(128) < 4),
        "mid-grey: {first:?}"
    );
    assert!(
        second[..3].iter().all(|&level| level > 180),
        "the frame after it: {second:?}"
    );
}

/// A frame reaches the sink while the capture's status is held elsewhere:
/// that status isn't what a frame waits for.
#[test]
fn a_frame_reaches_the_sink_while_its_status_is_held() {
    let frames = Frames::create(4, 2).unwrap();
    let (_controls, received) = channel::channel();
    let sink = Arc::new(Mutex::new(Sink {
        writer: Writer::open(&frames.path().unwrap()).unwrap(),
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: true,
        controls: received,
        server: None,
        sound: false,
        video: None,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        footage: Footage::InMemory,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    }));
    let captured = Arc::new(Mutex::new(Captured::default()));
    // Held as the loop holds it while it reads the capture's status.
    let held = captured.lock().unwrap();
    let (into, seen) = (sink.clone(), captured.clone());
    // Four pixels by two, a row every sixteen bytes, mid-grey.
    let argb = [128u8; 4 * 2 * 4];
    let taking = std::thread::spawn(move || {
        let mut scratch = Scratch::default();
        to_sink(
            &into,
            &seen,
            &mut scratch,
            &argb,
            16,
            ((4, 2), (4, 2)),
            &Tone::SDR,
        );
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while frames.take(|_, _, _| ()).is_none() {
        assert!(
            Instant::now() < deadline,
            "a frame waited on the capture's status"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(held);
    taking.join().unwrap();
    let captured = captured.lock().unwrap();
    assert_eq!(captured.size, Some((4, 2)));
}

#[test]
fn serves_a_test_pattern_until_stopped() {
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let job = Job::Share {
        source: Source::TestPattern,
        frames: frames.path().unwrap(),
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        fps: 60,
        effort: Effort::Light,
        hardware: false,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    let (input, mut feed) = std::io::pipe().unwrap();
    writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
    // The UI shows the preview, as the app tells the worker it does.
    writeln!(
        feed,
        "{}",
        serde_json::to_string(&Control::Preview(true)).unwrap()
    )
    .unwrap();
    let (sender, reports) = std::sync::mpsc::channel();
    let serving = std::thread::spawn(move || {
        serve(Box::new(BufReader::new(input)), &mut |report| {
            sender.send(report).unwrap();
        });
    });
    assert_eq!(reports.recv().unwrap(), Report::Ready);
    assert_eq!(
        reports.recv().unwrap(),
        Report::Sharing {
            width: 640,
            height: 360
        }
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let size = loop {
        if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
            break size;
        }
        assert!(Instant::now() < deadline, "no frame arrived");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(size, (640, 360, 640 * 360 * 4));
    writeln!(feed, "stop").unwrap();
    serving.join().unwrap();
    assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
}

#[test]
fn shares_a_camera_until_stopped() {
    // SAFETY: tests that read this variable all set it the same way.
    unsafe { std::env::set_var("YAPFAST_CAMERA_TEST_PATTERN", "1") };
    let listed = shareable();
    let camera = listed
        .iter()
        .find(|listed| listed.title == "Test pattern")
        .expect("the test pattern camera is listed");
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let job = Job::Share {
        source: camera.source.clone(),
        frames: frames.path().unwrap(),
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    let (input, mut feed) = std::io::pipe().unwrap();
    writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
    // The UI shows the preview, as the app tells the worker it does.
    writeln!(
        feed,
        "{}",
        serde_json::to_string(&Control::Preview(true)).unwrap()
    )
    .unwrap();
    let (sender, reports) = std::sync::mpsc::channel();
    let serving = std::thread::spawn(move || {
        serve(Box::new(BufReader::new(input)), &mut |report| {
            sender.send(report).unwrap();
        });
    });
    assert_eq!(reports.recv().unwrap(), Report::Ready);
    assert_eq!(
        reports.recv_timeout(Duration::from_secs(5)).unwrap(),
        Report::Sharing {
            width: 640,
            height: 360
        }
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let size = loop {
        if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
            break size;
        }
        assert!(Instant::now() < deadline, "no frame arrived");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(size, (640, 360, 640 * 360 * 4));
    writeln!(feed, "stop").unwrap();
    serving.join().unwrap();
    assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
}

/// The portal picks where libwebrtc captures through it: in a Wayland
/// session, and not on X11, as on XFCE, nor where no Wayland display is
/// named, where libwebrtc captures through X11.
#[test]
fn the_portal_picks_only_in_a_wayland_session() {
    assert!(wayland_session(Some("wayland"), true));
    assert!(!wayland_session(Some("x11"), false));
    assert!(!wayland_session(Some("x11"), true));
    assert!(!wayland_session(Some("tty"), false));
    assert!(!wayland_session(None, true));
    assert!(!wayland_session(Some("wayland"), false));
}

#[test]
fn a_screen_picture_survives_its_report_line() {
    let report = Report::Screen {
        id: 7,
        width: 2,
        height: 1,
        rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
    };
    let line = serde_json::to_string(&report).unwrap();
    assert!(line.contains(r#""rgba":"/wAA/wAA//8=""#), "{line}");
    assert_eq!(serde_json::from_str::<Report>(&line).unwrap(), report);
}

/// Takes a picture of each listed screen, as the share menu shows them.
/// On Linux, X11 lists them: a private X server, as from `env -u
/// WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run`, with a window as the
/// next test says.
#[test]
#[ignore = "needs a desktop where Yapfast lists what to share"]
fn glances_at_the_listed_screens() {
    let screens: Vec<u64> = shareable()
        .into_iter()
        .filter_map(|listed| match listed.source {
            Source::Display(id) => Some(id),
            _ => None,
        })
        .collect();
    assert!(!screens.is_empty(), "no screen");
    let job = Job::Glance {
        screens: screens.clone(),
        width: 320,
        height: 180,
    };
    let (input, mut feed) = std::io::pipe().unwrap();
    writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
    let mut reports = vec![];
    serve(Box::new(BufReader::new(input)), &mut |report| {
        reports.push(report)
    });
    let pictured: Vec<u64> = reports
        .iter()
        .filter_map(|report| match report {
            Report::Screen {
                id,
                width,
                height,
                rgba,
            } => {
                eprintln!("screen {id}: {width}x{height}");
                assert!(*width <= 320 && *height <= 180);
                assert_eq!(rgba.len(), (width * height * 4) as usize);
                Some(*id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(pictured, screens);
    assert_eq!(reports.last(), Some(&Report::Ended { error: None }));
}

/// Lists what there is to share, shares the first screen and the first
/// window for a moment each, then switches from the screen to the window.
/// Under Xvfb, as above, it needs a window with a title, and the
/// `WM_STATE` and `_NET_WM_WINDOW_TYPE` a window manager would give it:
/// without them libwebrtc's X11 capturer lists no window, or asks for a
/// property X doesn't know and Xlib ends the process.
#[test]
#[ignore = "needs a desktop where Yapfast lists what to share"]
fn shares_a_listed_screen_and_window() {
    let listed = shareable();
    eprintln!("{listed:#?}");
    let first = |window: bool| {
        listed
            .iter()
            .find(|listed| matches!(listed.source, Source::Window(_)) == window)
            .map(|listed| listed.source.clone())
    };
    let sources = [
        first(false).expect("no screen"),
        first(true).expect("no window"),
    ];
    for source in sources.clone() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            source: source.clone(),
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            ceiling: 0.07,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        let sharing = reports.recv_timeout(Duration::from_secs(10)).unwrap();
        eprintln!("{source:?}: {sharing:?}");
        assert!(matches!(sharing, Report::Sharing { .. }), "{sharing:?}");
        // How much of the last picture isn't black, a second and a half
        // in: what's shared may still be drawing at first.
        let deadline = Instant::now() + Duration::from_millis(1500);
        let mut last = None;
        while Instant::now() < deadline {
            let taken = frames.take(|width, height, pixels| {
                let lit = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [0; 3]);
                (
                    width,
                    height,
                    lit.count() as f32 / (pixels.len() / 4).max(1) as f32,
                )
            });
            last = taken.or(last);
            std::thread::sleep(Duration::from_millis(5));
        }
        let (width, height, lit) = last.expect("no frame arrived");
        eprintln!(
            "{source:?}: {width}x{height}, {:.0}% of it isn't black",
            lit * 100.0
        );
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        // The size may change as capture settles.
        let ended = reports
            .iter()
            .find(|report| !matches!(report, Report::Sharing { .. }));
        assert_eq!(ended, Some(Report::Ended { error: None }));
    }
    // Switching to the window while the screen is shared.
    let [screen, window] = sources;
    let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
    let job = Job::Share {
        source: screen,
        frames: frames.path().unwrap(),
        width: PREVIEW_WIDTH,
        height: PREVIEW_HEIGHT,
        fps: 30,
        effort: Effort::Light,
        hardware: false,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    let (input, mut feed) = std::io::pipe().unwrap();
    writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
    let (sender, reports) = std::sync::mpsc::channel();
    let serving = std::thread::spawn(move || {
        serve(Box::new(BufReader::new(input)), &mut |report| {
            sender.send(report).unwrap();
        });
    });
    assert_eq!(reports.recv().unwrap(), Report::Ready);
    let shared = reports.recv_timeout(Duration::from_secs(10)).unwrap();
    let switch = Control::Switch(window.clone());
    writeln!(feed, "{}", serde_json::to_string(&switch).unwrap()).unwrap();
    // The screen's size may still settle first.
    let switched = loop {
        let report = reports.recv_timeout(Duration::from_secs(10)).unwrap();
        if report != shared {
            break report;
        }
    };
    eprintln!("{shared:?}, then {window:?}: {switched:?}");
    assert!(matches!(switched, Report::Sharing { .. }), "{switched:?}");
    assert_ne!(switched, shared);
    writeln!(feed, "stop").unwrap();
    serving.join().unwrap();
    // A window closed since it was listed.
    let frames = Frames::create(16, 16).unwrap();
    let gone = Job::Share {
        source: Source::Window(u64::MAX),
        frames: frames.path().unwrap(),
        width: 16,
        height: 16,
        fps: 1,
        effort: Effort::Light,
        hardware: false,
        ceiling: 0.07,
        codec: StreamCodec::Automatic,
        capture: crate::capture::Preference::Auto,
    };
    let mut reports = Vec::new();
    let input = format!("{}\n", serde_json::to_string(&gone).unwrap());
    serve(Box::new(std::io::Cursor::new(input)), &mut |report| {
        reports.push(report);
    });
    assert_eq!(
        reports,
        [
            Report::Ready,
            Report::Ended {
                error: Some("That screen or window is gone.".into())
            }
        ]
    );
}

/// A frame that didn't decode leaves every delta after it describing a
/// picture the decoder never had, so the watch waits for a keyframe
/// instead of handing them over.
#[test]
fn a_frame_that_didnt_decode_waits_for_a_keyframe() {
    let junk = |keyframe| ReceivedFrame {
        codec: Codec::Vp8,
        data: b"not video".to_vec(),
        keyframe,
        timestamp: 0,
    };
    let mut decoding = Decoding::new();
    // libvpx says so, and the watch is left waiting.
    assert!(matches!(decoding.decode(&junk(false)), Decoded::Failed(_)));
    // The deltas after it reference the frame that didn't decode.
    assert!(matches!(decoding.decode(&junk(false)), Decoded::Waiting));
    // A keyframe is what the decoder can start again from, whatever it
    // holds, and the deltas after it are its own.
    assert!(matches!(decoding.decode(&junk(true)), Decoded::Failed(_)));
    assert!(matches!(decoding.decode(&junk(false)), Decoded::Waiting));
}

/// A stream that turns to another codec starts a decoder afresh, which
/// knows nothing of the pictures the one before it decoded.
#[test]
fn a_codec_change_waits_for_a_keyframe() {
    let frame = |codec, keyframe| ReceivedFrame {
        codec,
        data: b"not video".to_vec(),
        keyframe,
        timestamp: 0,
    };
    let mut decoding = Decoding::new();
    assert!(matches!(
        decoding.decode(&frame(Codec::Vp8, false)),
        Decoded::Failed(_)
    ));
    assert!(matches!(
        decoding.decode(&frame(Codec::H264, false)),
        Decoded::Waiting
    ));
    assert!(
        !matches!(decoding.decode(&frame(Codec::H264, true)), Decoded::Waiting),
        "a keyframe reaches the new decoder"
    );
}

/// A codec this build has no decoder for doesn't end the watch: the
/// stream's sound plays on, no frame of that codec costs another attempt,
/// and a codec this build decodes gets pictures back.
#[test]
fn a_codec_the_build_cant_decode_waits_for_one_it_can() {
    use std::sync::atomic::AtomicUsize;
    static MADE: AtomicUsize = AtomicUsize::new(0);
    fn no_h265(codec: Codec) -> Result<Live> {
        MADE.fetch_add(1, Ordering::Relaxed);
        match codec {
            Codec::H265 => bail!("this build of Yapfast can't decode H265"),
            codec => Live::new(codec),
        }
    }
    let frame = |codec, keyframe| ReceivedFrame {
        codec,
        data: b"not video".to_vec(),
        keyframe,
        timestamp: 0,
    };
    let mut decoding = Decoding {
        make: no_h265,
        ..Decoding::new()
    };
    // The watch waits through the codec it can't decode, however many
    // frames of it come, and does nothing with them.
    assert!(matches!(
        decoding.decode(&frame(Codec::H265, true)),
        Decoded::Waiting
    ));
    assert!(matches!(
        decoding.decode(&frame(Codec::H265, false)),
        Decoded::Waiting
    ));
    assert_eq!(MADE.load(Ordering::Relaxed), 1, "no attempt a frame");
    // A codec it does decode reaches a decoder of its own.
    assert!(matches!(
        decoding.decode(&frame(Codec::Vp8, false)),
        Decoded::Failed(_)
    ));
    assert_eq!(MADE.load(Ordering::Relaxed), 2);
}

/// Whether a share whose pictures are DMA-BUFs sends one, and whether it
/// publishes one for the window: a still desktop is answered from the picture
/// the capture last handed over, and the window is given it once rather than
/// a frame in ten.
#[cfg(target_os = "linux")]
#[test]
fn a_still_desktop_sends_when_asked_and_publishes_once() {
    // Nothing has been captured at all: nothing to send and nothing to show.
    assert!(!sending(false, false, true));
    // A picture that has just arrived is sent, whatever the stream wanted.
    assert!(sending(true, false, false));
    // A still desktop is sent only while the stream asks for a picture — a
    // viewer joining, a decoder that lost its place, a codec Discord chose.
    assert!(!sending(false, true, false));
    assert!(sending(false, true, true));
    // The window is published while a preview is wanted, and once: the
    // picture it was given is the one it keeps showing until another
    // replaces it, so a second pass owes it nothing.
    assert!(!publishing(false, true), "nothing shows a picture");
    assert!(!publishing(true, false), "the window has this one already");
    assert!(publishing(true, true), "a preview that has none yet");
}

/// A capture that answers what a test tells it to, and remembers that it was
/// dropped: a share that stops lets go of its capture, which is the portal's
/// session closing and the X server's redirection being undone.
#[cfg(target_os = "linux")]
struct Scripted {
    answers: std::vec::IntoIter<Result<Found>>,
    ending: String,
    dropped: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    /// Whether the script has run out once already.
    done: bool,
}

#[cfg(target_os = "linux")]
impl Pictures for Scripted {
    /// A picture or an ending at once, as a desktop that drew answers; a
    /// still desktop once the wait is over, as a capture says it.
    fn frame(&mut self, wait: Duration) -> Result<Found> {
        match self.answers.next() {
            Some(Ok(Found::Still)) => {
                std::thread::sleep(wait);
                Ok(Found::Still)
            }
            Some(answer) => answer,
            // A capture that has said everything it was told to says nothing
            // has changed — once, so a picture still waiting for its slot is
            // sent — and then asks the share to stop: a test of a share that
            // runs until it is stopped has no other way to end.
            None => {
                std::thread::sleep(wait);
                if self.done {
                    self.stop.store(true, Ordering::Relaxed);
                }
                self.done = true;
                Ok(Found::Still)
            }
        }
    }

    fn describe(&self) -> String {
        "a test's capture".to_owned()
    }

    fn ending(&self) -> String {
        self.ending.clone()
    }
}

#[cfg(target_os = "linux")]
impl Drop for Scripted {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

/// A picture of `width` by `height` in a file of its own, which is what a
/// compositor's buffer is to a share: memory, and the layout it was drawn in.
#[cfg(target_os = "linux")]
fn shot(width: u32, height: u32) -> Shot {
    let file = tempfile::tempfile().unwrap();
    let stride = width * 4;
    file.set_len(u64::from(stride) * u64::from(height)).unwrap();
    Shot::held(width, height, 0, stride, Some(0), Order::Bgra, file.as_fd())
        .unwrap_or_else(|error| panic!("a picture of {width}x{height}: {error:#}"))
}

/// The sink a share of a window that shows no preview reports through: a
/// frames file of its own, no stream, and nothing shown.
#[cfg(target_os = "linux")]
fn sink_of(width: u32, height: u32) -> Sink {
    let frames = Frames::create(width, height).unwrap();
    let path = frames.path().unwrap();
    let writer = Writer::open(&path).unwrap();
    frames.unlink();
    let (_controls, controls) = channel::channel();
    Sink {
        writer,
        preview: vec![],
        preview_tile: None,
        capture: String::new(),
        preview_shown: false,
        controls,
        server: None,
        sound: false,
        video: None,
        fps: 60,
        effort: Effort::default(),
        hardware: false,
        footage: Footage::Dmabuf,
        started: Instant::now(),
        streaming: false,
        preview_due: None,
        thumbnail: None,
        lost: None,
        diagnosis: Diagnosis::default(),
        #[cfg(nvcuda)]
        refused_pictures: 0,
    }
}

/// Runs a share of a scripted capture, and gives back what it reported and
/// what it ended as.
#[cfg(target_os = "linux")]
fn scripted(
    answers: Vec<Result<Found>>,
    ending: &str,
    stop: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
) -> (Vec<String>, Result<After>) {
    let mut reported = vec![];
    let mut report = |report: Report| {
        if let Report::Sharing { width, height } = report {
            reported.push(format!("{width}x{height}"));
        }
    };
    let opened = Opened {
        capture: Box::new(Scripted {
            answers: answers.into_iter(),
            ending: ending.to_owned(),
            dropped,
            stop: stop.clone(),
            done: false,
        }),
        rung: capture::LinuxRung::X11,
        why: "a test said so".to_owned(),
    };
    let outcome = capture_dmabufs(
        opened,
        Source::Screen,
        sink_of(1920, 1080),
        (SHARE_WIDTH, SHARE_HEIGHT),
        &stop,
        &mut report,
    );
    (reported, outcome.map(|(_sink, after)| after))
}

/// The screen changes size under a share: what it reports is the size it is
/// sending, once for each, and the picture drawn at the size before it is not
/// sent under the new one's numbers.
#[cfg(target_os = "linux")]
#[test]
fn a_resize_is_reported_at_the_size_the_picture_is() {
    let stop = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let (reported, outcome) = scripted(
        vec![
            Ok(Found::Picture(shot(640, 480))),
            Ok(Found::Picture(shot(640, 480))),
            Ok(Found::Resized),
            Ok(Found::Picture(shot(1280, 720))),
        ],
        "",
        stop,
        dropped.clone(),
    );
    assert!(matches!(outcome, Ok(After::Ended)), "the share ends");
    assert_eq!(
        reported,
        ["640x480", "1280x720"],
        "one report for each size, and the second only after the picture at it"
    );
    assert!(
        dropped.load(Ordering::SeqCst),
        "the capture goes with the share"
    );
}

/// Stopping a share ends it and lets go of its capture, which is the portal's
/// session and the X server's redirection. Nothing spins while it waits: the
/// loop asks the capture for a picture and the capture's own wait paces it.
#[cfg(target_os = "linux")]
#[test]
fn stopping_ends_the_share_and_its_capture() {
    let stop = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let (_, outcome) = scripted(
        vec![Ok(Found::Picture(shot(320, 180)))],
        "",
        stop.clone(),
        dropped.clone(),
    );
    assert!(matches!(outcome, Ok(After::Ended)));
    assert!(stop.load(Ordering::Relaxed), "the script ran out");
    assert!(dropped.load(Ordering::SeqCst), "the capture is let go");
}

/// A desktop that draws faster than the stream's frame rate, as a busy screen
/// on a fast monitor does.
#[cfg(target_os = "linux")]
struct Eager {
    until: Instant,
    pictures: u32,
    stop: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl Pictures for Eager {
    fn frame(&mut self, _wait: Duration) -> Result<Found> {
        if Instant::now() >= self.until {
            self.stop.store(true, Ordering::Relaxed);
            return Ok(Found::Still);
        }
        // Every other picture at another size, so each one the share sends is
        // a report of its own; and a moment apart, as frames are.
        self.pictures += 1;
        std::thread::sleep(Duration::from_millis(1));
        Ok(Found::Picture(if self.pictures.is_multiple_of(2) {
            shot(64, 48)
        } else {
            shot(32, 24)
        }))
    }

    fn describe(&self) -> String {
        "a busy desktop".to_owned()
    }

    fn ending(&self) -> String {
        String::new()
    }
}

/// A desktop that draws more often than the stream sends is sent at the
/// stream's rate: the pictures that come before their slot are held, and only
/// the newest goes out when the slot comes. Sending each as it came encoded a
/// 30 fps share at a monitor's refresh rate, which the send queue then dropped.
#[cfg(target_os = "linux")]
#[test]
fn a_busy_desktop_is_sent_at_the_streams_rate() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut sent = 0;
    let mut report = |report: Report| {
        if let Report::Sharing { .. } = report {
            sent += 1;
        }
    };
    let opened = Opened {
        capture: Box::new(Eager {
            until: Instant::now() + Duration::from_millis(1500),
            pictures: 0,
            stop: stop.clone(),
        }),
        rung: capture::LinuxRung::X11,
        why: "a test said so".to_owned(),
    };
    let mut sink = sink_of(64, 48);
    sink.fps = 20;
    let outcome = capture_dmabufs(
        opened,
        Source::Screen,
        sink,
        (SHARE_WIDTH, SHARE_HEIGHT),
        &stop,
        &mut report,
    );
    assert!(matches!(outcome, Ok((_, After::Ended))));
    // 1.5 s at 20 frames a second is thirty slots, and the desktop drew
    // hundreds of pictures in them. The band is wide because the slots are
    // wall-clock ones and this suite runs its tests side by side: a thread
    // that waits its turn sends some of them late, and what the test is for
    // is that the rate bounds the pictures sent rather than the capture.
    assert!(
        (12..=40).contains(&sent),
        "{sent} pictures sent in thirty slots"
    );
}

/// A capture whose share ends — the window was closed, or the person stopped
/// it — ends the share with the capture's own sentence, rather than waiting
/// for a picture that will never come.
#[cfg(target_os = "linux")]
#[test]
fn a_capture_that_ends_ends_the_share_with_its_reason() {
    let stop = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let (_, outcome) = scripted(
        vec![Ok(Found::Picture(shot(320, 180))), Ok(Found::Closed)],
        "window 0x2a was closed",
        stop,
        dropped.clone(),
    );
    let error = outcome.err().expect("the share ends");
    assert_eq!(format!("{error:#}"), "window 0x2a was closed");
    assert!(dropped.load(Ordering::SeqCst), "the capture is let go");
}

/// A source the card's paths can't take at all is a share that stays on the
/// processor: a camera and a test pattern have no picture on the card, and a
/// window, a display and the screen always do.
#[cfg(target_os = "linux")]
#[test]
fn a_linux_capture_takes_a_window_display_or_screen() {
    assert!(dmabuf_takes(&Source::Screen));
    assert!(dmabuf_takes(&Source::Display(0)));
    assert!(
        dmabuf_takes(&Source::Window(0x1234)),
        "a window is that window, and never the display it sits on"
    );
    assert!(!dmabuf_takes(&Source::Camera("cam".into())));
    assert!(!dmabuf_takes(&Source::TestPattern));
}

/// A screen's id, as Yapfast's own menu listed it, names the screen when it
/// is the root window's own id and a monitor when it is a place: a monitor is
/// never a window, and an id that names neither is refused rather than
/// captured as something else.
#[cfg(target_os = "linux")]
#[test]
fn a_listed_screen_is_the_screen_it_names_or_a_monitor() {
    use capture::x11::{Display, Target};
    let monitors = vec![
        Display {
            name: "eDP-1".to_owned(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        Display {
            name: "DP-1".to_owned(),
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        },
    ];
    // The root window is the screen the other windows sit on.
    assert_eq!(
        screen_target(0x1e0, 0x1e0, &monitors).ok(),
        Some(Target::Screen)
    );
    // A place is the monitor the menu would show there.
    assert_eq!(
        screen_target(1, 0x1e0, &monitors).ok(),
        Some(Target::Display(monitors[1].clone()))
    );
    assert_eq!(
        screen_target(0, 0x1e0, &monitors).ok(),
        Some(Target::Display(monitors[0].clone()))
    );
    // And an id that is neither is said so, which is a share that falls back
    // rather than one that captures a monitor nobody chose.
    let error = screen_target(7, 0x1e0, &monitors).unwrap_err();
    assert!(
        format!("{error:#}").contains("no monitor at place 7"),
        "{error:#}"
    );
}
