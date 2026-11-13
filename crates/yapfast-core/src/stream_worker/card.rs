//! `stream_worker`'s card.

use super::*;

/// One rung of the card's capture path: a display's picture arriving as a
/// texture the encoder reads where it lies, and the capture's own device that
/// texture is on.
///
/// The two rungs differ in which door they come through and not in what they
/// hand over: a borrowed BGRA texture and the device it was made on, so the
/// loop above them, the encoder and the preview are the same either way. Which
/// one a share got is kept here, because rebuilding it after the card takes its
/// device away means opening the same display the same way.
#[cfg(any(wgc, dup))]
pub(crate) enum Card {
    /// Windows Graphics Capture, which draws the pointer and, on a Windows
    /// whose session can't be asked, leaves its yellow border on.
    #[cfg(wgc)]
    Graphics {
        capture: wgc::Capture,
        /// What it captures: a display, or a window the person chose, which is
        /// never reinterpreted as the display it happens to be on.
        target: wgc::Target,
    },
    /// DXGI's Desktop Duplication, which has no border and paints the pointer
    /// into the frame itself. It is the rung a Windows whose Graphics Capture
    /// can't remove its border tries first, and the fallback everywhere else.
    #[cfg(dup)]
    Duplication {
        capture: dup::Capture,
        target: dup::Target,
    },
}

/// What the card had for the loop this pass: a new picture in the capture's
/// texture, a still screen, a size change, or the end of the capture.
#[cfg(any(wgc, dup))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrival {
    /// The capture's texture holds a new picture. A pointer that moved counts:
    /// the screen shows it somewhere else, and the frame is the picture the
    /// stream should send.
    Frame,
    /// Nothing has been drawn and nothing has moved. This is not a failure and
    /// makes no frame: the next pass is the next slot.
    Still,
    /// The screen changed size and the capture has rebuilt at the new one.
    Resized,
    /// What was captured ended. This rung hands out no more frames.
    Closed,
}

#[cfg(any(wgc, dup))]
impl Card {
    /// Asks for the next frame. `wait` is how long a duplication may wait for
    /// the desktop to change; it is zero because the stream's own slot is the
    /// pace on both rungs, and a capture that waited would spend a second wait
    /// on top of that slot. Graphics Capture's look never waits at all.
    pub(crate) fn frame(&mut self, wait: Duration) -> Result<Arrival> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => match capture.frame()? {
                wgc::Arrival::Frame => Ok(Arrival::Frame),
                // Nothing drawn since the last look is the compositor's own
                // rate, and the loop's next slot is when it asks again.
                wgc::Arrival::Idle => Ok(Arrival::Still),
                wgc::Arrival::Resized => Ok(Arrival::Resized),
                wgc::Arrival::Closed => Ok(Arrival::Closed),
            },
            // A duplication reports a moved pointer separately from a new
            // desktop image, and both leave the last picture in the texture
            // with the pointer where it is now: either is a frame to send.
            #[cfg(dup)]
            Self::Duplication { capture, .. } => match capture.frame(wait)? {
                dup::Arrival::Frame | dup::Arrival::Pointer => Ok(Arrival::Frame),
                dup::Arrival::Still => Ok(Arrival::Still),
            },
        }
    }

    pub(crate) fn content_size(&self) -> (u32, u32) {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.content_size(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.content_size(),
        }
    }

    /// The borrowed texture the last [`Arrival::Frame`] left here.
    pub(crate) fn texture(&self) -> *mut c_void {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.texture(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.texture(),
        }
    }

    /// The device the texture is on, which the encoder has to open on.
    pub(crate) fn device(&self) -> *mut c_void {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.device(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.device(),
        }
    }

    /// What this capture is, in one line, for the log that says which rung ran.
    pub(crate) fn describe(&self) -> String {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.describe(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.describe(),
        }
    }

    pub(crate) fn rung(&self) -> capture::Rung {
        match self {
            #[cfg(wgc)]
            Self::Graphics { .. } => capture::Rung::Graphics,
            #[cfg(dup)]
            Self::Duplication { .. } => capture::Rung::Duplication,
        }
    }

    /// Whether this capture's border is off, for a rung that draws one: a
    /// duplication never does, and answers `None` rather than a `true` that
    /// would read as something it did.
    pub(crate) fn borderless(&self) -> Option<bool> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => Some(capture.borderless()),
            #[cfg(dup)]
            Self::Duplication { .. } => None,
        }
    }

    /// The border's answer in the shim's own words, for the share that lets a
    /// bordered capture go and has to say what refused it.
    pub(crate) fn border(&self) -> String {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.border(),
            #[cfg(dup)]
            Self::Duplication { .. } => String::new(),
        }
    }

    /// Opens the capture again, of the display it was of, after the card took
    /// the device it was on away. The old capture is let go first, not after:
    /// DXGI gives an output to one duplicator at a time, so asking for the new
    /// one while the old one still holds it would be answered with another
    /// duplicator already having it. The encoder must already be let go too: a
    /// session read the old device, and this capture makes a new one.
    pub(crate) fn reopen(self) -> Result<Self> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, target } => {
                drop(capture);
                Ok(Self::Graphics {
                    capture: wgc::Capture::open(target)?,
                    target,
                })
            }
            #[cfg(dup)]
            Self::Duplication { capture, target } => {
                drop(capture);
                Ok(Self::Duplication {
                    capture: dup::Capture::open(target)?,
                    target,
                })
            }
        }
    }
}

