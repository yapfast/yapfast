//! A ban, over the screen: who, why, and Revoke Ban for someone who may.
//! Slint's `BanDialog` (`ui/moderation.slint:265`). The card is the
//! dialog's own (440 px, raised, edged, 12 px corners, a dialog's shadow); the backdrop, the
//! way in, Escape and the close (`Intent::Close`) are the layer's.

use iced::widget::{column, container, row, space, text};
use iced::{Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::bans::{Intent, Open};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

const WIDTH: f32 = 440.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const BUTTONS: f32 = 8.0;
const TITLE: f32 = 18.0;
const LABEL: f32 = 15.0;

fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: catalog::float::dialog(theme).shadow,
        ..container::Style::default()
    }
}

/// The dialog over the settings; `open` is what `Bans::open` holds.
pub fn dialog(open: &Open, motion: Motion) -> Element<'_, Intent> {
    let mut parts = column![
        text(&open.name)
            .size(TITLE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        column![
            text(tr!("Ban Reason"))
                .size(LABEL)
                .font(typography::MEDIUM)
                .style(catalog::text::primary),
            text(&open.reason)
                .size(typography::size::HEADING)
                .style(catalog::text::muted),
        ]
        .spacing(4),
    ]
    .spacing(SPACING);
    if !open.error.is_empty() {
        parts = parts.push(
            text(&open.error)
                .size(typography::size::SMALL)
                .style(catalog::text::danger)
                .width(Length::Fill)
                .boxed(),
        );
    }
    if open.may_unban {
        let revoke = if open.unbanning {
            tr!("Revoking...")
        } else {
            tr!("Revoke Ban")
        };
        let mut revoke = action_button(revoke, Kind::Danger, motion).enabled(!open.unbanning);
        if !open.unbanning {
            revoke = revoke.on_press(Intent::Unban);
        }
        parts = parts.push(
            row![
                space().width(Length::Fill),
                revoke,
                action_button(tr!("Done"), Kind::Primary, motion).on_press(Intent::Close),
            ]
            .spacing(BUTTONS)
            .boxed(),
        );
    }
    container(parts)
        .padding(PADDING)
        .width(WIDTH)
        .style(card)
        .boxed()
}
