//! The profile card (`ProfileCard`, `ui/people.slint:1326`): a banner with
//! the avatar over its edge and their status in a bubble beside it, then
//! the name, badges, activities, what they share with this account, the
//! bio, dates, roles, and the box that messages them; drawn on the card's
//! ground or its profile theme's gradient, with its profile effect over it
//! and its frame around it.
//!
//! It shows in five places: the popout over the window ([`view`]), from a
//! member row, a name or one's own account (with Edit Profile); docked
//! beside a one-to-one DM ([`panel`]); the full profile's left column and
//! the profile editor's preview, which draw their own [`Card`] through
//! [`card`] in their [`Place`]. The parts in [`look`] (sizes, the theme's
//! colours), [`head`], [`overlay`] (bubble, corner buttons), [`name`],
//! [`sections`] and [`extra`]; [`parts`] holds what other cards share.

mod extra;
mod head;
pub mod look;
mod name;
mod overlay;
pub mod parts;
mod sections;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{column, container, mouse_area, space, stack};
use iced::{Background, Border, Fill, Length, Padding, Shadow, Vector, gradient};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::people::profile::{Card, Intent, Place, Profile};
use anomi_vm::people::{self};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::effect::{Play, effect};
use anomi_widgets::frame::{self, frame};
use anomi_widgets::list::scroll_view;
use yapfast_lang::tr;

use look::{Inks, POPOUT, Size};

/// The popout's shadow, which the full profile's modal and the panel cast
/// for theirs (`:1456`).
const SHADOW: Shadow = Shadow {
    color: rgba(0x00000080),
    offset: Vector::new(0.0, 8.0),
    blur_radius: 24.0,
};
/// The ids of one's own card's Edit Profile and the panel's View Full
/// Profile, which take the keys.
pub const EDIT: &str = "profile-card-edit";
pub const FULL: &str = "profile-card-full";
/// The theme's border, its two colours at full strength.
const THEME_BORDER: f32 = 2.0;
/// The panel's foot, where its button sits 12 px from the bottom.
const PANEL_FOOT: f32 = 52.0;

/// The popout over the window (`Layer::Profile`).
pub fn view(profile: &Profile, motion: Motion) -> impl View<people::Intent> + '_ {
    card(&profile.card, Place::Popout, motion)
}

/// The card docked beside a one-to-one DM, when it has one.
pub fn panel(profile: &Profile, motion: Motion) -> Element<'_, people::Intent> {
    match &profile.panel {
        Some(shown) => card(shown, Place::Panel, motion),
        None => space().boxed(),
    }
}

/// `card` in `place`: 300 px wide over the window and in a preview, the
/// width and height it's given docked and on the full profile, where it
/// scrolls.
pub fn card(card: &Card, place: Place, motion: Motion) -> Element<'_, people::Intent> {
    let size = Size::of(place);
    let inks = Inks::new(card, place);
    let inset = if inks.theme.is_some() {
        THEME_BORDER
    } else {
        0.0
    };
    let body = column![
        head::view(card, place, size, &inks, motion),
        sections::view(card, place, size, &inks, motion)
    ]
    .padding(Padding {
        bottom: size.pad,
        ..Padding::ZERO
    });
    // Docked and on the full profile it fills its place; the popout is as
    // tall as it needs, up to the window's room, then scrolls (`limit`).
    let body: Element<'_, people::Intent> = match place {
        Place::Panel | Place::Side => scroll_view(body, motion).boxed(),
        Place::Popout => scroll_view(body, motion).height(Length::Shrink).boxed(),
        Place::Preview => body.boxed(),
    };
    let body = container(body).padding(Padding {
        top: inset,
        left: inset,
        right: inset,
        bottom: match place {
            Place::Panel => PANEL_FOOT,
            Place::Side => 0.0,
            _ => inset,
        },
    });
    let mut layers = stack![body];
    if let Some(status) = card.thought() {
        layers = layers.push(overlay::bubble(card, status, place, size, &inks, motion));
    }
    if matches!(place, Place::Popout | Place::Panel)
        && let Some(corner) = overlay::corner(card)
    {
        layers = layers.push(corner);
    }
    if let Some(shown) = card
        .effect
        .as_ref()
        .filter(|shown| !shown.layers.is_empty() || shown.still.is_some())
    {
        layers = layers.push(
            effect(shown, motion)
                .radius(size.radius)
                .play(Play::Hover)
                .on_wanted(Intent::EffectWanted(place).into())
                .boxed(),
        );
    }
    if place == Place::Panel {
        layers = layers.push(
            container(
                action_button(tr!("View Full Profile"), Kind::Secondary, motion)
                    .id(iced::widget::Id::new(FULL))
                    .width(Fill)
                    .on_press(Intent::Enlarge(place).into()),
            )
            .width(Fill)
            .height(Fill)
            .align_bottom(Fill)
            .padding(Padding {
                left: 16.0,
                right: 16.0,
                bottom: 12.0,
                top: 0.0,
            })
            .boxed(),
        );
    }
    // A theme's wash lies over the frame's back, its border under it.
    if let Some(theme) = inks.theme {
        layers = layers.push_under(wash(theme, place, size));
    }
    // No frame docked or in a preview (`has-frame`, `:1479`).
    let framed = !card.frame.is_empty() && matches!(place, Place::Popout | Place::Side);
    if framed {
        layers = layers
            .push(
                frame(&card.frame, frame::Side::Front)
                    .large(place.large())
                    .boxed(),
            )
            .push_under(
                frame(&card.frame, frame::Side::Back)
                    .large(place.large())
                    .boxed(),
            );
    }
    let layers = layers.push_under(ground(place, size, &inks));
    let (width, height) = match place {
        Place::Popout => (Length::Fixed(POPOUT), Length::Shrink),
        Place::Preview => (Length::Fill, Length::Shrink),
        Place::Panel | Place::Side => (Length::Fill, Length::Fill),
    };
    mouse_area(layers.width(width).height(height))
        .on_enter(Intent::Pointed(place, true).into())
        .on_exit(Intent::Pointed(place, false).into())
        .boxed()
}

