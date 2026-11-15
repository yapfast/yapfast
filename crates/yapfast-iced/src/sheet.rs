//! The frame of a server's and a channel's settings, as the Slint client's
//! `SettingsFrame` draws it without Settings' Discord layout, on a
//! `SettingsCard`: a card over the dimmed window, the pages down its left
//! on the sidebar's ground under their groups' headings, the chosen page on
//! the right with the round close button and "ESC" beside it, and an
//! editor's bar over the page's foot. Settings' own card shares the
//! [`backdrop`] and the [`nav_row`].

use std::borrow::Cow;

use iced::{
    Alignment, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text},
};

use crate::{
    app::{App, Message},
    controls::{self, lights},
    fonts::{BOLD, MEDIUM},
    form,
    pictures::Icon,
    style::{fill, scrollbar},
};

/// How far the card sits in from the window's edges each side, as the
/// Slint client's `settings-margin` (96 px in all), and its widest.
pub const MARGIN: f32 = 48.0;
pub const WIDEST: f32 = 1400.0;
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
pub enum Nav<'a> {
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
        press: Message,
    },
}

/// A settings frame being built: [`sheet`], then what goes over its page.
pub struct Sheet<'a> {
    nav: Vec<Nav<'a>>,
    page: Element<'a, Message>,
    close: Message,
    label: &'a str,
    bar: Option<Element<'a, Message>>,
    fill: bool,
}

/// A frame of `nav` and `page`; `close` closes it (the button, Escape's
/// owner, a click outside), `label` names what it is ("Server settings").
pub fn sheet<'a>(
    nav: Vec<Nav<'a>>,
    page: impl Into<Element<'a, Message>>,
    close: Message,
    label: &'a str,
) -> Sheet<'a> {
    Sheet {
        nav,
        page: page.into(),
        close,
        label,
        bar: None,
        fill: false,
    }
}

impl<'a> Sheet<'a> {
    /// An editor's bar over the page's foot, once something changed.
    pub fn bar(mut self, bar: Option<Element<'a, Message>>) -> Self {
        self.bar = bar;
        self
    }

    /// The page takes the frame's height and scrolls its own list, rather
    /// than the frame scrolling it.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    pub fn view(self, app: &'a App) -> Element<'a, Message> {
        let theme = &app.theme;
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
        let body: Element<'a, Message> = if self.fill {
            container(column_)
                .padding(padding)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            scrollable(container(column_).padding(padding).width(Length::Fill))
                .id(iced::widget::Id::new(PAGE))
                .style(scrollbar(theme))
                .height(Length::Fill)
                .into()
        };
        let close = column![
            controls_close(app, self.label, self.close.clone()),
            text("ESC")
                .size(12)
                .font(BOLD)
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
                    border: Border::default().rounded(8),
                    shadow: crate::style::card_shadow(),
                    ..Default::default()
                })
                .clip(true);
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
            scrollable(
                column(
                    self.nav
                        .into_iter()
                        .enumerate()
                        .map(|(index, row)| nav_entry(app, row, index == 0)),
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
        .style(move |_| fill(sidebar));
        let chat = theme.chat;
        let card = container(row![
            nav,
            container(content).width(Length::Fill).height(Length::Fill)
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| fill(chat));
        backdrop(theme, card, MARGIN, self.close)
    }
}

/// The page's scrollable, which a new page snaps to its top.
pub const PAGE: &str = "sheet-page";

/// The round close button at the card's top right.
fn controls_close<'a>(app: &'a App, label: &'a str, close: Message) -> Element<'a, Message> {
    let theme = &app.theme;
    let (raised, lit) = (theme.raised, theme.hover);
    let tip = format!("Close {label}");
    iced::widget::tooltip(
        button(app.glyph(Icon::X, 18, theme.secondary).boxed(CLOSE))
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
fn nav_entry<'a>(app: &'a App, row: Nav<'a>, first: bool) -> Element<'a, Message> {
    let theme = &app.theme;
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
            app,
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

/// A section's or page's row in a settings column, as the Slint client's
/// `ChannelRow`: its icon and name, on the active ground while chosen and
/// the hover ground under the pointer; a dangerous one red.
pub fn nav_row<'a>(
    app: &App,
    icon: Icon,
    label: impl text::IntoFragment<'a>,
    state: NavState,
    height: f32,
    press: Message,
) -> Element<'a, Message> {
    let NavState {
        selected,
        danger,
        fresh,
    } = state;
    let theme = &app.theme;
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
                app.glyph(icon, 20, icon_ink),
                text(label)
                    .size(15)
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
pub fn backdrop<'a>(
    theme: &crate::theme::Theme,
    card: impl Into<Element<'a, Message>>,
    margin: f32,
    close: Message,
) -> Element<'a, Message> {
    let edge = theme.edge;
    // No shadow: over the window dimmed by 70% it hardly shows, and the
    // renderer works a card this large's out pixel by pixel each time
    // anything on it is drawn again.
    let card = container(card)
        .width(Length::Fill)
        .max_width(WIDEST)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            border: Border {
                color: edge,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });
    let dim = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| fill(DIM)),
    )
    .on_press(close);
    opaque(stack![
        opaque(dim),
        container(opaque(card))
            .padding(margin)
            .center_x(Length::Fill)
            .height(Length::Fill)
    ])
}
