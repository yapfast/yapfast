//! A window or the screen on X11 as a DMA-BUF the card can read, for an X11
//! session: XFCE's, and every other X11 desktop that captures the X server's own
//! windows. It is the rung after the portal's where the desktop has one, and the
//! only one where it doesn't. A Wayland session never asks it, since an X server
//! there is Xwayland; see [`super::linux_order`].
//!
//! Without this, such a session shares through libwebrtc's X11 capturer, which
//! asks the server for the pixels of every frame and hands over I420 that the
//! processor assembled. This path asks for no pixels at all. The Composite
//! extension already keeps a window's contents in a pixmap of the server's own,
//! and DRI3 exports that pixmap as a DMA-BUF — a buffer the GPU that made it can
//! also read — so whatever was drawn is the frame, with nothing copied in
//! between.
//!
//! Two things are worth knowing before reading further.
//!
//! **The screen is taken from the overlay window.** The Composite protocol
//! refuses to redirect the root window — "The root window may not be
//! redirected. Doing so results in a Match error" — so the screen's own picture
//! cannot be named. What can be named is the Composite overlay window, the
//! window a compositing manager draws the finished screen into: redirecting it
//! gives this module the storage of exactly that picture. With no compositing
//! manager nothing draws into it, and the capture refuses by name rather than
//! sharing an empty rectangle, since the processor's capturer would have had the
//! real screen. A window needs no compositor: the server keeps a redirected
//! window's storage either way.
//!
//! **One export is every frame.** The window keeps drawing into the same
//! storage, and the DMA-BUF handed over here is a view of it, so a frame costs
//! no request at all: the same buffer answers again, holding whatever has been
//! drawn since. What the capture does ask is *when* something was drawn —
//! `DamageNotify` from XDamage — so a still desktop answers [`Arrival::Still`]
//! and a share stops feeding its encoder instead of re-sending an unchanged
//! picture at 60 Hz. A size change makes the server a new pixmap for the window,
//! which is what [`Arrival::Resized`] renames and re-exports.
//!
//! A server that cannot export — Xvfb has no display device, and a driver may
//! have no DRI3 — answers with an error naming what failed rather than a
//! picture, and a caller falls back to the processor's capturer. Buffers are
//! handed on only as 8-bit BGRA (`ARGB8888` in DRM's naming, whatever the
//! fourth byte holds), the layout the encoders in this crate already read; a
//! 16-bit or 10-bit screen is refused by name for the same reason.
//!
//! Nothing here is wired into a share yet. [`open`] is the entry point and
//! [`Capture::frame`] the loop; the note at the end of this file says how
//! `stream_worker` reaches them.
use std::fmt::Write as _;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use x11rb::connection::Connection as _;
use x11rb::errors::ConnectionError;
use x11rb::protocol::Event;
use x11rb::protocol::composite::{self, ConnectionExt as _};
use x11rb::protocol::damage::{self, ConnectionExt as _};
use x11rb::protocol::dri3::{self, ConnectionExt as _};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, EventMask, MapState, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// What a capture is of.
///
/// A window is never captured as the screen behind it: a [`Target::Window`] that
/// names the root window, or one that is not on the screen at all, is refused by
/// name, so a share that promised one window can only end in an error or in that
/// window's own picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The whole screen: every window on it, and the desktop behind them.
    Screen,
    /// One monitor of the screen, as [`listed`] names them: the screen's
    /// picture, cut to the monitor's part of it.
    Display(Display),
    /// One window, by the id [`listed`] names it by — the XID libwebrtc's own
    /// capturer lists it under as well.
    Window(u32),
}

impl From<Display> for Target {
    fn from(display: Display) -> Self {
        Self::Display(display)
    }
}

/// One monitor of the screen, as RandR describes it.
///
/// X11 has one root window per screen and no window per monitor, so a monitor is
/// a rectangle on the screen rather than a drawable of its own: it is captured
/// as the screen's picture with [`Region`] naming the part to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    /// The name the server knows it by, which is the connector's, as `xrandr`
    /// prints it: `eDP-1`, or `XWAYLAND0` for a screen Xwayland made.
    pub name: String,
    /// Where it sits on the screen, and how much of it it is.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A rectangle of a [`Buffer`], in pixels from its top-left corner.
///
/// A window's or the screen's buffer is the whole of it, so this is the whole
/// buffer and takes nothing to apply. A monitor's is the screen's buffer, so its
/// `x` and `y` are where the monitor's picture starts: a viewer reads that part
/// and leaves the rest, which is the other monitors'.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    /// Whether there is anything to show at all: an empty region is a capture of
    /// nothing, which is refused when it opens rather than handed out as a frame
    /// no encoder can take.
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// One capture's picture, as the card reads it: the server's storage of what was
/// captured, handed over as a file descriptor rather than as pixels.
///
/// It is a handle to live storage, not a copy. The window keeps being drawn into
/// it, so the same [`Picture`] read again shows what was drawn since, and it
/// outlives the pixmap that named it: freeing that name is what the capture does
/// once the export has the storage, and the buffer lives as long as this picture
/// does.
///
/// Cloning one is a count, because a share that hands the same buffer to a
/// preview and to an encoder should be holding one buffer, not two.
#[derive(Clone, Debug)]
pub struct Picture(Arc<Buffer>);

impl Picture {
    /// The buffer itself: its descriptor, its layout, and the part of it to show.
    pub fn buffer(&self) -> &Buffer {
        &self.0
    }
}

/// The DMA-BUF behind a [`Picture`].
///
/// One plane, in BGRA: bytes `B, G, R, X` for each pixel, `stride` bytes apart
/// down the picture. Importing it is the caller's half of the work — a GPU
/// texture made from `fd` with `stride`, `offset` and `modifier` — and the DRM
/// driver orders that import against whatever the server is drawing, so no fence
/// travels with it: the implicit model, as section 12 of the DRI3 protocol
/// describes it.
#[derive(Debug)]
pub struct Buffer {
    /// The buffer's size in pixels, which is what was captured: the window's, or
    /// the screen's for a monitor.
    pub width: u32,
    pub height: u32,
    /// How far one row of pixels is from the next, in bytes.
    pub stride: u32,
    /// Where the first pixel is in the buffer, in bytes.
    pub offset: u32,
    /// The DRM format modifier the server laid it out with, as `drm_fourcc.h`
    /// defines them, and `None` from a server whose DRI3 predates modifiers
    /// telling anyone.
    pub modifier: Option<u64>,
    /// The pixmap's depth and bits per pixel as the server reported them, always
    /// 24 and 32: every other pair is refused before a buffer is made.
    pub depth: u8,
    pub bpp: u8,
    /// The part of the buffer to show, which is all of it but a monitor's.
    pub region: Region,
    fd: OwnedFd,
}

impl Buffer {
    /// The buffer as a file descriptor, **borrowed**: it is closed with the
    /// picture, so an importer either takes it now or duplicates it.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// The buffer a DRI3 1.2 `BuffersFromPixmap` answered with.
    fn from_buffers(reply: dri3::BuffersFromPixmapReply, region: Region) -> Result<Self> {
        let planes = reply.buffers.len();
        let (Some(stride), Some(offset)) = (reply.strides.first(), reply.offsets.first()) else {
            bail!("the X server exported a pixmap of {planes} buffers with no strides or offsets");
        };
        let layout = Layout::of(
            reply.width,
            reply.height,
            *stride,
            *offset,
            Some(reply.modifier),
            reply.depth,
            reply.bpp,
        )?;
        if planes != 1 {
            bail!(
                "the X server exported a {}-plane pixmap: this hands on one plane, in BGRA",
                planes
            );
        }
        let Some(fd) = reply.buffers.into_iter().next() else {
            bail!("the X server exported a pixmap with no buffer in it");
        };
        Ok(layout.with(fd, region))
    }

