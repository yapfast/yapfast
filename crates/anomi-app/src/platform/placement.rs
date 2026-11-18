//! Where the window was and how big, kept in `layout.json`'s `window` as
//! the Slint app keeps it (`src/ui/placement.rs`), so either opens as the
//! other was left: its size inside the frame in physical pixels and whether
//! it was maximized. This interface adds the display's scale the size was
//! measured at, which the Slint app passes over (it writes the key without
//! it, and then the size is taken at the scale this run last had, or 1);
//! the Slint app's place on Windows rides along untouched.
//!
//! A window opens at the size kept, at the scale it guesses; once open, it
//! learns its display's scale and the monitor's room, and comes to the size
//! kept on that display, shrunk to nine tenths of the room where it has no
//! room for it. A maximized window keeps the size it goes back to.

use iced::{Size, Task, window};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::app::App;
use crate::message::Message;

/// The window's size and whether it was maximized.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// Inside the frame, in physical pixels.
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
    /// The display's scale factor the size was measured at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
    /// What the Slint app keeps beside them: its place on Windows.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// The placement the layout keeps, if it reads as one.
pub fn read(window: Option<&Value>) -> Option<Placement> {
    let placement: Placement = Placement::deserialize(window?).ok()?;
    (placement.width > 0 && placement.height > 0).then_some(placement)
}

/// How the window opens: as kept, else the first start's size; with Anomi's
/// frame or the desktop's, and its icon.
pub fn settings(app: &mut App) -> window::Settings {
    let mut settings = crate::window::settings();
    settings.decorations = !super::window::own_bar(&app.layout);
    settings.icon = anomi_desktop::icon::rgba()
        .and_then(|(pixels, width, height)| window::icon::from_rgba(pixels, width, height).ok());
    // The test display's size stands, so shots compare.
    if crate::window::test_size().is_some() {
        return settings;
    }
    if let Some(placement) = read(app.layout.window.as_ref()) {
        let scale = app.zoom.base_known().or(placement.scale).unwrap_or(1.0);
        settings.size = view(&placement, scale * app.zoom.level());
        settings.maximized = placement.maximized;
    }
    app.desktop.opened_at = Some(settings.size);
    settings
}

/// The size kept, in the view's pixels at `scale` (the display's and the
/// zoom's), no smaller than the window's least.
fn view(placement: &Placement, scale: f32) -> Size {
    let least = crate::window::settings().min_size.unwrap_or(Size::ZERO);
    Size::new(
        (placement.width as f32 / scale).max(least.width),
        (placement.height as f32 / scale).max(least.height),
    )
}

/// The window opened and knows its display's scale and its monitor's room
/// (in the view's pixels): the zoom takes the scale, and the window comes
/// to the size kept on this display, within the room.
pub fn placed(app: &mut App, base: f32, monitor: Option<Size>) -> Task<Message> {
    let zoomed = crate::zoom::rescaled(app, base);
    let (Some(id), Some(opened)) = (app.window, app.desktop.opened_at.take()) else {
        return zoomed;
    };
    let kept = read(app.layout.window.as_ref());
    if crate::window::test_size().is_some() || kept.as_ref().is_some_and(|kept| kept.maximized) {
        return zoomed;
    }
    let scale = app.zoom.base() * app.zoom.level();
    let wanted = kept.map_or(opened, |kept| view(&kept, scale));
    let fitted = monitor.map_or(wanted, |room| shrunk(wanted, room));
    let off =
        (fitted.width - opened.width).abs() > 1.0 || (fitted.height - opened.height).abs() > 1.0;
    if !off {
        return zoomed;
    }
    Task::batch([zoomed, window::resize(id, fitted)])
}

/// A size the room has no space for goes to nine tenths of it, as far as
/// it is too big: the window's frame and the grip to resize it must show.
pub fn shrunk(size: Size, room: Size) -> Size {
    let each = |size: f32, room: f32| if size > room { room * 0.9 } else { size };
    Size::new(each(size.width, room.width), each(size.height, room.height))
}

/// The window moved to a size, or was maximized or restored: keeps it in
/// the layout, which writes it a second after the last change.
pub fn keep(app: &mut App, maximized: bool) {
    let size = app.size;
    if app.window.is_none() || size.width < 1.0 || size.height < 1.0 {
        return;
    }
    let base = app.zoom.base();
    let scale = base * app.zoom.level();
    let before = read(app.layout.window.as_ref());
    let now = match before.clone() {
        // Maximized, the window keeps the size it goes back to.
        Some(before) if maximized => Placement {
            maximized,
            ..before
        },
        before => Placement {
            width: (size.width * scale).round() as u32,
            height: (size.height * scale).round() as u32,
            maximized,
            scale: Some(base),
            rest: before.map(|before| before.rest).unwrap_or_default(),
        },
    };
    if before.as_ref() == Some(&now) {
        return;
    }
    let Ok(value) = serde_json::to_value(&now) else {
        return;
    };
    app.layout.edit(|saved| saved.window = Some(value));
}

#[cfg(test)]
mod tests {
    use iced::Size;
    use serde_json::json;

    use super::{read, shrunk};

    #[test]
    fn the_slint_apps_placement_reads_and_keeps_its_place() {
        let slint = json!({"width": 1307, "height": 887, "maximized": false});
        let placement = read(Some(&slint)).unwrap();
        assert_eq!((placement.width, placement.height), (1307, 887));
        assert_eq!(placement.scale, None);
        // Written back, it reads as the Slint app wrote it, plus the scale.
        let windows = json!({"width": 800, "height": 600, "maximized": true, "position": [10, 20]});
        let mut placement = read(Some(&windows)).unwrap();
        placement.scale = Some(1.25);
        let written = serde_json::to_value(&placement).unwrap();
        assert_eq!(written["position"], json!([10, 20]));
        assert_eq!(written["scale"], json!(1.25));
        assert_eq!(
            read(Some(&json!({"width": 0, "height": 5, "maximized": false}))),
            None
        );
        assert_eq!(read(Some(&json!("odd"))), None);
    }

    #[test]
    fn a_window_too_big_for_the_room_shrinks_to_nine_tenths() {
        let size = Size::new(1307.0, 887.0);
        // A 1366x768 laptop with a panel 40px high.
        assert_eq!(
            shrunk(size, Size::new(1366.0, 728.0)),
            Size::new(1307.0, 655.2)
        );
        assert_eq!(shrunk(size, Size::new(3440.0, 1400.0)), size);
    }
}
