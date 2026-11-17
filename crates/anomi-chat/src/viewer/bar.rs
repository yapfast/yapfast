//! What lies over the picture: the bar with the name, the position, "Save"
//! and "Close", the two arrows at the sides and the pill at the bottom.
//! All of it is white over the dimmed window in every theme, as the Slint
//! viewer's (`ui/messages.slint:2692`-`:2727`).

use iced::widget::{button, column, container, row, svg, text};
use iced::{Background, Border, Color, Fill, Padding};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::typography::{self, size};
use anomi_vm::chat::viewer::{Intent, Note, Viewer};
use anomi_vm::icon::Icon;
use anomi_widgets::button::icon_button;

/// The bar's height and its padding (`:2694`).
pub const BAR: f32 = 64.0;
const LEFT: f32 = 20.0;
const RIGHT: f32 = 12.0;
const SPACING: f32 = 8.0;
/// The arrows: a circle of 48 with a glyph of 28, 20 from the window's edge.
const ARROW: f32 = 48.0;
const ARROW_GLYPH: f32 = 28.0;
const EDGE: f32 = 20.0;
/// The pill: 32 high, 20 above the bottom, 12 either side of its text.
const PILL: f32 = 32.0;
const PILL_RADIUS: f32 = 6.0;
const PILL_PAD: f32 = 12.0;
const ON_DIM: Color = Color::WHITE;
const ON_DIM_SOFT: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.7);
const ARROW_FILL: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.1);
/// Under the pointer (`:2702`).
const ARROW_HOVER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.2);
const PILL_FILL: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.7);
const PILL_INK: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.85);

/// The title bar: the name, "2 / 3" under it for several images, then
/// "Save" (not for a link's preview) and "Close".
pub fn bar(viewer: &Viewer, motion: Motion) -> Element<'_, Intent> {
    let name = text(&viewer.name)
        .size(size::BODY)
        .font(typography::BOLD)
        .color(ON_DIM)
        .ellipsis(text::Ellipsis::End)
        .wrapping(text::Wrapping::None)
        .width(Fill);
    let title = column![name]
        .extend(
            viewer
                .position()
                .map(|place| text(place).size(size::HEADING).color(ON_DIM_SOFT).boxed()),
        )
        .width(Fill);
    let save = (!viewer.preview).then(|| {
        icon_button(Icon::Download, viewer.save_tip.as_str(), motion)
            .tint(|_| ON_DIM)
            .on_press(Intent::Save {
                item: viewer.item.clone(),
                index: viewer.index,
            })
            .view()
            .boxed()
    });
    let close = icon_button(Icon::X, tr!("Close"), motion)
        .tint(|_| ON_DIM)
        .on_press(Intent::Close)
        .view()
        .boxed();
    row![title]
        .extend(save)
        .push(close)
        .spacing(SPACING)
        .align_y(iced::Center)
        .height(BAR)
        .padding(Padding::ZERO.left(LEFT).right(RIGHT))
        .boxed()
}

/// A round arrow at the side of the window, vertically centred. Slint's is a
/// plain rectangle: `ARROW_FILL` at rest, `ARROW_HOVER` under the pointer, the
/// chevron white in every theme, the pointer cursor, no tooltip.
pub fn arrow(next: bool) -> Element<'static, Intent> {
    let (icon, step) = if next {
        (Icon::ChevronRight, 1)
    } else {
        (Icon::ChevronLeft, -1)
    };
    let glyph = anomi_widgets::icon::icon(icon, ARROW_GLYPH).style(|_: &Theme, _| svg::Style {
        color: Some(ON_DIM),
    });
    let button = button(container(glyph).center(ARROW))
        .width(ARROW)
        .height(ARROW)
        .padding(0)
        .on_press(Intent::Step(step))
        .style(|_: &Theme, status| button::Style {
            background: Some(Background::Color(match status {
                button::Status::Hovered | button::Status::Pressed => ARROW_HOVER,
                _ => ARROW_FILL,
            })),
            text_color: ON_DIM,
            border: Border::default().rounded(ARROW / 2.0),
            ..button::Style::default()
        });
    let place = container(button).width(Fill).height(Fill).center_y(Fill);
    if next {
        place
            .align_right(Fill)
            .padding(Padding::ZERO.right(EDGE))
            .boxed()
    } else {
        place.padding(Padding::ZERO.left(EDGE)).boxed()
    }
}

/// What the picture's note says, in a pill at the bottom, or nothing.
pub fn note(viewer: &Viewer) -> Option<Element<'_, Intent>> {
    let words = match &viewer.note {
        Note::None => return None,
        Note::Loading => tr!("Loading the full image..."),
        Note::Failed(why) => why.as_str(),
    };
    let pill = container(text(words).size(size::HEADING).color(PILL_INK))
        .height(PILL)
        .padding(Padding::ZERO.left(PILL_PAD).right(PILL_PAD))
        .center_y(PILL)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(PILL_FILL)),
            border: Border::default().rounded(PILL_RADIUS),
            ..container::Style::default()
        });
    Some(
        container(pill)
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .align_bottom(Fill)
            .padding(Padding::ZERO.bottom(EDGE))
            .boxed(),
    )
}