/// A capture that opened, and why it is the one sharing: the setting's choice,
/// or what the fallbacks went through to reach it. The sentence goes into the
/// one log line that says which rung ran, so a share that fell through, or that
/// let a bordered capture go, says why where a reader is already looking.
#[cfg(any(wgc, dup))]
pub(crate) struct Chosen {
    pub(crate) card: Card,
    pub(crate) why: String,
}

/// Opens `rung`'s capture of `source`.
#[cfg(any(wgc, dup))]
pub(crate) fn open_rung(
    rung: capture::Rung,
    source: &Source,
    sink: &Sink,
    bounds: (u32, u32),
) -> Result<Card> {
    match rung {
        #[cfg(wgc)]
        capture::Rung::Graphics => graphics_capture(source, sink, bounds),
        #[cfg(dup)]
        capture::Rung::Duplication => duplication(source, sink, bounds),
        // A rung this build doesn't carry is never in the order; the arm keeps
        // a build with one carrier a complete match.
        #[cfg(not(all(wgc, dup)))]
        _ => bail!("this build of Yapfast has no {rung:?} capture"),
    }
}

/// Adds one rung's refusal to the line a share reports, in the order they were
/// tried, naming the rung and what it said.
#[cfg(any(wgc, dup))]
pub(crate) fn refuse(into: &mut String, rung: capture::Rung, reason: &str) {
    if !into.is_empty() {
        into.push_str("; ");
    }
    into.push_str(rung.name());
    into.push_str(": ");
    into.push_str(reason);
}

/// Why a share captures the way it does, in the sentence its one log line
/// carries. With nothing refused it is the choice itself — the setting's, or
/// Auto's reading of what this Windows can do about the border. With refusals
/// before it, it is those and the rung that came next: that is where a bordered
/// Graphics Capture that was let go says so, in the shim's own words.
#[cfg(any(wgc, dup))]
pub(crate) fn chosen_why(
    preference: capture::Preference,
    platform: capture::Platform,
    available: capture::Available,
    rung: capture::Rung,
    refused: &str,
) -> String {
    if !refused.is_empty() {
        return format!("{refused}; {} came next", rung.name());
    }
    let why = match (preference, rung) {
        (capture::Preference::GraphicsCapture, _) => {
            "the setting asks for Windows Graphics Capture"
        }
        (capture::Preference::Duplication, _) => "the setting asks for Desktop Duplication",
        // A build whose Windows SDK has no capture headers carries no Graphics
        // Capture rung at all, which is a different reason from a Windows that
        // has one and can't remove its border.
        (capture::Preference::Auto, _) if !available.graphics => {
            "this build has no Windows Graphics Capture, so Desktop Duplication captures"
        }
        (capture::Preference::Auto, capture::Rung::Graphics) if platform.can_remove_border() => {
            "this build of Windows can take Graphics Capture's border off, so it comes first"
        }
        (capture::Preference::Auto, capture::Rung::Graphics) => {
            "this build of Windows cannot take Graphics Capture's border off, and it is the rung \
             there is"
        }
        (capture::Preference::Auto, capture::Rung::Duplication) => {
            "this build of Windows cannot take Graphics Capture's border off, so Desktop \
             Duplication comes first"
        }
    };
    why.to_owned()
}