/// The card's ground: its place's colour in a 1 px edge, or a theme's two
/// colours at full strength, which its wash covers but for a 2 px border (`:1451`-`:1457`, `:1500`-`:1502`).
fn ground<'a>(place: Place, size: Size, inks: &Inks) -> Element<'a, people::Intent> {
    let radius = |top: f32, foot: f32| iced::border::Radius {
        top_left: top,
        top_right: top,
        bottom_right: foot,
        bottom_left: foot,
    };
    let shadow = if matches!(place, Place::Popout | Place::Preview) {
        SHADOW
    } else {
        Shadow::default()
    };
    let corners = radius(size.radius, size.foot);
    let Some(theme) = inks.theme else {
        let fill = inks.ground;
        return container(space())
            .width(Fill)
            .height(Fill)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(Background::Color(fill.at(palette))),
                    border: Border {
                        radius: corners,
                        width: 1.0,
                        color: palette.edge,
                    },
                    shadow,
                    ..container::Style::default()
                }
            })
            .boxed();
    };
    container(space())
        .width(Fill)
        .height(Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Gradient(
                gradient::Linear::new(iced::Degrees(180.0))
                    .add_stop(0.0, theme.primary)
                    .add_stop(1.0, theme.secondary)
                    .into(),
            )),
            border: Border {
                radius: corners,
                ..Border::default()
            },
            shadow,
            ..container::Style::default()
        })
        .boxed()
}

/// A theme's wash inside its 2 px border: flat down to the banner's end,
/// then fading to its second colour (`:1500`-`:1502`).
fn wash<'a>(
    theme: people::profile::Theme,
    place: Place,
    size: Size,
) -> Element<'a, people::Intent> {
    let radius = |top: f32, foot: f32| iced::border::Radius {
        top_left: top,
        top_right: top,
        bottom_right: foot,
        bottom_left: foot,
    };
    let (top, foot) = (
        size.radius - THEME_BORDER,
        (size.foot - THEME_BORDER).max(0.0),
    );
    let flat = container(space())
        .width(Fill)
        .height(size.banner)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(theme.start)),
            border: Border {
                radius: radius(top, 0.0),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let fading = container(space())
        .width(Fill)
        .height(Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Gradient(
                gradient::Linear::new(iced::Degrees(180.0))
                    .add_stop(0.0, theme.start)
                    .add_stop(1.0, theme.end)
                    .into(),
            )),
            border: Border {
                radius: radius(0.0, foot),
                ..Border::default()
            },
            ..container::Style::default()
        });
    container(column![flat, fading])
        .width(Fill)
        .height(Fill)
        .padding(Padding {
            top: THEME_BORDER,
            left: THEME_BORDER,
            right: THEME_BORDER,
            bottom: if place.large() { 0.0 } else { THEME_BORDER },
        })
        .boxed()
}

/// Its shots for `anomi-shots`, each the whole window from a fixture.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    shots::shots()
}
