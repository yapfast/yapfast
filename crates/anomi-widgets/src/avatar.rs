//! A person's picture, or a round initial in their own tint until it
//! arrives; with a presence dot at its corner, a decoration around it, a ring
//! while they speak. The Slint side: `Avatar` in `ui/controls.slint:548`.
//!
//! [`avatar`] is the usual one, a circle with its dot; [`portrait`] takes
//! the rest: a rounded square or other corners, a decoration, speaking.

mod halo;

use iced::border::Radius;
use iced::widget::image;
use iced::widget::{container, space, text};
use iced::{Background, Border, Color, Length};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, typography};
use anomi_vm::kind::{Avatar, Presence};

use crate::picture::{Fit, picture};
use crate::presence::{Ring, status_dot};

/// The dot's slot (dot and ring) as a share of the avatar, and where its
/// corner sits: 16 px at (20, 20) on the 32 px avatar of a member row
/// (`ui/rail.slint:360`); callers that Slint places differently say so with
/// [`Portrait::dot`].
const DOT_SIDE: f32 = 0.5;
const DOT_AT: f32 = 0.625;
/// The initial's size as a share of the avatar (`ui/controls.slint:614`).
const INITIAL: f32 = 0.42;

/// What a presence dot's ring is cut out of: the ground the avatar sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    Sidebar,
    /// A selected row.
    Active,
    /// A row under the pointer.
    Hover,
    Panel,
    /// A raised card: a popover's group, a profile's side card.
    Raised,
    Chat,
}

impl Ground {
    pub(crate) fn color(self, palette: &Palette) -> Color {
        match self {
            Self::Sidebar => palette.sidebar,
            Self::Active => palette.active,
            Self::Hover => palette.hover,
            Self::Panel => palette.panel,
            Self::Raised => palette.raised,
            Self::Chat => palette.chat,
        }
    }
}

/// An avatar's outline: a circle, a rounded square, or corners that differ
/// (Discord's folder previews have a large outer corner and small others).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Shape {
    #[default]
    Circle,
    Rounded(f32),
    /// Top left, top right, bottom right, bottom left.
    Corners([f32; 4]),
}

impl Shape {
    fn radius(self, size: f32) -> Radius {
        match self {
            Self::Circle => Radius::new(size / 2.0),
            Self::Rounded(radius) => Radius::new(radius),
            Self::Corners([tl, tr, br, bl]) => Radius {
                top_left: tl,
                top_right: tr,
                bottom_right: br,
                bottom_left: bl,
            },
        }
    }
}

/// An avatar `size` across; chain the setters, then [`Portrait::view`].
pub struct Portrait<'a> {
    avatar: &'a Avatar,
    size: f32,
    shape: Shape,
    ground: Ring,
    presence: Presence,
    decoration: Option<&'a image::Handle>,
    speaking: bool,
    dot: Option<(f32, (f32, f32))>,
    opacity: f32,
    motion: Motion,
}

pub fn portrait(avatar: &Avatar, size: f32) -> Portrait<'_> {
    Portrait {
        avatar,
        size,
        shape: Shape::Circle,
        ground: Ring::Ground(Ground::Sidebar),
        presence: Presence::None,
        decoration: None,
        speaking: false,
        dot: None,
        opacity: 1.0,
        motion: Motion::default(),
    }
}

impl<'a> Portrait<'a> {
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// The ground behind it, which the presence dot's ring is cut from.
    /// A [`Ground`] or any colour the palette has no name for.
    pub fn ground(mut self, ring: impl Into<Ring>) -> Self {
        self.ground = ring.into();
        self
    }

    /// A presence dot at the bottom right; only a circle has one.
    pub fn presence(mut self, presence: Presence) -> Self {
        self.presence = presence;
        self
    }

    /// The face and its decoration at `opacity` of their strength (an
    /// offline member's, `ui/people.slint:294`); the dot keeps its own.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// The decoration around it.
    pub fn decoration(mut self, decoration: Option<&'a image::Handle>) -> Self {
        self.decoration = decoration;
        self
    }

    /// Whether an animated avatar and decoration play: Discord's client
    /// animates them while the pointer is on their row or message, so the
    /// screen hands a still motion otherwise.
    pub fn motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    /// The presence dot's slot: its side, ring included, and its corner's
    /// offset (x, y) from the face's, as the Slint caller places it. Without
    /// it, 16 px at (20, 20) on 32, in proportion at other sizes.
    pub fn dot(mut self, side: f32, at: (f32, f32)) -> Self {
        self.dot = Some((side, at));
        self
    }

    /// Ringed while they speak.
    pub fn speaking(mut self, speaking: bool) -> Self {
        self.speaking = speaking;
        self
    }

