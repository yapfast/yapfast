//! A server a profile lists as mutual, as a card: its banner and icon, its
//! name with its counts, what they are called there, and a way in
//! (`ServerCard`, `ui/people.slint:884`). 300 px wide, as the window gives it.

use iced::widget::{Column, Row, text};
use iced::{Alignment, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::cards::{Intent, Server, WIDTH};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use super::{head, surface};

/// The body's sides and bottom (`padding-left: 16px`, `padding-bottom: 16px`)
/// and the space between its parts.
const SIDE: f32 = metrics::SPACE_L;
const GAP: f32 = 14.0;
/// The name's size (`font-size: 20px`).
const NAME: f32 = typography::size::DIALOG_TITLE;

pub fn view(server: &Server, motion: Motion) -> Element<'_, Intent> {
    let stand = head::Stand::Initial(&server.initial);
    let mut counts = Row::<Element<'_, Intent>>::new()
        .spacing(GAP)
        .align_y(Alignment::Center);
    if !server.members.is_empty() {
        counts = counts.push(head::count(|palette| palette.faint, &server.members));
    }
    counts = counts.push(head::count(|palette| palette.faint, &server.channels));
    let name = Column::<Element<'_, Intent>>::new()
        .push(
            text(server.name.as_str())
                .size(NAME)
                .font(typography::BOLD)
                .style(catalog::text::primary)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        )
        .push(counts.boxed())
        .spacing(metrics::SPACE_XS);
    let mut body = Column::<Element<'_, Intent>>::new()
        .push(name.boxed())
        .spacing(GAP)
        .padding(Padding {
            left: SIDE,
            right: SIDE,
            bottom: SIDE,
            ..Padding::ZERO
        });
    if !server.nick.is_empty() {
        body = body.push(
            Column::<Element<'_, Intent>>::new()
                .push(head::heading(tr!("THEIR NICKNAME")))
                .push(
                    text(server.nick.as_str())
                        .size(typography::size::SMALL)
                        .style(catalog::text::primary)
                        .wrapping(text::Wrapping::None)
                        .ellipsis(text::Ellipsis::End)
                        .boxed(),
                )
                .spacing(metrics::SPACE_XS)
                .boxed(),
        );
    }
    body = body.push(
        action_button(tr!("Go to server"), Kind::Primary, motion)
            .width(Fill)
            .on_press(Intent::OpenServer(server.id.clone()))
            .boxed(),
    );
    surface(
        Column::<Element<'_, Intent>>::new()
            .push(head::view(&server.banner, &server.icon, stand, motion))
            .push(body.boxed())
            .boxed(),
        WIDTH,
        metrics::RADIUS_DIALOG,
        |theme| theme.palette().sidebar,
    )
}
