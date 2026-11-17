//! The editor's card head: the banner, the avatar hanging over its lower
//! edge in a ring of the card's ground, and the status thought beside it
//! (`ui/server.slint:1790`-`:1900`). The banner and the avatar offer their
//! change under the pointer, with a pencil or a lock.

use iced::widget::{Column, button, container, hover, mouse_area, pin, space, stack, text};
use iced::{Alignment, Background, Border, Color, Fill, Length, Padding, mouse};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, rgba, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_vm::people::editor::{Editor, Intent, Picture};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::icon_button;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};
use anomi_widgets::presence::status_text;
use yapfast_lang::tr;

use super::card::Layout;
use super::edit_badge;
use crate::profile::card::look::{Inks, Tone};

pub const AVATAR: f32 = 120.0;
const RING: f32 = 6.0;

/// The banner and what hangs over it, `layout.head()` tall.
pub fn view<'a>(
    editor: &'a Editor,
    layout: Layout,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, Intent> {
    let base = Column::<Element<'a, Intent>>::new()
        .push(banner(editor, layout, motion))
        .push(space().height(layout.head() - layout.banner).boxed());
    let mut layers = stack![base].width(Fill).height(layout.head());
    if !editor.main {
        layers = layers.push(corner(editor, motion));
    }
    layers = layers.push(
        pin(avatar(editor, inks, motion))
            .x(layout.pad - RING)
            .y(layout.banner - AVATAR / 2.0 - RING)
            .boxed(),
    );
    if editor.main {
        layers = layers.push(thought(editor, layout, inks, motion));
    }
    layers.boxed()
}

/// The banner: its picture, else its colour, else the theme's first, else
/// raised; darkened with a badge on it under the pointer.
fn banner(editor: &Editor, layout: Layout, motion: Motion) -> Element<'_, Intent> {
    let face = |lit: bool| banner_face(editor, layout, motion, lit);
    let touchable = editor.banner_editable() || editor.banner_locked();
    if !touchable {
        return face(false);
    }
    let both = hover(face(false), face(true)).boxed();
    if !editor.banner_editable() {
        return both;
    }
    if editor.may.colour {
        return pressing(both, Intent::ColourBox(!editor.colour_open));
    }
    if editor.banner_reset {
        return opener(button(both).padding(0).style(catalog::button::bare))
            .on_press(|at| Intent::PictureMenu(Picture::Banner, Spot::below(at)))
            .boxed();
    }
    pressing(both, Intent::Pick(Picture::Banner))
}

/// The banner as it lies, `lit` while the pointer is on it.
fn banner_face<'a>(
    editor: &'a Editor,
    layout: Layout,
    motion: Motion,
    lit: bool,
) -> Element<'a, Intent> {
    let card = &editor.card;
    let corner = iced::border::top(layout.radius);
    let fill = card
        .accent
        .or(card.theme.map(|theme| theme.primary))
        .map_or(Tone::Token(|palette| palette.raised), Tone::Fixed);
    let ground: Element<'a, Intent> = match &card.banner {
        Expected::Ready(handle) => picture(handle.clone())
            .width(Fill)
            .height(layout.banner)
            .fit(Fit::Cover)
            .radius(corner)
            .motion(motion)
            .boxed(),
        _ => container(space())
            .width(Fill)
            .height(layout.banner)
            .style(move |theme: &Theme| container::Style {
                background: Some(Background::Color(fill.at(theme.palette()))),
                border: Border {
                    radius: corner,
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .boxed(),
    };
    let mut layers = stack![ground].width(Fill).height(layout.banner);
    if lit {
        layers = layers.push(
            container(space())
                .width(Fill)
                .height(Fill)
                .style(move |_: &Theme| container::Style {
                    background: Some(Background::Color(rgba(0x00000059))),
                    border: Border {
                        radius: corner,
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .boxed(),
        );
        layers = layers.push(
            container(edit_badge(editor.banner_locked()))
                .width(Fill)
                .height(Fill)
                .align_right(Fill)
                .padding(Padding {
                    top: 18.0,
                    right: 60.0,
                    ..Padding::ZERO
                })
                .boxed(),
        );
    }
    if editor.reading == Some(Picture::Banner) {
        layers = layers.push(
            container(
                text(tr!("Reading..."))
                    .size(14)
                    .font(typography::BOLD)
                    .color(Color::WHITE),
            )
            .center(Fill)
            .boxed(),
        );
    }
    layers.boxed()
}

fn pressing<'a>(content: Element<'a, Intent>, press: Intent) -> Element<'a, Intent> {
    button(content)
        .padding(0)
        .style(catalog::button::bare)
        .on_press(press)
        .boxed()
}

/// What it edits, over the banner's corner, and its close button: a server
/// profile's, which has no column to say it.
fn corner(editor: &Editor, motion: Motion) -> Element<'_, Intent> {
    let pill = container(
        text(editor.detail.as_str())
            .size(13)
            .font(typography::SEMIBOLD)
            .color(Color::WHITE),
    )
    .padding(Padding::from([5.0, 10.0]))
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(rgba(0x000000b3))),
        border: Border::default().rounded(14.0),
        ..container::Style::default()
    });
    let close = icon_button(Icon::X, tr!("Close"), motion)
        .size(36.0)
        .glyph(18.0)
        .radius(18.0)
        .fill(|_| rgba(0x000000b3))
        .tint(|_| Color::WHITE)
        .on_press(Intent::Close)
        .view();
    container(iced::widget::row![pill, space().width(Fill), close].align_y(Alignment::Start))
        .padding(16.0)
        .width(Fill)
        .boxed()
}

