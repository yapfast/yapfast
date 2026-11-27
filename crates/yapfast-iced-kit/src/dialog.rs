//! The frame every dialog is drawn in: over the dimmed window ([`dialog`]),
//! or over a server's or channel's settings ([`panel`]), with its head, its
//! buttons and the card of what it found. While one is open nothing under
//! it takes a click or a key.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, opaque, row, stack, text},
};

use crate::{fonts::BOLD, style::card_shadow, theme::Theme};

/// The width of a dialog with a question and its answers.
pub const CONFIRM_WIDTH: f32 = 440.0;
/// How far a dialog stays in from the window's edges, in all.
const ROOM: f32 = 48.0;

impl<'a> crate::look::Look<'a> {
    /// A dialog's title with its close button.
    pub fn dialog_head<M: Clone + 'a>(
        self,
        title: impl text::IntoFragment<'a>,
        close: M,
    ) -> Element<'a, M> {
        row![
            text(title)
                .size(self.theme.title_size(18.0))
                .font(self.theme.font("headings", BOLD))
                .color(self.theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            self.icon_button(crate::icons::Icon::X, "Close", close)
                .below(),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    /// A dialog's parts, 12 px apart in 20 px of padding, in [`dialog`]'s
    /// frame at `width`, kept inside the window: taller than the window
    /// leaves it, they scroll.
    pub fn framed<M: Clone + 'a>(
        self,
        body: impl IntoIterator<Item = Element<'a, M>>,
        width: f32,
        close: M,
    ) -> Element<'a, M> {
        let body = scroll_view(column(body).spacing(12).padding(20))
            .width(Length::Fill)
            .style(crate::style::scrollbar(self.theme));
        dialog(self.theme, body.into(), width, close)
    }

    /// What a dialog found, on the sunken ground: a picture beside them
    /// when given, its name in bold and the lines under it, as the join,
    /// event, gift and authorization dialogs show it.
    pub fn found_card<M: Clone + 'a>(
        self,
        picture: Option<Element<'a, M>>,
        name: String,
        lines: impl IntoIterator<Item = String>,
    ) -> Element<'a, M> {
        let theme = self.theme;
        let mut words = column![
            text(name)
                .size(theme.type_size("names", 16.0))
                .font(theme.font("names", BOLD))
                .color(theme.text)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(2)
        .width(Length::Fill);
        for line in lines.into_iter().filter(|line| !line.is_empty()) {
            words = words.push(crate::form::note(theme, line));
        }
        let shown: Element<'_, M> = match picture {
            Some(picture) => row![picture, words]
                .spacing(12)
                .align_y(Alignment::Center)
                .into(),
            None => words.into(),
        };
        let sunken = theme.sunken;
        container(shown)
            .padding(12)
            .width(Length::Fill)
            .clip(true)
            .style(move |_| crate::style::rounded(sunken, 8))
            .into()
    }
}

/// A dialog's buttons, at its end, a little below the rest.
pub fn buttons<'a, M: Clone + 'a>(
    parts: impl IntoIterator<Item = Element<'a, M>>,
) -> Element<'a, M> {
    let mut line = row![Space::new().width(Length::Fill)]
        .spacing(8)
        .align_y(Alignment::Center);
    for part in parts {
        line = line.push(part);
    }
    container(line)
        .padding(iced::Padding {
            top: 4.0,
            ..iced::Padding::ZERO
        })
        .into()
}

/// A dialog over the dimmed window: the raised ground in the theme's
/// edge, a card's shadow, the window dimmed under it ([`outside`]). A click
/// on the dimmed window sends `outside`; nothing under it takes a key.
pub fn dialog<'a, M: Clone + 'a>(
    theme: &Theme,
    body: Element<'a, M>,
    width: f32,
    on_outside: M,
) -> Element<'a, M> {
    let (mut ground, edge) = (theme.raised, theme.edge);
    if theme.decorated("dialog") || theme.package_paint("raised").is_some() {
        ground = iced::Color::TRANSPARENT;
    }
    let mut border = theme.surface_border("dialog", 12.0, 1.0, edge);
    let drawn_width = border.width;
    if theme.outline("dialog").is_some() {
        border.width = 0.0;
    }
    let shadow = theme.surface_shadow("dialog", card_shadow());
    let card = container(body)
        .width(width)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border,
            shadow,
            ..Default::default()
        });
    let card = opaque(crate::controls::themed_outline(
        card,
        theme,
        "dialog",
        edge,
        drawn_width,
        12.0,
    ));
    // Kept inside the window, by its padding, both ways.
    let scrim = theme.token_colour("scrim", crate::controls::DIM);
    opaque(stack![
        crate::controls::dimmed(on_outside, scrim),
        container(crate::widgets::motion::grow(card))
            .padding(ROOM / 2.0)
            .center(Length::Fill)
    ])
}

/// How dark the window goes under a dialog over a server's or channel's
/// settings (`#000000aa`).
pub const PANEL_DIM: iced::Color = iced::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.667,
};

/// A dialog over a server's or a channel's settings (Delete Role, Add
/// members, a ban, Security Actions): on the chat's ground with 8 px
/// corners and no edge, `width` across and, when given, `height` tall, the
/// window dimmed by [`PANEL_DIM`] under it. A click on the dimmed window
/// sends `on_outside`; nothing under it takes a key.
pub fn panel<'a, M: Clone + 'a>(
    theme: &Theme,
    body: Element<'a, M>,
    width: f32,
    height: Option<f32>,
    on_outside: M,
) -> Element<'a, M> {
    let ground = if theme.decorated("dialog") || theme.package_paint("raised").is_some() {
        iced::Color::TRANSPARENT
    } else {
        theme.chat
    };
    let mut border = theme.surface_border("dialog", 8.0, 0.0, iced::Color::TRANSPARENT);
    let drawn_width = border.width;
    if theme.outline("dialog").is_some() {
        border.width = 0.0;
    }
    let mut card = container(body)
        .width(width)
        .style(move |_| container::Style {
            border,
            ..crate::style::rounded(ground, border.radius)
        });
    if let Some(height) = height {
        card = card.height(height);
    }
    let card = crate::controls::themed_outline(card, theme, "dialog", theme.edge, drawn_width, 8.0);
    let scrim = theme.token_colour("scrim", PANEL_DIM);
    opaque(stack![
        crate::controls::dimmed(on_outside, scrim),
        container(crate::widgets::motion::grow(opaque(card))).center(Length::Fill)
    ])
}
