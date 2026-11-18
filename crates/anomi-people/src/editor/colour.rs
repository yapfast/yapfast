//! The colour popover (`ColourPopover`, `ui/server.slint:989`): a pad of
//! saturation across and brightness up, a bar of hue under it, and the
//! colour's code in a field, 240 by 250, as Discord's has it. A press or
//! drag on the pad or bar places the colour and writes its code into the
//! field; a code that is a colour moves them, and one that isn't stays as
//! typed ([`Colour`]). The shell draws the popover's surface and the way
//! in; this is its card. Task P15 (docs/iced/tasks/people.md).

mod pad;

use iced::widget::{Column, canvas, container, space, stack};
use iced::{Background, Border, Color, Degrees, Fill, Shadow, Vector, gradient};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::people::editor_preview::{Change, Colour, Intent};
use anomi_widgets::field::field;

use pad::{Kind, Pointer};

pub const WIDTH: f32 = 240.0;
pub const HEIGHT: f32 = 250.0;
const INSET: f32 = 10.0;
const PAD: f32 = 150.0;
const BAR: f32 = 10.0;
const FIELD: f32 = 42.0;
/// The bar's room: its knob stands 3 px over it each way.
const BAR_ROOM: f32 = 16.0;

/// The popover's card on `colour`. A name style window maps its intents the
/// way the editor does.
pub fn view(colour: &Colour, motion: Motion) -> impl View<Intent> + '_ {
    let area = WIDTH - 2.0 * INSET;
    let hue = colour.pure();
    let pad = stack![
        shade(
            gradient::Linear::new(Degrees(90.0))
                .add_stop(0.0, Color::WHITE)
                .add_stop(1.0, hue),
            area,
            PAD,
            8.0,
        ),
        shade(
            gradient::Linear::new(Degrees(180.0))
                .add_stop(0.0, Color::TRANSPARENT)
                .add_stop(1.0, Color::BLACK),
            area,
            PAD,
            8.0,
        ),
        pointer(
            Kind::Pad {
                sat: colour.sat,
                val: colour.val
            },
            area,
            PAD
        ),
    ];
    let mut rainbow = gradient::Linear::new(Degrees(90.0));
    for (at, rgb) in [
        (0.0, 0xff0000),
        (0.17, 0xffff00),
        (0.33, 0x00ff00),
        (0.5, 0x00ffff),
        (0.67, 0x0000ff),
        (0.83, 0xff00ff),
        (1.0, 0xff0000),
    ] {
        rainbow = rainbow.add_stop(at, rgba(rgb << 8 | 0xff));
    }
    let bar = stack![
        container(shade(rainbow, area, BAR, BAR / 2.0))
            .center_y(BAR_ROOM)
            .boxed(),
        pointer(Kind::Bar { hue: colour.hue }, area, BAR_ROOM),
    ];
    let code = field("#rrggbb", &colour.text)
        .on_input(|text| Intent::Colour(Change::Text(text)))
        .text_size(14.0)
        .height(FIELD)
        .radius(10.0)
        .motion(motion)
        .view();
    let body = Column::<Element<'_, Intent>>::new()
        .padding(INSET)
        .push(pad.boxed())
        .push(space().height(7).boxed())
        .push(bar.boxed())
        .push(space().height(11).boxed())
        .push(code);
    container(body)
        .width(WIDTH)
        .height(HEIGHT)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default()
                .rounded(12)
                .width(1)
                .color(theme.palette().edge),
            shadow: Shadow {
                color: rgba(0x00000080),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 24.0,
            },
            ..container::Style::default()
        })
}

/// A gradient fill, rounded, `width` by `height`.
fn shade<'a>(stops: gradient::Linear, width: f32, height: f32, radius: f32) -> Element<'a, Intent> {
    container(space().width(Fill).height(Fill))
        .width(width)
        .height(height)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Gradient(stops.into())),
            border: Border::default().rounded(radius),
            ..container::Style::default()
        })
        .boxed()
}

fn pointer<'a>(kind: Kind, width: f32, height: f32) -> Element<'a, Intent> {
    canvas(Pointer {
        kind,
        send: Intent::Colour,
    })
    .width(width)
    .height(height)
    .boxed()
}
