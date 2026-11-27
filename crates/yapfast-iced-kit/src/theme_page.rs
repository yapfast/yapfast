//! The pieces Settings → Themes and the theme creator are built of, beside
//! the form kit's: the page's underlined tabs, a theme's tag, the cards or
//! list toggle, a notice card that says what happened and what to do, a step
//! of what happens next, a card's outline while the catalog loads, and the
//! creator's swatches, picture tiles and hue bar. Every colour is the
//! theme's; see docs/client/kit.md.

use iced::{
    Alignment, Background, Border, Color, Element, Gradient, Length, Radians,
    gradient::Linear,
    widget::{Space, button, column, container, row, slider, stack, text},
};

use crate::{
    controls::lights,
    fonts::{BOLD, MEDIUM, REGULAR, SEMIBOLD},
    icons::Icon,
    look::Look,
    style::{faded, rounded},
    theme::Theme,
};

/// The tab strip's height, its tabs this far apart.
pub const TABS: f32 = 40.0;
const TAB_GAP: f32 = 24.0;

/// A page's tabs, as the role editor's: each name 15 px semibold over a
/// 2 px accent line while chosen, the strip on a 1 px rule.
pub fn tabs<'a, M: Clone + 'a>(
    theme: &Theme,
    tabs: impl IntoIterator<Item = (&'a str, bool, M)>,
) -> Element<'a, M> {
    tabs_focused(theme, tabs, None)
}

/// A focus ring round `content`, over it, as the keys' place is shown: it
/// takes no room, so nothing moves as the keys go from one to the next.
pub fn focus_ring<'a, M: 'a>(theme: &Theme, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let accent = theme.accent;
    stack![
        content.into(),
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: accent,
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
    ]
    .into()
}

/// [`tabs`], the one at `focused` ringed while the keys are on it.
pub fn tabs_focused<'a, M: Clone + 'a>(
    theme: &Theme,
    tabs: impl IntoIterator<Item = (&'a str, bool, M)>,
    focused: Option<usize>,
) -> Element<'a, M> {
    let mut strip = row![].spacing(TAB_GAP).height(TABS);
    for (index, (label, chosen, press)) in tabs.into_iter().enumerate() {
        let (text_ink, rest, accent) = (theme.text, theme.secondary, theme.accent);
        let words = text(label)
            .size(15)
            .font(SEMIBOLD)
            .wrapping(text::Wrapping::None);
        let underline = container(Space::new())
            .width(Length::Fill)
            .height(2)
            .style(move |_| {
                if chosen {
                    rounded(accent, 1)
                } else {
                    container::Style::default()
                }
            });
        let tab = button(
            column![
                container(words)
                    .height(TABS - 2.0)
                    .align_y(Alignment::Center),
                underline
            ]
            .width(Length::Shrink),
        )
        .padding(0)
        .on_press(press)
        .style(move |_, status| button::Style {
            text_color: if chosen || lights(status) {
                text_ink
            } else {
                rest
            },
            ..Default::default()
        });
        strip = strip.push(if focused == Some(index) {
            focus_ring(theme, tab)
        } else {
            tab.into()
        });
    }
    let line = theme.line;
    stack![
        column![
            Space::new().height(TABS - 1.0),
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(move |_| crate::style::fill(line)),
        ],
        strip
    ]
    .width(Length::Fill)
    .height(TABS)
    .into()
}

/// A word about a theme, 20 px tall: what it works in, where it's from.
/// `solid` fills it with its ink for the one that matters most (Applied).
pub fn tag<'a, M: Clone + 'a>(
    look: Look<'_>,
    words: impl text::IntoFragment<'a>,
    ink: Color,
    solid: bool,
    icon: Option<Icon>,
) -> Element<'a, M> {
    let theme = look.theme;
    let fore = if solid { theme.on_accent } else { ink };
    let mut line = row![].spacing(4).align_y(Alignment::Center);
    if let Some(icon) = icon {
        line = line.push(look.glyph(icon, 12, fore));
    }
    line = line.push(
        text(words)
            .size(12)
            .font(SEMIBOLD)
            .color(fore)
            .wrapping(text::Wrapping::None),
    );
    let ground = if solid { ink } else { faded(ink, 0.14) };
    container(line)
        .padding([0, 6])
        .height(20)
        .align_y(Alignment::Center)
        .style(move |_| rounded(ground, 4))
        .into()
}

