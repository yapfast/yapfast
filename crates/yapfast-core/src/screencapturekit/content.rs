//! What can be shared: the displays and windows `SCShareableContent` lists.

use std::{sync::mpsc, time::Duration};

use anyhow::{Context, Result, bail};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2_core_graphics::{CGDisplayPixelsHigh, CGDisplayPixelsWide, CGMainDisplayID};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{SCDisplay, SCShareableContent, SCWindow};

use super::permission;

/// How long to wait for the system to list what there is. It answers at once
/// unless the permission prompt is open.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Windows below this many points wide or high are no windows to share:
/// helpers, shadows and overlays.
const MIN_WINDOW_POINTS: f64 = 50.0;

/// A window on the normal layer is an application's; menus, the Dock and
/// overlays are on others.
const NORMAL_WINDOW_LAYER: isize = 0;

/// What the person picks to share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Capturable {
    /// The main display, which is what a share with no pick means.
    MainDisplay,
    Display(u32),
    Window(u32),
}

/// A display, as listed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayInfo {
    pub id: u32,
    /// Its size in points, which is smaller than its pixels on a Retina one.
    pub points: (f64, f64),
}

/// A window, as listed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub application: String,
    pub points: (f64, f64),
    pub layer: isize,
    pub on_screen: bool,
}

/// What [`list`] names, ready to show.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Listing {
    /// Each display's id and its label.
    pub screens: Vec<(u32, String)>,
    /// Each shareable window's id and its label.
    pub windows: Vec<(u32, String)>,
}

// ---- Pure logic: which windows count, and what to call things ----

/// Whether a window is one to offer: an application's own, on screen, titled
/// and big enough to be more than a helper.
pub(crate) fn is_shareable_window(window: &WindowInfo) -> bool {
    window.layer == NORMAL_WINDOW_LAYER
        && window.on_screen
        && !window.title.trim().is_empty()
        && window.points.0 >= MIN_WINDOW_POINTS
        && window.points.1 >= MIN_WINDOW_POINTS
}

/// A window's label: its application's name before its title.
pub(crate) fn window_label(window: &WindowInfo) -> String {
    let title = window.title.trim();
    if window.application.trim().is_empty() {
        title.to_owned()
    } else {
        format!("{}: {title}", window.application.trim())
    }
}

/// A display's label, among `count` of them.
pub(crate) fn screen_label(index: usize, count: usize) -> String {
    if count == 1 {
        "Entire screen".to_owned()
    } else {
        format!("Screen {}", index + 1)
    }
}

/// The display `wanted` asks for among `ids`: the one named, or for the main
/// display's pick the system's main one, else the first listed.
pub(crate) fn choose_display(ids: &[u32], main: u32, wanted: Capturable) -> Option<u32> {
    match wanted {
        Capturable::Display(id) => ids.contains(&id).then_some(id),
        _ => ids
            .iter()
            .copied()
            .find(|id| *id == main)
            .or_else(|| ids.first().copied()),
    }
}

/// Names what is listed: displays as screens, then the windows worth sharing.
pub(crate) fn label(displays: &[DisplayInfo], windows: &[WindowInfo]) -> Listing {
    Listing {
        screens: displays
            .iter()
            .enumerate()
            .map(|(index, display)| (display.id, screen_label(index, displays.len())))
            .collect(),
        windows: windows
            .iter()
            .filter(|window| is_shareable_window(window))
            .map(|window| (window.id, window_label(window)))
            .collect(),
    }
}

// ---- The system's list ----

/// A listing from the system.
pub(crate) struct Content {
    raw: Retained<SCShareableContent>,
}

// SAFETY: a listing is immutable once made, and ScreenCaptureKit's objects
// may be used from any thread.
unsafe impl Send for Content {}

impl Content {
    /// Asks the system what can be captured, on screen only.
    pub(crate) fn fetch() -> Result<Self> {
        permission::ensure()?;
        let (answer, answered) = mpsc::sync_channel::<Result<Content, String>>(1);
        let handler = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                // SAFETY: the system passes a live object or null for each.
                let content = unsafe { Retained::retain(content) };
                let outcome = match content {
                    Some(raw) => Ok(Content { raw }),
                    None => Err(describe(error)),
                };
                let _ = answer.try_send(outcome);
            },
        );
        // SAFETY: the handler is copied by the call and may run on any thread.
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                true, true, &handler,
            );
        }
        match answered.recv_timeout(LIST_TIMEOUT) {
            Ok(Ok(content)) => Ok(content),
            Ok(Err(message)) => {
                tracing::warn!(%message, "could not list what can be shared");
                bail!(message)
            }
            Err(_) => bail!("The system did not list the screens and windows in time."),
        }
    }

    pub(crate) fn displays(&self) -> Vec<DisplayInfo> {
        // SAFETY: plain property reads on listed objects.
        unsafe { self.raw.displays() }
            .iter()
            .map(|display| unsafe {
                let frame = display.frame();
                DisplayInfo {
                    id: display.displayID(),
                    points: (frame.size.width, frame.size.height),
                }
            })
            .collect()
    }

    pub(crate) fn windows(&self) -> Vec<WindowInfo> {
        // SAFETY: plain property reads on listed objects.
        unsafe { self.raw.windows() }
            .iter()
            .map(|window| unsafe {
                let frame = window.frame();
                WindowInfo {
                    id: window.windowID(),
                    title: window
                        .title()
                        .map(|title| title.to_string())
                        .unwrap_or_default(),
                    application: window
                        .owningApplication()
                        .map(|application| application.applicationName().to_string())
                        .unwrap_or_default(),
                    points: (frame.size.width, frame.size.height),
                    layer: window.windowLayer(),
                    on_screen: window.isOnScreen(),
                }
            })
            .collect()
    }

    /// The display `wanted` asks for.
    pub(crate) fn display(&self, wanted: Capturable) -> Result<Retained<SCDisplay>> {
        let ids: Vec<u32> = self.displays().iter().map(|display| display.id).collect();
        let id = choose_display(&ids, CGMainDisplayID(), wanted)
            .context("That screen or window is gone.")?;
        // SAFETY: plain property reads on listed objects.
        unsafe { self.raw.displays() }
            .iter()
            .find(|display| unsafe { display.displayID() } == id)
            .context("That screen or window is gone.")
    }

    pub(crate) fn window(&self, id: u32) -> Result<Retained<SCWindow>> {
        // SAFETY: plain property reads on listed objects.
        unsafe { self.raw.windows() }
            .iter()
            .find(|window| unsafe { window.windowID() } == id)
            .context("That screen or window is gone.")
    }

    /// Every running application's name and object, to leave some out of a
    /// capture.
    pub(crate) fn applications(
        &self,
    ) -> Vec<(
        String,
        Retained<objc2_screen_capture_kit::SCRunningApplication>,
    )> {
        // SAFETY: plain property reads on listed objects.
        unsafe { self.raw.applications() }
            .iter()
            .map(|application| {
                (
                    unsafe { application.applicationName() }.to_string(),
                    application,
                )
            })
            .collect()
    }
}

