//! The frame of a server's and a channel's settings: a card over the
//! dimmed window, the pages down its left on the sidebar's ground under
//! their groups' headings, the chosen page on the right with the round
//! close button and "ESC" beside it, and an editor's bar over the page's
//! foot. Settings' own card shares the [`backdrop`] and the [`nav_row`].

use crate::widgets::scroll_view;
use std::borrow::Cow;

use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{button, column, container, opaque, row, stack, text},
};

use crate::look::Look;
use crate::{
    controls::{self, lights},
    fonts::{BOLD, MEDIUM},
    form,
    icons::Icon,
    style::{corners, rounded, scrollbar},
};

/// How far the card sits in from the window's edges each side (96 px in
/// all), and its widest.
pub const MARGIN: f32 = 48.0;
pub const WIDEST: f32 = 1400.0;
/// The card's corners, and the editor's bar's.
pub const RADIUS: f32 = 12.0;

/// The corner of a card's parts: stock, [`RADIUS`], under an edge drawn
/// over them; under an installed theme, inside its 1 px edge
/// ([`backdrop`]), the theme's card corner less the edge, so the parts'
/// grounds end under the edge rather than at its outer arc.
pub fn inner_radius(theme: &crate::theme::Theme) -> f32 {
    if theme.packaged() {
        (theme.radius(RADIUS) - 1.0).max(0.0)
    } else {
        RADIUS
    }
}
pub const BAR_RADIUS: f32 = 8.0;
/// How dark the window goes under a card.
pub const DIM: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.7,
};
/// The pages' column and a page's row in it, as a channel's row.
const NAV: f32 = 232.0;
const NAV_TOP: f32 = 56.0;
pub const ROW: f32 = 32.0;
/// The page: at most this wide, so far in from the card's edges.
const COLUMN: f32 = 660.0;
const LEFT: f32 = 40.0;
const RIGHT: f32 = 96.0;
const TOP: f32 = 56.0;
/// Below the page, more while the bar covers its foot.
const BOTTOM: f32 = 56.0;
const BOTTOM_BAR: f32 = 140.0;
/// The bar at its widest, and how far it floats in from the page's edges.
const BAR: f32 = 700.0;
const BAR_INSET: f32 = 24.0;
/// The close button, and the column it sits in at the card's right.
const CLOSE: f32 = 36.0;
const CLOSE_COLUMN: f32 = 56.0;

/// A row of the pages' column.
pub enum Nav<'a, M> {
    /// A group's heading, in capitals.
    Heading(Cow<'a, str>),
    /// A line between groups.
    Line,
    Page {
        label: Cow<'a, str>,
        icon: Icon,
        selected: bool,
        /// Red, as Delete Server and Delete Channel.
        danger: bool,
        press: M,
    },
}

/// A settings frame being built: [`sheet`], then what goes over its page.
pub struct Sheet<'a, M> {
    nav: Vec<Nav<'a, M>>,
    page: Element<'a, M>,
    close: M,
    label: &'a str,
    bar: Option<Element<'a, M>>,
    fill: bool,
}

/// A frame of `nav` and `page`; `close` closes it (the button, Escape's
/// owner, a click outside), `label` names what it is ("Server settings").
pub fn sheet<'a, M: Clone + 'a>(
    nav: Vec<Nav<'a, M>>,
    page: impl Into<Element<'a, M>>,
    close: M,
    label: &'a str,
) -> Sheet<'a, M> {
    Sheet {
        nav,
        page: page.into(),
        close,
        label,
        bar: None,
        fill: false,
    }
}

