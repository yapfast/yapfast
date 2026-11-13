//! Which way a screen is captured, and the setting that decides it.
//!
//! Windows offers two ways to a screen as a texture: Windows Graphics Capture,
//! whose frames arrive with the pointer already in them, and DXGI's Desktop
//! Duplication, which hands over the compositor's own surface and has the
//! pointer drawn in by the shim. What they don't share is the border. Graphics
//! Capture draws a yellow outline around what it captures until
//! `IsBorderRequired(false)` takes it off, and that interface —
//! `IGraphicsCaptureSession3` — exists from Windows 11 22H2; a Windows 10 has no
//! way to remove it at all. A stream on Windows 10 that opened Graphics Capture
//! first therefore showed the outline while Duplication beside it would have
//! been clean.
//!
//! So the order the two are tried in is chosen from what the platform can do,
//! not fixed: [`Preference::Auto`] puts the rung that can be borderless first,
//! Duplication on a Windows that can't take the border off and Graphics Capture
//! on one that can. The setting's other values are an override for a machine
//! where the choice is wrong — a capture that opens but looks wrong, a driver
//! whose one way is broken — and the other rung stays as the fallback, so
//! choosing one can never leave a share with nothing to try.
//!
//! The choice is made once per share, before the capture opens, and costs
//! nothing a frame. It is the same shape as [`crate::gpu`] and
//! [`crate::noise`]: a process-wide value the UI sets from `layout.json`.
//!
//! What is left of this module is the ways a Linux session captures a screen
//! without that setting, each handing a share a picture the GPU made rather than
//! one the processor assembled: the desktop's screen sharing portal and the
//! PipeWire buffers it names (`capture::portal`, `capture::pipewire`), for a
//! session that shares through it, and `capture::x11`: a window or the screen
//! redirected through the X Composite extension, and its storage exported as a
//! DMA-BUF, for a session whose X server is what captures.
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// A window or the screen on X11, as a DMA-BUF, for a session whose X server
/// is what captures: one with no screen sharing portal, as XFCE has.
#[cfg(target_os = "linux")]
pub mod x11;

/// A camera on Linux as a DMA-BUF, for a device that films NV12 and exports what
/// it captures: the picture reaches the encoder where the driver left it.
#[cfg(target_os = "linux")]
pub mod v4l2;

/// The desktop's screen sharing portal, as far as a capture asks it for a screen:
/// a session, what to share, and the PipeWire node the desktop exported.
#[cfg(screencast)]
mod portal;

/// A screen or a window on Linux as the DMA-BUF buffers a PipeWire node carries,
/// for a session whose desktop shares its screen through the portal; see
/// [`pipewire`].
#[cfg(screencast)]
pub mod pipewire;

/// A picture the card drew, read back into memory: the one copy a share whose
/// pictures are DMA-BUFs is allowed, which is Discord's still of the stream.
#[cfg(target_os = "linux")]
pub mod linear;

/// Which way a share captures a screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preference {
    /// Whichever way can be borderless on this machine: Desktop Duplication on a
    /// Windows whose Graphics Capture cannot take its border off, Graphics
    /// Capture on one that can.
    #[default]
    Auto,
    /// Windows Graphics Capture first, for its frames arriving with nothing
    /// drawn into them.
    GraphicsCapture,
    /// DXGI's Desktop Duplication first, which draws no border on any Windows.
    Duplication,
}

/// What a Windows build has to be for Graphics Capture to be able to take its
/// border off: `IGraphicsCaptureSession3`, the interface carrying
/// `IsBorderRequired`, arrived with Windows 11 22H2, build 22621. Windows 11's
/// first release, build 22000, has no such interface, and neither does any
/// Windows 10.
pub const BORDER_REMOVAL_BUILD: u32 = 22621;

/// The build of Windows a share runs on, for the one question the order asks of
/// it: whether Graphics Capture can be borderless here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    build: u32,
}