/// The monitor Windows knows a listed screen by.
///
/// libwebrtc names a screen by its place in the system's own enumeration — the
/// number a [`Source::Display`] carries is that place, `0` for the first — and
/// the card's capture paths take the `HMONITOR` the same enumeration hands out.
/// They are not the same number, so the place is turned back into the handle
/// here: a picked screen was otherwise a capture of an HMONITOR that doesn't
/// exist, which both rungs refused and left on the processor.
#[cfg(any(wgc, dup))]
pub(crate) fn monitor_of(place: u64) -> Result<u64> {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows::core::BOOL;

    /// Collects each monitor the system enumerates, in its own order.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        monitors: LPARAM,
    ) -> BOOL {
        // SAFETY: `monitors` is the Vec borrowed below for this call, and this
        // borrow of it lasts the callback only.
        let monitors = unsafe { &mut *(monitors.0 as *mut Vec<u64>) };
        monitors.push(monitor.0 as u64);
        true.into()
    }

    let mut monitors: Vec<u64> = Vec::new();
    // SAFETY: the callback only appends to `monitors`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<u64> as isize),
        );
    }
    usize::try_from(place)
        .ok()
        .and_then(|place| monitors.get(place).copied())
        .with_context(|| {
            format!(
                "no display is listed at place {place}, where {} are enumerated",
                monitors.len()
            )
        })
}

/// What a share of `source` is captured as, before its handle is turned into a
/// `wgc::Target`: a window as itself, a display as the monitor the shell's list
/// named, and the primary display when nothing has been chosen.
///
/// It is the whole of the decision `graphics_capture` makes about which of the
/// two constructors it asks for, kept apart from the capture it opens so that
/// the rule can be checked without a screen or a window: a window is captured
/// as that window, and never as the display it sits on.
#[cfg(any(wgc, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardTarget {
    /// A window's own handle, which `CreateForWindow` is given.
    Window(u64),
    /// A display's monitor handle, or [`None`] for the one the desktop starts
    /// on, which `CreateForMonitor` is given as the primary display.
    Display(Option<u64>),
}

/// Which target `source` is captured as: see [`CardTarget`]. A camera and a
/// test pattern are not a screen, and saying so is the reason a share of one
/// stays on the processor.
#[cfg(any(wgc, test))]
pub(crate) fn card_target(source: &Source) -> Result<CardTarget> {
    match source {
        // The window's handle is carried unchanged: it is the one
        // `CreateForWindow` is given, and the display the window is composited
        // on is not what the person chose.
        Source::Window(window) => Ok(CardTarget::Window(*window)),
        Source::Screen => Ok(CardTarget::Display(None)),
        Source::Display(place) => Ok(CardTarget::Display(Some(*place))),
        Source::Camera(_) => bail!("a camera isn't a screen"),
        Source::TestPattern => bail!("a test pattern is drawn, not captured"),
    }
}

/// The Windows Graphics Capture rung: the display's compositor fills the
/// texture, so the pointer is already in it and no pixel is drawn here.
///
/// The stream's own session opens on the capture's device and is let go: what
/// it asks is whether NVENC opens where the capture fills its textures, which
/// is the one thing that decides whether these halves fit together. A session
/// reads a texture only on the device it opened on, so asked before the loop
/// starts, a card that can't answer leaves the share on the processor instead
/// of failing its first frame.
#[cfg(wgc)]
pub(crate) fn graphics_capture(
    source: &Source,
    sink: &Sink,
    (max_width, max_height): (u32, u32),
) -> Result<Card> {
    // Which of the two constructors this goes through, and the handle it names:
    // a window as itself, by the item `CreateForWindow` makes, and a display as
    // the monitor the shell's list named.
    let target = match card_target(source)? {
        CardTarget::Window(window) => {
            wgc::Target::from(wgc::Window::new(window as usize as *mut c_void))
        }
        CardTarget::Display(None) => wgc::Target::from(wgc::Monitor::PRIMARY),
        CardTarget::Display(Some(place)) => {
            wgc::Target::from(wgc::Monitor::new(monitor_of(place)? as usize as *mut c_void))
        }
    };
    let capture = wgc::Capture::open(target)?;
    let (width, height) = capture.content_size();
    if width == 0 || height == 0 {
        bail!("the screen is {width}x{height}, which is nothing to capture");
    }
    let (sent_width, sent_height) = fit(width, height, max_width, max_height);
    d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        sink.fps,
        kbps(sent_width, sent_height, sink.fps),
        d3d11::Surfaces::Ring,
    )
    .context("the card won't open an encoder on the device it captures with")?;
    Ok(Card::Graphics { capture, target })
}