impl<'a, M: Clone + 'a> Sheet<'a, M> {
    /// An editor's bar over the page's foot, once something changed.
    pub fn bar(mut self, bar: Option<Element<'a, M>>) -> Self {
        self.bar = bar;
        self
    }

    /// The page takes the frame's height and scrolls its own list, rather
    /// than the frame scrolling it.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    pub fn view(self, look: Look<'a>) -> Element<'a, M> {
        let theme = look.theme;
        let inner = inner_radius(theme);
        let bottom = if self.bar.is_some() {
            BOTTOM_BAR
        } else {
            BOTTOM
        };
        let padding = Padding {
            top: TOP,
            right: RIGHT,
            bottom,
            left: LEFT,
        };
        let column_ = container(self.page)
            .max_width(COLUMN)
            .width(Length::Fill)
            .height(if self.fill {
                Length::Fill
            } else {
                Length::Shrink
            });
        let body: Element<'a, M> = if self.fill {
            container(column_)
                .padding(padding)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            scroll_view(container(column_).padding(padding).width(Length::Fill))
                .id(iced::widget::Id::new(PAGE))
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        };
        let close = column![
            controls_close(look, self.label, self.close.clone()),
            text("ESC")
                .size(theme.type_size("small", 12.0))
                .font(theme.font("small", BOLD))
                .color(theme.muted)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        ]
        .spacing(6)
        .width(CLOSE_COLUMN)
        .align_x(Alignment::Center);
        let mut content = stack![
            body,
            container(close)
                .padding(Padding {
                    top: 48.0,
                    right: 24.0,
                    ..Padding::ZERO
                })
                .align_right(Length::Fill)
        ];
        if let Some(bar) = self.bar {
            let bar = container(bar)
                .max_width(BAR)
                .width(Length::Fill)
                .style(|_| container::Style {
                    border: Border::default().rounded(BAR_RADIUS),
                    shadow: crate::style::card_shadow(),
                    ..Default::default()
                });
            content = content.push(
                container(opaque(bar))
                    .padding(Padding {
                        left: BAR_INSET,
                        right: BAR_INSET,
                        bottom: BAR_INSET,
                        ..Padding::ZERO
                    })
                    .height(Length::Fill)
                    .align_bottom(Length::Fill),
            );
        }
        let sidebar = theme.sidebar;
        let nav = container(
            scroll_view(
                column(
                    self.nav
                        .into_iter()
                        .enumerate()
                        .map(|(index, row)| nav_entry(look, row, index == 0)),
                )
                .spacing(2)
                .padding(Padding {
                    top: NAV_TOP,
                    right: 16.0,
                    bottom: 16.0,
                    left: 16.0,
                }),
            )
            .style(scrollbar(theme))
            .height(Length::Fill),
        )
        .width(NAV)
        .height(Length::Fill)
        .style(move |_| rounded(sidebar, corners([inner, 0.0, 0.0, inner])));
        let chat = theme.chat;
        let card = container(row![
            nav,
            container(content).width(Length::Fill).height(Length::Fill)
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| rounded(chat, inner));
        backdrop(theme, card, MARGIN, self.close)
    }
}

/// The page's scrollable, which a new page snaps to its top.
pub const PAGE: &str = "sheet-page";

/// The round close button at the card's top right.
fn controls_close<'a, M: Clone + 'a>(look: Look<'a>, label: &'a str, close: M) -> Element<'a, M> {
    let theme = look.theme;
    let (raised, lit) = (theme.raised, theme.hover);
    let tip = format!("Close {label}");
    iced::widget::tooltip(
        button(look.glyph(Icon::X, 18, theme.secondary).boxed(CLOSE))
            .padding(0)
            .on_press(close)
            .style(move |_, status| button::Style {
                background: Some(if lights(status) { lit } else { raised }.into()),
                border: Border::default().rounded(CLOSE / 2.0),
                ..Default::default()
            }),
        controls::tip(theme, tip),
        iced::widget::tooltip::Position::Bottom,
    )
    .into()
}

/// A row of the pages' column.
fn nav_entry<'a, M: Clone + 'a>(look: Look<'a>, row: Nav<'a, M>, first: bool) -> Element<'a, M> {
    let theme = look.theme;
    match row {
        Nav::Heading(words) => container(form::heading(theme, words))
            .padding(Padding {
                top: if first { 0.0 } else { 16.0 },
                bottom: 6.0,
                left: 8.0,
                right: 0.0,
            })
            .into(),
        Nav::Line => container(form::rule(theme)).padding([12, 8]).into(),
        Nav::Page {
            label,
            icon,
            selected,
            danger,
            press,
        } => nav_row(
            look,
            icon,
            label,
            NavState {
                selected,
                danger,
                fresh: false,
            },
            ROW,
            press,
        ),
    }
}

/// A [`nav_row`]'s state: chosen, dangerous (red), and with a NEW tag.
#[derive(Clone, Copy, Default)]
pub struct NavState {
    pub selected: bool,
    pub danger: bool,
    pub fresh: bool,
}

