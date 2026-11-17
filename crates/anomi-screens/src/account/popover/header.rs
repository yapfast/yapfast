//! The popover's top: the banner (its picture, else the accent, else the
//! theme's colour, else Yapfast's sunken ground) 80 px tall, the avatar
//! 80 px in a ring 88 px across at (12, 36) with the status dot at its
//! lower right, and the name 20 px bold at (16, 128), with the server tag
//! after it (`header`, `ui/yapfast/account-popover.slint:63`).

use iced::font::Weight;
use iced::widget::{container, row, space, stack};
use iced::{Alignment, Border, Fill, Padding, border};

use anomi_theme::prelude::*;
use anomi_theme::{Motion, Theme, typography};
use anomi_vm::account::{Account, Intent};
use anomi_widgets::avatar::portrait;
use anomi_widgets::name::styled_name;
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::presence::status_dot;
use anomi_widgets::tag::server_tag;

use super::look::Look;

pub const HEIGHT: f32 = 156.0;
const BANNER: f32 = 80.0;
const RING: f32 = 88.0;
const AVATAR: f32 = 80.0;
const CORNER: f32 = 8.0;
const DOT: f32 = 20.0;
/// The name's line: the card less its 16 px sides (`width: parent.width - 32px`).
const LINE: f32 = super::WIDTH - 32.0;
/// Where the avatar's ring, the dot over it and the name sit (`:63`-`:75`).
const RING_AT: (f32, f32) = (12.0, 36.0);
const DOT_AT: (f32, f32) = (RING_AT.0 + 66.0, RING_AT.1 + 65.0);
const NAME_AT: (f32, f32) = (16.0, 128.0);
/// The name's line is 24 px high, its tag 6 px after it.
const NAME_HEIGHT: f32 = 24.0;
const NAME_GAP: f32 = 6.0;

pub fn header<'a>(account: &'a Account, look: Look, motion: Motion) -> impl View<Intent> + 'a {
    let profile = &account.popover.profile;
    let banner = {
        let (accent, theme) = (profile.accent, look.theme);
        let ground = container(space())
            .width(Fill)
            .height(BANNER)
            .style(move |theme_: &Theme| container::Style {
                background: Some(
                    accent
                        .or(theme.map(|theme| theme.primary))
                        .unwrap_or(theme_.palette().sunken)
                        .into(),
                ),
                border: Border::default().rounded(border::top(CORNER)),
                ..container::Style::default()
            });
        match &profile.banner {
            Some(handle) => stack![
                ground,
                picture(handle.clone())
                    .width(Fill)
                    .height(BANNER)
                    .fit(Fit::Cover)
                    .radius(border::top(CORNER))
                    .motion(motion)
            ]
            .boxed(),
            None => ground.boxed(),
        }
    };
    let (presence, ring_look) = (account.popover.choice.presence(), look);
    let avatar = container(
        portrait(&account.avatar, AVATAR)
            .decoration(account.decoration.as_ref())
            .motion(motion)
            .view(),
    )
    .center(RING)
    .style(move |theme: &Theme| container::Style {
        background: Some(ring_look.ring(theme.palette()).into()),
        border: Border::default().rounded(RING / 2.0),
        ..container::Style::default()
    });
    // The dot sits over the ring's lower right, cut out in the ring's colour.
    let dot = status_dot(presence, DOT, look.dot_ring());
    let name = styled_name(&account.name, profile.name_style)
        .size(typography::size::DIALOG_TITLE)
        .weight(Weight::Bold)
        .ink(look.ink());
    let mut line = row![name]
        .spacing(NAME_GAP)
        .align_y(Alignment::Center)
        .width(LINE)
        .height(NAME_HEIGHT);
    if let Some(tag) = &profile.tag {
        let tint = look.tag();
        let mut chip = server_tag(tag, motion).fill(tint);
        if let Some(ink) = look.themed_ink() {
            chip = chip.color(ink);
        }
        line = line.push(chip.view().boxed());
    }
    // The base layer sets the stack's size (a fixed-height first child
    // would be stretched to it); the banner is only 80 px of it.
    stack![
        space().width(Fill).height(HEIGHT),
        banner,
        at(avatar, RING_AT),
        at(dot, DOT_AT),
        at(line, NAME_AT)
    ]
    .width(Fill)
    .height(HEIGHT)
}

fn at<'a>(content: impl View<Intent> + 'a, (x, y): (f32, f32)) -> impl View<Intent> + 'a {
    container(content).padding(Padding {
        top: y,
        left: x,
        ..Padding::ZERO
    })
}
