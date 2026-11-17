//! The rail down the picker's left, as Discord's: a 36 px button a
//! section, its icon 32 px, the one the grid's top is in marked; a press
//! sends the grid to its heading, and the pointer over one names it in the
//! foot (`PickerRail`, `ui/messages.slint:116`). It scrolls without a bar
//! (`ScrollBarPolicy.always-off`).

use iced::widget::{button, container, mouse_area};
use iced::{Background, Border, Fill, Padding, Size};

use anomi_theme::prelude::*;
use anomi_vm::chat::emoji_picker::{EmojiPicker, Hovered, Intent, Section};
use anomi_widgets::list::list;

use super::face;

/// The rail's width (`:128`).
pub const WIDTH: f32 = 48.0;
/// A button, the space over it, and its corners (`:134`-`:136`).
const BUTTON: f32 = 36.0;
const TOP: f32 = 4.0;
const RADIUS: f32 = 6.0;
/// The icon's size (`RAIL_GLYPH`, `src/emoji.rs:79`).
const ICON: f32 = 32.0;
/// A button's lift under the pointer (`:139`).
const LIFT: f32 = 0.12;

pub fn view<'a>(picker: &'a EmojiPicker, motion: Motion) -> Element<'a, Intent> {
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

/// The button of the section at `index`, `marked` when the grid's top is
/// in it.
fn section(section: &Section, index: usize, marked: bool) -> Element<'_, Intent> {
    let tile = button(face::rail(section, ICON, Size::new(BUTTON, BUTTON)))
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
    let tile = mouse_area(tile).on_enter(Intent::Hover(Hovered::Section(index)));
    container(tile)
        .padding(Padding {
            top: TOP,
            ..Padding::ZERO
        })
        .center_x(WIDTH)
        .boxed()
}