impl Platform {
    /// The platform with `build` as its Windows build number, as
    /// `RtlGetVersion` reports it.
    pub const fn new(build: u32) -> Self {
        Self { build }
    }

    /// Whether Graphics Capture on this build has the interface that can take
    /// its yellow border off. A build number of 0 — one that couldn't be asked —
    /// reads as one without it, which is the safe answer: Duplication draws no
    /// border, and a capture kept because it seemed borderless is the complaint
    /// this exists to fix.
    pub const fn can_remove_border(self) -> bool {
        self.build >= BORDER_REMOVAL_BUILD
    }
}

/// One way of capturing a screen, as the card's capture path knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    /// Windows Graphics Capture, whose frames need nothing drawn into them and
    /// which draws an outline until it is told not to.
    Graphics,
    /// DXGI's Desktop Duplication, which draws no border and paints the pointer
    /// into the frame itself.
    Duplication,
}

impl Rung {
    /// What this rung is called in the one log line that says which ran.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Graphics => "Windows Graphics Capture",
            Self::Duplication => "Desktop Duplication",
        }
    }
}

/// Which of the two ways this build of Yapfast carries. A build without the
/// Windows SDK's capture headers has no Graphics Capture to open, and every
/// Windows build has Desktop Duplication; asking here keeps the order honest
/// without a `cfg` inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Available {
    pub graphics: bool,
    pub duplication: bool,
}

impl Available {
    const fn has(self, rung: Rung) -> bool {
        match rung {
            Rung::Graphics => self.graphics,
            Rung::Duplication => self.duplication,
        }
    }
}

/// The rungs a share tries, in the order it tries them: the one the setting
/// asks for, or under [`Preference::Auto`] the one that can be borderless on
/// this build of Windows, and then the other. Rungs this build doesn't carry
/// aren't in it, so a build with one way has one entry and a build with neither
/// has none — which is a share that stays on the processor.
///
/// This is the whole of the ordering, and it is a function of what the machine
/// is rather than of what a capture reported, so it can be checked without one.
/// A capture that opens and is still bordered is the runtime half: see
/// [`keeps`].
pub fn order(preference: Preference, platform: Platform, available: Available) -> Vec<Rung> {
    let (first, second) = match preference {
        Preference::GraphicsCapture => (Rung::Graphics, Rung::Duplication),
        Preference::Duplication => (Rung::Duplication, Rung::Graphics),
        // The border is the one difference between the two that a viewer sees,
        // so Auto is not "the better one first" but "the one that can be
        // clean here first".
        Preference::Auto if platform.can_remove_border() => (Rung::Graphics, Rung::Duplication),
        Preference::Auto => (Rung::Duplication, Rung::Graphics),
    };
    [first, second]
        .into_iter()
        .filter(|rung| available.has(*rung))
        .collect()
}

/// Whether the capture that just opened is the one the share keeps.
///
/// Under [`Preference::Auto`], a Graphics Capture whose border is still on is
/// the complaint this whole choice exists for: when there is another rung to
/// try, the capture is let go in full and that rung opens, rather than a share
/// that works and shows an outline nobody asked for. It is kept when there is
/// nothing else to try — a capture with an outline is a stream, and losing the
/// picture would be worse than the outline. An explicit preference is an answer
/// to this question, so it keeps what it asked for.
///
/// `borderless` is what the rung answered about a border it draws: `Some` for
/// Graphics Capture, and `None` for a rung with no border to ask about.
pub fn keeps(
    preference: Preference,
    rung: Rung,
    borderless: Option<bool>,
    next: Option<Rung>,
) -> bool {
    let bordered =
        preference == Preference::Auto && rung == Rung::Graphics && borderless == Some(false);
    !(bordered && next.is_some())
}

static PREFERENCE: AtomicU8 = AtomicU8::new(Preference::Auto as u8);