    /// The buffer a DRI3 1.0 `BufferFromPixmap` answered with: the same picture,
    /// from a server too old to say how it laid the buffer out.
    fn from_buffer(reply: dri3::BufferFromPixmapReply, region: Region) -> Result<Self> {
        let layout = Layout::of(
            reply.width,
            reply.height,
            u32::from(reply.stride),
            0,
            None,
            reply.depth,
            reply.bpp,
        )?;
        Ok(layout.with(reply.pixmap_fd, region))
    }
}

/// A buffer's layout, once it is known to be one a share can read: the whole
/// answer from the server except the descriptor, which is what
/// [`Buffer::from_buffers`] and [`Buffer::from_buffer`] add to it.
///
/// Everything here is decided without a server to talk to, which is why the
/// clamp lives in it: the formats a server may answer with are the server's,
/// and the one format this crate can hand on is the crate's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    width: u32,
    height: u32,
    stride: u32,
    offset: u32,
    modifier: Option<u64>,
    depth: u8,
    bpp: u8,
}

impl Layout {
    /// The layout of a pixmap the server described as `depth` bits deep and
    /// `bpp` bits to a pixel, when a share can read it.
    ///
    /// Only depth 24 with 32 bits to a pixel is one: the screen's own depth on
    /// every desktop this runs on, and `ARGB8888` to DRM — a byte each of blue,
    /// green and red, and a fourth the server promises nothing about. Anything
    /// else — a 16-bit screen, a 30-bit one whose channels are packed ten bits
    /// apiece, a 1-bit cursor — would reach an encoder as noise, so it is an
    /// error naming the depth rather than a picture.
    fn of(
        width: u16,
        height: u16,
        stride: u32,
        offset: u32,
        modifier: Option<u64>,
        depth: u8,
        bpp: u8,
    ) -> Result<Self> {
        if depth != 24 || bpp != 32 {
            bail!(
                "the X server exported a {depth}-bit picture of {bpp} bits a pixel, and this hands on 24-bit BGRA"
            );
        }
        if width == 0 || height == 0 {
            bail!("the X server exported an empty picture, {width}x{height}");
        }
        let shortest = u32::from(width) * 4;
        if stride < shortest {
            bail!(
                "the X server's stride of {stride} bytes leaves a row of {width} BGRA pixels {shortest} bytes long short"
            );
        }
        Ok(Self {
            width: u32::from(width),
            height: u32::from(height),
            stride,
            offset,
            modifier,
            depth,
            bpp,
        })
    }

    /// The buffer this layout describes, once the descriptor that holds it has
    /// arrived.
    fn with(self, fd: OwnedFd, region: Region) -> Buffer {
        Buffer {
            width: self.width,
            height: self.height,
            stride: self.stride,
            offset: self.offset,
            modifier: self.modifier,
            depth: self.depth,
            bpp: self.bpp,
            region,
            fd,
        }
    }
}

/// The part of `size`'s storage a capture of `target` shows: all of it, and for
/// a monitor the rectangle it occupies on the screen.
fn region(target: &Target, size: (u32, u32)) -> Region {
    match target {
        Target::Screen | Target::Window(_) => Region {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        },
        Target::Display(display) => cut(display, size),
    }
}

/// A monitor's rectangle inside a screen of `screen`, clamped to it.
///
/// A monitor is where the screen's picture is, not a picture of its own — X11
/// has one root window per screen and no window per monitor — so the capture is
/// of the screen and this is the part of it to show. A monitor that a mode
/// change left hanging off the edge is the part of it that is still on the
/// screen, and one that is entirely off it cuts to nothing, which a capture
/// refuses rather than handing out as an empty frame.
fn cut(display: &Display, screen: (u32, u32)) -> Region {
    let x = display.x.clamp(0, screen.0 as i32) as u32;
    let y = display.y.clamp(0, screen.1 as i32) as u32;
    Region {
        x,
        y,
        width: display.width.min(screen.0.saturating_sub(x)),
        height: display.height.min(screen.1.saturating_sub(y)),
    }
}

/// What asking a [`Capture`] for its picture found.
///
/// None of these but [`Arrival::Closed`] is a failure: a desktop that hasn't
/// changed has nothing new to send, and a resize is a capture that has to be
/// rebuilt rather than one that ended.
#[derive(Debug)]
pub enum Arrival {
    /// Something was drawn since the last frame: here is the buffer holding it,
    /// at [`Capture::content_size`] and with [`Picture::buffer`]'s region.
    Frame(Picture),
    /// Nothing has been drawn since the last frame. Ask again after a sleep —
    /// this is the desktop's own rate, not a rate to spin at — and the last
    /// picture is still the picture.
    Still,
    /// What is being captured changed size, and the capture has already renamed
    /// and re-exported its storage at the new one, which
    /// [`Capture::content_size`] reports. An encoder built for the old size
    /// reads the next frame wrongly, so it is rebuilt here; the next
    /// [`Capture::frame`] answers with the first frame at the new size.
    Resized,
    /// What was being captured is gone: its window was closed, or it was taken
    /// off the screen and unmapped. The capture is over and hands out nothing
    /// more; [`Capture::ending`] says which of the two happened.
    Closed,
}

/// Where a target is captured from on the server: the screen's overlay window,
/// or the window itself.
///
/// This is the whole of the decision, and it is made from the target and the
/// server's root window id alone, so it is checked without a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    /// The screen, through the Composite overlay window of the root.
    Screen,
    /// One window, redirected and named as itself.
    Window(u32),
}

impl Plan {
    /// Where `target` is captured from, given the id of the root window.
    ///
    /// A window that names the root is refused: on X11 the root *is* the
    /// desktop every other window sits on, so a share that asked for the window
    /// `0x…` the screen happens to be, or for a window id it guessed, would be
    /// handed the desktop under the name of a window — the one thing a window
    /// capture must never be.
    fn of(target: &Target, root: u32) -> Result<Self> {
        match target {
            Target::Screen | Target::Display(_) => Ok(Self::Screen),
            Target::Window(window) if *window == 0 => {
                bail!("no window was named: there is no window 0 and no default one")
            }
            Target::Window(window) if *window == root => bail!(
                "window 0x{window:x} is the screen's own root window, and a window is never captured as the desktop behind it"
            ),
            Target::Window(window) => Ok(Self::Window(*window)),
        }
    }
}

/// What one capture is of, for a log line.
enum Subject {
    Screen,
    Window(u32, String),
}

impl std::fmt::Display for Subject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Screen => formatter.write_str("the screen"),
            // A window's own title is not always there — one that has just been
            // mapped has none yet — so the id is what always says which it is.
            Self::Window(window, title) if title.is_empty() => {
                write!(formatter, "window 0x{window:x}")
            }
            Self::Window(window, title) => {
                write!(formatter, "window 0x{window:x} (\u{201c}{title}\u{201d})")
            }
        }
    }
}

/// A capture of a window or the screen, delivering its storage as one DMA-BUF
/// that is re-exported only when the size changes.
///
/// It owns the connection it opened, and lets go of everything it asked the
/// server for when it is dropped: the redirection, the overlay window's use, and
/// the damage object. Dropping it is how a share stops holding a window's
/// storage.
pub struct Capture {
    conn: RustConnection,
    root: u32,
    plan: Plan,
    /// The window whose storage is the picture: the overlay window for the
    /// screen, and the window itself for a window.
    watched: u32,
    subject: Subject,
    /// What was asked for, which is what decides the part of the storage to
    /// show: all of it, or the monitor's part of the screen.
    target: Target,
    /// The size of `watched` when the picture was exported.
    size: (u32, u32),
    picture: Option<Picture>,
    /// Whether the storage was just renamed and its first frame not yet handed
    /// over. A still desktop draws nothing more, so a fresh export is owed to the
    /// caller rather than waited for.
    fresh: bool,
    /// Why the capture ended, in the words of the [`Arrival::Closed`] it
    /// answered, and empty while it still runs.
    ending: String,
    /// The damage object on `watched`, or 0 where the server has no XDamage and
    /// every frame is reported as new.
    damage: u32,
    composite: (u32, u32),
    dri3: (u32, u32),
    /// The display device the server exported from, kept open so the card stays
    /// the one the picture was made on.
    device: OwnedFd,
}

