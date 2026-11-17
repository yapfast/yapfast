//! One person the search found: avatar with Discord's mark, the name, the
//! line under it and a quiet "Message" button (`ui/dialogs.slint:187`-`:221`).

use iced::widget::{button, column, container, row as line, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::kind::Presence;
use anomi_vm::new_dm::{Intent, Person};
use anomi_widgets::avatar::{Ground, avatar};
use anomi_widgets::badge::discord_badge;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::presence::Ring;
use yapfast_lang::tr;

pub const HEIGHT: f32 = 48.0;
/// The avatar's box (`width: 36px; height: 32px`) and the mark in it, whose
/// ring is cut out of the card's ground.
const BOX: f32 = 36.0;
const MARK: f32 = 14.0;
const MARK_AT: (f32, f32) = (17.0, 15.0);

pub fn row(index: usize, who: &Person, starting: bool, motion: Motion) -> Element<'_, Intent> {
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
    // Quiet until pointed at is the Slint button's; the row's own hover
    // already says which one is chosen, and the button lights when pointed.
    let write = action_button(tr!("Message"), Kind::Ghost, motion)
        .small()
        .enabled(!starting)
        .on_press(Intent::Start(index));
    let content = line![picture, lines, write]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center);
    let row = button(container(content).center_y(HEIGHT))
        .padding([0.0, metrics::SPACE_S])
        .width(Fill)
        .height(HEIGHT)
        .style(move |theme: &Theme, status| {
            let mut style = catalog::button::plain(theme, status);
            if starting {
                style.background = None;
            }
            style
        });
    if starting {
        row.boxed()
    } else {
        row.on_press(Intent::Start(index)).boxed()
    }
}
