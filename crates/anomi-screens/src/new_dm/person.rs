//! One person the search found: avatar with Discord's mark, the name, the
//! line under it and a quiet "Message" button (`ui/dialogs.slint:187`-`:221`).

use iced::widget::{button, column, container, hover, row as line, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::kind::Presence;
use anomi_vm::new_dm::{Intent, Person};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::badge::discord_badge;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::presence::Ring;
use yapfast_lang::text_in;

pub const HEIGHT: f32 = 48.0;
/// The avatar's box (`width: 36px; height: 32px`) and the mark in it, whose
/// ring is cut out of the card's ground.
const BOX: f32 = 36.0;
const MARK: f32 = 14.0;
const MARK_AT: (f32, f32) = (18.0, 16.0);

pub fn row(index: usize, who: &Person, starting: bool, motion: Motion) -> Element<'_, Intent> {
    let rest = container(body(index, who, starting, false, motion))
        .width(Fill)
        .height(HEIGHT);
    let row = if starting {
        rest.boxed()
    } else {
        let over = container(body(index, who, starting, true, motion))
            .width(Fill)
            .height(HEIGHT)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().hover)),
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..container::Style::default()
            });
        hover(rest, over).boxed()
    };
    let row = button(row)
        .padding(0)
        .width(Fill)
        .height(HEIGHT)
        .style(catalog::button::bare);
    if starting {
        row.boxed()
    } else {
        row.on_press(Intent::Start(index)).boxed()
    }
}

/// What the row holds; pointed at, its button is the primary one.
fn body(
    index: usize,
    who: &Person,
    starting: bool,
    pointed: bool,
    motion: Motion,
) -> Element<'_, Intent> {
    let face = avatar(
        &who.avatar,
        metrics::AVATAR_ROW,
        Presence::None,
        Ground::Panel,
    );
    let mut picture = stack![container(face).width(BOX).height(metrics::AVATAR_ROW)];
    if who.discord {
        picture = picture.push(
            container(
                container(discord_badge(MARK, Ring::Color(Color::TRANSPARENT)))
                    .padding(0)
                    .style(|theme: &Theme| container::Style {
                        background: Some(Background::Color(theme.palette().raised)),
                        border: Border::default().rounded(10_000.0),
                        ..container::Style::default()
                    }),
            )
            .padding(Padding::ZERO.left(MARK_AT.0).top(MARK_AT.1))
            .boxed(),
        );
    }
    let lines = column![
        text(who.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::MEDIUM)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
        text(who.detail.as_str())
            .size(typography::size::LABEL)
            .style(catalog::text::muted)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .width(Fill);
    // `primary: person-touch.has-hover` (`ui/dialogs.slint:213`): the row's
    // button is the accent one while the row is pointed at.
    let kind = if pointed { Kind::Primary } else { Kind::Ghost };
    let mut write = action_button(text_in("NewDmDialog", "Message"), kind, motion)
        .small()
        .enabled(!starting);
    if pointed {
        write = write.on_press(Intent::Start(index));
    }
    line![picture, lines, write]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center)
        .padding([0.0, metrics::SPACE_S])
        .height(HEIGHT)
        .boxed()
}