    /// The box is `size` square, with a dot or without (Slint's `Avatar`);
    /// the dot overflows it at the bottom right, and the decoration and the
    /// speaking ring beyond it all round.
    pub fn view<Message: 'a>(self) -> impl View<Message> + 'a {
        let Self {
            avatar,
            size,
            shape,
            ground,
            presence,
            decoration,
            speaking,
            dot,
            opacity,
            motion,
        } = self;
        let face = face(avatar, size, shape, opacity, motion);
        let dotted = presence != Presence::None && shape == Shape::Circle;
        if decoration.is_none() && !speaking && !dotted {
            return face;
        }
        let mut halo =
            halo::Halo::new(face, decoration.cloned(), speaking, motion).opacity(opacity);
        if dotted {
            let (outer, at) = dot.unwrap_or((size * DOT_SIDE, (size * DOT_AT, size * DOT_AT)));
            let dot = status_dot(presence, outer, ground).boxed();
            halo = halo.dot(dot, outer, at);
        }
        halo.boxed()
    }
}

/// The picture once it came, else the initial on the tint.
fn face<'a, Message: 'a>(
    avatar: &'a Avatar,
    size: f32,
    shape: Shape,
    opacity: f32,
    motion: Motion,
) -> Element<'a, Message> {
    let radius = shape.radius(size);
    match &avatar.picture {
        Some(handle) => {
            let face = picture(handle.clone())
                .side(size)
                .fit(Fit::Cover)
                .opacity(opacity)
                .motion(motion);
            // Round stays round at whatever size it is laid out at.
            if shape == Shape::Circle {
                face.round().boxed()
            } else {
                face.radius(radius).boxed()
            }
        }
        None => {
            let tint = avatar.tint;
            container(
                text(avatar.initial.as_str())
                    .size(size * INITIAL)
                    .font(typography::BOLD)
                    .color(Color::WHITE.scale_alpha(opacity)),
            )
            .center(size)
            .style(move |theme: &Theme| container::Style {
                // No tint set: the theme's own.
                background: Some(Background::Color(
                    if tint.a == 0.0 {
                        theme.palette().avatar
                    } else {
                        tint
                    }
                    .scale_alpha(opacity),
                )),
                border: Border::default().rounded(radius),
                ..container::Style::default()
            })
            .boxed()
        }
    }
}

/// The circle alone: the picture once it came, else the initial on the tint.
pub fn circle<'a, Message: 'a>(avatar: &'a Avatar, size: f32) -> impl View<Message> + 'a {
    face(avatar, size, Shape::Circle, 1.0, Motion::default())
}

/// The circle with a presence dot at its bottom right, ringed in the
/// ground's colour; no dot for [`Presence::None`].
pub fn avatar<'a, Message: 'a>(
    avatar: &'a Avatar,
    size: f32,
    presence: Presence,
    ground: Ground,
) -> impl View<Message> + 'a {
    portrait(avatar, size)
        .presence(presence)
        .ground(ground)
        .view()
}

/// An empty box as large as an avatar's, for a row whose picture isn't there.
pub fn gap<'a, Message: 'a>(size: f32) -> impl View<Message> + 'a {
    space()
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;
    use iced_test::Simulator;

    type Sim<'a> = Simulator<'a, (), Theme, anomi_theme::Renderer>;

    fn sim(view: impl View<()> + 'static) -> Sim<'static> {
        Simulator::with_size(iced::Settings::default(), Size::new(120.0, 120.0), view)
    }

    #[test]
    fn without_a_picture_the_initial_shows() {
        let who = Avatar {
            initial: "Q".into(),
            ..Avatar::default()
        };
        let who: &'static Avatar = Box::leak(Box::new(who));
        let mut ui = sim(avatar(who, 40.0, Presence::Online, Ground::Chat));
        assert!(ui.find("Q").is_ok());
    }

    #[test]
    fn a_dot_leaves_the_box_the_size_of_one_without() {
        let who: &'static Avatar = Box::leak(Box::new(Avatar::default()));
        let sized = |presence| {
            let view = avatar::<()>(who, 32.0, presence, Ground::Sidebar);
            let widget: &dyn iced::Widget<(), Theme, anomi_theme::Renderer> = &view;
            widget.size()
        };
        assert_eq!(sized(Presence::None), sized(Presence::Mobile));
    }

    #[test]
    fn the_face_is_laid_out_at_size_with_a_dot() {
        let who: &'static Avatar = Box::leak(Box::new(Avatar {
            initial: "Q".into(),
            ..Avatar::default()
        }));
        let centre = |presence| {
            let mut ui = sim(avatar(who, 32.0, presence, Ground::Chat));
            let at = ui.find("Q").expect("the initial").bounds().center();
            (at.x, at.y)
        };
        let (plain, dotted) = (centre(Presence::None), centre(Presence::Online));
        assert!((plain.0 - dotted.0).abs() < 0.01 && (plain.1 - dotted.1).abs() < 0.01);
        assert!((plain.0 - 16.0).abs() < 1.0, "{plain:?}");
    }

    #[test]
    fn corners_name_each_radius() {
        let radius = Shape::Corners([8.0, 2.0, 2.0, 2.0]).radius(40.0);
        assert_eq!((radius.top_left, radius.bottom_right), (8.0, 2.0));
        assert_eq!(Shape::Circle.radius(40.0).top_left, 20.0);
    }
}
