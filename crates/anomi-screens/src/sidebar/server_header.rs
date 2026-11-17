//! The server's header at the top of its channel list: the name with its
//! badge and chevron, over the server's banner where it has one, the invite
//! button, and under them the boost goal and the events row. The Slint
//! side: `ServerHeader` (`ui/rail.slint:946`) and the header box in
//! `ui/yapfast.slint:2817`; the dropdown it opens is the window's menu
//! (F06), asked for through [`Intent::Menu`].
//!
//! The sidebar draws a full-width rule under the whole block (F01's
//! `sidebar/mod.rs`); the rules between this block's own parts are drawn
//! here, and the last part leaves its rule to that one.

mod below;
mod lit;
mod name;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{Column, Row, Space, container, rule, stack};
use iced::{Fill, Gradient, Length, Padding, gradient};

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
/// The scrim keeping a white name readable over a bright banner.
const SCRIM_ALPHA: f32 = 0.6;

pub fn view(header: &ServerHeader, motion: Motion) -> impl View<Intent> + '_ {
    let below = below::rows(header, motion);
    let banner = header.has_banner();
    // The part above the rows: 49 px and its rule, which together are the
    // header's 50; the sidebar's rule stands in for it where nothing follows.
    let height = metrics::HEADER_HEIGHT - 1.0 + if banner { BANNER_EXTRA } else { 0.0 };
    let mut block = Column::new().push(
        container(top(header, motion))
            .width(Fill)
            .height(Length::Fixed(height))
            .boxed(),
    );
    if !below.is_empty() && !banner {
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
    let Some(handle) = header.banner_picture() else {
        return line.boxed();
    };
    let banner = picture(handle.clone())
        .width(Fill)
        .height(Fill)
        .fit(Fit::Cover)
        .motion(motion);
    stack![banner, scrim(), line]
        .width(Fill)
        .height(Fill)
        .boxed()
}

/// A shade from the top edge, over the banner behind the name.
fn scrim<'a>() -> impl View<Intent> + 'a {
    container(Space::new())
        .width(Fill)
        .height(metrics::HEADER_HEIGHT - 1.0)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(
                Gradient::Linear(
                    gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, iced::Color::BLACK.scale_alpha(SCRIM_ALPHA))
                        .add_stop(1.0, iced::Color::TRANSPARENT),
                )
                .into(),
            ),
            ..Default::default()
        })
}