/// The Desktop Duplication rung: DXGI hands over the compositor's own surface,
/// and native/dup.cc turns it upright, converts it to BGRA and paints the
/// pointer into it on the card before the loop sees it. No border is drawn and
/// nothing is left on the screen, so this is the rung a Windows whose Graphics
/// Capture can't remove its border tries first, and where a screen reaches a
/// stream on a Windows whose Graphics Capture is missing or won't open.
///
/// The device is the duplication's own, made by the shim on the adapter that
/// drives the display, because a duplication exists only on its own card's
/// device: the encoder then opens its session there rather than on a device of
/// its choosing. That is the same one-device rule as the first rung, and it is
/// why a machine whose NVENC is on another card refuses here too.
#[cfg(dup)]
pub(crate) fn duplication(
    source: &Source,
    sink: &Sink,
    (max_width, max_height): (u32, u32),
) -> Result<Card> {
    let target = duplication_target(source)?;
    let capture = dup::Capture::open(target)?;
    let (width, height) = capture.content_size();
    if width == 0 || height == 0 {
        bail!("the screen is {width}x{height}, which is nothing to duplicate");
    }
    let (sent_width, sent_height) = fit(width, height, max_width, max_height);
    d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        sink.fps,
        kbps(sent_width, sent_height, sink.fps),
        d3d11::Surfaces::Ring,
    )
    .context(
        "the card won't open an encoder on the device the duplication fills its textures on",
    )?;
    Ok(Card::Duplication { capture, target })
}

/// Which display a duplication captures, following the rule the first rung's
/// `card_target` follows: the primary display when the person hasn't chosen
/// one, and otherwise the one they chose — here as the adapter and output index
/// DXGI knows it by, since a duplication is opened by those and not by a monitor
/// handle.
#[cfg(dup)]
pub(crate) fn duplication_target(source: &Source) -> Result<dup::Target> {
    match source {
        Source::Screen => dup::Target::primary(),
        Source::Display(screen) => target_of_monitor(monitor_of(*screen)?),
        Source::Window(_) => bail!(
            "a window is captured on the processor: what DXGI duplicates here is a whole display"
        ),
        Source::Camera(_) => bail!("a camera isn't a display to duplicate"),
        Source::TestPattern => bail!("a test pattern is drawn, not copied from a display"),
    }
}

/// The adapter and output driving the display `monitor`, as DXGI numbers them.
///
/// The monitor handle is the one the shell's list gives — the same one Windows
/// Graphics Capture takes — and DXGI names an output by its adapter's LUID and
/// an index, so the two are matched by asking every adapter for every output's
/// monitor, which is what native/wgc.cc does with the same handle. It is asked
/// once per share, before the loop, so it costs nothing a frame.
#[cfg(dup)]
pub(crate) fn target_of_monitor(monitor: u64) -> Result<dup::Target> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    // The first rung's `Monitor::new(0)` is the primary display, and the same
    // handle from a stale list means the same thing here rather than a search
    // for an output with no monitor at all.
    if monitor == 0 {
        return dup::Target::primary();
    }
    let handle = monitor as usize as *mut std::ffi::c_void;
    // SAFETY: DXGI is asked for its factory and then for each adapter and output
    // by index, and every one it hands back is released when it goes out of
    // scope.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|status| anyhow!("no DXGI factory: {status}"))?;
    let mut at = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(at) } {
        at += 1;
        let Ok(described) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        let mut output = 0;
        while let Ok(screen) = unsafe { adapter.EnumOutputs(output) } {
            let Ok(desc) = (unsafe { screen.GetDesc() }) else {
                output += 1;
                continue;
            };
            if desc.Monitor.0 == handle {
                return Ok(dup::Target::new(
                    described.AdapterLuid.HighPart as u32,
                    described.AdapterLuid.LowPart,
                    output,
                ));
            }
            output += 1;
        }
    }
    bail!("no display at monitor handle 0x{monitor:x}, which DXGI enumerates")
}

