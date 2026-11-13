//! `stream_worker`'s linux.

#[allow(unused_imports)]
use super::*;

/// One way a Linux share captures a screen as the DMA-BUFs the compositor drew,
/// as the loop above it asks one: what the next picture is, and what the capture
/// is.
///
/// The two captures differ in where their pictures come from and not in what they
/// hand over — a descriptor, its layout, and the size it is — so the loop, the
/// encoder and the window are the same either way. A capture is a boxed trait
/// object rather than an enum because a test has no portal and no X server to
/// open, and the loop against a capture is the half worth checking.
#[cfg(target_os = "linux")]
pub(crate) trait Pictures {
    /// Waits up to `wait` for the picture to change, and answers with what
    /// arrived: the capture's own wait is the share's pace, so a still desktop
    /// sleeps here rather than spinning beside the loop.
    fn frame(&mut self, wait: Duration) -> Result<Found>;

    /// What this capture is, in one line, for the log that says which ran.
    fn describe(&self) -> String;

    /// Why the share ended, in the capture's own words: empty while it still
    /// runs, and the sentence [`Found::Closed`] is reported with.
    fn ending(&self) -> String;
}

/// What asking a Linux capture for its next picture found, in the terms the loop
/// works in: both captures answer these four, whatever they call them.
#[cfg(target_os = "linux")]
pub(crate) enum Found {
    /// A picture, which this share keeps: see [`Shot`].
    Picture(Shot),
    /// Nothing has been drawn since the last picture, which is not a failure and
    /// makes no frame of its own.
    Still,
    /// What is captured changed size: whatever encodes the frames was built for
    /// the old one, and the next picture is at the new.
    Resized,
    /// The desktop stopped sharing: the person ended it, the window closed, or
    /// the session went away. The capture hands out nothing more.
    Closed,
}

/// What became of a picture the desktop captured as a DMA-BUF.
#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    /// The stream has it.
    Sent,
    /// It didn't reach the encoder this pass — no stream yet, a full send queue,
    /// a codec Discord withdrew — and the next picture may.
    Held,
    /// It can't reach the encoder at all, and it can't be read back on the
    /// processor either: the reason, in the words of the read that failed. A
    /// share of these pictures goes on from the capturer that has pixels.
    Processor(String),
}

/// A picture the desktop captured, as the loop hands it on: the compositor's own
/// DMA-BUF, in a descriptor this share keeps rather than the capture that handed
/// it over.
///
/// No copy of the picture is made here. The layout is kept beside the descriptor
/// so that one can be built from it whenever it is wanted — once for the window,
/// once for the encoder, and once more for Discord's still — and the descriptor
/// is the picture itself, which the window imports and draws where it lies.
///
/// The fd is this share's own, duplicated from the capture's, because a picture
/// outlives the call that gave it: a desktop that hasn't drawn anything since is
/// still a stream a viewer joining can be shown, and the picture it is shown is
/// the one the capture handed over before.
#[cfg(target_os = "linux")]
pub(crate) struct Shot {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The one plane and the descriptor it is in: a compositor's picture is one
    /// buffer, in one of the two four-byte orders a desktop is drawn in.
    pub(crate) planes: [dmabuf::Plane; 1],
    pub(crate) fds: [std::os::fd::RawFd; 1],
    /// The fd `fds` names, closed when this share lets go of the picture.
    pub(crate) memory: std::os::fd::OwnedFd,
    /// The DRM modifier the driver laid the memory out with, when the capture
    /// could say: `None` from a DRI3 server older than modifiers telling anyone,
    /// which is read as the linear layout such a server draws under.
    pub(crate) modifier: Option<u64>,
    pub(crate) order: Order,
}

