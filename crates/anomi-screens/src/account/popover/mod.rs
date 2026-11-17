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
mod rows;

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Fill, Padding, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, catalog, typography};
use anomi_vm::account::{Account, Game, Intent};
use anomi_widgets::icon::Icon;
use anomi_widgets::list::scroll_view;
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use anomi_vm::people::badges::Intent as BadgeIntent;
use look::Look;

pub const WIDTH: f32 = 300.0;
const RADIUS: f32 = 8.0;
/// The popover's shadow (`account-popover.slint:48`: `#00000080`, 24 px
/// blur, 8 px down).
const SHADOW: iced::Color = iced::Color::from_rgba(0.0, 0.0, 0.0, 128.0 / 255.0);

pub fn popover(account: &Account, motion: Motion) -> impl View<Intent> + '_ {
    let popover = &account.popover;
    let look = Look::of(&popover.profile);
    let mut body = column![].spacing(12).padding(Padding {
        top: 8.0,
        right: 16.0,
        bottom: 16.0,
        left: 16.0,
    });
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
    let custom = if popover.custom.is_some() {
        tr!("Change custom status")
    } else {
        tr!("Set a custom status")
    };
    let mut status = column![
        rows::plain(Icon::Pencil, tr!("Edit Profile"), Intent::EditProfile),
        rows::rule(),
        rows::status(popover),
    ]
    .spacing(4);
    if popover.expanded {
        status = status.push(rows::options(popover, motion).boxed());
    }
    status = status.push(rows::plain(Icon::SmilePlus, custom, Intent::CustomStatus).boxed());
    body = body.push(group(status).boxed());
    let mut more = column![].spacing(4);
    if !account.user_id.is_empty() {
        more = more.push(rows::plain(Icon::Copy, tr!("Copy User ID"), Intent::CopyId).boxed());
    }
    more = more.push(rows::plain(Icon::Settings, tr!("User Settings"), Intent::Settings).boxed());
    body = body.push(group(more).boxed());
    let mut card = column![header::header(account, look, motion)];
    if !popover.profile.badges.badges.is_empty() {
        // 16 px in, as the Slint card places them.
        let badges = anomi_people::profile::badges::view(&popover.profile.badges, motion);
        card = card.push(
            container(badges.boxed().map(|intent| match intent {
                BadgeIntent::OpenLink(url) => Intent::OpenLink(url),
                _ => unreachable!("a badge only opens its link"),
            }))
            .padding(Padding::default().left(16))
            .boxed(),
        );
    }
    card = card.push(body.boxed());
    // It scrolls when the window is shorter (`ScrollView`, `:50`).
    container(scroll_view(card, motion))
        .width(WIDTH)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(look.ground(palette)),
                border: Border::default()
                    .rounded(RADIUS)
                    .width(1)
                    .color(look.border(palette)),
                shadow: Shadow {
                    color: SHADOW,
                    offset: Vector::new(0.0, 8.0),
                    blur_radius: 24.0,
                },
                ..container::Style::default()
            }
        })
}

/// A card of rows on the raised ground, 8 px in.
fn group<'a>(rows: impl View<Intent> + 'a) -> impl View<Intent> + 'a {
    container(rows).padding(8).width(Fill).style(look::group)
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
    .spacing(4);
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
        .padding(12)
        .width(Fill)
        .height(80)
        .center_y(80)
        .style(look::group)
}
