//! The small parts the profile editor is made of: a tile of the styles panel
//! (`CustomizeCell`, `ui/server.slint:739`), the plus on an empty slot
//! (`SlotPlus`, `:761`), the badge over a picture that can be changed
//! (`EditBadge`, `:684`) and the outline round a text that can be edited
//! (`HoverEdit`, `:697`). The card's parts are in `slots/`.

pub mod bar;
pub mod card;
mod fields;
mod head;
mod sections;
#[cfg(feature = "shots")]
pub mod shot;

use iced::widget::{Space, button, container, hover, row};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_widgets::icon::icon;

/// A tile's height, and the corner of all of them.
pub const TILE: f32 = 100.0;
pub const TILE_RADIUS: f32 = 6.0;

/// The plus on a slot with nothing in it, in the theme's quieter text so it
/// shows on a light tile as on a dark one.
pub fn plus<'a, M: 'a>(side: f32) -> Element<'a, M> {
    icon(Icon::CirclePlus, side)
        .style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().secondary),
        })
        .boxed()
}

/// A glyph in one colour of the palette.
pub fn glyph<'a, M: 'a>(
    which: Icon,
    side: f32,
    tint: fn(&anomi_theme::Palette) -> Color,
) -> Element<'a, M> {
    icon(which, side)
        .style(move |theme: &Theme, _| iced::widget::svg::Style {
            color: Some(tint(theme.palette())),
        })
        .boxed()
}

/// A tile of the styles panel: raised, hovered lighter while it can be
/// pressed; a disabled one does nothing and shows no hover. Its content sits
/// in the middle. Slint also grows it by 3% under the pointer; iced has no
/// transform, so only the colour changes.
pub fn cell<'a, M: Clone + 'a>(
    content: impl View<M> + 'a,
    height: f32,
    on_press: Option<M>,
) -> Element<'a, M> {
    let body: Element<'a, M> = container(content.boxed())
        .width(Fill)
        .height(height)
        .center_x(Fill)
        .center_y(height)
        .clip(true)
        .boxed();
    button(body)
        .padding(0)
        .width(Length::Fill)
        .on_press_maybe(on_press)
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            let fill = match status {
                button::Status::Hovered | button::Status::Pressed => palette.hover,
                _ => palette.raised,
            };
            button::Style {
                background: Some(Background::Color(fill)),
                border: Border::default().rounded(TILE_RADIUS),
                ..catalog::button::bare(theme, status)
            }
        })
        .boxed()
}

/// A tile with its own ground instead of the raised one (a banner colour, a
/// theme's gradient), which the hover brightens by a veil.
pub fn painted<'a, M: Clone + 'a>(
    content: impl View<M> + 'a,
    fill: Background,
    height: f32,
    on_press: Option<M>,
) -> Element<'a, M> {
    let body: Element<'a, M> = container(content.boxed())
        .width(Fill)
        .height(height)
        .center_x(Fill)
        .center_y(height)
        .clip(true)
        .boxed();
    button(body)
        .padding(0)
        .width(Length::Fill)
        .on_press_maybe(on_press)
        .style(move |theme: &Theme, status| button::Style {
            background: Some(fill),
            border: Border::default().rounded(TILE_RADIUS),
            ..catalog::button::bare(theme, status)
        })
        .boxed()
}

/// The badge over a picture that can be changed: a pencil, or a lock where
/// Nitro is wanted, on a round dark disc.
pub fn edit_badge<'a, M: 'a>(locked: bool) -> Element<'a, M> {
    container(
        icon(if locked { Icon::Lock } else { Icon::Pencil }, 16.0).style(|_: &Theme, _| {
            iced::widget::svg::Style {
                color: Some(Color::WHITE),
            }
        }),
    )
    .center(32.0)
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(anomi_theme::rgba(0x000000b3))),
        border: Border::default().rounded(16.0),
        ..container::Style::default()
    })
    .boxed()
}

/// What says a text can be changed: an outline round it while pointed at,
/// and on the name a pencil at its end, as Discord's editor has it, 17%
/// white, 7 px round. The outline reaches `out` past the content's sides,
/// so the caller places it that much out (see [`inset`]): the content sits
/// where it would without one. With `enabled` false the room stays and
/// nothing lights.
pub fn hover_edit<'a, M: Clone + 'a>(
    content: impl View<M> + 'a,
    enabled: bool,
    out: f32,
    pencil: bool,
    on_press: M,
) -> Element<'a, M> {
    // The name keeps room for its pencil, 9 px after it and 12 px in from
    // the outline's end, lit or not, so nothing moves under the pointer.
    let end = if pencil { PENCIL_END } else { out };
    let mut line = row![content.boxed()]
        .align_y(Alignment::Center)
        .spacing(9.0);
    if pencil {
        line = line.push(Space::new().width(PENCIL).boxed());
    }
    let base = container(line).padding(Padding {
        left: out,
        right: end,
        ..Padding::ZERO
    });
    if !enabled {
        return base.boxed();
    }
    let mut mark = row![Space::new().width(Fill).boxed()]
        .height(Fill)
        .align_y(Alignment::Center);
    if pencil {
        mark = mark.push(
            icon(Icon::Pen, PENCIL)
                .style(|_: &Theme, _| iced::widget::svg::Style {
                    color: Some(anomi_theme::rgba(0xacadb3ff)),
                })
                .boxed(),
        );
    }
    let lit = container(mark)
        .width(Fill)
        .height(Fill)
        .padding(Padding {
            right: end,
            ..Padding::ZERO
        })
        .style(|_: &Theme| container::Style {
            border: Border {
                color: anomi_theme::rgba(0xffffff2c),
                width: 1.0,
                radius: 7.0.into(),
            },
            ..container::Style::default()
        });
    button(hover(base, lit))
        .padding(0)
        .style(catalog::button::bare)
        .on_press(on_press)
        .boxed()
}

/// The name's pencil and the room it keeps from the outline's end.
const PENCIL: f32 = 15.0;
const PENCIL_END: f32 = 12.0;

/// How far an outline reaches past a text (`HoverEdit`'s `x: -8px`).
pub const EDGE: f32 = 8.0;

/// A part of the card without an outline, placed [`EDGE`] in from the
/// body's sides as the outlined ones' texts are.
pub fn inset<'a, M: 'a>(content: impl View<M> + 'a) -> Element<'a, M> {
    container(content.boxed())
        .padding(Padding::from([0.0, EDGE]))
        .boxed()
}