/// A section's or page's row in a settings column: its icon and name, on
/// the active ground while chosen and the hover ground under the pointer;
/// a dangerous one red.
pub fn nav_row<'a, M: Clone + 'a>(
    look: Look<'_>,
    icon: Icon,
    label: impl text::IntoFragment<'a>,
    state: NavState,
    height: f32,
    press: M,
) -> Element<'a, M> {
    let NavState {
        selected,
        danger,
        fresh,
    } = state;
    let theme = look.theme;
    let icon_ink = if danger {
        theme.danger
    } else if selected {
        theme.text
    } else {
        theme.faint
    };
    let (active, hover, text_ink, rest_ink, lit_ink, red) = (
        theme.active,
        theme.hover,
        theme.text,
        theme.muted,
        theme.secondary,
        theme.danger,
    );
    button(
        container(
            row![
                look.glyph(icon, 20, icon_ink),
                text(label)
                    .size(15.0)
                    .font(MEDIUM)
                    .wrapping(text::Wrapping::None)
            ]
            .push(fresh.then(|| crate::form::new_tag(theme)))
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .clip(true),
    )
    .padding([0, 8])
    .width(Length::Fill)
    .height(height)
    .on_press(press)
    .style(move |_, status| {
        let lit = lights(status);
        button::Style {
            background: if selected {
                Some(active.into())
            } else {
                lit.then(|| hover.into())
            },
            text_color: if danger {
                red
            } else if selected {
                text_ink
            } else if lit {
                lit_ink
            } else {
                rest_ink
            },
            border: Border::default().rounded(6),
            ..Default::default()
        }
    })
    .into()
}

/// A card over the window dimmed by [`DIM`], `margin` in from its edges
/// and at most [`WIDEST`] across, centred, with the theme's edge round it;
/// a click on the dimmed window sends `close`. Nothing under it takes a
/// click.
pub fn backdrop<'a, M: Clone + 'a>(
    theme: &crate::theme::Theme,
    card: impl Into<Element<'a, M>>,
    margin: f32,
    close: M,
) -> Element<'a, M> {
    let (edge, ground) = (theme.edge, theme.chat);
    let card: Element<'a, M> = if theme.packaged() {
        // The card's ground and edge are one rounded shape at the theme's
        // card corner, with its shadow; its parts lie 1 px inside the edge
        // and round their grounds to `inner_radius`, so no part's ground
        // shows past the edge's arc (two shapes edge to edge leave a light
        // fringe where their soft edges meet).
        let radius = theme.radius(RADIUS);
        container(container(card).padding(1))
            .width(Length::Fill)
            .max_width(WIDEST)
            .height(Length::Fill)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: radius.into(),
                },
                shadow: crate::style::raised_shadow(),
                ..Default::default()
            })
            .into()
    } else {
        // Stock: its parts round their own grounds to its corners; its edge
        // goes over them. The ground under them is opaque, so its shadow
        // is drawn only round it.
        container(controls::edged(card, edge, RADIUS))
            .width(Length::Fill)
            .max_width(WIDEST)
            .height(Length::Fill)
            .clip(true)
            .style(move |_| {
                crate::style::rounded(ground, RADIUS).shadow(crate::style::raised_shadow())
            })
            .into()
    };
    let scrim = theme.token_colour("scrim", DIM);
    opaque(stack![
        controls::dimmed(close, scrim),
        container(crate::widgets::motion::grow(opaque(card)))
            .padding(margin)
            .center_x(Length::Fill)
            .height(Length::Fill)
    ])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{RADIUS, inner_radius};
    use crate::theme::{Metrics, Theme};

    #[test]
    fn a_stock_cards_parts_keep_its_corner_and_a_themes_lie_inside_its_edge() {
        let stock = Theme::new("test:sheet", true);
        assert_eq!(inner_radius(&stock), RADIUS);
        let metrics = Metrics {
            radii: [None, None, Some(20.0)],
            ..Metrics::default()
        };
        let dressed = stock.with_metrics(Arc::new(metrics));
        assert_eq!(inner_radius(&dressed), dressed.radius(RADIUS) - 1.0);
        // Even at Wired's corner, inside the edge.
        let plain = Theme::new("test:sheet", true).with_metrics(Arc::new(Metrics::default()));
        assert_eq!(inner_radius(&plain), RADIUS - 1.0);
    }
}
