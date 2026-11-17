//! The picture itself: shown at its size, shrunk to fit the room between the
//! bar and the arrows and never enlarged, then zoomed, centred and clipped to the room (`overflow`) (`ui/messages.slint:2676`
//! to `:2690`). A click on it zooms in and back; a right click opens its menu.

use iced::widget::{container, mouse_area, responsive};
use iced::{Fill, Padding, Size, mouse};

use anomi_theme::prelude::*;
use anomi_vm::chat::viewer::{Intent, Viewer, Zoom};
use anomi_vm::menu::Spot;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};

use super::bar::BAR;
use super::overflow::overflow;

/// What the arrows and the bar leave: 80 either side and the bar's height
/// above and below, so the picture sits in the window's middle
/// (`root.width - 160px`, `root.height - 128px`).
const SIDES: f32 = 80.0;

/// The picture's size at `room`: its own, or the room's if smaller, times the
/// zoom. Unknown until the size is, then nothing.
pub fn shown(size: (u32, u32), room: Size, zoom: f32) -> Option<Size> {
    let (width, height) = (size.0.max(1) as f32, size.1.max(1) as f32);
    if size.0 == 0 || size.1 == 0 {
        return None;
    }
    let fit = (room.width / width)
        .min(room.height / height)
        .clamp(0.0, 1.0);
    Some(Size::new(width * fit * zoom, height * fit * zoom))
}

pub fn stage(viewer: &Viewer, motion: Motion) -> Element<'_, Intent> {
    let room = responsive(move |room: Size| -> Element<'_, Intent> {
        let (Some(handle), Some(size)) = (
            viewer.picture.clone(),
            shown(viewer.size, room, viewer.zoom),
        ) else {
            return iced::widget::space().boxed();
        };
        let picture = picture(handle)
            .width(size.width)
            .height(size.height)
            .fit(Fit::Contain)
            .motion(motion);
        let mut shown = opener(picture);
        if viewer.has_menu() {
            shown = shown.on_right_press(|at| Intent::Menu(Spot::pointer(at)));
        }
        let cursor = if viewer.zoom > 1.0 {
            mouse::Interaction::ZoomOut
        } else {
            mouse::Interaction::ZoomIn
        };
        let clickable = mouse_area(shown)
            .on_press(Intent::Zoom(Zoom::Toggle))
            .interaction(cursor);
        overflow(clickable).boxed()
    });
    container(room)
        .width(Fill)
        .height(Fill)
        .padding(Padding {
            top: BAR,
            bottom: BAR,
            left: SIDES,
            right: SIDES,
        })
        .clip(true)
        .boxed()
}
