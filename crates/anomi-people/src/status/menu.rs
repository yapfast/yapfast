//! The classic style's menu of statuses (`StatusMenu`,
//! `ui/dialogs.slint:621`): the game being played, the four statuses with
//! what two of them do, and the ways into the custom status and the
//! profile editor. The Enabled style, which the iced interface draws, shows
//! the account's card instead, so nothing opens this; its rows send the
//! status intents for the one that places it.
//!
//! The game's duration changes by the minute: the app's minute clock
//! writes it again while it shows and the window is in front, and as the
//! window comes back (`anomi-app/src/clocks.rs`).

use std::borrow::Cow;

use iced::widget::{column, container, space, text};
use iced::{Background, Border, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::account::Choice;
use anomi_vm::people::status::{Intent, Menu};
use yapfast_lang::tr;

use super::option::option;
use anomi_widgets::menu::item;

/// The popup's width (`:2413`) and `padding: 6px; spacing: 2px`.
pub const WIDTH: f32 = 248.0;
const PAD: f32 = 6.0;
const GAP: f32 = 2.0;
/// The game's block: `padding` 8 and 4 above, 6 below.
const GAME_X: f32 = 8.0;
const GAME_TOP: f32 = 4.0;
const GAME_BOTTOM: f32 = 6.0;
/// The line between groups, with 8 and 4 px around it.
const LINE: f32 = 1.0;
const RULE_X: f32 = 8.0;
const RULE_Y: f32 = 4.0;
/// The menu's shadow (`:634`): 24 px blur, 8 px down, `#00000080`.
const SHADOW_ALPHA: u8 = 0x80;
const SHADOW_DOWN: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;

pub fn view(menu: &Menu, motion: Motion) -> impl View<Intent> + '_ {
    let mut rows: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(game) = &menu.game {
        let playing: Cow<'_, str> = if game.duration.is_empty() {
            Cow::Borrowed(tr!("Playing now"))
        } else {
            Cow::Owned(tr!("Playing for {}", game.duration))
        };
        let block = column![
            text(game.name.as_str())
                .size(typography::size::SMALL)
                .font(typography::BOLD)
                .width(Fill)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End),
            text(playing)
                .size(typography::size::CAPTION)
                .width(Fill)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::secondary),
            rule(),
        ]
        .spacing(GAP)
        .padding(iced::Padding {
            left: GAME_X,
            right: GAME_X,
            top: GAME_TOP,
            bottom: GAME_BOTTOM,
        });
        rows.push(block.boxed());
    }
    let statuses: [(Choice, &str, &str); 4] = [
        (Choice::Online, tr!("Online"), ""),
        (Choice::Idle, tr!("Idle"), ""),
        (
            Choice::DoNotDisturb,
            tr!("Do Not Disturb"),
            tr!("You won't get message notifications."),
        ),
        (
            Choice::Invisible,
            tr!("Invisible"),
            tr!("You'll show as offline, but can use Yapfast as usual."),
        ),
    ];
    for (choice, name, note) in statuses {
        rows.push(option(
            choice,
            name,
            note,
            menu.choice == choice,
            Intent::Choose(choice),
        ));
    }
    rows.push(container(rule()).padding([RULE_Y, RULE_X]).boxed());
    for row in &menu.rows {
        rows.push(item(row, motion).boxed());
    }
    container(column(rows).spacing(GAP).padding(PAD))
        .width(WIDTH)
        .style(card_style)
}

/// A one pixel line (`Theme.line`).
fn rule<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Fill)
        .height(LINE)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        })
        .boxed()
}

/// `background: Theme.raised; border: edge 1px; radius 8px;` and its shadow
/// (24 px blur, 8 px down, `#00000080`).
fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_CARD)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA as f32 / 255.0),
            offset: Vector::new(0.0, SHADOW_DOWN),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}