/// A capture of `source` from the card, or the reason this share stays on the
/// processor.
///
/// Each thing that stops the card's path is worth saying rather than guessing
/// at, because each one leaves the share doing what it has always done: a
/// machine whose hardware setting is off, a display that isn't a screen, and a
/// card NVENC won't open a session on. A screen larger than the stream is no
/// longer one of them: the session that reads the capture's texture scales it
/// to the stream's size on the card.
///
/// The order the two ways are tried in is [`capture::order`]'s, which is the
/// setting's choice or, under Auto, whichever way can be borderless on this
/// build of Windows: Desktop Duplication on a Windows whose Graphics Capture
/// has no interface to take its border off, Graphics Capture where it does. A
/// rung that refuses is named and the other opens; under Auto a Graphics
/// Capture that opens with its border still on is refused the same way, once it
/// has been let go in full, which is the case that used to leave a Windows 10
/// share drawing a yellow outline beside a duplication that would have been
/// clean. Either rung hands the loop the same borrowed BGRA texture on the
/// device its capture made, so everything after this decision is the same.
#[cfg(any(wgc, dup))]
pub(crate) fn card_capture(source: &Source, sink: &Sink, bounds: (u32, u32)) -> Result<Chosen> {
    if !sink.hardware {
        bail!("the stream is set to encode on the processor");
    }
    let preference = capture::preference();
    let platform = capture::platform();
    let available = capture::Available {
        graphics: cfg!(wgc),
        duplication: cfg!(dup),
    };
    let rungs = capture::order(preference, platform, available);
    let mut refused = String::new();
    for (at, &rung) in rungs.iter().enumerate() {
        match open_rung(rung, source, sink, bounds) {
            Ok(card) => {
                let next = rungs.get(at + 1).copied();
                if capture::keeps(preference, rung, card.borderless(), next) {
                    return Ok(Chosen {
                        card,
                        why: chosen_why(preference, platform, available, rung, &refused),
                    });
                }
                // Auto asked for a capture that can be borderless and this one
                // opened with its border still on. It is let go in full before
                // the next rung opens, because a share holds one capture at a
                // time and a duplication can't take the display while a
                // Graphics Capture still has it.
                let why = card.border();
                drop(card);
                refuse(&mut refused, rung, &why);
            }
            Err(why) => refuse(&mut refused, rung, &format!("{why:#}")),
        }
    }
    if refused.is_empty() {
        bail!("this build of Yapfast has no way to capture a screen on the card");
    }
    bail!("{refused}")
}

