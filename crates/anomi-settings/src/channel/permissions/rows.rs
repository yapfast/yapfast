//! A role or member as a row: the overwrites on the left of the page and
//! the roles and members of Add's picker. A coloured dot or a glyph leads
//! the name; the row is a [`Leaf`] with a see-through glyph in the dot's
//! place and the dot laid over it.

use iced::widget::{container, row, stack, text};
use iced::{Border, Color, Fill, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Presence;
use anomi_vm::settings::permissions::{Found, Intent, Overwrite};
use anomi_widgets::avatar::{Ground, avatar};

use crate::kit::{Ink, Leaf, mix};

const RADIUS: f32 = 4.0;
const DOT: f32 = 10.0;
const USER: f32 = 14.0;

/// A role's colour; `@everyone` and roles without one get the grey.
fn tint(theme: &Theme, color: u32) -> Color {
    if color == 0 {
        theme.palette().muted
    } else {
        Color::from_rgb8((color >> 16) as u8, (color >> 8) as u8, color as u8)
    }
}

/// The dot of a role, over the leaf's see-through glyph.
fn dot<'a>(color: u32, pad: f32, height: f32) -> Element<'a, Intent> {
    container(
        container(iced::widget::space().width(DOT).height(DOT)).style(move |theme: &Theme| {
            container::Style {
                background: Some(tint(theme, color).into()),
                border: Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            }
        }),
    )
    .padding(Padding::ZERO.left(pad))
    .height(height)
    .align_y(alignment::Vertical::Center)
    .boxed()
}

/// A leaf that names a row. `selected` is the page list's choice, whose
/// names rest in `secondary` until chosen; the picker's (`None`) stay
/// bright. `person` shows the person glyph; otherwise its room is kept for
/// a dot laid over it.
fn leaf<'a>(
    name: &'a str,
    selected: Option<bool>,
    person: bool,
    motion: Motion,
    on_press: Intent,
) -> Leaf<'a, Intent> {
    let mut leaf = Leaf::new(name, motion, move |theme: &Theme, lit: f32| {
        let palette = theme.palette();
        let fill = if selected == Some(true) {
            palette.active
        } else {
            mix(Color::TRANSPARENT, palette.hover, lit)
        };
        Ink {
            fill,
            text: if selected == Some(false) {
                palette.secondary
            } else {
                palette.text
            },
            glyph: if person {
                palette.muted
            } else {
                Color::TRANSPARENT
            },
        }
    })
    .on_press(on_press);
    leaf.icon = Some(Icon::User);
    leaf.glyph = if person { USER } else { DOT };
    leaf.pad = 8.0;
    leaf.font = typography::UI;
    leaf.stretch = true;
    leaf.radius = RADIUS;
    leaf
}

/// An overwrite of the page's list: 32 px, a dot or a person, the name.
pub fn overwrite(over: &Overwrite, motion: Motion) -> Element<'_, Intent> {
    let mut base = leaf(
        &over.name,
        Some(over.selected),
        over.member,
        motion,
        Intent::Show(over.id.clone()),
    );
    base.height = 32.0;
    base.size = typography::size::SMALL;
    if over.member {
        return container(base).width(Fill).boxed();
    }
    stack![base, dot(over.color, 8.0, 32.0)].width(Fill).boxed()
}

/// A role of Add's picker: 36 px, a dot, the name 10 px after it.
pub fn role(over: &Overwrite, motion: Motion) -> Element<'_, Intent> {
    let mut base = leaf(
        &over.name,
        None,
        false,
        motion,
        Intent::AddRole(over.id.clone()),
    );
    base.height = 36.0;
    base.gap = 10.0;
    stack![base, dot(over.color, 8.0, 36.0)].width(Fill).boxed()
}

/// A member of Add's picker: 40 px, their picture, name and username.
pub fn member(found: &Found, motion: Motion) -> Element<'_, Intent> {
    let mut base = leaf("", None, false, motion, Intent::AddMember(found.id.clone()));
    base.glyph = 0.0;
    base.pad = 6.0;
    base.height = 40.0;
    let mut names = row![
        text(&found.name)
            .size(typography::size::BODY)
            .font(typography::SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed(),
    ]
    .spacing(6)
    .align_y(alignment::Vertical::Center);
    if !found.username.is_empty() {
        names = names.push(
            text(&found.username)
                .size(typography::size::SMALL)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    let content = row![
        avatar(&found.avatar, 26.0, Presence::None, Ground::Raised),
        names
    ]
    .spacing(10)
    .align_y(alignment::Vertical::Center);
    stack![
        base,
        container(content)
            .padding(Padding::ZERO.left(6))
            .height(40)
            .align_y(alignment::Vertical::Center)
    ]
    .width(Fill)
    .boxed()
}
