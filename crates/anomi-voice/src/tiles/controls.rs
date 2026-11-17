//! A tile's buttons and who watches it, 10 px in from its edges, at rest or
//! under the pointer (`StageTile`'s `TouchArea`, `ui/voice.slint:775`): at
//! the top right the viewers, and while a watched stream's tile is large
//! or pointed at, the button that stops watching it, 8 px after them; at
//! the bottom right of the large tile under the pointer, the buttons that
//! hide the tiles beneath it and go full screen. Each is a white icon on a
//! round dark scrim, 32 px.
//!
//! Slint fades these in over `QUICK`; iced draws no layer at an opacity,
//! so they show at once.

use iced::widget::{Row, Space, column};
use iced::{Alignment, Color, Fill};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_vm::voice::tiles::{Intent, Tile};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::Icon;

use super::{INSET, SCRIM, viewers::viewers};

/// A button's side, its corners, and the room between two (`:792`, `:800`).
const BUTTON: f32 = 32.0;
const GAP: f32 = 8.0;

/// The layer at rest, or under the pointer when `hovered`.
pub fn view(tile: &Tile, hovered: bool, motion: Motion) -> Element<'_, Intent> {
    let mut top: Row<Element<'_, Intent>> = Row::new()
        .align_y(Alignment::Center)
        .height(BUTTON)
        .spacing(GAP)
        .push(Space::new().width(Fill).boxed())
        .push(viewers(&tile.viewers, motion));
    if tile.watched() && (hovered || tile.focused) {
        top = top.push(button(
            Icon::X,
            tr!("Stop watching"),
            Intent::StopWatching,
            motion,
        ));
    }
    let mut bottom: Row<Element<'_, Intent>> = Row::new()
        .spacing(GAP)
        .push(Space::new().width(Fill).boxed());
    if hovered && tile.focused {
        if !tile.full && tile.can_hide_others {
            let (shape, label) = if tile.others_hidden {
                (Icon::Users, tr!("Show Members"))
            } else {
                (Icon::EyeOff, tr!("Hide Members"))
            };
            let hide = Intent::HideOthers(!tile.others_hidden);
            bottom = bottom.push(button(shape, label, hide, motion));
        }
        let (shape, label) = if tile.full {
            (Icon::Minimize, tr!("Exit full screen"))
        } else {
            (Icon::Maximize, tr!("Full screen"))
        };
        bottom = bottom.push(button(shape, label, Intent::FullScreen(!tile.full), motion));
    }
    column![top, Space::new().height(Fill), bottom]
        .padding(INSET)
        .width(Fill)
        .height(Fill)
        .boxed()
}

fn button<'a>(shape: Icon, label: &'a str, intent: Intent, motion: Motion) -> Element<'a, Intent> {
    icon_button(shape, label, motion)
        .tint(|_| Color::WHITE)
        .fill(|_| SCRIM)
        .radius(BUTTON / 2.0)
        .on_press(intent)
        .view()
        .boxed()
}
