//! The server's header at the top of its channel list: the name with its
//! badge and chevron, over the server's banner where it has one, the invite
//! button, and under them the boost goal and the events row. The Slint
//! side: `ServerHeader` (`ui/rail.slint:946`) and the header box in
//! `ui/yapfast.slint:2817`; the dropdown it opens is the window's menu
//! (F06), asked for through [`Intent::Menu`].
//!
//! The block ends in Slint's rules, drawn here: each row has its own, a bare
//! header ends in a full-width one, and a banner with no rows in none.

mod below;
mod chevron;
mod lit;
mod name;
#[cfg(feature = "shots")]
mod shots;

use iced::border::Radius;
use iced::widget::{Column, Row, Space, Stack, container, rule};
use iced::{Border, Fill, Gradient, Length, Padding, gradient};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics};
use anomi_vm::sidebar::server_header::{Intent, ServerHeader};
use anomi_widgets::button::{Side, icon_button};
use anomi_widgets::icon::Icon;
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

#[cfg(feature = "shots")]
pub use shots::shots;

/// How much taller the header is with a banner behind the name.
const BANNER_EXTRA: f32 = 81.0;
/// The name's box, and the room above it.
const NAME_HEIGHT: f32 = 32.0;
/// The sheet's rounded top-left corner (`ui/yapfast.slint:2818`).
const SHEET_CORNER: f32 = 12.0;
/// The scrim keeping a white name readable over a bright banner.
const SCRIM_ALPHA: f32 = 0.6;

pub fn view(header: &ServerHeader, motion: Motion) -> impl View<Intent> + '_ {
    let below = below::rows(header, motion);
    let banner = header.has_banner();
    // The part above the rows: 49 px, then its full-width rule where no
    // banner is and (a row or nothing) follows. Under a banner nothing draws
    // that rule, so the top is the whole 131.
    let height = if banner {
        metrics::HEADER_HEIGHT + BANNER_EXTRA
    } else {
        metrics::HEADER_HEIGHT - 1.0
    };
    let mut block = Column::new().push(
        container(top(header, motion))
            .width(Fill)
            .height(Length::Fixed(height))
            .boxed(),
    );
    if !banner {
        block = block.push(rule::horizontal(1).style(catalog::rule::outline).boxed());
    }
    block.extend(below).width(Fill)
}

/// The name, the invite button, and the banner behind them.
fn top(header: &ServerHeader, motion: Motion) -> Element<'_, Intent> {
    let mut line = Row::new()
        .push(name::name(header, motion).boxed())
        .spacing(metrics::SPACE_S);
    if header.invite {
        line = line.push(
            icon_button(Icon::UserPlus, tr!("Invite people"), motion)
                .on_press(Intent::Invite)
                .tip_side(Side::Below)
                .view()
                .boxed(),
        );
    }
    let line = container(line)
        .padding(Padding::new(metrics::SPACE_S).bottom(0.0))
        .width(Fill)
        .height(NAME_HEIGHT + metrics::SPACE_S);
    if !header.has_banner() {
        return line.boxed();
    }
    // The scrim shows from the banner's URL on; the picture joins it when
    // it arrives.
    let mut layers = Stack::new().width(Fill).height(Fill);
    if let Some(handle) = header.banner_picture() {
        layers = layers.push(
            picture(handle.clone())
                .width(Fill)
                .height(Fill)
                .fit(Fit::Cover)
                .radius(Radius::default().top_left(SHEET_CORNER))
                .motion(motion)
                .boxed(),
        );
    }
    layers.push(scrim().boxed()).push(line.boxed()).boxed()
}

/// A shade from the top edge, over the banner behind the name.
fn scrim<'a>() -> impl View<Intent> + 'a {
    container(Space::new())
        .width(Fill)
        .height(metrics::HEADER_HEIGHT)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(
                Gradient::Linear(
                    gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, iced::Color::BLACK.scale_alpha(SCRIM_ALPHA))
                        .add_stop(1.0, iced::Color::TRANSPARENT),
                )
                .into(),
            ),
            border: Border {
                radius: Radius::default().top_left(SHEET_CORNER),
                ..Border::default()
            },
            ..Default::default()
        })
}