#[cfg(target_os = "linux")]
impl Shot {
    /// The picture as the window and the encoder are handed it.
    pub(crate) fn descriptor(&self) -> dmabuf::Descriptor<'_> {
        dmabuf::Descriptor {
            width: self.width,
            height: self.height,
            fourcc: self.order.fourcc(),
            modifier: self.modifier.unwrap_or(0),
            planes: &self.planes,
            colour: dmabuf::Colour::SDR,
            fds: &self.fds,
        }
    }

    /// The picture's memory, mapped for reading.
    pub(crate) fn read(&self) -> Result<linear::Readback> {
        linear::read(
            self.memory.as_fd(),
            self.width,
            self.height,
            self.planes[0].offset,
            self.planes[0].stride,
            self.modifier,
        )
    }

    /// The same picture as the encoder reads it when it reads memory, at the size
    /// the stream sends, `sent`: converted into the buffer kept for it, and scaled
    /// to `sent` where the screen is larger than the stream.
    ///
    /// This is the copy [`Sink::dmabuf`] makes of a frame while the encoder can't
    /// take a descriptor, and it is made of a linear picture only: a tiled one is
    /// refused by name rather than read as noise.
    pub(crate) fn converted<'a>(
        &self,
        scratch: &'a mut Scratch,
        sent: (u32, u32),
    ) -> Result<&'a I420Buffer> {
        let read = self.read()?;
        let full = buffer(&mut scratch.full, self.width, self.height);
        convert_ordered(full, read.rows(), read.stride(), self.order);
        if (self.width, self.height) == sent {
            return Ok(full);
        }
        let scaled = buffer(&mut scratch.shared, sent.0, sent.1);
        scale_into(&planes(full), scaled, sent.0, sent.1);
        Ok(scaled)
    }

    /// The picture as the still is made from, in the buffer the stream's scale
    /// left: Discord's thumbnail is the whole picture scaled down to fit its own
    /// box, so this is the conversion without the scale to the stream's size.
    pub(crate) fn still(&self, scratch: &mut Scratch) -> Result<()> {
        let read = self.read()?;
        let full = buffer(&mut scratch.full, self.width, self.height);
        convert_ordered(full, read.rows(), read.stride(), self.order);
        Ok(())
    }

    /// The shot a capture hands over, with the fd duplicated out of the capture
    /// that owns it.
    pub(crate) fn held(
        width: u32,
        height: u32,
        offset: u32,
        stride: u32,
        modifier: Option<u64>,
        order: Order,
        memory: BorrowedFd<'_>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!("the desktop shared a {width}x{height} picture, which is nothing to send");
        }
        let memory = memory
            .try_clone_to_owned()
            .context("the desktop's picture couldn't be kept past the frame that brought it")?;
        Ok(Self {
            width,
            height,
            planes: [dmabuf::Plane { offset, stride }],
            fds: [memory.as_raw_fd()],
            memory,
            modifier,
            order,
        })
    }
}

/// The screen sharing portal's capture, as the loop above it asks it.
#[cfg(screencast)]
pub(crate) struct Portal(capture::pipewire::Capture);

#[cfg(screencast)]
impl Pictures for Portal {
    fn frame(&mut self, wait: Duration) -> Result<Found> {
        use capture::pipewire::{Arrival, Format};
        Ok(match self.0.frame(wait)? {
            Arrival::Frame(picture) => {
                let order = match picture.format() {
                    // A compositor draws a desktop into one of these two, and the
                    // other two are the same bytes with a fourth nothing reads.
                    Format::Bgra | Format::Bgrx => Order::Bgra,
                    Format::Rgba | Format::Rgbx => Order::Rgba,
                    other => bail!(
                        "the desktop shares its screen as {}, which is no picture a stream can read",
                        other.name()
                    ),
                };
                let (width, height) = picture.size();
                Found::Picture(Shot::held(
                    width,
                    height,
                    picture.offset(),
                    picture.stride(),
                    Some(picture.modifier()),
                    order,
                    picture.fd(),
                )?)
            }
            Arrival::Still => Found::Still,
            Arrival::Resized => Found::Resized,
            Arrival::Closed => Found::Closed,
        })
    }

    fn describe(&self) -> String {
        self.0.describe()
    }

    fn ending(&self) -> String {
        self.0.ending()
    }
}

/// The X server's capture, as the loop above it asks it.
#[cfg(target_os = "linux")]
pub(crate) struct Server(capture::x11::Capture);