/// Cards or a list: two small cells on the sunken ground, each drawn as
/// what it shows, the chosen one raised.
pub fn view_toggle<'a, M: Clone + 'a>(
    theme: &Theme,
    cards: bool,
    as_list: M,
    as_cards: M,
) -> Element<'a, M> {
    let cell = |index: usize, press: M| -> Element<'a, M> {
        let chosen = (index == 1) == cards;
        let (text_ink, muted, active) = (theme.text, theme.muted, theme.active);
        let glyph = move |ink: Color| -> Element<'a, M> {
            let piece = move |w: f32, h: f32| {
                container(Space::new())
                    .width(w)
                    .height(h)
                    .style(move |_| rounded(ink, 1))
            };
            if index == 0 {
                column![piece(14.0, 2.0), piece(14.0, 2.0), piece(14.0, 2.0)]
                    .spacing(3)
                    .into()
            } else {
                column![
                    row![piece(5.0, 5.0), piece(5.0, 5.0)].spacing(2),
                    row![piece(5.0, 5.0), piece(5.0, 5.0)].spacing(2)
                ]
                .spacing(2)
                .into()
            }
        };
        let rest = if chosen { text_ink } else { muted };
        button(container(glyph(rest)).center(Length::Fill))
            .width(28)
            .height(24)
            .padding(0)
            .on_press(press)
            .style(move |_, status| button::Style {
                background: if chosen {
                    Some(active.into())
                } else if lights(status) {
                    Some(faded(active, 0.5).into())
                } else {
                    None
                },
                border: Border::default().rounded(4),
                ..Default::default()
            })
            .into()
    };
    let sunken = theme.sunken;
    container(row![cell(0, as_list), cell(1, as_cards)].spacing(0))
        .padding(2)
        .width(60)
        .height(28)
        .style(move |_| rounded(sunken, 6))
        .into()
}

/// A tab with nothing to show, or something to say: an icon on a tinted
/// disc, what happened and what to do, and its buttons, on the sidebar's
/// ground.
pub fn notice<'a, M: Clone + 'a>(
    look: Look<'_>,
    icon: Icon,
    tint: Color,
    title: impl text::IntoFragment<'a>,
    words: Option<String>,
    buttons: Vec<Element<'a, M>>,
) -> Element<'a, M> {
    let theme = look.theme;
    let disc_ground = faded(tint, 0.14);
    let disc = container(look.glyph(icon, 20, tint))
        .center(40)
        .style(move |_| rounded(disc_ground, 20));
    let mut body = column![text(title).size(16).font(SEMIBOLD).color(theme.text)]
        .spacing(4)
        .width(Length::Fill);
    if let Some(words) = words {
        body = body.push(
            text(words)
                .size(14)
                .font(theme.font("body", REGULAR))
                .color(theme.muted),
        );
    }
    if !buttons.is_empty() {
        body = body.push(
            container(row(buttons).spacing(8).align_y(Alignment::Center)).padding(iced::Padding {
                top: 8.0,
                ..iced::Padding::ZERO
            }),
        );
    }
    let ground = theme.sidebar;
    container(row![disc, body].spacing(16).align_y(Alignment::Start))
        .padding(16)
        .width(Length::Fill)
        .style(move |_| rounded(ground, 8))
        .into()
}

