//! The window's right pane: how the style looks on a profile card, a message
//! and a member row, each overlapping the one before as Discord lays them
//! out, with the dark and light switch (`ui/server.slint:1407`-`:1470`).

use iced::font::Weight;
use iced::widget::{button, column, container, pin, row, stack, svg, text};
use iced::{
    Alignment, Background, Border, Color, Degrees, Fill, Padding, Shadow, Vector, gradient,
};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::people::name_style::{Intent, NameStyle};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::icon::icon;
use anomi_widgets::name::{Ink, styled_name};
use anomi_widgets::presence::Ring;
use yapfast_lang::tr;

use crate::profile::badges::one;

const LIGHT_FILL: Color = Color::WHITE;
const LIGHT_INK: Color = Color::from_rgb(0.192, 0.2, 0.22);

fn shadow(blur: f32, drop: f32, alpha: f32) -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, alpha),
        offset: Vector::new(0.0, drop),
        blur_radius: blur,
    }
}

/// A sample's box, light or dark.
fn sample<'a>(
    content: Element<'a, Intent>,
    light: bool,
    size: (f32, f32),
    radius: f32,
    (blur, drop): (f32, f32),
) -> Element<'a, Intent> {
    container(content)
        .width(size.0)
        .height(size.1)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if light {
                    LIGHT_FILL
                } else {
                    palette.raised
                })),
                border: Border {
                    color: if light {
                        Color::TRANSPARENT
                    } else {
                        palette.edge
                    },
                    width: 1.0,
                    radius: radius.into(),
                },
                shadow: shadow(blur, drop, 0.33),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The profile card as the window draws it, 305 by 216: no banner, the
/// picture with its dot, the name in the style chosen, the handle and up
/// to three badges, on the theme's two washes (`ui/server.slint:1434`).
fn profile(window: &NameStyle, motion: Motion) -> Element<'_, Intent> {
    let card = &window.card;
    let theme = card.theme.as_ref();
    let ink = theme.map(|theme| theme.ink());
    let (top, bottom) = (theme.map(|t| t.start), theme.map(|t| t.end));
    // The dot fills its gap ring at 87% of the radius out toward the lower
    // right (0.61r, 0.62r), 0.291 of the picture across.
    let dot = 80.0 * 0.291;
    let picture = portrait(&card.avatar, 80.0)
        .presence(card.presence)
        .ground(top.map_or(Ring::Ground(Ground::Raised), Ring::Color))
        .dot(dot, (40.0 + 24.4 - dot / 2.0, 40.0 + 24.8 - dot / 2.0))
        .motion(motion)
        .view();
    let name = styled_name(&card.name, card.name_style)
        .size(21.0)
        .weight(Weight::Bold)
        .ink(ink.map_or(Ink::Token(|p| p.text), Ink::Fixed));
    let mut line = row![
        text(&card.handle)
            .size(13)
            .color(ink.unwrap_or(Color::WHITE))
    ]
    .spacing(6.0)
    .height(26.0)
    .align_y(Alignment::Center);
    for badge in card.badges.badges.iter().take(3) {
        let badge = Element::map(one::view(badge, motion).boxed(), |_| Intent::Ignored);
        line = line.push(container(badge).width(22.0).boxed());
    }
    let layers = stack![
        pin(picture).x(16.0).y(56.0),
        pin(name).x(18.0).y(145.0),
        pin(line).x(18.0).y(176.0),
    ];
    container(layers)
        .width(305.0)
        .height(216.0)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Gradient(
                    gradient::Linear::new(Degrees(180.0))
                        .add_stop(0.0, top.unwrap_or(palette.raised))
                        .add_stop(1.0, bottom.unwrap_or(palette.raised))
                        .into(),
                )),
                border: Border {
                    color: palette.edge,
                    width: 1.0,
                    radius: 14.0.into(),
                },
                shadow: shadow(24.0, 8.0, 0.4),
                ..container::Style::default()
            }
        })
        .boxed()
}

