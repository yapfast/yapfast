//! What the editor asks, over the window: Delete Role or Remove member
//! (`RoleConfirmDialog`, `ui/roles.slint:664`) and Add Members
//! (`AddMembersDialog`, `:713`). Each is the card alone (440 px, raised,
//! edged, 12 px corners, a dialog's shadow); the backdrop, the way in,
//! Escape and the close (`Intent::Cancel`, `Intent::AddingClose`) are the
//! layer's.

use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::role_editor::{Adding, Confirm, Intent, Member};
use anomi_widgets::avatar::circle;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use anomi_widgets::icon::icon;
use anomi_widgets::list::list;
use yapfast_lang::tr;

use super::super::members::names;

const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 520.0;
const PADDING: f32 = 20.0;
const SPACING: f32 = 12.0;
const BUTTONS: f32 = 8.0;
const TITLE: f32 = 18.0;
const LINE: f32 = 44.0;
const BOX: f32 = 18.0;
const BOX_RADIUS: f32 = 4.0;
const TICK: f32 = 14.0;
/// Add Members' search field, which takes the keys as it opens.
pub const ADD_SEARCH: &str = "role-add-search";
const AVATAR: f32 = 28.0;
const GAP: f32 = 10.0;
const ROW_SIDES: f32 = 6.0;
const ROW_RADIUS: f32 = 4.0;
const SEARCH: f32 = 36.0;

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

fn title<'a>(words: &'a str) -> Element<'a, Intent> {
    text(words)
        .size(TITLE)
        .font(typography::BOLD)
        .style(catalog::text::primary)
        .boxed()
}

fn buttons<'a>(cancel: Intent, accept: Element<'a, Intent>, motion: Motion) -> Element<'a, Intent> {
    row![
        action_button(tr!("Cancel"), Kind::Ghost, motion)
            .on_press(cancel)
            .boxed(),
        accept
    ]
    .spacing(BUTTONS)
    .align_y(Alignment::Center)
    .boxed()
}

/// Delete Role, or Remove member, in red.
pub fn confirm(asked: &Confirm, motion: Motion) -> Element<'_, Intent> {
    let (head, body, accept) = match asked {
        Confirm::Delete { role } => (
            tr!("Delete Role"),
            tr!(
                "Are you sure you want to delete the {} role? This action cannot be undone.",
                role
            ),
            tr!("Delete"),
        ),
        Confirm::Remove { role, member } => (
            tr!("Remove member"),
            tr!("Remove {} from role {}?", member, role),
            tr!("Remove"),
        ),
    };
    let accept = action_button(accept, Kind::Danger, motion)
        .on_press(Intent::Confirm)
        .boxed();
    container(
        column![
            title(head),
            text(body)
                .size(typography::size::BODY)
                .style(catalog::text::secondary)
                .width(Length::Fill),
            container(buttons(Intent::Cancel, accept, motion)).align_right(Length::Fill),
        ]
        .spacing(SPACING),
    )
    .padding(PADDING)
    .width(WIDTH)
    .style(card)
    .boxed()
}

/// A candidate: a box that ticks, the avatar and the names.
fn candidate<'a>(member: &'a Member) -> Element<'a, Intent> {
    let chosen = member.chosen;
    let tick = container(
        chosen.then(|| icon(Icon::Check, TICK).style(catalog::svg::colored(Color::WHITE))),
    )
    .center(BOX)
    .style(move |theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            background: chosen.then(|| palette.accent.into()),
            border: Border::default()
                .rounded(BOX_RADIUS)
                .width(1)
                .color(palette.line),
            ..container::Style::default()
        }
    });
    let line = row![tick, circle(&member.avatar, AVATAR), names(member)]
        .spacing(GAP)
        .align_y(Alignment::Center)
        .padding([0.0, ROW_SIDES])
        .height(LINE);
    button(line)
        .padding(0)
        .width(Length::Fill)
        .style(|theme: &Theme, status| button::Style {
            background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                .then(|| theme.palette().lift(0.06).into()),
            border: Border::default().rounded(ROW_RADIUS),
            ..button::Style::default()
        })
        .on_press(Intent::AddingPick(member.id.clone()))
        .boxed()
}

/// Add Members: a search of the server's members and those picked.
pub fn adding(adding: &Adding, motion: Motion) -> Element<'_, Intent> {
    let candidates = &adding.candidates;
    let add = action_button(tr!("Add"), Kind::Primary, motion)
        .enabled(adding.may_add())
        .on_press(Intent::AddingAdd)
        .boxed();
    container(
        column![
            title(tr!("Add members")),
            text(tr!(
                "Select up to 30 members to add to role {}",
                adding.role
            ))
            .size(typography::size::SMALL)
            .style(catalog::text::secondary)
            .width(Length::Fill),
            field(tr!("Search members"), adding.search.as_str())
                .id(ADD_SEARCH)
                .on_input(Intent::AddingSearch)
                .text_size(typography::size::SMALL)
                .height(SEARCH)
                .motion(motion)
                .view(),
            list(
                candidates.len(),
                |at| candidates[at].id.parse().unwrap_or(at as u64),
                move |at| candidate(&candidates[at]),
                motion,
            )
            .estimate(LINE)
            .height(Length::Fill),
            container(buttons(Intent::AddingClose, add, motion)).align_right(Length::Fill),
        ]
        .spacing(SPACING),
    )
    .padding(PADDING)
    .width(WIDTH)
    .height(HEIGHT)
    .style(card)
    .boxed()
}
