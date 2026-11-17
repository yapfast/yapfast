//! Security Actions: Slint's `SecurityActionsDialog`
//! (`ui/server_people.slint:400`), Discord's dialog to pause invites, DMs
//! or both for some hours. The card is the dialog's own (480 px, raised,
//! edged, 12 px corners, `padding: 20px; spacing: 12px`); the backdrop, the
//! way in and Escape are the layer's.

use iced::widget::{column, container, row, space, text};
use iced::{Border, Color, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::safety::{Actions, Intent};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use super::choices::LINE;
use crate::kit::{picker, switch, words};

const WIDTH: f32 = 480.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const BUTTONS: f32 = 8.0;
const TITLE: f32 = 18.0;
const SHADOW_BLUR: f32 = 32.0;
const SHADOW_DROP: f32 = 12.0;

fn dialog(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.4),
            offset: Vector::new(0.0, SHADOW_DROP),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// The dialog over the settings; `actions` is what `Safety::actions` holds.
pub fn actions(actions: &Actions, motion: Motion) -> Element<'_, Intent> {
    let ready = !actions.saving;
    let hours = words(&[
        tr!("1 hour"),
        tr!("2 hours"),
        tr!("4 hours"),
        tr!("6 hours"),
        tr!("12 hours"),
        tr!("24 hours"),
    ]);
    let pick = ready.then_some(Intent::Hours);
    let mut parts = column![
        text(tr!("Security Actions"))
            .size(TITLE)
            .line_height(LINE)
            .font(typography::BOLD)
            .style(catalog::text::primary),
        picker(hours, actions.hours, pick),
        switch(tr!("Pause Invites"), actions.pause_invites, motion)
            .description(tr!(
                "Temporarily stop new members from joining this server via invite or vanity links."
            ))
            .enabled(ready)
            .on_toggle(|_| Intent::PauseInvites)
            .view(),
        switch(tr!("Pause DMs"), actions.pause_dms, motion)
            .description(tr!(
                "Temporarily stop new direct messages from being sent between members in your server. Friends can still DM each other, moderators can still DM members, and your Apps can still DM members."
            ))
            .enabled(ready)
            .on_toggle(|_| Intent::PauseDms)
            .view(),
    ]
    .spacing(SPACING);
    if !actions.error.is_empty() {
        parts = parts.push(
            text(&actions.error)
                .size(typography::size::SMALL)
                .line_height(LINE)
                .style(catalog::text::danger)
                .width(Length::Fill)
                .boxed(),
        );
    }
    let save = if actions.saving {
        tr!("Saving...")
    } else {
        tr!("Save")
    };
    parts = parts.push(
        row![
            space().width(Length::Fill),
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .enabled(ready)
                .on_press(Intent::CloseActions),
            action_button(save, Kind::Primary, motion)
                .enabled(actions.may_save && ready)
                .on_press(Intent::SaveActions),
        ]
        .spacing(BUTTONS)
        .boxed(),
    );
    container(parts)
        .padding(PADDING)
        .width(WIDTH)
        .style(dialog)
        .boxed()
}
