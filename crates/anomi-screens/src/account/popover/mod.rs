//! The popover above the account card: Discord's own card of the account
//! (`AccountPopover`, `ui/yapfast/account-popover.slint:11`), 300 px wide,
//! 8 px round, in the profile's theme once it has come. The shell draws it
//! for `Layer::Account`, above the card from the spot the card gave.
//!
//! From the top: the banner, avatar and name ([`header`]); the badges
//! (`anomi_people::profile::badges`, 16 px in); then 16 px in, 12 px apart, the custom status, the game
//! they play, a group with Edit Profile, the status and the custom status,
//! and a group with Copy User ID and User Settings.

mod header;
mod look;
mod ringed;
mod rows;

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Fill, Length, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, catalog, typography};
use anomi_vm::account::{Account, Game, Intent};
use anomi_widgets::list::scroll_view;
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use look::Look;

pub const WIDTH: f32 = 300.0;
const RADIUS: f32 = 8.0;
/// The body's `VerticalLayout` (`account-popover.slint:84`): 12 px between
/// its blocks, 16 px in, 8 px under the badges' room, 16 px at the foot.
const BODY_GAP: f32 = 12.0;
const BODY_PAD: Padding = Padding {
    top: 8.0,
    right: 16.0,
    bottom: 16.0,
    left: 16.0,
};
/// The badges sit 16 px in (`:79`).
const BADGES_INSET: f32 = 16.0;
/// A group's rows, 4 px apart inside its 8 px (`:96`).
const GROUP_PAD: f32 = 8.0;
const GROUP_GAP: f32 = 4.0;
/// The game's card: 80 px high, 12 px in, 4 px between its lines (`:91`).
const GAME_HEIGHT: f32 = 80.0;
const GAME_PAD: f32 = 12.0;
const GAME_GAP: f32 = 4.0;
/// The popover's shadow (`account-popover.slint:48`: `#00000080`, 24 px
/// blur, 8 px down).
const SHADOW: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0);
const SHADOW_DROP: f32 = 8.0;
const SHADOW_BLUR: f32 = 24.0;
/// The border's width (`:47`).
const BORDER: f32 = 1.0;

pub fn popover(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let popover = &account.popover;
    let look = Look::of(&popover.profile);
    let mut body = column![].spacing(BODY_GAP).padding(BODY_PAD);
    if let Some(custom) = &popover.custom {
        let mut line = status_text(custom, motion);
        if let Some(ink) = look.themed_ink() {
            line = line.ink(ink);
        }
        body = body.push(line.view::<Intent>().boxed());
    }
    if let Some(game) = &popover.game {
        body = body.push(playing(game).boxed());
    }
    let rows = &popover.rows;
    let mut status = column![
        rows::plain(&rows.edit, motion),
        rows::rule(),
        rows::status(popover, motion),
    ]
    .spacing(GROUP_GAP);
    if popover.expanded {
        status = status.push(rows::options(popover).boxed());
    }
    status = status.push(rows::plain(&rows.custom, motion).boxed());
    body = body.push(group(status).boxed());
    let mut more = column![].spacing(GROUP_GAP);
    if let Some(copy) = &rows.copy {
        more = more.push(rows::plain(copy, motion).boxed());
    }
    more = more.push(rows::plain(&rows.settings, motion).boxed());
    body = body.push(group(more).boxed());
    let mut card = column![header::header(account, look, motion)];
    if !popover.profile.badges.badges.is_empty() {
        // 16 px in, as the Slint card places them.
        let badges = anomi_people::profile::badges::view(&popover.profile.badges, motion);
        card = card.push(
            container(badges.boxed().map(Intent::Badge))
                .padding(Padding::default().left(BADGES_INSET))
                .boxed(),
        );
    }
    card = card.push(body.boxed());
    // As tall as its content, and it scrolls when the room is shorter
    // (`min(available-height, content.preferred-height)`, `:50`).
    container(scroll_view(card, motion).height(Length::Shrink))
        .width(WIDTH)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(look.ground(palette)),
                border: Border::default()
                    .rounded(RADIUS)
                    .width(BORDER)
                    .color(look.border(palette)),
                shadow: Shadow {
                    color: SHADOW,
                    offset: Vector::new(0.0, SHADOW_DROP),
                    blur_radius: SHADOW_BLUR,
                },
                ..container::Style::default()
            }
        })
}

/// A card of rows on the raised ground, 8 px in.
fn group<'a>(rows: impl View<Intent> + 'a) -> impl View<Intent> + 'a {
    container(rows)
        .padding(GROUP_PAD)
        .width(Fill)
        .style(look::group)
}

/// The game they play and for how long: "Playing", its name, its time.
fn playing(game: &Game) -> impl View<Intent> + '_ {
    let mut lines = column![
        text(tr!("Playing"))
            .size(typography::size::CAPTION)
            .font(typography::BOLD)
            .style(catalog::text::secondary),
        text(game.name.as_str())
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(GAME_GAP);
    if !game.duration.is_empty() {
        lines = lines.push(
            text(game.duration.as_str())
                .size(typography::size::CAPTION)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().live),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    container(row![lines].align_y(Alignment::Center))
        .padding(GAME_PAD)
        .width(Fill)
        .height(GAME_HEIGHT)
        .center_y(GAME_HEIGHT)
        .style(look::group)
}