/// A step of what happens next: its icon on a disc, what it is and a word
/// on it.
pub fn step<'a, M: Clone + 'a>(
    look: Look<'_>,
    icon: Icon,
    title: impl text::IntoFragment<'a>,
    words: impl text::IntoFragment<'a>,
) -> Element<'a, M> {
    let theme = look.theme;
    let ground = theme.sidebar;
    row![
        container(look.glyph(icon, 16, theme.secondary))
            .center(32)
            .style(move |_| rounded(ground, 16)),
        column![
            text(title)
                .size(15)
                .font(theme.font("body", REGULAR))
                .color(theme.text),
            text(words)
                .size(13)
                .font(theme.font("body", REGULAR))
                .color(theme.muted),
        ]
        .spacing(2)
        .width(Length::Fill)
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

/// A card's outline while what it shows loads: a picture's place and the
/// lines of its words, `height` tall.
pub fn skeleton<'a, M: Clone + 'a>(theme: &Theme, height: f32) -> Element<'a, M> {
    let (block, ground) = (theme.wash(0.07), theme.wash(0.035));
    let bar = move |width: Length, h: f32| {
        container(Space::new())
            .width(width)
            .height(h)
            .style(move |_| rounded(block, 4))
    };
    container(
        column![
            container(Space::new())
                .width(Length::Fill)
                .height(Length::FillPortion(3))
                .style(move |_| rounded(block, crate::style::corners([8.0, 8.0, 0.0, 0.0]))),
            container(
                column![
                    bar(Length::FillPortion(1), 14.0),
                    bar(Length::Fixed(120.0), 10.0),
                    bar(Length::Fill, 10.0),
                    bar(Length::Fixed(160.0), 10.0),
                ]
                .spacing(8)
            )
            .padding(12)
            .height(Length::FillPortion(2)),
        ]
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(height)
    .style(move |_| rounded(ground, 8))
    .into()
}

/// A round swatch of a colour, 32 px; the chosen one ringed in the text's
/// colour.
pub fn swatch<'a, M: Clone + 'a>(
    theme: &Theme,
    colour: Color,
    chosen: bool,
    press: M,
) -> Element<'a, M> {
    let (ring, faint) = (theme.text, theme.faint);
    button(
        container(
            container(Space::new())
                .width(22)
                .height(22)
                .style(move |_| rounded(colour, 11)),
        )
        .center(Length::Fill),
    )
    .width(32)
    .height(32)
    .padding(0)
    .on_press(press)
    .style(move |_, status| button::Style {
        border: Border {
            color: if chosen {
                ring
            } else if lights(status) {
                faint
            } else {
                Color::TRANSPARENT
            },
            width: 2.0,
            radius: 16.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// A choice drawn as a picture of itself (`art`, `art_height` tall), its
/// name under it; the chosen one has an accent ring and a tick.
pub fn tile<'a, M: Clone + 'a>(
    look: Look<'_>,
    label: impl text::IntoFragment<'a>,
    chosen: bool,
    art: Element<'a, M>,
    art_height: f32,
    press: M,
) -> Element<'a, M> {
    let theme = look.theme;
    let mut name = row![
        text(label)
            .size(13)
            .font(if chosen { BOLD } else { MEDIUM })
            .color(if chosen { theme.text } else { theme.secondary })
            .wrapping(text::Wrapping::None)
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if chosen {
        name = name.push(look.glyph(Icon::Check, 14, theme.accent_text()));
    }
    let (accent, faint, line, hover) = (theme.accent, theme.faint, theme.line, theme.hover);
    button(
        column![
            container(art)
                .width(Length::Fill)
                .height(art_height)
                .clip(true)
                .style(|_| container::Style {
                    border: Border::default().rounded(4),
                    ..Default::default()
                }),
            container(name)
                .center_x(Length::Fill)
                .height(22)
                .align_y(Alignment::Center),
        ]
        .spacing(4),
    )
    .padding(iced::Padding {
        top: 8.0,
        right: 8.0,
        bottom: 4.0,
        left: 8.0,
    })
    .width(Length::Fill)
    .on_press(press)
    .style(move |_, status| {
        let lit = lights(status);
        button::Style {
            background: lit.then(|| hover.into()),
            border: Border {
                color: if chosen {
                    accent
                } else if lit {
                    faint
                } else {
                    line
                },
                width: if chosen { 2.0 } else { 1.0 },
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

/// A bar of hues drawn in the colours each gives (`stops`, by place from 0
/// to 1, at least two), with a thumb at `at` (0 to 1) holding `colour`; a
/// drag or click moves it, and the choice is made on release.
pub fn hue_bar<'a, M: Clone + 'a>(
    theme: &Theme,
    stops: &[(f32, Color)],
    at: f32,
    colour: Color,
    on_drag: impl Fn(f32) -> M + 'a,
    on_release: M,
) -> Element<'a, M> {
    // A linear gradient holds eight stops: the bar is drawn in as many
    // pieces as it needs, side by side.
    let mut pieces = row![].height(16);
    let mut start = 0;
    while start + 1 < stops.len() {
        let end = (start + 7).min(stops.len() - 1);
        let (from, to) = (stops[start].0, stops[end].0);
        let span = (to - from).max(f32::EPSILON);
        let mut linear = Linear::new(Radians(std::f32::consts::FRAC_PI_2));
        for (place, stop) in &stops[start..=end] {
            linear = linear.add_stop((place - from) / span, *stop);
        }
        let gradient = Background::Gradient(Gradient::Linear(linear));
        let first = start == 0;
        let last = end == stops.len() - 1;
        pieces = pieces.push(
            container(Space::new())
                .width(Length::FillPortion((span * 1000.0).round().max(1.0) as u16))
                .height(16)
                .style(move |_| container::Style {
                    background: Some(gradient),
                    border: Border::default().rounded(crate::style::corners([
                        if first { 8.0 } else { 0.0 },
                        if last { 8.0 } else { 0.0 },
                        if last { 8.0 } else { 0.0 },
                        if first { 8.0 } else { 0.0 },
                    ])),
                    ..Default::default()
                }),
        );
        start = end;
    }
    let ring = theme.text;
    let thumb = slider(0.0..=1.0, at.clamp(0.0, 1.0), on_drag)
        .step(0.001_f32)
        .on_release(on_release)
        .height(20)
        .style(move |_, _| slider::Style {
            rail: slider::Rail {
                backgrounds: (
                    Background::Color(Color::TRANSPARENT),
                    Background::Color(Color::TRANSPARENT),
                ),
                width: 16.0,
                border: Border::default(),
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 10.0 },
                background: colour.into(),
                border_width: 4.0,
                border_color: ring,
            },
        });
    stack![container(pieces).padding([2, 0]).width(Length::Fill), thumb]
        .width(Length::Fill)
        .height(20)
        .into()
}