impl Capture {
    /// Opens a capture of `target` and starts it, exporting its first picture.
    ///
    /// Everything that can refuse does so here, before a frame: a server with no
    /// Composite or no DRI3, a screen with no compositing manager to draw it into
    /// anything, a window that has gone or is not on the screen, and a picture in
    /// a format a share can't read are each an error naming what it looked at.
    /// A caller that gets one **falls back to the processor's capturer**; there
    /// is nothing here to retry.
    ///
    /// A [`Target::Window`] is a window of its own here, redirected if nothing
    /// had redirected it — a desktop with no compositing manager — and named as
    /// itself either way. It is never the screen's root, and never the desktop
    /// its window sits on.
    pub fn open(target: impl Into<Target>) -> Result<Self> {
        let target = target.into();
        let (conn, screen) = x11rb::connect(None).context("no X server to capture from")?;
        let Some(setup) = conn.setup().roots.get(screen) else {
            bail!("the X server has no screen number {screen}");
        };
        let root = setup.root;
        let screen_size = (
            u32::from(setup.width_in_pixels),
            u32::from(setup.height_in_pixels),
        );
        let plan = Plan::of(&target, root)?;
        // Composite is what puts the picture somewhere it can be named, and DRI3
        // is what carries it out; a server without either has nothing for this
        // module and is named rather than asked again.
        let composite = match conn.composite_query_version(0, 4) {
            Ok(cookie) => cookie
                .reply()
                .context("asking the X server for Composite's version")?,
            Err(ConnectionError::UnsupportedExtension) => bail!(
                "this X server has no Composite extension, so nothing here can be redirected to a pixmap"
            ),
            Err(error) => return Err(error).context("asking the X server for Composite's version"),
        };
        let dri3 = match conn.dri3_query_version(1, 2) {
            Ok(cookie) => cookie
                .reply()
                .context("asking the X server for DRI3's version")?,
            Err(ConnectionError::UnsupportedExtension) => bail!(
                "this X server has no DRI3 extension, so a redirected pixmap cannot be exported as a DMA-BUF"
            ),
            Err(error) => return Err(error).context("asking the X server for DRI3's version"),
        };
        // The card the picture is made on, which the export is a buffer of. A
        // server exporting through nothing — Xvfb — answers with an error here,
        // and a share falls back rather than looking for another way in.
        let device = match conn.dri3_open(root, 0) {
            Ok(cookie) => match cookie.reply() {
                Ok(reply) => reply.device_fd,
                Err(error) => bail!(
                    "the X server would not hand over the display's device, so it has no buffer to export: {error}"
                ),
            },
            Err(error) => bail!(
                "the X server would not hand over the display's device, so it has no buffer to export: {error}"
            ),
        };
        let mut capture = Self {
            conn,
            root,
            plan,
            watched: root,
            subject: Subject::Screen,
            target,
            size: screen_size,
            picture: None,
            fresh: false,
            ending: String::new(),
            damage: 0,
            composite: (composite.major_version, composite.minor_version),
            dri3: (dri3.major_version, dri3.minor_version),
            device,
        };
        capture.watch(screen)?;
        capture.rename()?;
        Ok(capture)
    }

    /// Finds what is being captured, and asks the server to keep it: redirects
    /// it, watches it for the events that end or resize it, and asks to be told
    /// when it is drawn into.
    fn watch(&mut self, screen: usize) -> Result<()> {
        let watched = match self.plan {
            Plan::Screen => self.overlay(screen)?,
            Plan::Window(window) => {
                self.examine(window)?;
                // The title is read before anything is asked of the window, so
                // that every message about it can name it the way a person
                // knows it.
                self.subject = Subject::Window(window, self.title(window)?.unwrap_or_default());
                window
            }
        };
        self.watched = watched;
        // Redirection is what gives the window off-screen storage of its own to
        // name. Automatic says the server keeps showing it where it is: a
        // capture is not a compositing manager and must not take the picture away
        // from the person using the desktop.
        self.conn
            .composite_redirect_window(self.watched, composite::Redirect::AUTOMATIC)?
            .check()
            .with_context(|| format!("redirecting {}", self.subject))?;
        // A resize is a new pixmap, an unmap is a picture that is no longer on
        // the screen, and a close is both of them ending; all three arrive as
        // events on the window itself.
        self.conn
            .change_window_attributes(
                self.watched,
                &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
            )?
            .check()
            .with_context(|| format!("watching {}", self.subject))?;
        // Damage is what says a frame was drawn; a server without it still has
        // every frame to hand, so it is missed rather than needed.
        self.damage = 0;
        if let Ok(damage) = self.conn.generate_id() {
            let created = self
                .conn
                .damage_create(damage, self.watched, damage::ReportLevel::NON_EMPTY)
                .ok()
                .is_some_and(|cookie| cookie.check().is_ok());
            if created {
                self.damage = damage;
            }
        }
        Ok(())
    }

    /// The Composite overlay window of `screen`, redirected so that it has
    /// storage this capture can name.
    ///
    /// It is only the screen's picture while a compositing manager draws the
    /// screen into it, so a screen with no manager is refused by name here: an
    /// overlay window nobody draws into is an empty rectangle, and a share of it
    /// would be worse than the processor's capture it replaced.
    fn overlay(&mut self, screen: usize) -> Result<u32> {
        if !self.composited(screen)? {
            bail!(
                "no compositing manager draws this X screen into its overlay window (nothing owns _NET_WM_CM_S{screen}), so the screen has no picture to export"
            );
        }
        let overlay = self
            .conn
            .composite_get_overlay_window(self.root)?
            .reply()
            .context("asking for the Composite overlay window")?
            .overlay_win;
        if overlay == 0 {
            bail!(
                "this X server has no Composite overlay window, so the screen has nothing to name"
            );
        }
        Ok(overlay)
    }

