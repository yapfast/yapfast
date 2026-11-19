//! A role in the list (`RoleListRow`, `ui/roles.slint:143`): its handle to
//! drag it by, its mark, its name with what gives it and its lock, how many
//! hold it, and Edit (View for a locked role) and More. The row lights
//! under the pointer and opens the role's editor on a click.

use iced::widget::{container, row, space, stack, text};
use iced::{Alignment, Border, Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::menu::{Align, Spot};
use anomi_vm::settings::roles::{Intent, Role, Roles};
use anomi_widgets::button::{Colours, Face, icon_button};
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::tip::{Side, tip};
use yapfast_lang::tr;

use super::handle::{self, handle};
use crate::kit::{Ink, Leaf, mix};

/// A row's height (`row-height: 52px`), its corners, and the room at its
/// ends and between its parts.
pub const HEIGHT: f32 = 52.0;
const RADIUS: f32 = 6.0;
pub const LEFT: f32 = 4.0;
pub const RIGHT: f32 = 8.0;
pub const SPACING: f32 = 10.0;
/// How wide the members' count is, and the buttons after it.
pub const MEMBERS: f32 = 80.0;
pub const BUTTON: f32 = 32.0;
/// How far a press may be from the row's left to pick it up: the handle.
pub const GRIP: f32 = LEFT + handle::WIDTH;
/// The mark: its place, the dot in it, an emoji's size.
const MARK: f32 = 20.0;
const DOT: f32 = 14.0;
const EMOJI: f32 = 16.0;
/// What gives a role: 16 px tall, 4 px each side, 3 px corners, 10 px bold.
const BADGE: f32 = 16.0;
const BADGE_PAD: f32 = 4.0;
const BADGE_RADIUS: f32 = 3.0;
const BADGE_TEXT: f32 = 10.0;
/// The lock and the person after the count.
const LOCK: f32 = 14.0;
const PERSON: f32 = 16.0;
/// The hovered row's lift (`Theme.overlay.with-alpha(0.05)`).
const HOVER: f32 = 0.05;
/// How strong More is when it can't open (`opacity: 0.4`).
const DIM: f32 = 0.4;

/// An icon in a colour of the palette.
pub(super) fn glyph<'a>(which: Icon, size: f32, ink: fn(&Theme) -> Color) -> Element<'a, Intent> {
    icon(which, size)
        .style(move |theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(ink(theme)),
        })
        .boxed()
}

/// Its icon once the picture is here, else its emoji, else its colour's dot
/// (`RoleMark`, `ui/roles.slint:127`).
fn mark<'a>(role: &'a Role, motion: Motion) -> Element<'a, Intent> {
    let inside: Element<'a, Intent> = match &role.icon {
        Expected::Ready(handle) => picture(handle.clone())
            .side(MARK)
            .fit(Fit::Contain)
            .motion(motion)
            .boxed(),
        _ if !role.emoji.is_empty() => text(&role.emoji).size(EMOJI).boxed(),
        _ => {
            let color = role.color;
            container(space())
                .width(DOT)
                .height(DOT)
                .style(move |_: &Theme| container::Style {
                    background: Some(color.into()),
                    border: Border::default().rounded(DOT / 2.0),
                    ..container::Style::default()
                })
                .boxed()
        }
    };
    container(inside).center(MARK).boxed()
}

/// What gives the role, "BOT", on the accent.
fn badge<'a>(words: &'a str) -> Element<'a, Intent> {
    container(
        text(words)
            .size(BADGE_TEXT)
            .font(typography::BOLD)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().on_accent),
                ..text::Style::default()
            })
            .wrapping(text::Wrapping::None),
    )
    .height(BADGE)
    .padding(Padding::ZERO.left(BADGE_PAD).right(BADGE_PAD))
    .align_y(iced::alignment::Vertical::Center)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().accent.into()),
        border: Border::default().rounded(BADGE_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

/// The name, then what gives the role and its lock, which tells why on
/// hover: they follow the name, so the names stay in one column.
fn name<'a>(role: &'a Role, motion: Motion) -> Element<'a, Intent> {
    let mut line = row![
        text(&role.name)
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .boxed()
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if !role.badge.is_empty() {
        line = line.push(badge(&role.badge));
    }
    if role.locked() {
        let lock = glyph(Icon::Lock, LOCK, |theme| theme.palette().muted);
        line = line.push(tip(lock, &role.lock, Side::Above, motion).boxed());
    }
    container(line).width(Length::Fill).clip(true).boxed()
}

/// How many hold it, and a person.
fn members<'a>(role: &'a Role) -> Element<'a, Intent> {
    container(
        row![
            text(&role.members)
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .wrapping(text::Wrapping::None)
                .boxed(),
            glyph(Icon::User, PERSON, |theme| theme.palette().secondary),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .width(MEMBERS)
    .boxed()
}

/// Edit, or View for a role this account can't change.
fn edit<'a>(role: &'a Role, motion: Motion) -> Element<'a, Intent> {
    let (glyph, label) = if role.locked() {
        (Icon::Eye, tr!("RoleListRow" => "View"))
    } else {
        (Icon::Pencil, tr!("Edit"))
    };
    icon_button(glyph, label, motion)
        .size(BUTTON)
        .fill(|palette| palette.raised)
        .radius(BUTTON / 2.0)
        .on_press(Intent::Open(role.id.clone()))
        .view()
        .boxed()
}

/// More, which opens the role's menu below it, at its right; faint and
/// still for a role that can't be copied.
fn more<'a>(role: &'a Role, motion: Motion) -> Element<'a, Intent> {
    if !role.may_copy {
        // Slint fades the whole still button; iced fades only colours, so
        // its fill and its ink take the fade themselves.
        return Face::new(motion, |theme: &Theme, _, _, _| {
            let palette = theme.palette();
            Colours::new(
                palette.raised.scale_alpha(DIM),
                palette.faint.scale_alpha(DIM),
            )
        })
        .leading(Icon::Ellipsis)
        .width(BUTTON)
        .height(BUTTON)
        .radius(BUTTON / 2.0)
        .enabled(false)
        .boxed();
    }
    let button = icon_button(Icon::Ellipsis, tr!("More"), motion)
        .size(BUTTON)
        .radius(BUTTON / 2.0);
    let id = role.id.clone();
    opener(button.fill(|palette| palette.raised).view())
        .on_press(move |bounds| Intent::More(id.clone(), Spot::below(bounds).align(Align::End)))
        .boxed()
}

/// The role at `index` in the list.
pub fn role<'a>(roles: &'a Roles, index: usize, motion: Motion) -> Element<'a, Intent> {
    let role = &roles.rows[index];
    let held = roles.dragging.as_deref() == Some(role.id.as_str());
    let mut backdrop = Leaf::new("", motion, |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        Ink {
            fill: mix(Color::TRANSPARENT, palette.lift(HOVER), lit),
            text: palette.text,
            glyph: palette.text,
        }
    })
    .on_press(Intent::Open(role.id.clone()));
    backdrop.stretch = true;
    backdrop.height = HEIGHT;
    backdrop.radius = RADIUS;
    let parts = row![
        handle(roles.may_move(index), held, motion).boxed(),
        mark(role, motion),
        name(role, motion),
        members(role),
        edit(role, motion),
        more(role, motion),
    ]
    .spacing(SPACING)
    .padding(Padding::ZERO.left(LEFT).right(RIGHT))
    .height(HEIGHT)
    .align_y(Alignment::Center);
    stack![backdrop, parts].boxed()
}