/// Shares a screen from the card: each frame either capture fills a texture
/// with goes to the encoder as that texture — scaled on the card to the
/// stream's size where the screen is larger — and the bitstream out of the
/// driver is the only time a frame's bytes touch the processor.
///
/// Nothing here keeps a frame. The texture is the capture's own, borrowed until
/// the next [`Card::frame`]; the encoder's buffers come back to it as frames go
/// out; and the one picture held in memory is the preview's tile, scaled on the
/// card and read back at no more than [`PREVIEW_WIDTH`] by [`PREVIEW_HEIGHT`].
#[cfg(any(wgc, dup))]
pub(crate) fn capture_textures(
    chosen: Chosen,
    mut sink: Sink,
    (max_width, max_height): (u32, u32),
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<(Sink, After)> {
    let Chosen {
        card: mut capture,
        why,
    } = chosen;
    let (screen_width, screen_height) = capture.content_size();
    let (sent_width, sent_height) = fit(screen_width, screen_height, max_width, max_height);
    let described = capture.describe();
    tracing::info!(
        rung = capture.rung().name(),
        why = why.as_str(),
        capture = described.as_str(),
        screen = format!("{screen_width}x{screen_height}"),
        stream = format!("{sent_width}x{sent_height}"),
        "sharing from the card"
    );
    let fps = sink.fps;
    sink.footage = Footage::OnTheCard;
    sink.capture = format!(
        "{} · {screen_width}×{screen_height} screen",
        capture.rung().name()
    );
    let started = Instant::now();
    // Only the pace of the loop matters here: a screen that drew nothing is a
    // frame skipped, not one captured twice.
    let mut slot = 0u64;
    let mut reported = None;
    let mut sampler = pressure::Sampler::new();
    // The session a preview's tile is read back through, opened on the capture's
    // device when something first wants a preview and let go when nothing does.
    // A session of its own, because the encoder scales into one texture it keeps
    // for each size, and a tile sharing the stream's session would remake that
    // texture every frame, alternating the stream's size with the tile's. It
    // keeps one of the capture's surfaces rather than a ring: it reads a frame
    // in ten, so there is no rotation to keep up with, and every surface it held
    // would be a full-size one the capture couldn't fill.
    let mut tile: Option<d3d11::D3d11Encoder> = None;
    // The tile's pixels, in the card's order: BGRA, one row after another.
    let mut pixels: Vec<u8> = vec![];
    let mut read: Option<Instant> = None;
    // The picture the UI's window shows where the card drew it, made with the
    // tile's own session: the same scale that fills the tile fills the texture
    // the UI opens, so one session and one destination serve both, and a share
    // that can't be read back can still be shown. `showable` is whether the UI
    // says it can show one at all, which a refusal settles for the stream.
    let mut shared: Option<d3d11::Shared> = None;
    let mut showable = true;
    // How many times the session and the capture have been put back together
    // since a frame last went out.
    let mut restarts = 0u32;
    // Whether a picture has been made since the source or the capture changed,
    // and whether a preview became wanted since one was last made: both are
    // demand a still desktop answers, and neither outlasts the answer.
    let mut saw_picture = false;
    let mut preview_pending = false;
    let mut was_wanted = false;
    while next_frame(started, &mut slot, fps, stop).is_some() {
        // One look per stream slot, paced by `next_frame` on both rungs: a
        // duplication asked with no wait answers at once whether the desktop
        // changed, and a still one makes no frame here and no spin either. A
        // capture that stops — a duplication loses its desktop on a secure
        // screen or a display change — is put back together below with the
        // encoder, exactly as a device the card took away is.
        let mut taken = Ok(());
        let arrival = match capture.frame(Duration::ZERO) {
            Ok(arrival) => arrival,
            Err(error) => {
                taken = Err(error);
                Arrival::Still
            }
        };
        // When the capture answered with a picture, which is what this side's
        // queue delay is measured from.
        if arrival == Arrival::Frame {
            sink.diagnosis.captured_at = Some(Instant::now());
        }
        // Everything the sink has to say about this pass: the controls, which is
        // also where its reports go, whether a preview is wanted, the UI's
        // process, and the frame itself when one arrived. None of it is done
        // under a lock, because there is none: this sink is the loop's own — the
        // other capture path shares its sink with a capturer's callback thread
        // and locks it for that, and a share of the card has no second thread to
        // share with. Sole ownership is what keeps a frame's texture from being
        // read by two threads at once; the compiler holds it to that, and the
        // GPU work below — scale, encode, readback, the picture the UI shows —
        // runs with nothing held at all.
        let switched = sink.check(sampler.read(Instant::now()), report)?;
        let wanted = sink.preview_wanted();
        // A preview that has just been asked for is owed a picture now, even if
        // the screen doesn't change: waiting for the pointer to move is what a
        // tile that came back would otherwise do.
        if wanted && !was_wanted {
            preview_pending = true;
        }
        was_wanted = wanted;
        // The UI's process, which a picture drawn here is shared into: zero
        // until it opens these frames, and a process that has gone is one
        // with nothing to show anything in.
        let ui = sink.writer.ui_process();
        // Whether the capture's own texture still holds a picture of what is
        // shared: a frame put one there, and a resize, a capture put back
        // together or another source ended that. Nothing is made from a
        // picture the texture doesn't hold, so a missing one waits for the
        // capture rather than showing the source before it.
        let mut size = None;
        match arrival {
            Arrival::Frame => {
                saw_picture = true;
                let content = capture.content_size();
                let sent = fit(content.0, content.1, max_width, max_height);
                taken = sink.textured(
                    // SAFETY: the capture's own surface, on the device the capture
                    // holds, alive for as long as the capture is and not filled
                    // again until this frame has been encoded.
                    unsafe { d3d11::Texture::new(capture.texture()) },
                    content,
                    sent,
                    capture.device(),
                );
                size = Some(sent);
            }
            // The screen changed size and the capture's texture with it, so
            // the next frame is at the new size: `ready` opens an encoder
            // for it and tells the sender, and a session's first frame is
            // whole.
            Arrival::Resized => {
                // What the texture held was drawn at the size before it.
                saw_picture = false;
                tracing::info!("the screen changed size");
            }
            // Nothing has been drawn and nothing has moved, which is the
            // desktop's own rate: the next pass is the next slot. It is not a
            // reason for a stream to have nothing to say, though — a viewer
            // joining, a decoder that lost its place, a preview coming back are
            // all served from the picture the capture last made, which is still
            // in the capture's own texture, and only while one of those asks for
            // it. A desktop that is neither moving nor wanted encodes nothing.
            Arrival::Still => {
                // A still is answered from the capture's own texture only when
                // the capture answered with nothing new: a call that failed
                // hands back no texture, and its error is the one to keep.
                if taken.is_ok() && saw_picture && sink.picture_needed() {
                    let content = capture.content_size();
                    let sent = fit(content.0, content.1, max_width, max_height);
                    taken = sink.textured(
                        // SAFETY: as above: the capture's own surface, on the
                        // capture's device, alive for as long as the capture is.
                        unsafe { d3d11::Texture::new(capture.texture()) },
                        content,
                        sent,
                        capture.device(),
                    );
                    size = Some(sent);
                }
            }
            // What was being captured ended. This capture hands out no more
            // frames, so the share ends rather than waits on one that won't
            // come.
            Arrival::Closed => bail!("the capture of the screen ended"),
        }
        if let Some((width, height)) = size
            && reported != size
        {
            reported = size;
            report(Report::Sharing { width, height });
        }
        if let Some(source) = switched {
            // Another screen: another capture, and the session that encodes it
            // and the tile's own were both built on the device the old capture
            // made, so all three are let go before the new one opens — the
            // capture first, because DXGI gives an output to one duplicator at
            // a time, and the encoder before it, because a session reads on
            // the device its capture made. What can't be opened is said, and
            // the share carries on from the processor rather than ending: a
            // stream that stops is worse than one on the processor.
            if on_the_card(&source) {
                tile = None;
                shared = None;
                read = None;
                sink.let_go();
                drop(capture);
                // What the old capture's texture held went with it.
                saw_picture = false;
                preview_pending = false;
                match card_capture(&source, &sink, (max_width, max_height)) {
                    Ok(next) => {
                        let (width, height) = next.card.content_size();
                        let (sent_width, sent_height) = fit(width, height, max_width, max_height);
                        tracing::info!(
                            rung = next.card.rung().name(),
                            capture = next.card.describe().as_str(),
                            screen = format!("{width}x{height}"),
                            stream = format!("{sent_width}x{sent_height}"),
                            "sharing another screen from the card"
                        );
                        sink.capture =
                            format!("{} · {width}×{height} screen", next.card.rung().name());
                        capture = next.card;
                        // The next frame says what is shared now, even at the
                        // size the screen before it was.
                        reported = None;
                        restarts = 0;
                    }
                    Err(why) => {
                        tracing::info!(
                            why = format!("{why:#}"),
                            "switching screens: sharing from the processor"
                        );
                        // The stream goes on from the processor, and the
                        // encoder that read the card's textures goes with the
                        // device the capture made: see `fall_back_to_the_processor`.
                        sink.fall_back_to_the_processor();
                        return Ok((sink, After::Processor(source)));
                    }
                }
            } else {
                // A window or a camera: the card's paths take a display, and
                // the processor's capturer shares one, with the stream going
                // on from here.
                sink.fall_back_to_the_processor();
                return Ok((sink, After::Processor(source)));
            }
        }
        match taken {
            // A session the card took away, which a driver reset is the usual
            // cause of. Both halves are built again, since the device that
            // filled the textures is the one that went, and a card that answers
            // every frame with the same failure is one to stop sharing on.
            Err(error) => {
                restarts = restarts.saturating_add(1);
                if restarts > CARD_RESTARTS {
                    return Err(error.context("the card's session stopped"));
                }
                tracing::warn!(
                    error = format!("{error:#}"),
                    "the card stopped; opening the capture and the encoder again"
                );
                // Nothing that reads the old device may outlive it: the tile's
                // session goes now, and the stream's before the capture that
                // made the device is let go of.
                tile = None;
                shared = None;
                // A session the card took away is one whose picture went with
                // it.
                saw_picture = false;
                preview_pending = false;
                sink.let_go();
                capture = capture.reopen()?;
            }
            Ok(()) => restarts = 0,
        }
        // A preview is the one thing a texture can't be made into: the UI's tile
        // and Discord's thumbnail are pictures in memory, and this frame is on
        // the card. So the frame is scaled on the card to fit the tile and read
        // back into the pixels kept for it, ten times a second at most and only
        // while something shows one.
        match (wanted, tile.is_some()) {
            (true, false) => {
                let content = capture.content_size();
                let preview = preview_size(content.0, content.1, sink.constrained());
                match d3d11::D3d11Encoder::open_on(
                    capture.device(),
                    stream_connection::SEND_CODEC,
                    preview.0,
                    preview.1,
                    fps,
                    kbps(preview.0, preview.1, fps),
                    d3d11::Surfaces::One,
                ) {
                    Ok(opened) => {
                        // The picture the UI's window shows where it lies is made
                        // with this session and at the tile's own size: the scale
                        // that fills the tile fills it, so the stream holds a
                        // texture rather than a frame either way.
                        if showable && ui != 0 {
                            match d3d11::Shared::new(&opened, preview.0, preview.1, ui) {
                                Ok(made) => shared = Some(made),
                                Err(error) => {
                                    tracing::info!(
                                        error = format!("{error:#}"),
                                        "showing the pixels of a stream: the card won't share a picture"
                                    );
                                    showable = false;
                                    sink.diagnosis.fallback =
                                        Some("the card won't share a picture".to_owned());
                                }
                            }
                        }
                        tile = Some(opened);
                        read = Some(Instant::now());
                        // A session that has just opened has no picture of its
                        // own yet, so the one the capture made is what it is
                        // owed.
                        preview_pending = true;
                    }
                    Err(error) => tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without a preview: the card won't open a session for one"
                    ),
                }
            }
            // Nobody is looking at one and none is due, so it goes, and its
            // session goes with it.
            (false, true) => {
                tile = None;
                shared = None;
                read = None;
            }
            _ => {}
        }
        // A preview read back into memory is what a window that can't show the
        // card's own picture shows, and what Discord's still is made from.
        // While the picture itself is what the window shows, neither is
        // written: one frame is published once, rather than the tile
        // alternating between a texture and a smaller copy of it ten times a
        // second. A still desktop has a picture to read back too, whether a
        // preview has just come back, a session has just opened for one, or
        // Discord's still is due: those are what ask for it, and once each.
        let read_back = shared.is_none() || sink.still_wanted();
        let mut previewed = false;
        if let Some(tile) = &mut tile
            && saw_picture
            && (arrival == Arrival::Frame || preview_pending || sink.still_wanted())
            && read_back
            && read.is_none_or(|last| last.elapsed() >= PREVIEW_EVERY)
        {
            read = Some(Instant::now());
            let taken = sink.preview_from_card(
                tile,
                // SAFETY: the capture's own surface, on the capture's device,
                // alive for as long as the capture is: the readback is done
                // before this frame returns.
                unsafe { d3d11::Texture::new(capture.texture()) },
                capture.content_size(),
                &mut pixels,
            );
            match taken {
                Ok(()) => previewed = true,
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "sharing without a preview")
                }
            }
        }
        // The picture the UI's window shows where the card drew it: the tile's
        // own size, scaled on the card into a texture both processes hold, so a
        // window drawn on the GPU shows it without a copy of anything.
        //
        // The frames file's slot decides whether this frame has a picture at
        // all: a slot is only drawn into once the window showing it has moved
        // past what was there, so a picture a person is looking at is never
        // written over. A frame whose slot is still being shown keeps its
        // picture a frame longer — the stream itself is unaffected, since it
        // encodes the capture's own texture. A window that stops saying it is
        // done with pictures is one the writer stops asking: the tile's pixels
        // go out from the frame after that, which is a preview going on rather
        // than a tile frozen on the last picture it took.
        let mut withdrawn = false;
        if let (Some(tile), Some(shared)) = (tile.as_mut(), shared.as_mut())
            && saw_picture
            && (arrival == Arrival::Frame || preview_pending)
        {
            match sink.writer.claim() {
                Some(slot) => {
                    let taken = sink.share_from_card(
                        slot,
                        tile,
                        shared,
                        // SAFETY: the capture's own surface, on the capture's
                        // device, alive for as long as the capture is: the copy
                        // into the shared picture is queued before this frame
                        // returns.
                        unsafe { d3d11::Texture::new(capture.texture()) },
                    );
                    match taken {
                        Ok(true) => previewed = true,
                        // The UI looked and can't show a picture from this card,
                        // or its window draws on another one: the tile's pixels
                        // are what it shows, and the share it never opened is
                        // let go of.
                        Ok(false) => withdrawn = true,
                        Err(error) => {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "sharing without the picture drawn on the card"
                            );
                            withdrawn = true;
                        }
                    }
                }
                // The slot the writer would draw in is one nothing has
                // finished with for a second: the share stops drawing there
                // and the tile's pixels go out instead.
                None if sink.writer.given_up() => withdrawn = true,
                // A slot the window is still showing, which the frame after
                // this one asks for again.
                None => {}
            }
        }
        if withdrawn {
            showable = false;
            shared = None;
            // The tile is a copy of the picture; the stream's own path is
            // untouched, so the report's fallback stays clear for it: what the
            // window shows is the presentation's to say.
            sink.diagnosis.presentation = Some(Presentation::Copy);
        }
        // What the window does with the pictures is the one thing only the two
        // processes together know: a picture the window took is shown from the
        // card's own memory, and anything else is a copy of the pixels.
        if previewed && let Some(shared) = shared.as_ref() {
            sink.diagnosis.presentation = Some(Presentation::Shared);
            sink.diagnosis.gpu_luid = Some(shared.luid());
        }
        // The preview that was owed has been made, whichever way it went out.
        if previewed {
            preview_pending = false;
        }
    }
    Ok((sink, After::Ended))
}