/// Chooses how every share started from now on captures its screen. A share
/// that is already running keeps the capture it started with, so the choice
/// takes effect at the next one.
pub fn set(preference: Preference) {
    PREFERENCE.store(preference as u8, Ordering::Relaxed);
}

pub fn preference() -> Preference {
    match PREFERENCE.load(Ordering::Relaxed) {
        1 => Preference::GraphicsCapture,
        2 => Preference::Duplication,
        _ => Preference::Auto,
    }
}

/// The build of Windows this process runs on, as the shim answers it. Only a
/// build with native/wgc.cc can ask, and a build without it has no Graphics
/// Capture rung at all, so the answer it would use never decides anything: with
/// no way to ask, `0` reads as "no border to remove", which is true of every
/// capture such a build can make.
#[cfg(wgc)]
pub fn platform() -> Platform {
    Platform::new(crate::wgc::build())
}

#[cfg(all(dup, not(wgc)))]
pub fn platform() -> Platform {
    Platform::new(0)
}

/// One way a Linux session captures a screen on the card.
///
/// They differ in which door they come through and not in what they hand over:
/// a DMA-BUF the compositor drew, which the window imports where it lies. That
/// is why there is no setting for them the way Windows has one for its two
/// rungs: the border that choice is about doesn't exist here, and the portal
/// rung is strictly the better one where it is there.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxRung {
    /// The desktop's screen sharing portal and the PipeWire node it names, whose
    /// buffers are the compositor's own: nothing of a frame passes through this
    /// process on its way to the window.
    Portal,
    /// The X server's Composite redirection, exported through DRI3 as a
    /// DMA-BUF, for a session whose X server is what captures.
    X11,
}

#[cfg(target_os = "linux")]
impl LinuxRung {
    /// What this rung is called in the one log line that says which ran.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Portal => "the screen sharing portal's PipeWire stream",
            Self::X11 => "X11's DRI3 capture",
        }
    }
}

/// Whether a Linux session is Wayland, which decides what an X11 capture is:
/// under Wayland it is Xwayland's windows and not the desktop they sit on.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    wayland: bool,
}

#[cfg(target_os = "linux")]
impl Session {
    pub const fn new(wayland: bool) -> Self {
        Self { wayland }
    }

    pub const fn wayland(self) -> bool {
        self.wayland
    }
}

/// What a Linux session can capture a screen with, as the rung choice asks: the
/// facts a build and a desktop supply, kept apart from the captures themselves
/// so that the choice can be checked without a portal, a DRI3 export or even an
/// X server.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Linux {
    /// Whether a screen sharing portal answers this session's bus, and this
    /// build carries the capture that reads the PipeWire stream it names. The
    /// portal answering is not the person saying yes: that answer comes from the
    /// dialog, when the capture opens, and a refusal is the fallback.
    pub portal: bool,
    /// Whether this session names an X server for a capture to open. Whether
    /// that server exports a redirected pixmap through DRI3 is the opening
    /// capture's answer rather than this one's, and a server that doesn't is a
    /// rung that refused.
    pub x11: bool,
}

/// The rungs a Linux share tries, in the order it tries them: the desktop's own
/// portal first, since its frames are the compositor's and nothing in this
/// process copies them, and then the X server.
///
/// A Wayland session is never offered the X11 rung, whoever answers the portal
/// and whether one does at all: what an X11 capture sees under Wayland is
/// Xwayland's windows and not the desktop they sit on, and the screen it names
/// is Xwayland's overlay window, whose storage is empty — a share of nothing, and
/// a refusal in the log that reads as if it were about the desktop. The X server
/// is an X session's rung alone, and a Wayland share with no portal stays on the
/// processor, where libwebrtc asks the portal itself. A session with neither is a
/// share that stays on the processor — which is what every Linux share did before
/// this.
#[cfg(target_os = "linux")]
pub fn linux_order(session: Session, available: Linux) -> Vec<LinuxRung> {
    let mut rungs = Vec::new();
    if available.portal {
        rungs.push(LinuxRung::Portal);
    }
    if available.x11 && !session.wayland() {
        rungs.push(LinuxRung::X11);
    }
    rungs
}