    /// Whether a compositing manager has taken the screen's own selection, which
    /// every compositor on X11 does: `_NET_WM_CM_S0` is the desktop's way of
    /// saying "this screen is composited", and the overlay window is only a
    /// picture where it is true.
    fn composited(&self, screen: usize) -> Result<bool> {
        let atom = self
            .conn
            .intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes())?
            .reply()
            .context("naming the screen's compositing manager selection")?
            .atom;
        let owner = self
            .conn
            .get_selection_owner(atom)?
            .reply()
            .context("asking who composites this screen")?
            .owner;
        Ok(owner != x11rb::NONE)
    }

    /// Refuses a window that is not one this capture can be of, by name: a
    /// window that has gone, one that is not on the screen, an input-only
    /// window, and one with no area are each a failure rather than a capture of
    /// something else.
    fn examine(&self, window: u32) -> Result<()> {
        let attributes = self
            .conn
            .get_window_attributes(window)
            .with_context(|| format!("looking for window 0x{window:x}"))?
            .reply()
            .with_context(|| format!("looking for window 0x{window:x}, which is not there"))?;
        if attributes.class == WindowClass::INPUT_ONLY {
            bail!("window 0x{window:x} takes no drawing, so there is nothing of it to capture");
        }
        if attributes.map_state != MapState::VIEWABLE {
            bail!("window 0x{window:x} is not on the screen, so there is nothing of it to capture");
        }
        Ok(())
    }

    /// Renames the captured window's storage with a fresh pixmap, exports it, and
    /// lets go of the name again: from here the buffer is the storage, and the
    /// server's pixmap id is a handle this capture no longer needs. The protocol
    /// asks exactly this of a client that named a window — "it is important that
    /// the client use the FreePixmap request when it is done with the contents,
    /// and to create a new name for the newly allocated pixmap" — since the
    /// server makes a new pixmap for every resize.
    fn rename(&mut self) -> Result<()> {
        let size = self.size_of(self.watched)?;
        if size.0 == 0 || size.1 == 0 {
            bail!("{} has no area to capture", self.subject);
        }
        let region = region(&self.target, size);
        if region.is_empty() {
            bail!("{} is not on the screen being captured", self.subject);
        }
        let pixmap = self
            .conn
            .generate_id()
            .context("making an id for the captured window's storage")?;
        self.conn
            .composite_name_window_pixmap(self.watched, pixmap)?
            .check()
            .with_context(|| format!("naming the off-screen storage of {}", self.subject))?;
        // The name holds the storage only for the export to be made from, so it
        // is freed however the export goes: a refusal on a reconfigure would
        // otherwise keep a window-sized pixmap in the server for as long as the
        // connection lives.
        let exported = self.export(pixmap, region);
        let _ = self.conn.free_pixmap(pixmap);
        let picture = exported?;
        self.size = size;
        self.picture = Some(picture);
        self.fresh = true;
        Ok(())
    }

    /// The window's storage, named as `pixmap`, exported as a DMA-BUF of
    /// `region`. The caller frees the name, whatever this answers.
    fn export(&self, pixmap: u32, region: Region) -> Result<Picture> {
        // A server can name a pixmap that has nothing in it — Xwayland answers
        // the Composite overlay window's name with an empty one — and the export
        // of an empty pixmap fails with an error about a pixmap that is not
        // there, which reads as a bug in this module rather than as a server
        // that has no picture. What it is, is a capture of nothing.
        let stored = self
            .conn
            .get_geometry(pixmap)?
            .reply()
            .with_context(|| format!("measuring the storage of {}", self.subject))?;
        if stored.width == 0 || stored.height == 0 {
            bail!(
                "the off-screen storage of {} is {}x{}, so this X server has no picture of it to export",
                self.subject,
                stored.width,
                stored.height
            );
        }
        // DRI3 1.2 reports how the buffer is laid out; older servers report only
        // its stride, and an import has to assume the default layout.
        let picture = if self.dri3.1 >= 2 {
            let reply = self
                .conn
                .dri3_buffers_from_pixmap(pixmap)?
                .reply()
                .with_context(|| format!("exporting {} as a DMA-BUF", self.subject))?;
            Picture(Arc::new(Buffer::from_buffers(reply, region)?))
        } else {
            let reply = self
                .conn
                .dri3_buffer_from_pixmap(pixmap)?
                .reply()
                .with_context(|| format!("exporting {} as a DMA-BUF", self.subject))?;
            Picture(Arc::new(Buffer::from_buffer(reply, region)?))
        };
        Ok(picture)
    }

    /// Asks for the next frame: the picture if anything was drawn since the last
    /// one, [`Arrival::Still`] when nothing has and `timeout` passed, and a
    /// rebuild or an ending when the window changed size or went away.
    ///
    /// A frame is the whole of the cost of a still desktop: nothing is asked of
    /// the server until its next draw, only events read from the connection this
    /// capture already has open. `timeout` is how long a still desktop is waited
    /// for before [`Arrival::Still`] answers; [`Duration::ZERO`] looks once and
    /// returns.
    pub fn frame(&mut self, timeout: Duration) -> Result<Arrival> {
        if !self.ending.is_empty() {
            return Ok(Arrival::Closed);
        }
        // A picture that has just been exported owes the caller a frame before
        // anything is drawn again: a window that opens and then sits still would
        // otherwise never deliver the picture it already has.
        if self.fresh {
            self.fresh = false;
            return self.preview();
        }
        let deadline = Instant::now() + timeout;
        loop {
            let mut damaged = false;
            while let Some(event) = self.next()? {
                match event {
                    Event::DamageNotify(notify) if notify.drawable == self.watched => {
                        damaged = true;
                    }
                    Event::ConfigureNotify(notify)
                        if notify.window == self.watched || notify.window == self.root =>
                    {
                        if (u32::from(notify.width), u32::from(notify.height)) != self.size {
                            return self.rebuild();
                        }
                    }
                    Event::UnmapNotify(notify) if notify.window == self.watched => {
                        // Closing a window unmaps it first, so whether the window
                        // is still there decides whether this is a window taken
                        // off the screen or one that has gone.
                        return self.close(if self.gone() {
                            format!("{} was closed", self.subject)
                        } else {
                            format!("{} was taken off the screen", self.subject)
                        });
                    }
                    Event::DestroyNotify(notify)
                        if notify.window == self.watched || notify.window == self.root =>
                    {
                        return self.close(format!("{} was closed", self.subject));
                    }
                    // Nothing else is asked for, and an error nobody waited for
                    // is the server refusing something in flight — a damage
                    // object on a window that has just gone, say — which the
                    // next frame would fail on anyway.
                    _ => {}
                }
            }
            if damaged {
                if self.damage != 0 {
                    let _ = self
                        .conn
                        .damage_subtract(self.damage, x11rb::NONE, x11rb::NONE);
                }
                return self.preview();
            }
            // A server with no damage reporting cannot say whether anything was
            // drawn — Xwayland advertises XDamage and refuses to create one — so
            // the picture is handed over and the caller paces itself. Answering
            // [`Arrival::Still`] here would be a share that froze after its first
            // frame.
            if self.damage == 0 {
                return self.preview();
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(Arrival::Still);
            }
            wait(&self.conn, deadline - now)?;
        }
    }

    /// Renames and re-exports the storage after a resize, and reports what the
    /// new size is through [`Capture::content_size`].
    ///
    /// A window that is resized can be closed in the same breath, which is an
    /// ending rather than a failure: the server refuses to name the storage of a
    /// window that is no longer there, and that refusal is this capture's last
    /// frame.
    fn rebuild(&mut self) -> Result<Arrival> {
        match self.rename() {
            Ok(()) => Ok(Arrival::Resized),
            Err(_) if self.gone() => self.close(format!("{} was closed", self.subject)),
            Err(why) => Err(why),
        }
    }

    /// Whether the captured window is still there at all.
    fn gone(&self) -> bool {
        self.conn
            .get_window_attributes(self.watched)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_none()
    }

    /// The stored picture as a frame.
    fn preview(&self) -> Result<Arrival> {
        let Some(picture) = &self.picture else {
            bail!("{} has no picture to hand over", self.subject);
        };
        Ok(Arrival::Frame(picture.clone()))
    }

    /// Ends the capture with `why` as its reason.
    fn close(&mut self, why: String) -> Result<Arrival> {
        self.ending = why;
        self.picture = None;
        Ok(Arrival::Closed)
    }

    /// The next event the server has already sent, without waiting for one.
    fn next(&self) -> Result<Option<Event>> {
        self.conn
            .poll_for_event()
            .context("reading from the X server")
    }

    /// The size of `window` now.
    fn size_of(&self, window: u32) -> Result<(u32, u32)> {
        let geometry = self
            .conn
            .get_geometry(window)
            .context("asking the X server how big the captured window is")?
            .reply()
            .context("asking the X server how big the captured window is")?;
        Ok((u32::from(geometry.width), u32::from(geometry.height)))
    }

    /// A window's title, as the share menu would show it: its `_NET_WM_NAME`,
    /// which is UTF-8, and its `WM_NAME` when it has no such thing, which is one
    /// of the server's own encodings and read as best it can be.
    fn title(&self, window: u32) -> Result<Option<String>> {
        let net_wm_name = self
            .conn
            .intern_atom(false, b"_NET_WM_NAME")?
            .reply()
            .context("naming _NET_WM_NAME")?
            .atom;
        let wm_name = self
            .conn
            .intern_atom(false, b"WM_NAME")?
            .reply()
            .context("naming WM_NAME")?
            .atom;
        let utf8 = self.property(window, net_wm_name)?;
        let legacy = self.property(window, wm_name)?;
        Ok(titled(utf8.as_deref(), legacy.as_deref()))
    }

    /// A window's property as its bytes, when it has one.
    fn property(&self, window: u32, atom: u32) -> Result<Option<Vec<u8>>> {
        let reply = self
            .conn
            .get_property(false, window, atom, AtomEnum::ANY, 0, 4096)?
            .reply()
            .context("reading a window's property")?;
        Ok((reply.format == 8 && reply.value_len > 0).then_some(reply.value))
    }

    /// The size the frames are at now: the window's, or the screen's, and the new
    /// one after [`Arrival::Resized`].
    pub fn content_size(&self) -> (u32, u32) {
        self.size
    }

    /// The display device the server exported from, **borrowed**: the card the
    /// picture was made on, which is the card to import it on. It is what a
    /// caller needs when a machine has more than one.
    pub fn device(&self) -> BorrowedFd<'_> {
        self.device.as_fd()
    }

    /// Why the capture ended, when it did: the sentence written for the
    /// [`Arrival::Closed`] it answered, and empty while the capture still runs.
    pub fn ending(&self) -> &str {
        &self.ending
    }

    /// Whether a still desktop can be told apart from one being drawn into,
    /// which needs the server's XDamage extension: without it every frame is
    /// reported as new and a share re-encodes a picture that hasn't changed.
    pub fn damaged(&self) -> bool {
        self.damage != 0
    }

    /// What this capture is and how it went, in one line: what was captured, the
    /// extensions and their versions, the buffer's layout, and whether a still
    /// desktop can be told from a moving one.
    pub fn describe(&self) -> String {
        let mut line = format!(
            "X11 {}, Composite {}.{}, DRI3 {}, device fd {}",
            self.subject,
            self.composite.0,
            self.composite.1,
            format_args!("{}.{}", self.dri3.0, self.dri3.1),
            self.device.as_fd().as_raw_fd(),
        );
        match &self.picture {
            Some(picture) => {
                let buffer = picture.buffer();
                let _ = write!(
                    line,
                    ", {}x{} BGRA stride {} offset {} modifier {}, region {}x{} at {},{}",
                    buffer.width,
                    buffer.height,
                    buffer.stride,
                    buffer.offset,
                    match buffer.modifier {
                        Some(modifier) => format!("0x{modifier:x}"),
                        None => "unknown".to_owned(),
                    },
                    buffer.region.width,
                    buffer.region.height,
                    buffer.region.x,
                    buffer.region.y,
                );
            }
            None => line.push_str(", no picture"),
        }
        if self.damage != 0 {
            line.push_str(", damage reporting");
        } else {
            line.push_str(", no damage reporting: every frame is new");
        }
        line
    }
}

