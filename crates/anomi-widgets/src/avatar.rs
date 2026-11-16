//! A round picture of a person, a group or a server, with a presence dot.
//! The Slint side: `Avatar` in `ui/controls.slint`.

use iced::widget::{container, space, stack, text};
use iced::{Background, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics, typography};
use anomi_vm::kind::{Avatar, Presence};

/// What a presence dot's ring is cut out of: the ground the avatar sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    Sidebar,
    /// A selected row.
    Active,
    Panel,
    Chat,
}

impl Ground {
    fn color(self, palette: &Palette) -> Color {
        (self.picker())(palette)
    }

    fn picker(self) -> fn(&Palette) -> Color {
        match self {
            Self::Sidebar => |palette| palette.sidebar,
            Self::Active => |palette| palette.active,
            Self::Panel => |palette| palette.panel,
            Self::Chat => |palette| palette.chat,
        }
    }
}

/// Discord's presence colours, the same in every theme.
pub fn presence_color(presence: Presence) -> Option<Color> {
    let rgb = anomi_theme::rgb;
    match presence {
        Presence::None => None,
        Presence::Online | Presence::Mobile => Some(rgb(0x23a55a)),
        Presence::Idle => Some(rgb(0xf0b232)),
        Presence::DoNotDisturb => Some(rgb(0xf23f43)),
        Presence::Offline => Some(rgb(0x80848e)),
    }
}

/// The circle alone: the picture once it came, else the initial on the tint.
pub fn circle<'a, Message: 'a>(
    avatar: &'a Avatar,
    size: f32,
    ground: Ground,
) -> impl View<Message> + 'a {
    match &avatar.picture {
        Some(picture) => {
            crate::picture::rounded(picture.clone(), size, size / 2.0, ground.picker()).boxed()
        }
        None => {
            let tint = avatar.tint;
            container(
                text(avatar.initial.as_str())
                    .size((size * 0.42).round())
                    .font(typography::SEMIBOLD)
                    .color(Color::WHITE),
            )
            .center(size)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(tint)),
                border: Border::default().rounded(size / 2.0),
                ..container::Style::default()
            })
            .boxed()
        }
    }
}

/// The circle with a presence dot at its bottom right, ringed in the
/// ground's colour; no dot for [`Presence::None`].
pub fn avatar<'a, Message: 'a>(
    avatar: &'a Avatar,
    size: f32,
    presence: Presence,
    ground: Ground,
) -> impl View<Message> + 'a {
    let ring = metrics::PRESENCE_RING;
    // The same box with a dot or without, so names beside avatars line up.
    let Some(fill) = presence_color(presence) else {
        return container(circle(avatar, size, ground))
            .width(size + ring)
            .height(size + ring)
            .boxed();
    };
    let dot = (size * 0.3125).round().max(metrics::PRESENCE_DOT);
    let outer = dot + 2.0 * ring;
    let badge = container(space().width(Length::Fixed(dot)).height(Length::Fixed(dot)))
        .center(outer)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(fill)),
            border: Border::default()
                .rounded(outer / 2.0)
                .width(ring)
                .color(ground.color(theme.palette())),
            ..container::Style::default()
        });
    let corner = container(badge)
        .width(size + ring)
        .height(size + ring)
        .align_right(size + ring)
        .align_bottom(size + ring);
    stack![circle(avatar, size, ground), corner]
        .width(size + ring)
        .height(size + ring)
        .boxed()
}