/// Adds one rung's refusal to the line a share reports, in the order they were
/// tried, naming the rung and what it said.
#[cfg(target_os = "linux")]
pub fn refused(into: &mut String, rung: LinuxRung, reason: &str) {
    if !into.is_empty() {
        into.push_str("; ");
    }
    into.push_str(rung.name());
    into.push_str(": ");
    into.push_str(reason);
}

/// Why a Linux share captures the way it does, in the sentence its one log line
/// carries. With nothing refused it is the fact that decided the order — a
/// portal that answers, or a session that has none; with refusals before it, it
/// is those and the rung that came next, which is where a desktop whose portal
/// answered and then wouldn't open says so in the portal's own words.
#[cfg(target_os = "linux")]
pub fn linux_why(session: Session, available: Linux, rung: LinuxRung, refused: &str) -> String {
    if !refused.is_empty() {
        return format!("{refused}; {} came next", rung.name());
    }
    match rung {
        LinuxRung::Portal if session.wayland() => {
            "a Wayland session shares its screen through the desktop's portal"
        }
        LinuxRung::Portal => {
            "this desktop answers the screen sharing portal, so its own pictures are what a share \
             gets"
        }
        LinuxRung::X11 if !available.portal => {
            "this desktop has no screen sharing portal, so the X server's own capture runs"
        }
        LinuxRung::X11 => "the portal wouldn't open, so the X server's own capture runs",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        Available, BORDER_REMOVAL_BUILD, Platform, Preference, Rung, keeps, order, preference, set,
    };

    fn both() -> Available {
        Available {
            graphics: true,
            duplication: true,
        }
    }

    fn only_duplication() -> Available {
        Available {
            graphics: false,
            duplication: true,
        }
    }

    fn only_graphics() -> Available {
        Available {
            graphics: true,
            duplication: false,
        }
    }

    /// A Windows 10 22H2 build, and a Windows 11 22H2 one: the build where the
    /// interface arrived is the boundary, and both sides of it are here.
    const WINDOWS_10: u32 = 19045;
    const WINDOWS_11: u32 = BORDER_REMOVAL_BUILD;

    /// The UI's layout.json cut to this one field, which a file written before
    /// the setting has no key for.
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Layout {
        capture: Preference,
    }

    #[test]
    fn auto_is_the_default_and_layout_json_names_it_in_kebab_case() {
        assert_eq!(Preference::default(), Preference::Auto);
        // The words the file holds, as `noise_suppression` holds `"rnnoise"`.
        assert_eq!(
            serde_json::to_string(&Preference::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&Preference::GraphicsCapture).unwrap(),
            "\"graphics-capture\""
        );
        assert_eq!(
            serde_json::to_string(&Preference::Duplication).unwrap(),
            "\"duplication\""
        );
        assert_eq!(
            serde_json::from_str::<Layout>("{\"capture\":\"duplication\"}")
                .unwrap()
                .capture,
            Preference::Duplication
        );
        assert_eq!(
            serde_json::from_str::<Layout>("{\"capture\":\"graphics-capture\"}")
                .unwrap()
                .capture,
            Preference::GraphicsCapture
        );
        // A file from before the setting is Auto, which is what a share did
        // before it existed — except on a Windows 10, where it now does better.
        assert_eq!(
            serde_json::from_str::<Layout>("{}").unwrap().capture,
            Preference::Auto
        );
    }

    /// Auto asks the platform which rung can be borderless: Graphics Capture
    /// first where its session can take the border off, Duplication first where
    /// it can't. The build where the interface arrived is the line.
    #[test]
    fn auto_puts_the_rung_that_can_be_borderless_first() {
        for build in [WINDOWS_10, 22000] {
            assert!(!Platform::new(build).can_remove_border(), "{build}");
            assert_eq!(
                order(Preference::Auto, Platform::new(build), both()),
                [Rung::Duplication, Rung::Graphics],
                "a build at {build} cannot take the border off"
            );
        }
        for build in [WINDOWS_11, 26100] {
            assert!(Platform::new(build).can_remove_border(), "{build}");
            assert_eq!(
                order(Preference::Auto, Platform::new(build), both()),
                [Rung::Graphics, Rung::Duplication],
                "a build at {build} can take the border off"
            );
        }
        // A build number that couldn't be asked reads as one without the
        // interface: an outline is the thing being avoided.
        assert_eq!(
            order(Preference::Auto, Platform::new(0), both()),
            [Rung::Duplication, Rung::Graphics]
        );
    }

    /// The advanced setting is the override, and it is the whole of the order:
    /// the rung it names is first on any build, and the other is still there to
    /// fall back to.
    #[test]
    fn the_setting_chooses_the_first_rung_on_every_build() {
        for build in [WINDOWS_10, WINDOWS_11] {
            let platform = Platform::new(build);
            assert_eq!(
                order(Preference::GraphicsCapture, platform, both()),
                [Rung::Graphics, Rung::Duplication],
                "{build}"
            );
            assert_eq!(
                order(Preference::Duplication, platform, both()),
                [Rung::Duplication, Rung::Graphics],
                "{build}"
            );
        }
    }

    /// A build with one way tries it and nothing else, whichever way the
    /// preference or the platform would have chosen; a build with neither has
    /// nothing to try, which is a share that stays on the processor.
    #[test]
    fn a_build_with_one_rung_tries_only_that_one() {
        let prefs = [
            Preference::Auto,
            Preference::GraphicsCapture,
            Preference::Duplication,
        ];
        for build in [WINDOWS_10, WINDOWS_11] {
            let platform = Platform::new(build);
            for preference in prefs {
                assert_eq!(
                    order(preference, platform, only_duplication()),
                    [Rung::Duplication],
                    "{preference:?} at {build}"
                );
                assert_eq!(
                    order(preference, platform, only_graphics()),
                    [Rung::Graphics],
                    "{preference:?} at {build}"
                );
                assert!(
                    order(
                        preference,
                        platform,
                        Available {
                            graphics: false,
                            duplication: false,
                        }
                    )
                    .is_empty(),
                    "{preference:?} at {build}"
                );
            }
        }
    }

    /// The case this was written for: Auto, a Graphics Capture that opened and
    /// reported its border still on, and Duplication beside it. The capture is
    /// not the one to keep, and the next rung to open is Duplication.
    #[test]
    fn auto_lets_graphics_capture_go_for_duplication_when_the_border_stays_on() {
        let platform = Platform::new(WINDOWS_11);
        let rungs = order(Preference::Auto, platform, both());
        assert_eq!(rungs, [Rung::Graphics, Rung::Duplication]);
        assert!(!keeps(
            Preference::Auto,
            rungs[0],
            Some(false),
            rungs.get(1).copied()
        ));
        assert_eq!(rungs.get(1), Some(&Rung::Duplication));
    }

    /// A bordered capture is still kept when it is the only way there is, and
    /// one whose border came off is kept anywhere; the setting's own answer is
    /// kept even with the border on, because that is what it asked for.
    #[test]
    fn a_bordered_capture_is_kept_when_nothing_else_can_open() {
        assert!(keeps(Preference::Auto, Rung::Graphics, Some(false), None));
        assert!(keeps(
            Preference::Auto,
            Rung::Graphics,
            Some(true),
            Some(Rung::Duplication)
        ));
        assert!(keeps(
            Preference::GraphicsCapture,
            Rung::Graphics,
            Some(false),
            Some(Rung::Duplication)
        ));
        assert!(keeps(
            Preference::Duplication,
            Rung::Duplication,
            None,
            Some(Rung::Graphics)
        ));
        assert!(keeps(
            Preference::Auto,
            Rung::Duplication,
            None,
            Some(Rung::Graphics)
        ));
    }

    /// The rung choice on Linux is a function of what the session and the build
    /// answer, and not of a capture: the portal first wherever it answers,
    /// because its frames are the compositor's own, and the X server after it.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_rungs_are_the_portal_first_and_then_the_x_server() {
        use super::{Linux, LinuxRung, Session, linux_order};

        let both = Linux {
            portal: true,
            x11: true,
        };
        // An X11 desktop with a portal, such as GNOME on X11: the portal is
        // the compositor's own pictures, so it comes first, and the X server
        // is what runs when the portal's dialog is cancelled.
        assert_eq!(
            linux_order(Session::new(false), both),
            [LinuxRung::Portal, LinuxRung::X11]
        );
        // A Wayland session: the portal, and no X11 rung after it, since what
        // an X11 capture sees there is Xwayland's windows and not the desktop.
        assert_eq!(linux_order(Session::new(true), both), [LinuxRung::Portal]);
        // A desktop with no portal, as XFCE on X11: the X server's DRI3
        // export is the only way to a picture the card drew.
        assert_eq!(
            linux_order(
                Session::new(false),
                Linux {
                    portal: false,
                    x11: true
                }
            ),
            [LinuxRung::X11]
        );
        // A Wayland session with no portal has no rung here at all, rather
        // than Xwayland's: the screen an X11 capture would name there is the
        // overlay window's empty storage, and the share belongs to libwebrtc's
        // capturer, which asks the portal itself.
        assert!(
            linux_order(
                Session::new(true),
                Linux {
                    portal: false,
                    x11: true
                }
            )
            .is_empty()
        );
        // Neither is a share that stays on the processor.
        assert!(linux_order(Session::new(false), Linux::default()).is_empty());
        assert!(linux_order(Session::new(true), Linux::default()).is_empty());
    }

    /// What a share says it did, in the one line a reader looks at: the fact
    /// that put the rung first, or the refusals that came before it, in the
    /// order they were tried and in each rung's own words.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_share_says_why_it_captures_the_way_it_does() {
        use super::{Linux, LinuxRung, Session, linux_why, refused};

        let both = Linux {
            portal: true,
            x11: true,
        };
        let none = Linux::default();
        assert!(linux_why(Session::new(true), both, LinuxRung::Portal, "").contains("Wayland"));
        assert!(
            linux_why(Session::new(false), both, LinuxRung::Portal, "")
                .contains("answers the screen sharing portal")
        );
        assert!(
            linux_why(Session::new(false), none, LinuxRung::X11, "")
                .contains("no screen sharing portal")
        );
        // A portal that answered and then wouldn't open: its refusal is the
        // sentence, and the rung that came next is named after it.
        let mut refusals = String::new();
        refused(&mut refusals, LinuxRung::Portal, "the person said no");
        let why = linux_why(Session::new(true), both, LinuxRung::X11, &refusals);
        assert_eq!(
            why,
            "the screen sharing portal's PipeWire stream: the person said no; X11's DRI3 \
             capture came next"
        );
        // And one refusal after another keeps the order they were tried in.
        refused(&mut refusals, LinuxRung::X11, "no DRI3 here");
        assert_eq!(
            refusals,
            "the screen sharing portal's PipeWire stream: the person said no; X11's DRI3 capture: \
             no DRI3 here"
        );
    }

    /// The choice is one value for the process, as the GPU's and the
    /// suppressor's are, and Auto until something sets it. Put back at the end
    /// because the tests run beside each other.
    #[test]
    fn the_choice_is_process_wide() {
        let before = preference();
        set(Preference::Duplication);
        assert_eq!(preference(), Preference::Duplication);
        set(Preference::GraphicsCapture);
        assert_eq!(preference(), Preference::GraphicsCapture);
        set(Preference::Auto);
        assert_eq!(preference(), Preference::Auto);
        set(before);
    }
}
