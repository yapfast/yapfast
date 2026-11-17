//! One friend in the picker: avatar with its dot, the name, and a box that
//! ticks when chosen; 44 px tall, the whole row a button (`ui/header.slint:
//! 578`). A row that may not be chosen (no room left, or one is on its way)
//! answers nothing, shows no hover and dims.

use iced::widget::{button, container, row, space, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Presence;
use anomi_vm::people::add_people::{Intent, Person};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::icon::icon;

/// The row's height and corners, its inner space and spacing.
pub const HEIGHT: f32 = 44.0;
const SIDE: f32 = metrics::SPACE_S;
const GAP: f32 = metrics::SPACE_M;
/// The avatar (`32px`), the name's size, the box (`22px`, 2 px border,
/// 6 px corners) and its tick (`16px`).
const FACE: f32 = metrics::AVATAR_ROW;
const NAME: f32 = 16.0;
const BOX: f32 = 22.0;
const TICK: f32 = 16.0;
const BOX_RADIUS: f32 = metrics::RADIUS_ROW;
/// What an unavailable row keeps of its colours (`opacity: 0.5`); the
/// pictures stay as they are, iced having no layer opacity.
const DIM: f32 = 0.5;

pub fn view(person: &Person, may: bool) -> Element<'_, Intent> {
    let dim = if may || person.chosen { 1.0 } else { DIM };
    let presence = match person.presence {
        Presence::None | Presence::Offline => Presence::None,
        other => other,
    };
    let name = text(person.name.as_str())
        .size(NAME)
        .font(typography::MEDIUM)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill)
        .style(move |theme: &Theme| text::Style {
            color: Some(theme.palette().text.scale_alpha(dim)),
            ..text::Style::default()
        });
    let content = row![
        avatar(&person.avatar, FACE, presence, Ground::Sidebar),
        name,
        tick(person.chosen, dim),
    ]
    .spacing(GAP)
    .align_y(Alignment::Center)
    .padding(Padding::from([0.0, SIDE]));
    let mut face = button(container(content).height(HEIGHT).center_y(HEIGHT))
        .padding(0)
        .width(Fill)
        .height(HEIGHT)
        .style(catalog::button::plain);
    if may {
        face = face.on_press(Intent::Choose(person.id.clone()));
    }
    face.boxed()
}

/// The box at the row's end: the accent's fill and a tick when chosen, the
/// faint outline when not.
fn tick<'a>(chosen: bool, dim: f32) -> Element<'a, Intent> {
    let inner: Element<'a, Intent> = if chosen {
        icon(Icon::Check, TICK)
            .style(|theme: &Theme, _status| iced::widget::svg::Style {
                color: Some(theme.palette().on_accent),
            })
            .boxed()
    } else {
        space().boxed()
    };
    container(inner)
        .width(BOX)
        .height(BOX)
        .center(BOX)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            let edge = if chosen {
                palette.accent
            } else {
                palette.faint
            };
            container::Style {
                background: chosen.then(|| Background::Color(palette.accent.scale_alpha(dim))),
                border: Border::default()
                    .rounded(BOX_RADIUS)
                    .width(2)
                    .color(edge.scale_alpha(dim)),
                ..container::Style::default()
            }
        })
        .boxed()
}
