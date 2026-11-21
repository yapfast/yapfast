//! The frame every dialog is drawn in: over the dimmed window ([`dialog`]),
//! or over a server's or channel's settings ([`panel`]), with its head, its
//! buttons and the card of what it found. While one is open nothing under
//! it takes a click or a key.

use crate::widgets::scroll_view;
use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, opaque, row, stack, text},
};

use crate::{controls::outside, fonts::BOLD, style::card_shadow, theme::Theme};

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
                .size(18)
                .font(BOLD)
                .color(self.theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            self.icon_button(crate::icons::Icon::X, crate::words::say("Close"), close)
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
                .size(16)
                .font(BOLD)
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
    // Discord's dialog on a dark theme is ringed in (59, 59, 65), a step
    // lighter than the theme's edge: the raised ground (36, 36, 41) and
    // (23, 23, 24) of 255.
    let ground = theme.raised;
    let edge = if theme.dark {
        let lift = |channel: f32, by: f32| (channel + by / 255.0).min(1.0);
        iced::Color {
            r: lift(ground.r, 23.0),
            g: lift(ground.g, 23.0),
            b: lift(ground.b, 24.0),
            a: ground.a,
        }
    } else {
        theme.edge
    };
    let card = opaque(
        container(body)
            .width(width)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: crate::style::edge(edge, 12.0),
                shadow: card_shadow(),
                ..Default::default()
            }),
    );
    // Kept inside the window, by its padding, both ways, and centred in
    // the whole window, as Discord's are: the content lays out under the
    // window's title bar, so its foot is padded by the bar's height.
    opaque(stack![
        outside(on_outside, true),
        container(crate::widgets::motion::grow(card))
            .padding(iced::Padding {
                bottom: ROOM / 2.0 + crate::widgets::motion::bar(),
                ..iced::Padding::new(ROOM / 2.0)
            })
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
    let ground = theme.chat;
    let mut card = container(body)
        .width(width)
        .style(move |_| crate::style::rounded(ground, 8));
    if let Some(height) = height {
        card = card.height(height);
    }
    opaque(stack![
        crate::controls::dimmed(on_outside, PANEL_DIM),
        // Centred in the whole window, as the dialog is.
        container(crate::widgets::motion::grow(opaque(card)))
            .padding(iced::Padding {
                bottom: crate::widgets::motion::bar(),
                ..iced::Padding::ZERO
            })
            .center(Length::Fill)
    ])
}