/// The avatar in a ring of the card's ground, its presence dot cut from the
/// same, with its change offered under the pointer.
fn avatar<'a>(editor: &'a Editor, inks: &Inks, motion: Motion) -> Element<'a, Intent> {
    let card = &editor.card;
    let dot = AVATAR * 0.291;
    let at = (
        AVATAR / 2.0 + AVATAR * 0.305 - dot / 2.0,
        AVATAR / 2.0 + AVATAR * 0.31 - dot / 2.0,
    );
    let ground = inks.ground;
    let side = AVATAR + 2.0 * RING;
    let face = || -> Element<'a, Intent> {
        let mut face = portrait(&card.avatar, AVATAR)
            .ground(inks.ring)
            .decoration(card.decoration.as_ref())
            .motion(motion);
        if editor.main && card.presence != anomi_vm::kind::Presence::None {
            face = face.presence(card.presence).dot(dot, at);
        }
        container(face.view())
            .center(side)
            .style(move |theme: &Theme| container::Style {
                background: Some(Background::Color(ground.at(theme.palette()))),
                border: Border::default().rounded(side / 2.0),
                ..container::Style::default()
            })
            .boxed()
    };
    let veil = |lit: bool| -> Element<'a, Intent> {
        let mut layers = stack![face()];
        if lit {
            layers = layers.push(
                container(edit_badge(!editor.may.avatar))
                    .center(side)
                    .style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(rgba(0x00000073))),
                        border: Border::default().rounded(side / 2.0),
                        ..container::Style::default()
                    })
                    .boxed(),
            );
        }
        layers.boxed()
    };
    let reading = editor.reading == Some(Picture::Avatar);
    if reading {
        return stack![
            veil(false),
            container(
                text(tr!("Reading..."))
                    .size(13)
                    .font(typography::BOLD)
                    .color(Color::WHITE)
            )
            .center(side)
            .boxed()
        ]
        .boxed();
    }
    if !editor.ready() {
        return veil(false);
    }
    let both = hover(veil(false), veil(true)).boxed();
    if !editor.avatar_editable() {
        return mouse_area(both).boxed();
    }
    if editor.avatar_reset {
        return opener(button(both).padding(0).style(catalog::button::bare))
            .on_press(|at| Intent::PictureMenu(Picture::Avatar, Spot::below(at)))
            .boxed();
    }
    mouse_area(pressing(both, Intent::Pick(Picture::Avatar)))
        .interaction(mouse::Interaction::Pointer)
        .boxed()
}

/// What you are up to, a thought beside the picture: two dots rising toward
/// the avatar and the bubble under the banner's edge.
fn thought<'a>(
    editor: &'a Editor,
    layout: Layout,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, Intent> {
    let (fill, edge) = (inks.bubble, inks.bubble_edge);
    let round = move |side: f32| {
        container(space().width(side).height(side)).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(fill.at(palette))),
                border: Border::default()
                    .rounded(side / 2.0)
                    .width(1.0)
                    .color(edge.at(palette)),
                ..container::Style::default()
            }
        })
    };
    let beside = layout.pad - RING + AVATAR;
    let words: Element<'a, Intent> = match editor.card.thought() {
        Some(status) => {
            let words = status_text(status, motion)
                .size(14.0)
                .wrap(true)
                .max_lines(2);
            match inks.ink {
                Tone::Fixed(ink) => words.ink(ink),
                Tone::Token(token) => words.color(token),
            }
            .view()
            .boxed()
        }
        None => {
            let ink2 = inks.ink2;
            text(tr!("Set a custom status"))
                .size(14)
                .style(move |theme: &Theme| text::Style {
                    color: Some(ink2.at(theme.palette())),
                    ..text::Style::default()
                })
                .boxed()
        }
    };
    let long = editor
        .card
        .thought()
        .is_some_and(|status| status.text.chars().count() > 20);
    let body = container(words)
        .width(if long {
            Length::Fixed(180.0)
        } else {
            Length::Shrink
        })
        .height(if long { 54.0 } else { 36.0 })
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, 13.0]))
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(fill.at(palette))),
                border: Border::default()
                    .rounded(16.0)
                    .width(1.0)
                    .color(edge.at(palette)),
                ..container::Style::default()
            }
        });
    let body = button(body)
        .padding(0)
        .style(catalog::button::bare)
        .on_press(Intent::EditStatus);
    stack![
        space().width(Fill).height(Fill),
        pin(round(10.0)).x(beside + 14.0).y(layout.banner + 4.0),
        pin(round(20.0)).x(beside + 28.0).y(layout.banner + 11.0),
        pin(body).x(beside + 18.0).y(layout.banner + 17.0),
    ]
    .width(Fill)
    .height(layout.head())
    .boxed()
}