impl Drop for Capture {
    /// Lets go of everything asked of the server: the damage object, the
    /// redirection, and the overlay window's use — which unmaps it when no other
    /// client is using it. All of it is best-effort: a connection that has
    /// already gone has nothing left to release, and letting go of a capture must
    /// not wait on a server that is not answering.
    fn drop(&mut self) {
        if self.damage != 0 {
            let _ = self.conn.damage_destroy(self.damage);
        }
        let _ = self
            .conn
            .composite_unredirect_window(self.watched, composite::Redirect::AUTOMATIC);
        if self.plan == Plan::Screen {
            let _ = self.conn.composite_release_overlay_window(self.root);
        }
        let _ = self.conn.flush();
    }
}

/// What there is to capture on X11, as a share menu shows it: the screen's
/// monitors, then the windows a desktop has on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub target: Target,
    pub title: String,
}

/// The id of the root window — the screen every other window on it sits on — of
/// the default screen.
///
/// It is what libwebrtc's X11 capturer lists a screen by, so it is what a
/// [`Source::Display`](crate::source::Source::Display) chosen from
/// Yapfast's own menu carries for a whole screen. Asked on a connection of its
/// own, which closes when the id is read.
pub fn root_window() -> Result<u32> {
    let (conn, screen) = x11rb::connect(None).context("no X server to ask for the screen's id")?;
    conn.setup()
        .roots
        .get(screen)
        .map(|setup| setup.root)
        .with_context(|| format!("the X server has no screen number {screen}"))
}

/// Lists what this X11 session offers a share, on a connection of its own, which
/// closes when the list is made.
///
/// Where the session's dialog picks — a Wayland session, see
/// [`portal_picks`](crate::source::portal_picks) — nothing is listed and
/// this isn't asked. On X11 with no screen sharing portal this is what the menu
/// has instead, and every entry is a capture this module can open: a monitor
/// whose rectangle is inside the screen, and a window that is on it with a title
/// to show.
pub fn listed() -> Result<Vec<Listed>> {
    let (conn, screen) = x11rb::connect(None).context("no X server to list what to share")?;
    let Some(setup) = conn.setup().roots.get(screen) else {
        bail!("the X server has no screen number {screen}");
    };
    let root = setup.root;
    let size = (
        u32::from(setup.width_in_pixels),
        u32::from(setup.height_in_pixels),
    );
    let mut listed = displays(&conn, root, size)?;
    listed.extend(windows(&conn, root)?);
    Ok(listed)
}

/// The screen's monitors, as RandR reports them; a server whose RandR has no
/// monitors — anything before RandR 1.5 — lists the screen as one.
fn displays(conn: &RustConnection, root: u32, size: (u32, u32)) -> Result<Vec<Listed>> {
    let monitors = conn
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.monitors)
        .unwrap_or_default();
    if monitors.is_empty() {
        return Ok(vec![Listed {
            target: Target::Screen,
            title: "Entire screen".to_owned(),
        }]);
    }
    let count = monitors.len();
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            // A monitor's name is an atom the server names it by, which is the
            // connector's when there is one; a screen Xwayland made has none for
            // a monitor it invented, and a menu still needs a word for it.
            let name = conn
                .get_atom_name(monitor.name)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.name).into_owned())
                .filter(|name| !name.trim().is_empty());
            let title = match (&name, count) {
                (Some(name), _) => name.clone(),
                (None, 1) => "Entire screen".to_owned(),
                (None, _) => format!("Screen {}", index + 1),
            };
            let display = Display {
                name: name.unwrap_or_default(),
                x: i32::from(monitor.x),
                y: i32::from(monitor.y),
                width: u32::from(monitor.width),
                height: u32::from(monitor.height),
            };
            Listed {
                // A monitor that has been unplugged can sit off the screen until
                // the server re-arranges it, and a capture of it shows nothing,
                // so the part of it that is left is what a menu offers.
                target: Target::Display(display),
                title,
            }
        })
        .filter(|listed| match &listed.target {
            Target::Display(display) => !cut(display, size).is_empty(),
            _ => true,
        })
        .collect())
}

/// The windows on this screen that a share can be of.
///
/// The desktop's own list, `_NET_CLIENT_LIST`, when the window manager keeps one:
/// those are the windows a person sees in a taskbar, in its order. Otherwise the
/// tree is walked for the windows a window manager marked with `WM_STATE`, which
/// is the same set said a harder way, and the way an Xvfb with no manager at all
/// still answers.
fn windows(conn: &RustConnection, root: u32) -> Result<Vec<Listed>> {
    let client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST")?.reply()?.atom;
    let ordered = conn
        .get_property(false, root, client_list, AtomEnum::WINDOW, 0, 4096)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .filter(|reply| reply.format == 32)
        .and_then(|reply| reply.value32().map(|windows| windows.collect::<Vec<_>>()));
    let ids = match ordered {
        Some(ids) if !ids.is_empty() => ids,
        // A desktop with no list is walked, which is a request per window; the
        // depth is bounded so a pathological tree cannot walk forever.
        _ => {
            let state = conn.intern_atom(false, b"WM_STATE")?.reply()?.atom;
            let mut ids = Vec::new();
            clients(conn, root, state, 0, &mut ids);
            ids
        }
    };
    let mut listed = Vec::with_capacity(ids.len());
    for window in ids {
        if let Some(one) = window_listed(conn, root, window)? {
            listed.push(one);
        }
    }
    Ok(listed)
}