fn message(window: &NameStyle) -> Element<'_, Intent> {
    let card = &window.card;
    let light = window.light;
    // A message's name stays plain here, as Discord's preview has it: its
    // face and nothing else.
    let name = styled_name(&card.name, card.name_style.map(|style| style.font_only()))
        .size(17.0)
        .weight(Weight::Semibold)
        .ink(Ink::Token(|p| p.text));
    let name = if light {
        name.ink(Ink::Fixed(LIGHT_INK))
    } else {
        name
    };
    let body = text("does anyone read this?").size(15);
    let body = if light {
        body.color(LIGHT_INK)
    } else {
        body.style(catalog::text::primary)
    };
    let content = row![
        portrait(&card.avatar, 40.0).view(),
        column![name, body].spacing(5.0)
    ]
    .spacing(12.0)
    .padding(Padding {
        top: 22.0,
        left: 20.0,
        ..Padding::ZERO
    });
    sample(content.boxed(), light, (320.0, 85.0), 12.0, (18.0, 6.0))
}

fn member(window: &NameStyle) -> Element<'_, Intent> {
    let card = &window.card;
    let name = styled_name(&card.name, card.name_style)
        .size(17.0)
        .weight(Weight::Semibold);
    let content = row![portrait(&card.avatar, 32.0).view(), name]
        .spacing(10.0)
        .align_y(Alignment::Center)
        .padding(Padding::from([9.0, 10.0]));
    sample(
        content.boxed(),
        window.light,
        (282.0, 50.0),
        10.0,
        (14.0, 4.0),
    )
}

fn mode<'a>(glyph: Icon, light: bool, window: &NameStyle) -> Element<'a, Intent> {
    let chosen = window.light == light;
    let tint = Color::from_rgba(1.0, 1.0, 1.0, if chosen { 1.0 } else { 0.65 });
    let glyph = icon(glyph, 16.0).style(move |_: &Theme, _| svg::Style { color: Some(tint) });
    button(container(glyph).center(Fill))
        .width(32.0)
        .height(32.0)
        .padding(0)
        .on_press(Intent::Light(light))
        .style(move |theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let alpha = if chosen {
                0.15
            } else if hovered {
                0.08
            } else {
                0.0
            };
            button::Style {
                background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, alpha))),
                border: Border::default().rounded(9.0),
                ..catalog::button::bare(theme, status)
            }
        })
        .boxed()
}

fn switch(window: &NameStyle) -> Element<'_, Intent> {
    container(
        row![
            mode(Icon::Moon, false, window),
            mode(Icon::Sun, true, window)
        ]
        .spacing(4.0),
    )
    .padding(4.0)
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
        border: Border::default().rounded(12.0),
        ..container::Style::default()
    })
    .boxed()
}

pub fn view<'a>(window: &'a NameStyle, motion: Motion) -> Element<'a, Intent> {
    let foot = row![
        text(tr!(
            "Colours and effects don't show in servers, and may look different across Discord."
        ))
        .size(typography::size::CAPTION)
        .style(catalog::text::muted)
        .width(Fill),
        switch(window),
    ]
    .spacing(20.0)
    .align_y(Alignment::End);
    let layers = stack![
        pin(profile(window, motion)).x(40.0).y(132.0),
        pin(message(window)).x(65.0).y(338.0),
        pin(member(window)).x(40.0).y(418.0),
        // The note 28 px in, the switch 24 px from the corner.
        container(foot).align_bottom(Fill).padding(Padding {
            left: 28.0,
            ..Padding::new(24.0)
        }),
    ];
    container(layers)
        .width(Fill)
        .height(Fill)
        .clip(true)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            let (a, s) = (palette.accent, palette.sunken);
            let wash = Color::from_rgb(
                s.r + (a.r - s.r) * 0.05,
                s.g + (a.g - s.g) * 0.05,
                s.b + (a.b - s.b) * 0.05,
            );
            container::Style {
                background: Some(Background::Gradient(
                    gradient::Linear::new(Degrees(160.0))
                        .add_stop(0.0, palette.sunken)
                        .add_stop(1.0, wash)
                        .into(),
                )),
                ..container::Style::default()
            }
        })
        .boxed()
}