#[cfg(target_os = "linux")]
impl Pictures for Server {
    fn frame(&mut self, wait: Duration) -> Result<Found> {
        use capture::x11::Arrival;
        Ok(match self.0.frame(wait)? {
            Arrival::Frame(picture) => {
                let buffer = picture.buffer();
                // A monitor is the screen's picture cut to its part of it, so the
                // picture starts later in the memory than the buffer does.
                let region = buffer.region;
                let stride = buffer.stride;
                Found::Picture(Shot::held(
                    region.width,
                    region.height,
                    buffer.offset + region.y * stride + region.x * Order::Bgra.bytes(),
                    stride,
                    buffer.modifier,
                    // The one depth and bpp a capture here hands on is 24 and 32,
                    // which is B, G, R and a byte nothing reads: see `x11`.
                    Order::Bgra,
                    buffer.fd(),
                )?)
            }
            Arrival::Still => Found::Still,
            Arrival::Resized => Found::Resized,
            Arrival::Closed => Found::Closed,
        })
    }

    fn describe(&self) -> String {
        self.0.describe()
    }

    fn ending(&self) -> String {
        self.0.ending().to_owned()
    }
}

/// A capture that opened, and the rung it is: the sentence saying why it is the
/// one sharing, and the name it goes in the share's report under.
#[cfg(target_os = "linux")]
pub(crate) struct Opened {
    pub(crate) capture: Box<dyn Pictures>,
    pub(crate) rung: capture::LinuxRung,
    pub(crate) why: String,
}

/// Whether a Linux capture can take `source` at all: the portal and the X server
/// both take a window, a display or the screen, where a camera and a test
/// pattern are the processor's or nobody's.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn dmabuf_takes(source: &Source) -> bool {
    match source {
        Source::Screen | Source::Display(_) => true,
        // Both rungs capture a window as that window: the portal asks its dialog
        // for windows, and the X server redirects the window's own storage, never
        // the desktop behind it.
        Source::Window(_) => true,
        Source::Camera(_) | Source::TestPattern => false,
    }
}

/// What a Linux share can capture with, as the rungs it is about to try.
#[cfg(target_os = "linux")]
pub(crate) fn available() -> capture::Linux {
    capture::Linux {
        // A build without the `screencast` feature carries no capture of the
        // portal's stream at all, and asking a bus for a portal this build can't
        // read is worse than not asking.
        #[cfg(screencast)]
        portal: capture::pipewire::answers(),
        #[cfg(not(screencast))]
        portal: false,
        // Whether an X server is named at all. Whether it exports a redirected
        // pixmap through DRI3 is the opening capture's answer, and a server that
        // doesn't is a rung that refused.
        x11: std::env::var_os("DISPLAY").is_some(),
    }
}

/// Whether the window shows a picture the worker publishes as a DMA-BUF, which is
/// the whole point of capturing this way: [`Writer::dmabuf_showable`] answers
/// with the device the window imports one on, and `None` while it hasn't looked,
/// while it refused them, or while it has no importer at all.
///
/// Asked once per share, before a capture opens: a window's answer is settled for
/// the stream. A picture it can't show is a share that stays on the processor,
/// where its frames have always been pixels it can be given.
#[cfg(target_os = "linux")]
pub(crate) fn showable(writer: &mut Writer) -> bool {
    writer.dmabuf_showable().is_some()
}

/// Whether this build has an encoder that reads a descriptor where it lies: the
/// VA-API import, or the CUDA rung NVIDIA's cards need. The question is the
/// encoder's, not the window's — a share whose tile is drawn on the processor
/// still sends from the card, with its preview read back instead of shown.
#[cfg(target_os = "linux")]
pub(crate) fn encodes_descriptors() -> bool {
    #[cfg(any(dmabufenc, nvcuda))]
    {
        true
    }
    #[cfg(not(any(dmabufenc, nvcuda)))]
    {
        false
    }
}