/// Adds the windows under `window` that a window manager marked as clients,
/// descending into the ones it didn't: a container of a client window has no
/// `WM_STATE` of its own.
fn clients(conn: &RustConnection, window: u32, state: u32, depth: usize, ids: &mut Vec<u32>) {
    const DEEPEST: usize = 8;
    if depth > DEEPEST {
        return;
    }
    let Some(children) = conn
        .query_tree(window)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.children)
    else {
        return;
    };
    for child in children {
        let marked = conn
            .get_property(false, child, state, AtomEnum::ANY, 0, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|reply| reply.type_ != x11rb::NONE);
        if marked {
            ids.push(child);
        } else {
            clients(conn, child, state, depth + 1, ids);
        }
    }
}

/// One window as a share menu shows it, when it is one a share can be of.
fn window_listed(conn: &RustConnection, root: u32, window: u32) -> Result<Option<Listed>> {
    let attributes = conn
        .get_window_attributes(window)
        .ok()
        .and_then(|cookie| cookie.reply().ok());
    let Some(attributes) = attributes else {
        return Ok(None);
    };
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
    let wm_name = conn.intern_atom(false, b"WM_NAME")?.reply()?.atom;
    let property = |atom| -> Option<Vec<u8>> {
        let reply = conn
            .get_property(false, window, atom, AtomEnum::ANY, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        (reply.format == 8 && reply.value_len > 0).then_some(reply.value)
    };
    let title = titled(
        property(net_wm_name).as_deref(),
        property(wm_name).as_deref(),
    );
    let candidate = Candidate {
        window,
        root,
        viewable: attributes.map_state == MapState::VIEWABLE,
        input_only: attributes.class == WindowClass::INPUT_ONLY,
        title,
    };
    Ok(lists(&candidate).then(|| Listed {
        target: Target::Window(window),
        title: candidate.title.unwrap_or_default(),
    }))
}

/// What a window is, for the one question the share menu asks of it.
struct Candidate {
    window: u32,
    root: u32,
    viewable: bool,
    input_only: bool,
    title: Option<String>,
}

/// Whether a window belongs in a share menu: one that is on the screen, can be
/// drawn, and has a title to name it by — and that is not the screen itself,
/// which is listed as a screen and never as a window.
///
/// The same rule as [`Plan::of`]'s, on the listing side: a person who picks a
/// window gets that window, and a menu that offered the root among the windows
/// would be offering the desktop twice under two names.
fn lists(candidate: &Candidate) -> bool {
    candidate.window != 0
        && candidate.window != candidate.root
        && candidate.viewable
        && !candidate.input_only
        && candidate
            .title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty())
}

/// A window's title from the two properties that carry one: the UTF-8 one
/// desktop standards define when it is there, and the older one otherwise, read
/// as UTF-8 because there is nothing better to read it as.
fn titled(net_wm_name: Option<&[u8]>, wm_name: Option<&[u8]>) -> Option<String> {
    let shown = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes).trim().to_owned();
        (!text.is_empty()).then_some(text)
    };
    net_wm_name
        .and_then(shown)
        .or_else(|| wm_name.and_then(shown))
}

