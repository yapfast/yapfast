//! Anomi's icon in the system tray, which keeps it running with its window
//! closed: a StatusNotifierItem on Linux, on the session bus it is given, and
//! a notification-area icon on Windows. Clicking it opens the window; its
//! menu opens Anomi, leaves a call and quits. Both clients start it with a
//! [`Chosen`] that takes each choice onto their own event loop.

use std::cell::Cell;
use std::sync::Arc;

/// The xdg-activation token of the tray click or Open Anomi that is
/// opening the window, which lets a Wayland window come to the front.
#[cfg(target_os = "linux")]
static TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The token of the tray click that asked for the window, once.
pub fn take_activation_token() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        TOKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// What the tray asks for, or says, handed to the [`Chosen`] the tray was
/// started with, from the tray's own thread or window procedure: each window
/// toolkit takes it onto its event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Open,
    Disconnect,
    Quit,
    /// A tray came or went, which decides what closing the window does.
    Available,
}

/// Where the tray's choices go. It runs off the window's event loop, or
/// inside a window procedure, so it only hands the choice on.
pub type Chosen = Arc<dyn Fn(Choice) + Send + Sync>;

/// What the icon and its menu show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// A mention or a DM waits unread: the icon has a dot.
    pub unread: bool,
    /// In a call: the menu offers Disconnect.
    pub call: bool,
}

pub struct Tray {
    platform: platform::Tray,
    shown: Cell<Shown>,
}

impl Tray {
    /// Puts Anomi's icon in the tray. `chosen` is handed what the person
    /// picked. `None` where there's no tray to use. On Windows it starts on
    /// the window's thread, whose message loop runs the icon's window.
    pub fn start(chosen: Chosen) -> Option<Self> {
        let platform = platform::Tray::start(chosen)?;
        Some(Self {
            platform,
            shown: Cell::new(Shown::default()),
        })
    }

    /// Whether a tray shows the icon now, so the window can close into it.
    pub fn available(&self) -> bool {
        self.platform.available()
    }

    /// Shows the unread dot and the call's Disconnect, telling the tray only
    /// when they change.
    pub fn show(&self, shown: Shown) {
        if self.shown.replace(shown) != shown {
            self.platform.show(shown);
        }
    }

    /// Words the menu and tooltip again, in the language chosen since.
    pub fn reword(&self) {
        self.platform.show(self.shown.get());
    }
}

/// Anomi's picture as drawn to read at a tray's size, `size` pixels square,
/// with a red dot for unread messages cut into its corner.
fn logo(size: u32, unread: bool) -> Option<image::RgbaImage> {
    let picture = image::load_from_memory_with_format(
        include_bytes!("../../../../ui/icons/anomi-small.png"),
        image::ImageFormat::Png,
    )
    .ok()?
    .into_rgba8();
    let mut pixels = match picture.width() == size {
        true => picture,
        false => {
            image::imageops::resize(&picture, size, size, image::imageops::FilterType::Lanczos3)
        }
    };
    if unread {
        // Where Yapfast's logo, 78 units across, had its dot: centred 62
        // units in, 15 in radius, with a gap of 6 round it.
        let unit = size as f32 / 78.;
        let (centre, dot, gap) = (62. * unit, 15. * unit, 21. * unit);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let distance = (x as f32 + 0.5 - centre).hypot(y as f32 + 0.5 - centre);
            // How much of the pixel each covers, smoothed over its width.
            let red = (dot - distance + 0.5).clamp(0., 1.);
            let kept = (distance - gap + 0.5).clamp(0., 1.);
            pixel.0 = match red > 0. {
                true => [0xf0, 0x54, 0x5e, (red * 255.).round() as u8],
                false => {
                    let [r, g, b, a] = pixel.0;
                    [r, g, b, (f32::from(a) * kept).round() as u8]
                }
            };
        }
    }
    Some(pixels)
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(target_os = "linux", windows)))]
use none as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(not(any(target_os = "linux", windows)))]
mod none {
    use super::{Chosen, Shown};

    pub struct Tray;

    impl Tray {
        pub fn start(_: Chosen) -> Option<Self> {
            None
        }

        pub fn available(&self) -> bool {
            false
        }

        pub fn show(&self, _: Shown) {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_logo_is_anomis_and_unread_adds_a_red_dot() {
        let plain = super::logo(32, false).unwrap();
        let unread = super::logo(32, true).unwrap();
        assert_eq!(plain.dimensions(), (32, 32));
        assert_eq!(super::logo(64, false).unwrap().width(), 64);
        // The light violet of its outline.
        let violet = |pixel: &image::Rgba<u8>| {
            let [r, g, b, a] = pixel.0;
            a > 200 && r > 140 && b > 200 && g < r
        };
        let red = |pixel: &image::Rgba<u8>| pixel.0 == [0xf0, 0x54, 0x5e, 255];
        assert!(plain.pixels().any(violet));
        assert!(!plain.pixels().any(red));
        assert!(unread.pixels().any(violet));
        // The dot is in the bottom-right corner, with a clear ring round it.
        assert!(red(unread.get_pixel(25, 25)));
        assert_eq!(unread.get_pixel(25, 18).0[3], 0);
    }
}
