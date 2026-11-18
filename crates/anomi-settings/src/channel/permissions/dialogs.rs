//! What Permissions asks or tells, and Add's picker, over the screen:
//! `OverwriteConfirmDialog` and `AddOverwriteDialog`
//! (`ui/channel_settings.slint:322`, `:383`). The card is the dialog's own
//! (raised, edged, 12 px corners); the backdrop, the way in, Escape and a
//! press outside (`Intent::Cancel`, `Intent::CloseAdd`) are the layer's.

use iced::widget::{column, container, row, space, text};
use iced::{Alignment, Border, Color, Fill, Length, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::permissions::{Adding, Confirm, Intent, Permissions};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use super::rows;
use crate::kit::heading;

const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const BUTTONS: f32 = 8.0;
const CONFIRM_WIDTH: f32 = 440.0;
const ADD_WIDTH: f32 = 400.0;
const ADD_HEIGHT: f32 = 480.0;
const TITLE: f32 = 18.0;
/// The window's dialogs' shadow: 32 px blur, 12 px down, `#00000066`.
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0x66 as f32 / 255.0),
    offset: Vector::new(0.0, 12.0),
    blur_radius: 32.0,
};

fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: SHADOW,
        ..container::Style::default()
    }
}

fn title(words: &str) -> Element<'_, Intent> {
    text(words)
        .width(Fill)
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .boxed()
}

/// The question or notice of `permissions.confirm`, or nothing.
pub fn confirm(permissions: &Permissions, motion: Motion) -> Option<Element<'_, Intent>> {
    let kind = permissions.confirm?;
    let name = permissions.confirm_name.as_str();
    let head = match kind {
        Confirm::Remove => tr!("Delete Permission Settings"),
        Confirm::Sync => tr!("Sync permissions?"),
        Confirm::Refused => tr!("Pretty sure you don't want to do this"),
    };
    let body = match kind {
        Confirm::Remove => tr!(
            "Are you sure you want to delete {} permissions? This action cannot be undone.",
            name
        ),
        Confirm::Sync => tr!(
            "Would you like to sync {} permissions with {}?",
            permissions.channel,
            permissions.category
        ),
        Confirm::Refused => tr!(
            "You cannot deny this permission to {} because it will deny it to you as well. Allow another role or yourself before trying again.",
            name
        ),
    };
    let leave = match kind {
        Confirm::Sync => tr!("Keep Current Permissions"),
        Confirm::Refused => tr!("Okay"),
        Confirm::Remove => tr!("Cancel"),
    };
    let mut buttons: Vec<Element<'_, Intent>> = vec![
        action_button(
            leave,
            if kind == Confirm::Refused {
                Kind::Primary
            } else {
                Kind::Ghost
            },
            motion,
        )
        .on_press(Intent::Cancel)
        .boxed(),
    ];
    match kind {
        Confirm::Remove => {
            buttons.push(
                action_button(tr!("Delete"), Kind::Danger, motion)
                    .on_press(Intent::Confirm)
                    .boxed(),
            );
        }
        Confirm::Sync => {
            buttons.push(
                action_button(tr!("Sync Permissions"), Kind::Primary, motion)
                    .on_press(Intent::Confirm)
                    .boxed(),
            );
        }
        Confirm::Refused => {}
    }
    // 440 px, or as wide as its buttons take: a translation's words run
    // longer than the English. The column shrinks to its widest piece that
    // isn't fluid, the buttons over a 400 px floor, then wraps the words in it.
    let buttons = column![
        space().width(CONFIRM_WIDTH - 2.0 * PADDING),
        row(buttons).spacing(BUTTONS),
    ]
    .align_x(Alignment::End);
    Some(
        container(
            column![
                title(head),
                text(body)
                    .width(Fill)
                    .size(typography::size::BODY)
                    .style(catalog::text::secondary),
                buttons,
            ]
            .spacing(SPACING)
            .width(Length::Shrink),
        )
        .padding(PADDING)
        .width(Length::Shrink)
        .style(card)
        .boxed(),
    )
}

/// Add's picker: the roles without an overwrite, then the members found.
pub fn add<'a>(adding: &'a Adding, motion: Motion) -> Element<'a, Intent> {
    let mut list: Vec<Element<'_, Intent>> = Vec::new();
    if !adding.roles.is_empty() {
        list.push(heading(tr!("ROLES")).boxed());
        for role in &adding.roles {
            list.push(rows::role(role, motion));
        }
    }
    if !adding.members.is_empty() {
        list.push(heading(tr!("MEMBERS")).boxed());
        for member in &adding.members {
            list.push(rows::member(member, motion));
        }
    }
    container(
        column![
            title(tr!("Add:")),
            field(tr!("Role/Member"), &adding.query)
                .id(SEARCH)
                .text_size(typography::size::SMALL)
                .on_input(Intent::Search)
                .motion(motion)
                .view(),
            container(scroll_view(column(list), motion)).height(Length::Fill),
            row![
                space().width(Fill),
                action_button(tr!("Cancel"), Kind::Ghost, motion).on_press(Intent::CloseAdd),
            ],
        ]
        .spacing(SPACING),
    )
    .padding(PADDING)
    .width(ADD_WIDTH)
    .height(ADD_HEIGHT)
    .style(card)
    .boxed()
}

/// The search field's id, for the app to focus when Add opens.
pub const SEARCH: &str = "channel-permissions-add";