/// Waits until the server has something to say, or `timeout` passes, without
/// reading any of it: the caller drains the connection itself.
///
/// Requests are only written when a reply is waited for or the connection is
/// flushed, and a wait sees nothing of them until they are, so they are flushed
/// first — which is also what makes a `DamageNotify` arrive at all.
fn wait(conn: &RustConnection, timeout: Duration) -> Result<()> {
    conn.flush().context("writing to the X server")?;
    // SAFETY: `describing` is one live `pollfd` whose descriptor is the
    // connection's own stream, open for as long as the connection is, and the
    // array's length is passed with it.
    let (ready, describing) = {
        let descriptor = conn.stream().as_fd().as_raw_fd();
        let mut describing = [libc::pollfd {
            fd: descriptor,
            events: libc::POLLIN,
            revents: 0,
        }];
        let millis = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        // SAFETY: as above: one initialized entry, and its own address.
        let ready = unsafe { libc::poll(describing.as_mut_ptr(), 1, millis) };
        (ready, describing[0].revents)
    };
    if ready < 0 {
        let error = std::io::Error::last_os_error();
        // A signal arriving while waiting is the wait being cut short, not the
        // server having nothing: the caller's next look finds what interrupted
        // it.
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error).context("waiting on the X server");
        }
    }
    let _ = describing;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        Arrival, Buffer, Candidate, Capture, Display, Layout, Plan, Region, Target, cut, listed,
        lists, titled,
    };
    use x11rb::connection::Connection as _;
    use x11rb::protocol::dri3;
    use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _, MapState, WindowClass};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    const ROOT: u32 = 0x6a0;

    /// A descriptor for a buffer a test never reads, so that a reply can be built
    /// without a server having exported one.
    fn spare() -> std::os::fd::OwnedFd {
        std::fs::File::open("/dev/null").expect("/dev/null").into()
    }

    fn exported(
        width: u16,
        height: u16,
        stride: u32,
        modifier: u64,
        depth: u8,
        bpp: u8,
    ) -> dri3::BuffersFromPixmapReply {
        dri3::BuffersFromPixmapReply {
            sequence: 0,
            length: 0,
            width,
            height,
            modifier,
            depth,
            bpp,
            strides: vec![stride],
            offsets: vec![0],
            buffers: vec![spare()],
        }
    }

    fn whole(width: u32, height: u32) -> Region {
        Region {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The clamp: a 24-bit picture of 32 bits a pixel is what every encoder here
    /// reads, and the stride and modifier the server gave travel with it.
    #[test]
    fn a_server_that_describes_bgra_is_handed_on() {
        let buffer = Buffer::from_buffers(
            exported(64, 4, 256, 0x0100_0000_0000_0001, 24, 32),
            whole(64, 4),
        )
        .expect("a BGRA pixmap");
        assert_eq!((buffer.width, buffer.height), (64, 4));
        assert_eq!(buffer.stride, 256, "a stride of its own, not width * 4");
        assert_eq!(buffer.offset, 0);
        assert_eq!(
            buffer.modifier,
            Some(0x0100_0000_0000_0001),
            "the modifier the server laid it out with"
        );
        assert_eq!((buffer.depth, buffer.bpp), (24, 32));
        assert_eq!(buffer.region, whole(64, 4));
    }

    /// Everything else a server can describe is refused by name rather than
    /// handed to an encoder that would read it as noise: a 16-bit screen, a
    /// 30-bit one, an empty picture, a stride too short for its own rows, and a
    /// pixmap in more planes than this hands on.
    #[test]
    fn a_picture_a_share_cannot_read_is_refused_by_name() {
        let narrow = exported(64, 4, 256, 0, 16, 16);
        let why = format!(
            "{:#}",
            Buffer::from_buffers(narrow, whole(64, 4)).expect_err("16-bit")
        );
        assert!(why.contains("16-bit"), "{why}");

        let deep = exported(64, 4, 512, 0, 30, 32);
        let why = format!(
            "{:#}",
            Buffer::from_buffers(deep, whole(64, 4)).expect_err("30-bit")
        );
        assert!(why.contains("30-bit"), "{why}");

        let empty = exported(0, 0, 0, 0, 24, 32);
        let why = format!(
            "{:#}",
            Buffer::from_buffers(empty, whole(0, 0)).expect_err("empty")
        );
        assert!(why.contains("empty"), "{why}");

        let short = exported(64, 4, 128, 0, 24, 32);
        let why = format!(
            "{:#}",
            Buffer::from_buffers(short, whole(64, 4)).expect_err("short")
        );
        assert!(why.contains("stride"), "{why}");

        let mut planes = exported(64, 4, 256, 0, 24, 32);
        planes.strides.push(256);
        planes.offsets.push(256 * 4);
        planes.buffers.push(spare());
        let why = format!(
            "{:#}",
            Buffer::from_buffers(planes, whole(64, 4)).expect_err("planes")
        );
        assert!(why.contains("2-plane"), "{why}");

        // A reply that lost its layout is a server that answered nothing usable,
        // rather than a buffer with a guessed one.
        let mut bare = exported(64, 4, 256, 0, 24, 32);
        bare.strides.clear();
        bare.offsets.clear();
        let why = format!(
            "{:#}",
            Buffer::from_buffers(bare, whole(64, 4)).expect_err("bare")
        );
        assert!(why.contains("no strides"), "{why}");
    }

    /// A DRI3 1.0 server says how wide a row is and how big the picture is, and
    /// nothing about how it was laid out: the buffer is handed on with no
    /// modifier rather than with a guessed one.
    #[test]
    fn an_older_server_exports_a_buffer_with_no_modifier() {
        let reply = dri3::BufferFromPixmapReply {
            nfd: 1,
            sequence: 0,
            length: 0,
            size: 64 * 4 * 4,
            width: 64,
            height: 4,
            stride: 256,
            depth: 24,
            bpp: 32,
            pixmap_fd: spare(),
        };
        let buffer = Buffer::from_buffer(reply, whole(64, 4)).expect("a BGRA pixmap");
        assert_eq!(buffer.modifier, None);
        assert_eq!((buffer.stride, buffer.offset), (256, 0));
    }

    /// A monitor is a rectangle on the screen, so its region is that rectangle
    /// and never more of the buffer than the screen has: one that a mode change
    /// left hanging off the edge is captured as the part still on it.
    #[test]
    fn a_monitor_is_cut_out_of_the_screen_and_never_past_it() {
        let screen = (1920, 1080);
        let inside = Display {
            name: "eDP-1".to_owned(),
            x: 0,
            y: 0,
            width: 1280,
            height: 1080,
        };
        assert_eq!(super::cut(&inside, screen), whole(1280, 1080));
        let beside = Display {
            name: "HDMI-1".to_owned(),
            x: 1280,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            cut(&beside, screen),
            Region {
                x: 1280,
                y: 0,
                // 1920 wide on a screen with 640 left of it.
                width: 640,
                height: 1080,
            }
        );
        let hung = Display {
            name: "DP-3".to_owned(),
            x: -100,
            y: 2000,
            width: 800,
            height: 600,
        };
        let cut = cut(&hung, screen);
        assert_eq!(cut.y, 1080, "below the screen is off it");
        assert!(cut.is_empty(), "nothing of it is left");
    }

    /// A window is captured as itself, and the root is refused by name: on X11
    /// the root is the desktop, so a "window" that is the root would be a screen
    /// capture under a window's name.
    #[test]
    fn a_window_is_never_the_desktop_it_sits_on() {
        assert_eq!(Plan::of(&Target::Screen, ROOT).unwrap(), Plan::Screen);
        assert_eq!(
            Plan::of(
                &Target::Display(Display {
                    name: "eDP-1".to_owned(),
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                }),
                ROOT
            )
            .unwrap(),
            Plan::Screen,
            "a monitor is the screen, cut to its part"
        );
        assert_eq!(
            Plan::of(&Target::Window(0x1200003), ROOT).unwrap(),
            Plan::Window(0x1200003)
        );
        let why = format!(
            "{:#}",
            Plan::of(&Target::Window(ROOT), ROOT).expect_err("the root")
        );
        assert!(why.contains("root window"), "{why}");
        let why = format!(
            "{:#}",
            Plan::of(&Target::Window(0), ROOT).expect_err("no window")
        );
        assert!(why.contains("no window"), "{why}");
    }

    /// The share menu lists a window that is on the screen with a title, and
    /// nothing else: not the root, not an unmapped window, not an input-only one,
    /// and not one that would show as a blank line.
    #[test]
    fn a_window_is_listed_only_when_it_is_one_to_show() {
        let firefox = candidate();
        assert!(lists(&firefox));
        for (what, one) in [
            (
                "the root",
                Candidate {
                    window: ROOT,
                    ..candidate()
                },
            ),
            (
                "nothing",
                Candidate {
                    window: 0,
                    ..candidate()
                },
            ),
            (
                "unmapped",
                Candidate {
                    viewable: false,
                    ..candidate()
                },
            ),
            (
                "input only",
                Candidate {
                    input_only: true,
                    ..candidate()
                },
            ),
            (
                "untitled",
                Candidate {
                    title: None,
                    ..candidate()
                },
            ),
            (
                "spaces",
                Candidate {
                    title: Some("   ".to_owned()),
                    ..candidate()
                },
            ),
        ] {
            assert!(!lists(&one), "{what} is not a window to share");
        }
    }

    /// A window that is one to share: on the screen, drawn, with a title.
    fn candidate() -> Candidate {
        Candidate {
            window: 0x1200003,
            root: ROOT,
            viewable: true,
            input_only: false,
            title: Some("Firefox".to_owned()),
        }
    }

    /// A title is the UTF-8 one the desktop standards define when the window has
    /// it, and the older one otherwise, which is read as UTF-8 because there is
    /// nothing better; `WM_NAME` is the encoding the property says it is.
    #[test]
    fn a_title_is_the_utf8_one_when_there_is_one() {
        assert_eq!(
            titled(Some(b"Firefox"), Some(b"firefox-bin")),
            Some("Firefox".to_owned())
        );
        assert_eq!(
            titled(None, Some(b"firefox-bin")),
            Some("firefox-bin".to_owned())
        );
        assert_eq!(
            titled(Some(b"  "), Some(b"firefox-bin")),
            Some("firefox-bin".to_owned())
        );
        assert_eq!(
            titled(Some(b"Firefox"), Some(b"  ")),
            Some("Firefox".to_owned())
        );
        assert_eq!(titled(None, None), None);
        assert_eq!(
            titled(Some(b"\xff\xfe"), None),
            Some("\u{fffd}\u{fffd}".to_owned())
        );
    }

    /// The layout the clamp keeps is the one the buffer is made from, and a
    /// region a monitor cut out of the screen travels with it.
    #[test]
    fn the_layout_is_the_buffers_own() {
        let layout = Layout::of(32, 8, 128, 4, Some(0), 24, 32).expect("BGRA");
        let buffer = layout.with(spare(), whole(30, 8));
        assert_eq!((buffer.width, buffer.height), (32, 8));
        assert_eq!((buffer.stride, buffer.offset), (128, 4));
        assert_eq!(buffer.region, whole(30, 8));
        assert_eq!(buffer.modifier, Some(0));
    }

    /// A connection of its own with a window on it, as an application has: a
    /// capture redirects a window from the capture's connection, so the window
    /// has to be another client's to be the thing being captured.
    struct App {
        conn: RustConnection,
        root: u32,
        window: u32,
    }

    impl App {
        fn open(title: &str) -> Self {
            let (conn, screen) = x11rb::connect(None).expect("an X server");
            let setup = &conn.setup().roots[screen];
            let root = setup.root;
            let window = conn.generate_id().expect("an id");
            conn.create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                64,
                48,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                // Override-redirect so that the X server maps it at once:
                // Xwayland finishes an ordinary window's mapping only when the
                // compositor behind it has the window, which is a wait this
                // test has no reason to take.
                &xproto::CreateWindowAux::new()
                    .override_redirect(1)
                    .background_pixel(setup.white_pixel),
            )
            .expect("a window");
            conn.map_window(window).expect("a mapping");
            conn.change_property8(
                xproto::PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .expect("a title");
            conn.sync().expect("the server to agree");
            let app = Self { conn, root, window };
            // A window is not on the screen the moment it is mapped on Xwayland:
            // the mapping is finished by the compositor behind the X server, so
            // this waits for the server to say it is there.
            for _ in 0..60 {
                if app.viewable() {
                    return app;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            panic!("window 0x{window:x} never came on the screen");
        }

        /// Whether the server says the window is on the screen.
        fn viewable(&self) -> bool {
            self.conn
                .get_window_attributes(self.window)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .is_some_and(|attributes| attributes.map_state == MapState::VIEWABLE)
        }

        fn resize(&self, width: u16, height: u16) {
            self.conn
                .configure_window(
                    self.window,
                    &xproto::ConfigureWindowAux::new()
                        .width(u32::from(width))
                        .height(u32::from(height)),
                )
                .expect("a resize");
            self.conn.sync().expect("the server to agree");
        }

        /// Marks the window the way a window manager does, for a desktop that has
        /// none: `WM_STATE` is what makes a window a client's rather than a
        /// container's.
        fn marked(&self) {
            let state = self
                .conn
                .intern_atom(false, b"WM_STATE")
                .expect("an atom")
                .reply()
                .expect("a reply")
                .atom;
            self.conn
                .change_property32(
                    xproto::PropMode::REPLACE,
                    self.window,
                    state,
                    state,
                    &[1, 0],
                )
                .expect("WM_STATE");
            self.conn.sync().expect("the server to agree");
        }

        /// Whether a window manager on this server keeps the desktop's own list
        /// of client windows, which is the list a share menu starts from.
        fn keeps_a_client_list(&self) -> bool {
            let atom = self
                .conn
                .intern_atom(false, b"_NET_CLIENT_LIST")
                .expect("an atom")
                .reply()
                .expect("a reply")
                .atom;
            self.conn
                .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 4096)
                .expect("a request")
                .reply()
                .expect("a reply")
                .format
                == 32
        }
    }

    /// A window captured through Composite, end to end, where the server can
    /// export at all: its picture on the first ask, its resize reported, and its
    /// closing the end of the capture. A server that cannot export — Xvfb has no
    /// device to export from — is a failure that names why, which is what a share
    /// falls back to the processor on.
    #[test]
    #[ignore = "needs an X server, as from `env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run -a`"]
    fn a_window_is_captured_as_a_buffer_or_says_why() {
        let app = App::open("yapfast capture test");
        let mut capture = match Capture::open(Target::Window(app.window)) {
            Ok(capture) => {
                eprintln!("opened {}", capture.describe());
                capture
            }
            Err(why) => {
                let why = format!("{why:#}");
                eprintln!("no buffer from this server: {why}");
                assert!(!why.trim().is_empty(), "a refusal says why");
                return;
            }
        };
        // The first look owes the picture: a window that opens and then sits
        // still draws nothing more to wait for.
        let picture = match capture.frame(Duration::from_millis(200)).expect("a look") {
            Arrival::Frame(picture) => picture,
            other => panic!("a still window is owed its picture, and answered {other:?}"),
        };
        let buffer = picture.buffer();
        assert_eq!((buffer.width, buffer.height), (64, 48));
        assert_eq!((buffer.depth, buffer.bpp), (24, 32));
        assert!(buffer.stride >= 64 * 4, "a stride of {}", buffer.stride);
        // Nothing has been drawn since, so nothing is new — or the server has no
        // XDamage and every frame is reported as one.
        match capture.frame(Duration::from_millis(50)).expect("a look") {
            Arrival::Still => assert!(capture.damaged(), "without damage, nothing is still"),
            Arrival::Frame(_) => {
                assert!(!capture.damaged(), "a damage server said nothing was new")
            }
            other => panic!("{other:?} of a window nothing was drawn into"),
        }
        app.resize(96, 72);
        let mut resized = false;
        for _ in 0..40 {
            if matches!(
                capture.frame(Duration::from_millis(50)).expect("a look"),
                Arrival::Resized
            ) {
                resized = true;
                break;
            }
        }
        assert!(resized, "the resize was reported");
        assert_eq!(capture.content_size(), (96, 72));
        let picture = match capture.frame(Duration::from_millis(200)).expect("a look") {
            Arrival::Frame(picture) => picture,
            other => panic!("the first frame at the new size, and it answered {other:?}"),
        };
        assert_eq!(
            (picture.buffer().width, picture.buffer().height),
            (96, 72),
            "the buffer was exported again at the new size"
        );
        app.conn
            .destroy_window(app.window)
            .expect("the window to close");
        app.conn.sync().expect("the server to agree");
        let mut closed = false;
        for _ in 0..40 {
            if matches!(
                capture.frame(Duration::from_millis(50)).expect("a look"),
                Arrival::Closed
            ) {
                closed = true;
                break;
            }
        }
        assert!(closed, "a closed window ends the capture");
        eprintln!("ended: {}", capture.ending());
        assert!(capture.ending().contains("closed"), "{}", capture.ending());
        assert!(
            matches!(
                capture.frame(Duration::ZERO).expect("a look"),
                Arrival::Closed
            ),
            "a capture that ended stays ended"
        );
    }

    /// The screen is the overlay window's picture, which needs a compositing
    /// manager to draw the screen into it: a server with none is refused by name
    /// rather than shared as an empty rectangle, and a share falls back to the
    /// processor's capturer.
    #[test]
    #[ignore = "needs an X server, as from `env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run -a`"]
    fn a_screen_is_the_overlay_windows_picture_or_says_why() {
        match Capture::open(Target::Screen) {
            Ok(mut capture) => {
                eprintln!("opened {}", capture.describe());
                let (width, height) = capture.content_size();
                assert!(width > 0 && height > 0, "{width}x{height}");
                match capture.frame(Duration::from_millis(500)).expect("a look") {
                    Arrival::Frame(picture) => {
                        let buffer = picture.buffer();
                        assert!(buffer.region.width <= buffer.width);
                        assert!(buffer.region.height <= buffer.height);
                        assert_eq!((buffer.depth, buffer.bpp), (24, 32));
                    }
                    other => eprintln!("nothing drawn yet: {other:?}"),
                }
            }
            Err(why) => {
                let why = format!("{why:#}");
                eprintln!("this server has no screen to capture: {why}");
                assert!(
                    why.contains("compositing manager")
                        || why.contains("DRI3")
                        || why.contains("Composite")
                        || why.contains("storage")
                        || why.contains("device"),
                    "a refusal names what is missing: {why}"
                );
            }
        }
    }

    /// What a share menu shows on this X server: the screen's monitors and the
    /// windows on it, never the screen under a window's name, and never an entry
    /// without a title.
    #[test]
    #[ignore = "needs an X server, as from `env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run -a`"]
    fn what_there_is_to_share_is_listed() {
        let app = App::open("yapfast listing test");
        // A window manager marks a client window with WM_STATE, and a desktop
        // with none is walked for exactly that mark.
        app.marked();
        let listed = listed().expect("a list to share");
        eprintln!("{listed:#?}");
        assert!(!listed.is_empty(), "a screen at least");
        for one in &listed {
            assert_ne!(
                one.target,
                Target::Window(app.root),
                "the root window is no window to share"
            );
            assert!(!one.title.trim().is_empty(), "{one:?} has no title");
        }
        assert!(
            listed
                .iter()
                .any(|one| matches!(one.target, Target::Screen | Target::Display(_))),
            "the screen is always there"
        );
        // A manager's own client list is the desktop's answer, and this test's
        // window is not in it; a server with none is walked, and there it is.
        if !app.keeps_a_client_list() {
            let found = listed
                .iter()
                .find(|one| one.target == Target::Window(app.window));
            assert_eq!(
                found.map(|one| one.title.as_str()),
                Some("yapfast listing test"),
                "the window this test made is listed by its title"
            );
        }
    }
}