/// Opens `rung`'s capture of `source`, whose pictures are the DMA-BUFs the
/// compositor drew.
#[cfg(target_os = "linux")]
pub(crate) fn open_rung(rung: capture::LinuxRung, source: &Source) -> Result<Box<dyn Pictures>> {
    match rung {
        #[cfg(screencast)]
        capture::LinuxRung::Portal => {
            // Which kind of thing to share is the one thing the person's pick
            // says to the dialog: a window is never asked for as the screen it
            // sits on, and a display is one monitor rather than every screen.
            // The id itself is Yapfast's own list's, which the portal has no way
            // of being told, so the dialog picks inside that kind.
            let asked = match source {
                Source::Window(_) => capture::pipewire::Source::Windows,
                Source::Display(_) => capture::pipewire::Source::Monitors,
                _ => capture::pipewire::Source::Both,
            };
            Ok(Box::new(Portal(capture::pipewire::Capture::open(asked)?)))
        }
        #[cfg(not(screencast))]
        capture::LinuxRung::Portal => bail!(
            "this build has no capture of the portal's stream: the `screencast` feature and \
             PipeWire's headers are what build one"
        ),
        capture::LinuxRung::X11 => Ok(Box::new(Server(capture::x11::Capture::open(x11_target(
            source,
        )?)?))),
    }
}

/// Which of the X server's drawables `source` names, following the rule the rest
/// of the share follows: a window is that window, a display is one monitor, and
/// the screen is the desktop.
///
/// The id a [`Source::Display`] carries is the one Yapfast's menu listed the
/// screen by, which on X11 is libwebrtc's own: the root window's id for the
/// screen, or a screen's place in its list. Both are answered here — the root id
/// as the screen, and a place as the monitor the list would show there — and an
/// id that is neither is refused rather than captured as something else.
#[cfg(target_os = "linux")]
pub(crate) fn x11_target(source: &Source) -> Result<capture::x11::Target> {
    use capture::x11::Target;
    match source {
        Source::Screen => Ok(Target::Screen),
        Source::Window(window) => Ok(Target::Window(
            u32::try_from(*window)
                .with_context(|| format!("0x{window:x} is no X window: an id there is 32 bits"))?,
        )),
        Source::Display(id) => {
            let root = capture::x11::root_window()?;
            let monitors: Vec<capture::x11::Display> = capture::x11::listed()?
                .into_iter()
                .filter_map(|listed| match listed.target {
                    Target::Display(display) => Some(display),
                    Target::Window(_) | Target::Screen => None,
                })
                .collect();
            screen_target(*id, root, &monitors)
        }
        Source::Camera(_) => bail!("a camera isn't a screen"),
        Source::TestPattern => bail!("a test pattern is drawn, not captured"),
    }
}

/// The target a listed screen's id names, given the server's root window and the
/// monitors it has: the whole screen for the root's own id, the monitor at that
/// place for a place, and nothing for anything else.
///
/// This is the whole of the decision [`x11_target`] makes, kept apart from the
/// server it asks so that it can be checked without one.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn screen_target(
    id: u64,
    root: u32,
    monitors: &[capture::x11::Display],
) -> Result<capture::x11::Target> {
    if id == u64::from(root) {
        return Ok(capture::x11::Target::Screen);
    }
    usize::try_from(id)
        .ok()
        .and_then(|place| monitors.get(place).cloned())
        .map(capture::x11::Target::Display)
        .with_context(|| {
            format!(
                "the X server has no screen {id:#x} and no monitor at place {id}, where it has {}",
                monitors.len()
            )
        })
}

/// A capture of `source` whose pictures are the compositor's DMA-BUFs, or the
/// reason this share stays on the processor.
///
/// The rungs are tried in [`capture::linux_order`]'s order — the portal where it
/// answers, and the X server after it — and each that refuses is named with what
/// it said. Two things are asked before any of them opens, because either leaves
/// the share with nothing to do: whether the window can show a picture the worker
/// published as a DMA-BUF at all, and whether this source is one the card's paths
/// take.
#[cfg(target_os = "linux")]
pub(crate) fn dmabuf_capture(source: &Source, sink: &mut Sink) -> Result<Opened> {
    if !dmabuf_takes(source) {
        bail!("this source has no picture the card made");
    }
    if !showable(&mut sink.writer) && !encodes_descriptors() {
        bail!("nothing here can read a picture the card made");
    }
    let session = capture::Session::new(portal_picks());
    let available = available();
    let mut refused = String::new();
    for rung in capture::linux_order(session, available) {
        match open_rung(rung, source) {
            Ok(capture) => {
                let why = capture::linux_why(session, available, rung, &refused);
                return Ok(Opened { capture, rung, why });
            }
            Err(why) => capture::refused(&mut refused, rung, &format!("{why:#}")),
        }
    }
    if refused.is_empty() {
        bail!("this session has no way to capture a screen on the card");
    }
    bail!("{refused}")
}

