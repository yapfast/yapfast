//! The name style window over the profile editor: font, effect and colour
//! tiles on the left, the style on a card, a message and a member row on
//! the right (task P16, `NameStyleWindow`, `ui/server.slint:1158`). The
//! shell draws it over the editor's dialog; it arrives and leaves as a
//! layer of its own over a dim backdrop (`ui/server.slint:2330`), which
//! takes the keys while it's open: Escape closes it.

use iced::widget::{column, container, mouse_area, space, stack};
use iced::{Alignment, Background, Border, Color, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::people::name_style::{Intent, NameStyle};
use anomi_widgets::button::{Kind, action_button, icon_button};
use anomi_widgets::layer::{self, Backdrop};
use yapfast_lang::tr;

mod choices;
mod preview;

const WIDTH: f32 = 836.0;
const HEIGHT: f32 = 760.0;

fn footer(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let apply = Intent::Apply {
        font: window.font,
        effect: window.effect,
        colours: window.key.clone(),
    };
    iced::widget::row![
        space().width(Fill),
        action_button(tr!("Surprise Me"), Kind::Secondary, motion).on_press(Intent::Surprise),
        action_button(tr!("Apply"), Kind::Primary, motion)
            .enabled(window.changed())
            .on_press(apply),
    ]
    .spacing(8.0)
    .padding(16.0)
    .align_y(Alignment::Center)
    .boxed()
}

fn line<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Fill)
        .height(1.0)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        })
        .boxed()
}

fn window(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let choices = anomi_widgets::list::scroll_view(choices::view(window, motion), motion)
        .width(choices::WIDTH);
    let panes = iced::widget::row![choices, preview::view(window, motion)].height(Fill);
    let body = column![panes.boxed(), line(), footer(window, motion)];
    let close = container(
        icon_button(Icon::X, tr!("Close"), motion)
            .on_press(Intent::Close)
            .view(),
    )
    .width(Fill)
    .align_right(Fill)
    .padding(12.0);
    // As large as Slint's, or the window less 16 px each side: a fixed
    // size gives way to the room there is.
    let framed = container(stack![body.boxed(), close.boxed()])
        .width(WIDTH)
        .height(HEIGHT)
        .clip(true)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sidebar)),
            border: Border::default().rounded(16.0),
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.7),
                offset: Vector::new(0.0, 16.0),
                blur_radius: 48.0,
            },
            ..container::Style::default()
        });
    // A press in the window away from the colour picker closes it. The
    // area stays when it's shut, so the window keeps its widgets' state.
    let area = mouse_area(framed);
    if window.picking() {
        area.on_press(Intent::Shut).boxed()
    } else {
        area.boxed()
    }
}

pub fn view(name_style: &NameStyle, motion: Motion) -> impl View<Intent> + '_ {
    let mut layers = layer::layers(space().width(Fill).height(Fill), motion);
    if name_style.open || name_style.shown {
        layers = layers.push(
            layer::layer("name-style", window(name_style, motion))
                .open(name_style.open)
                .backdrop(Backdrop::Dim(0.5))
                .on_close(Intent::Close)
                .on_gone(Intent::Gone),
        );
    }
    layers
}

/// Its shots for `anomi-shots`: the window over the editor, from a fixture
/// (see `crate::shots`). Slint has no golden of it.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::people::name_style as sample;
    let shot = |name: &'static str, vm: fn() -> anomi_vm::Vm| crate::shots::Shot {
        area: "profile",
        name,
        size: crate::shots::GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("name_style", sample::shot),
        shot("name_style_picker", sample::shot_picking),
        shot("name_style_prism", sample::shot_prism),
    ]
}
