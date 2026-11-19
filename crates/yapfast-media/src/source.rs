//! What a share can capture, and what Yapfast can list to pick from: the
//! screens, windows and cameras a person chooses between, and the one value a
//! whole stream carries that choice in.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::picture;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A screen or window the person picks in the system's dialog, or what
    /// they picked from [`shareable`] where there's none.
    Screen,
    /// A screen by the id [`shareable`] listed it with.
    Display(u64),
    /// A window by the id [`shareable`] listed it with.
    Window(u64),
    /// A camera by its id in [`crate::camera::list`], such as OBS's virtual
    /// camera.
    Camera(String),
    /// Moving colour bars, for testing without a screen to capture.
    TestPattern,
}

/// Whether the system's dialog, the screen sharing portal, picks the screen or
/// window to share, rather than Yapfast listing them: in a Wayland session.
/// libwebrtc captures through the portal only there, and through X11 in any
/// other Linux session, as it decides by the same variables; its X11
/// capturers list what there is, as Windows' do. XFCE, for one, has no
/// screen sharing portal on X11.
pub fn portal_picks() -> bool {
    cfg!(target_os = "linux")
        && wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
}

/// Whether a session of this `kind`, in `XDG_SESSION_TYPE`, with a Wayland
/// display named or not, is one libwebrtc captures through the portal in.
pub fn wayland_session(kind: Option<&str>, wayland_display: bool) -> bool {
    kind.is_some_and(|kind| kind.starts_with("wayland")) && wayland_display
}

/// A screen, window or camera [`shareable`] lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shareable {
    pub source: Source,
    pub title: String,
}

/// What Yapfast can share: the screens, then the windows, unless
/// [`portal_picks`], then the cameras. On Windows, Yapfast's own windows
/// aren't among them, nor its console's.
pub fn shareable() -> Vec<Shareable> {
    let mut listed = screens_and_windows();
    listed.extend(crate::camera::list().into_iter().map(|device| Shareable {
        source: Source::Camera(device.id),
        title: device.name,
    }));
    listed
}

/// What a screen is called in the share menu: screens are counted, so the
/// system's own name for a display (`DISPLAY1`) never shows, and the primary
/// one of several says it is. `count` is how many screens there are, `index`
/// this one's place in the list, and `primary` whether the desktop starts on
/// it.
fn screen_title(count: usize, index: usize, primary: bool) -> String {
    if count <= 1 {
        "Entire screen".to_owned()
    } else if primary {
        format!("Screen {} (primary)", index + 1)
    } else {
        format!("Screen {}", index + 1)
    }
}

pub fn screens_and_windows() -> Vec<Shareable> {
    if portal_picks() {
        return Vec::new();
    }
    use livekit::webrtc::desktop_capturer::DesktopCaptureSourceType;
    let screens = sources(DesktopCaptureSourceType::Screen);
    // The screens are numbered the way a person counts them, the primary
    // first, rather than named the way the system names them: the system's
    // own name is a display's (`DISPLAY1`), which one screen answers with
    // and another may not, so a menu of it reads "DISPLAY1 (primary)" beside
    // "Screen 2". The system's name still orders them, since that is what
    // sorts `DISPLAY1` before `DISPLAY2`, and the names it can't give come
    // last, in the order the capturer listed them.
    let mut listed: Vec<(String, u64, bool)> = screens
        .into_iter()
        .enumerate()
        .map(|(index, screen)| {
            let id = screen.id();
            let name = picture::screen_name(id);
            let key = name.clone().unwrap_or_else(|| format!("~{index:03}"));
            (key, id, picture::screen_is_primary(id))
        })
        .collect();
    listed.sort_by(|a, b| a.0.cmp(&b.0));
    let count = listed.len();
    let screens = listed
        .into_iter()
        .enumerate()
        .map(|(index, (_, id, primary))| Shareable {
            source: Source::Display(id),
            title: screen_title(count, index, primary),
        });
    // The console belongs to its host, so libwebrtc lists it as another
    // application's window.
    #[cfg(windows)]
    // SAFETY: no arguments; it only reads this process's console.
    let console = unsafe { windows::Win32::System::Console::GetConsoleWindow() }.0 as u64;
    #[cfg(not(windows))]
    let console = 0;
    let windows = sources(DesktopCaptureSourceType::Window)
        .into_iter()
        .filter_map(|window| {
            let title = window.title();
            (!title.trim().is_empty() && window.id() != console).then(|| Shareable {
                source: Source::Window(window.id()),
                title,
            })
        });
    screens.chain(windows).collect()
}

pub fn sources(
    kind: livekit::webrtc::desktop_capturer::DesktopCaptureSourceType,
) -> Vec<livekit::webrtc::desktop_capturer::CaptureSource> {
    livekit::webrtc::desktop_capturer::DesktopCapturer::new(
        livekit::webrtc::desktop_capturer::DesktopCapturerOptions::new(kind),
    )
    .map(|capturer| capturer.get_source_list())
    .unwrap_or_default()
}

/// What [`Source::Screen`] shares: the pick from [`shareable`] in the share
/// menu, until another. Where [`portal_picks`], the pick may be
/// [`Source::Screen`] itself, which the system's dialog picks.
pub static PICKED: Mutex<Source> = Mutex::new(Source::Screen);

/// Shares `source` from now on where [`Source::Screen`] is asked for.
pub fn pick(source: Source) {
    *PICKED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = source;
}

/// `source`, with [`Source::Screen`] as what was picked.
pub fn picked(source: Source) -> Source {
    match source {
        Source::Screen => PICKED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        source => source,
    }
}

#[cfg(test)]
mod tests {
    use super::screen_title;

    #[test]
    fn screens_are_named_the_way_a_person_counts_them() {
        assert_eq!(screen_title(1, 0, true), "Entire screen", "one screen");
        assert_eq!(screen_title(2, 0, true), "Screen 1 (primary)");
        assert_eq!(screen_title(2, 1, false), "Screen 2");
        assert_eq!(screen_title(3, 2, false), "Screen 3");
    }
}