/// Whether a pass of a share whose pictures are DMA-BUFs has a picture to send:
/// the one that has just arrived, or — for a still desktop — the one the capture
/// last handed over, which a viewer joining, a decoder that lost its place or a
/// codec Discord chose can be answered from.
///
/// A still picture is sent only while the stream asks for one. A desktop that
/// neither moves nor is wanted sends nothing at all, which is what keeps a share
/// of a screen nobody is touching from encoding the same picture for ever.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn sending(fresh: bool, saw_picture: bool, needed: bool) -> bool {
    fresh || (saw_picture && needed)
}

/// Whether a pass publishes a picture for the window, which is what makes it the
/// tile the person sees: only while something shows one, and only while the
/// window hasn't been given it.
///
/// A picture published is the picture the window keeps showing until another
/// slot replaces it, and publishing the same one again costs a message, a
/// descriptor and an import for a picture already on the screen. So one is owed
/// when a preview is first wanted and when the share has just moved to another
/// picture; a still desktop owes none.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn publishing(wanted: bool, owed: bool) -> bool {
    wanted && owed
}

/// Shares a screen or a window the desktop captured as the DMA-BUFs its
/// compositor drew: each picture goes to the window as the descriptor it already
/// is, which the window imports and draws where it lies, and to the encoder as
/// the same descriptor — or as the same picture converted on the processor, when
/// the encoder reads memory and no import of one is built; see [`Sink::dmabuf`].
///
/// Nothing here keeps a picture. The descriptor is the capture's own, borrowed
/// until its next frame and kept only as the one duplicate of an fd that a still
/// desktop is answered from; the one picture in this process's memory is Discord's
/// still, read back when it is due and scaled down to a thumbnail.
///
/// A capture that ends ends the share — the person stopped it, the window closed
/// — and one that can't give a picture the encoder can take is a share that
/// carries on from the processor's capturer rather than ending: a stream that
/// stops is worse than one that costs a copy.
#[cfg(target_os = "linux")]
pub(crate) fn capture_dmabufs(
    opened: Opened,
    mut source: Source,
    mut sink: Sink,
    (max_width, max_height): (u32, u32),
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<(Sink, After)> {
    let Opened {
        mut capture,
        mut rung,
        why,
    } = opened;
    tracing::info!(
        rung = rung.name(),
        why = why.as_str(),
        capture = capture.describe().as_str(),
        "sharing from the card"
    );
    sink.footage = Footage::Dmabuf;
    sink.diagnosis.presentation = Some(Presentation::Shared);
    // The display's own word about the screen these pictures come from, which is
    // what the card's kernels read their codes with: a screen showing HDR hands
    // its frames over as HDR's, and one showing SDR hands over what it looks
    // like, so its pictures are converted exactly as they were. Asked here rather
    // than per frame, and asked again below when another screen is opened.
    let mut hdr = super::processor::tone_of(&source).is_hdr();
    let started = Instant::now();
    // Only the pace of the loop matters here: the capture is asked for a picture
    // until the next slot is due, and its own wait is where a still desktop spends
    // the time. Nothing here sleeps beside it and nothing spins. A capture answers
    // as soon as the desktop draws, which a busy screen does at its refresh rate,
    // so a picture that comes before its slot is kept and the next one asked for:
    // the newest is sent when the slot comes. Sending each as it came encoded a
    // 30 fps stream at the compositor's rate, filled the send queue, and had the
    // encoder overshoot a rate it shares out over the stream's frame rate.
    let mut slot = 0u64;
    // Whether the picture in `last` arrived since one was last sent.
    let mut fresh = false;
    // What the stream is sent at, and what the window has been told: both change
    // when the shared thing does.
    let mut size: Option<(u32, u32)> = None;
    let mut sampler = pressure::Sampler::new();
    // The picture the capture last handed over, which a still desktop is answered
    // from when a viewer asks for one, and which is dropped when the size changes
    // under it.
    let mut last: Option<Shot> = None;
    // Whether a picture has been made since the source or the capture changed,
    // and whether a preview became wanted since one was last published: both are
    // demand a still desktop answers, and neither outlasts the answer.
    let mut saw_picture = false;
    let mut preview_pending = false;
    let mut was_wanted = false;
    // Whether the tile's preview is read back, because the window draws on the
    // processor and can't sample the picture the card made: decided once, on the
    // first picture the window refused.
    // Whether a read-back preview failed already, which is said once.
    let mut preview_refused = false;
    let mut readback = false;
    // The buffers a picture is converted in, when the encoder reads memory: one
    // for the picture and one for the stream's size, made again only when a
    // picture comes at another size.
    let mut scratch = Scratch::default();
    // When the capture's account of itself was last logged while it had still
    // produced nothing: a stream that negotiates nothing waits here, and the line
    // the share opens with is a snapshot taken before that wait began.
    let mut waited: Option<Instant> = None;
    // When the desktop last drew something, and whether the share has said that
    // it stopped drawing: a screen nothing is drawing to sends no pictures, so a
    // stream that goes quiet and a preview that stops following it are the
    // desktop's doing rather than the share's, and worth one line each way.
    let mut drew_at: Option<Instant> = None;
    let mut idle_said = false;
    while !stop.load(Ordering::Relaxed) {
        // The next slot and how long it is away. The capture waits that long for
        // the desktop to change, so a still screen costs one wake-up a slot and
        // no spin at all.
        let (due, wait) = pace(started, slot, sink.fps, Instant::now());
        let deadline = Instant::now() + wait.unwrap_or_default();
        let arrival = capture.frame(wait.unwrap_or_default())?;
        // Answered before the slot is due: nothing is sent yet, and the slot
        // stays the one being waited for.
        let early = Instant::now() < deadline;
        if !early {
            slot = due.saturating_add(1);
        }
        // Told to stop while the capture was waiting: the capture goes with this
        // frame, which closes the portal's session and lets go of the X server's
        // redirection.
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Everything the sink has to say about this pass: the controls, which is
        // also where its reports go, and whether a preview is wanted. None of it
        // needs the picture.
        let switched = sink.check(sampler.read(Instant::now()), report)?;
        let wanted = sink.preview_wanted();
        if wanted && !was_wanted {
            preview_pending = true;
        }
        was_wanted = wanted;
        if let Some(next) = switched {
            // Another screen or window. The descriptor of the one before belongs
            // to the capture that made it, so the picture goes with it, and the
            // sink keeps its encoder: nothing on this path read on a device the
            // capture made. What can't be opened is said, and the share carries on
            // from the processor rather than ending.
            match dmabuf_capture(&next, &mut sink) {
                Ok(opened) => {
                    tracing::info!(
                        rung = opened.rung.name(),
                        why = opened.why.as_str(),
                        capture = opened.capture.describe().as_str(),
                        "sharing another screen from the card"
                    );
                    capture = opened.capture;
                    rung = opened.rung;
                    source = next;
                    // The screen that is shared now, which may be showing HDR
                    // where the one before it was not.
                    hdr = super::processor::tone_of(&source).is_hdr();
                    // The next picture says what is shared now, even at the size
                    // the screen before it was.
                    size = None;
                    saw_picture = false;
                    last = None;
                    fresh = false;
                    preview_pending = true;
                }
                Err(why) => {
                    tracing::info!(
                        why = format!("{why:#}"),
                        "switching screens: sharing from the processor"
                    );
                    sink.fall_back_to_the_processor();
                    return Ok((sink, After::Processor(next)));
                }
            }
        }
        match arrival {
            Found::Picture(shot) => {
                let now = Instant::now();
                sink.diagnosis.captured_at = Some(now);
                if idle_said {
                    idle_said = false;
                    tracing::info!(
                        still_for_ms =
                            drew_at.map_or(0, |at| now.duration_since(at).as_millis() as u64),
                        "the desktop is drawing again"
                    );
                }
                drew_at = Some(now);
                saw_picture = true;
                last = Some(shot);
                fresh = true;
                if early {
                    continue;
                }
            }
            Found::Resized => {
                // What the capture held was drawn at the size before it, and the
                // next picture is at the new one. Nothing is sent of a picture of
                // the old size under the new one's numbers: the next picture
                // opens the encoder at its own size and tells the sender.
                tracing::info!("the shared picture changed size");
                saw_picture = false;
                last = None;
                fresh = false;
                size = None;
                preview_pending = true;
                continue;
            }
            Found::Still => {
                // Nothing has come yet: what the capture is waiting for, now and
                // then, rather than only in the line this share opened with. A
                // portal stream that never negotiates says which state it is in
                // and what the producer answered, which is otherwise a share that
                // shows nothing with no reason anywhere.
                if !saw_picture && waited.is_none_or(|last| last.elapsed() >= WAITING_EVERY) {
                    waited = Some(Instant::now());
                    tracing::info!(
                        capture = capture.describe().as_str(),
                        "the share has no picture yet"
                    );
                }
                // A screen that has drawn something and then stops sends nothing:
                // the compositor draws a frame when it has one, and a share's
                // pictures are the compositor's. Said once, so a quiet stream is
                // known to be a quiet desktop rather than a share that stalled,
                // and the tile is known to hold its last picture on purpose.
                if saw_picture && !idle_said && drew_at.is_some_and(|at| at.elapsed() >= IDLE_EVERY)
                {
                    idle_said = true;
                    tracing::info!(
                        still_for_ms = drew_at.map(|at| at.elapsed().as_millis() as u64),
                        "the desktop has drawn nothing for a while: the share keeps its last picture, and the stream sends nothing until it draws"
                    );
                }
                // Nothing has been drawn since the last picture. It is still a
                // stream a viewer joining, a decoder that lost its place or a
                // preview coming back can be shown — from the picture the capture
                // last handed over, and only while one of those asks for it.
                // Woken before the slot, as for a format: the slot is still to
                // come, and with it whatever arrives meanwhile.
                if early {
                    continue;
                }
                if !sending(fresh, saw_picture, sink.picture_needed()) {
                    continue;
                }
            }
            Found::Closed => {
                let why = capture.ending();
                bail!(if why.is_empty() {
                    "the screen this share captured ended".to_owned()
                } else {
                    why
                })
            }
        }
        let Some(shot) = last.as_ref() else {
            continue;
        };
        // Whether this pass has a picture that has just arrived, which the stream
        // sends and the window is shown: both are answered once, and the two are
        // not the same — a still desktop is one the window can be shown again
        // without the stream sending it.
        let arrived = fresh;
        fresh = false;
        let sent = fit(shot.width, shot.height, max_width, max_height);
        if size != Some(sent) {
            size = Some(sent);
            sink.capture = format!("{} · {}×{}", rung.name(), sent.0, sent.1);
            report(Report::Sharing {
                width: sent.0,
                height: sent.1,
            });
        }
        match sink.dmabuf(shot, sent, &mut scratch, hdr)? {
            Taken::Sent => {}
            Taken::Held => continue,
            Taken::Processor(why) => {
                tracing::info!(
                    why = why.as_str(),
                    "sharing from the processor: a picture nothing here can read"
                );
                sink.fall_back_to_the_processor();
                // The reason this share left the card is this one, and not the
                // sentence the fallback says on its own.
                sink.diagnosis.fallback = Some(why);
                return Ok((sink, After::Processor(source)));
            }
        }
        // Discord's still, and only that: a screen is black until what is shared
        // draws, so the picture is looked at before one is made, and it is made
        // from the frame the share has just sent. This is the one picture a share
        // of DMA-BUFs copies.
        if sink.still_wanted() {
            // A share the card encodes has the picture it just sent on the card
            // already, at the stream's size, and a tiled screen has no rows the
            // processor could read it from instead.
            #[cfg(nvcuda)]
            let still = sink
                .card_still(&mut scratch)
                .unwrap_or_else(|| shot.still(&mut scratch));
            #[cfg(not(nvcuda))]
            let still = shot.still(&mut scratch);
            match still {
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without Discord's still: the picture couldn't be read back"
                    );
                    // Asked again in a while, not on every frame.
                    sink.preview_due = Some(Instant::now() + STILL_RETRY);
                }
                Ok(()) => {
                    if let Some(full) = scratch.full.as_ref() {
                        let shown = planes(full);
                        if !is_black(shown.y) {
                            match thumbnail(&shown) {
                                Ok(jpeg) => sink.thumbnail = Some(jpeg),
                                Err(error) => tracing::warn!(
                                    error = format!("{error:#}"),
                                    "could not make a preview"
                                ),
                            }
                            sink.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
                        }
                    }
                }
            }
        }
        // The picture the window shows where the compositor drew it: the tile is
        // the descriptor itself, published where a slot is free and only while
        // something shows one. Every picture that arrives is published rather
        // than the one that was there when the preview came up: the capture's
        // buffers cycle through the compositor's pool, so a tile left on one of
        // them shows what that buffer held the last time it came round, and the
        // window redraws only when it is handed another. A still desktop
        // publishes nothing, the buffer the window has keeping what it shows. A
        // window that can't sample one after all — its socket has gone, or it
        // stopped retiring slots — leaves the share on the processor, which is
        // where its pixels are.
        if publishing(wanted, preview_pending || arrived)
            && !readback
            && let Some(_slot) = sink.writer.claim()
        {
            if sink.writer.publish_dmabuf(&shot.descriptor()) {
                sink.diagnosis.presentation = Some(Presentation::Shared);
                preview_pending = false;
            } else if encodes_descriptors() {
                // The window draws on the processor and can't sample a descriptor,
                // so its preview is read back — the same copy Discord's still is
                // made of — while the encoder goes on reading the card's own
                // picture. Said once, since it holds for the rest of the share.
                tracing::info!(
                    "the window draws on the processor: its preview is a copy of the picture, and \
                     the stream is still the card's own"
                );
                readback = true;
                // The tile is a copy; the stream is not. The report's
                // presentation says what the window shows, and the fallback is
                // reserved for the stream's own path — a share whose pictures
                // the card encodes would otherwise read as one the processor
                // took over.
                sink.diagnosis.presentation = Some(Presentation::Copy);
            } else {
                tracing::info!(
                    "sharing from the processor: the window can't show the card's picture"
                );
                sink.fall_back_to_the_processor();
                sink.diagnosis.fallback =
                    Some("the window can't show the picture the card drew".to_owned());
                return Ok((sink, After::Processor(source)));
            }
        }
        // A read-back preview is made only while a tile shows one, and only of
        // the picture the window can't sample: one copy of the screen a preview
        // frame, which is what a software renderer costs a share.
        if readback && wanted {
            // A picture the card's own path took through GL's door — a tiled
            // screen, which has no rows to read — is scaled on the card and only
            // the preview comes back; one with rows is read by the processor.
            #[cfg(nvcuda)]
            let read = match sink.card_preview(&mut scratch) {
                Some(Ok(())) => Ok(()),
                _ => shot.still(&mut scratch),
            };
            #[cfg(not(nvcuda))]
            let read = shot.still(&mut scratch);
            if let Err(error) = read {
                // Once a share: a picture that can't be read back now won't be
                // on the next frame either, and this is asked at frame rate.
                if !preview_refused {
                    preview_refused = true;
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without a preview: the picture couldn't be read back"
                    );
                }
            } else if let Some(full) = scratch.full.as_ref() {
                sink.preview(full);
                preview_pending = false;
            }
        }
    }
    Ok((sink, After::Ended))
}