/// The pixel size of display `id`.
pub(crate) fn display_pixels(id: u32) -> (u32, u32) {
    (
        CGDisplayPixelsWide(id) as u32,
        CGDisplayPixelsHigh(id) as u32,
    )
}

/// What an error from the system says, for the person.
pub(crate) fn describe(error: *mut NSError) -> String {
    // SAFETY: the system passes a live error or null.
    match unsafe { error.as_ref() } {
        Some(error) => permission::explain(error.code(), &error.localizedDescription().to_string()),
        None => "The system gave no screens or windows.".to_owned(),
    }
}

/// What can be shared now: each screen, then each window worth sharing.
pub(crate) fn list() -> Result<Listing> {
    let content = Content::fetch()?;
    let listing = label(&content.displays(), &content.windows());
    tracing::info!(
        screens = listing.screens.len(),
        windows = listing.windows.len(),
        "listed what can be shared"
    );
    Ok(listing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(title: &str, layer: isize, on_screen: bool, points: (f64, f64)) -> WindowInfo {
        WindowInfo {
            id: 1,
            title: title.into(),
            application: "Safari".into(),
            points,
            layer,
            on_screen,
        }
    }

    #[test]
    fn an_ordinary_titled_window_is_offered() {
        assert!(is_shareable_window(&window(
            "Docs",
            0,
            true,
            (800.0, 600.0)
        )));
    }

    #[test]
    fn helpers_are_not_offered() {
        assert!(!is_shareable_window(&window("", 0, true, (800.0, 600.0))));
        assert!(!is_shareable_window(&window("  ", 0, true, (800.0, 600.0))));
        assert!(!is_shareable_window(&window(
            "Menu",
            25,
            true,
            (800.0, 600.0)
        )));
        assert!(!is_shareable_window(&window(
            "Docs",
            0,
            false,
            (800.0, 600.0)
        )));
        assert!(!is_shareable_window(&window("Dot", 0, true, (10.0, 10.0))));
    }

    #[test]
    fn a_window_is_named_by_its_application_and_title() {
        assert_eq!(
            window_label(&window(" Docs ", 0, true, (1.0, 1.0))),
            "Safari: Docs"
        );
        let mut nameless = window("Docs", 0, true, (1.0, 1.0));
        nameless.application.clear();
        assert_eq!(window_label(&nameless), "Docs");
    }

    #[test]
    fn one_screen_is_the_entire_screen_and_more_are_numbered() {
        assert_eq!(screen_label(0, 1), "Entire screen");
        assert_eq!(screen_label(0, 2), "Screen 1");
        assert_eq!(screen_label(1, 2), "Screen 2");
    }

    #[test]
    fn the_main_display_is_the_system_main_else_the_first() {
        assert_eq!(choose_display(&[7, 9], 9, Capturable::MainDisplay), Some(9));
        assert_eq!(choose_display(&[7, 9], 3, Capturable::MainDisplay), Some(7));
        assert_eq!(choose_display(&[], 3, Capturable::MainDisplay), None);
    }

    #[test]
    fn a_named_display_must_still_be_there() {
        assert_eq!(choose_display(&[7, 9], 9, Capturable::Display(7)), Some(7));
        assert_eq!(choose_display(&[7, 9], 9, Capturable::Display(5)), None);
    }

    #[test]
    fn listings_name_screens_then_only_worthwhile_windows() {
        let displays = [
            DisplayInfo {
                id: 1,
                points: (1.0, 1.0),
            },
            DisplayInfo {
                id: 2,
                points: (1.0, 1.0),
            },
        ];
        let windows = [
            window("Docs", 0, true, (800.0, 600.0)),
            window("", 0, true, (800.0, 600.0)),
        ];
        let listing = label(&displays, &windows);
        assert_eq!(
            listing.screens,
            [(1, "Screen 1".to_owned()), (2, "Screen 2".to_owned())]
        );
        assert_eq!(listing.windows, [(1, "Safari: Docs".to_owned())]);
    }
}
