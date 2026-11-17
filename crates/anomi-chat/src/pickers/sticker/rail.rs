//! The rail down the picker's left, as Discord's: a 36 px button a
//! collection, its icon 32 px, the one the grid's top is in marked; a press
//! sends the grid to its heading, and the pointer over one names it in the
//! foot (`PickerRail`, `ui/messages.slint:116`). It scrolls without a bar
//! (`ScrollBarPolicy.always-off`).

use iced::widget::{button, container, mouse_area, space, svg};
use iced::{Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_vm::chat::sticker_picker::{Hovered, Intent, Section, StickerPicker};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::list::list;
use anomi_widgets::picture::{Fit, picture};

/// The rail's width (`width: 48px`).
pub const WIDTH: f32 = 48.0;
/// A button, the space over it, and its corners.
const BUTTON: f32 = 36.0;
const TOP: f32 = 4.0;
const RADIUS: f32 = 6.0;
/// The icon's size, and the corners of a picture in it (`width: 32px;
/// border-radius: 8px`).
const ICON: f32 = 32.0;
const ICON_RADIUS: f32 = 8.0;
/// A button's lift under the pointer (`Theme.overlay.with-alpha(0.12)`).
const LIFT: f32 = 0.12;

pub fn view<'a>(picker: &'a StickerPicker, motion: Motion) -> Element<'a, Intent> {
    let marked = picker.marked();
    let sections = &picker.sections;
    let rows = list(
        sections.len(),
        |at| at as u64,
        move |at| section(&sections[at], at, marked == Some(at)),
        motion,
    )
    .scrollbar(false)
    .estimate(TOP + BUTTON);
    container(rows)
        .width(WIDTH)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().panel.into()),
            ..container::Style::default()
        })
        .boxed()
}

/// What shows a section: the clock, or its picture rounded as the server
/// rail rounds its own, on a tile of the active colour while it comes. The
/// picture holds its first frame, as Slint's rail draws only the still.
fn face(section: &Section) -> Element<'_, Intent> {
    let Some(expected) = section.icon.picture() else {
        let clock = icon(Icon::Clock, ICON).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().secondary),
        });
        return clock.boxed();
    };
    match expected.ready() {
        Some(handle) => picture(handle.clone())
            .side(ICON)
            .radius(ICON_RADIUS)
            .fit(Fit::Contain)
            .boxed(),
        None => container(space())
            .width(ICON)
            .height(ICON)
            .style(|theme: &Theme| container::Style {
                background: Some(theme.palette().active.into()),
                border: Border::default().rounded(ICON_RADIUS),
                ..container::Style::default()
            })
            .boxed(),
    }
}

/// The button of the section at `index`, `marked` when the grid's top is in
/// it.
fn section(section: &Section, index: usize, marked: bool) -> Element<'_, Intent> {
    let tile = button(container(face(section)).center(BUTTON))
        .on_press(Intent::Jump(index))
        .padding(0)
        .width(BUTTON)
        .height(BUTTON)
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let ground = match status {
                button::Status::Hovered | button::Status::Pressed => Some(palette.lift(LIFT)),
                _ if marked => Some(palette.active),
                _ => None,
            };
            button::Style {
                background: ground.map(Background::Color),
                text_color: palette.text,
                border: Border::default().rounded(RADIUS),
                ..button::Style::default()
            }
        });
    let tile = mouse_area(tile)
        .on_enter(Intent::Hover(Hovered::Section(index)))
        .on_exit(Intent::Leave(Hovered::Section(index)));
    container(tile)
        .padding(Padding {
            top: TOP,
            ..Padding::ZERO
        })
        .center_x(WIDTH)
        .boxed()
}
