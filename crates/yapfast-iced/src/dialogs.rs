//! Dialogs over the dimmed window, as the Slint client's (`ui/dialogs.slint`):
//! the frame every dialog is drawn in ([`dialog`]), and Delete's
//! confirmation. While one is open nothing under it takes a click or a key.

use iced::{
    Alignment, Border, Element, Length, Task,
    keyboard::key,
    widget::{Space, column, container, opaque, row, scrollable, stack, text},
};

use crate::{
    app::{App, Message},
    controls::{Kind, action, outside},
    fonts::{BOLD, MEDIUM},
    style::card_shadow,
    theme::Theme,
};

/// The width of a dialog with a question and its answers.
pub const CONFIRM_WIDTH: f32 = 440.0;
/// How far a dialog stays in from the window's edges, in all.
const ROOM: f32 = 48.0;

impl App {
    /// The confirmation's keys: Escape cancels, Enter does what the focused
    /// button does, Tab and the side arrows go round the two buttons.
    pub fn confirm_key(&mut self, named: Option<key::Named>) -> Task<Message> {
        match named {
            Some(key::Named::Escape) => self.update(Message::CancelDelete),
            Some(key::Named::Enter) => {
                if self.confirm_focus {
                    self.update(Message::ConfirmDelete)
                } else {
                    self.update(Message::CancelDelete)
                }
            }
            Some(key::Named::Tab | key::Named::ArrowLeft | key::Named::ArrowRight) => {
                self.confirm_focus = !self.confirm_focus;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Delete's confirmation over the dimmed window: the question, the
    /// message as it shows, Cancel and Delete (focused first). A click on
    /// the dimmed window cancels; nothing under it takes a key.
    pub fn delete_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let item = state.confirm_delete.as_ref()?;
        let message = state.message(item);
        let focus = self.confirm_focus;
        let theme = &self.theme;
        let line = theme.line;
        let mut body = column![
            text("Delete Message").size(20).font(BOLD).color(theme.text),
            text("Are you sure you want to delete this message?")
                .size(15)
                .color(theme.secondary),
        ]
        .spacing(12)
        .padding(16);
        if let Some(message) = message {
            body = body.push(
                container(
                    column![
                        text(message.author.clone()).font(MEDIUM).color(theme.text),
                        text(message.body.clone())
                            .color(theme.text)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    ]
                    .spacing(2),
                )
                .padding(12)
                .width(Length::Fill)
                .max_height(160)
                .clip(true)
                .style(move |_| container::Style {
                    border: Border {
                        color: line,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }),
            );
        }
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                action(theme, "Cancel", Kind::Ghost, Some(Message::CancelDelete)).ring(!focus),
                action(theme, "Delete", Kind::Danger, Some(Message::ConfirmDelete)).ring(focus),
            ]
            .spacing(8),
        );
        Some(dialog(
            theme,
            body.into(),
            CONFIRM_WIDTH,
            Message::CancelDelete,
        ))
    }
}

impl App {
    /// The question before following a masked link (one whose words aren't
    /// where it goes), as the Slint client's `LeavingDialog`: where it goes,
    /// Go back and Visit site. Escape goes back, Enter visits.
    pub fn leaving_view(&self) -> Option<Element<'_, Message>> {
        let link = self.leaving.as_deref()?;
        let theme = &self.theme;
        let sunken = theme.sunken;
        let body = column![
            text("Leaving Yapfast")
                .size(18)
                .font(BOLD)
                .color(theme.text),
            text("This link goes to").size(15).color(theme.secondary),
            text(yapfast_core::links::host(link))
                .size(17)
                .font(BOLD)
                .color(theme.text),
            container(
                text(link)
                    .size(14)
                    .color(theme.secondary)
                    .wrapping(text::Wrapping::Glyph),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(move |_| crate::style::rounded(sunken, 6)),
            row![
                Space::new().width(Length::Fill),
                action(theme, "Go back", Kind::Ghost, Some(Message::Leave(false))),
                action(
                    theme,
                    "Visit site",
                    Kind::Primary,
                    Some(Message::Leave(true))
                ),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .padding(20);
        Some(dialog(
            theme,
            body.into(),
            CONFIRM_WIDTH,
            Message::Leave(false),
        ))
    }

    /// The question's answer: the link opens, or not.
    pub fn leave(&mut self, visit: bool) -> Task<Message> {
        if let Some(link) = self.leaving.take()
            && visit
        {
            crate::browser::open(&link);
        }
        Task::none()
    }
}

impl App {
    /// A dialog's title with its close button, as the Slint client's
    /// dialogs in `ui/server.slint` start.
    pub fn dialog_head<'a>(&'a self, title: String, close: Message) -> Element<'a, Message> {
        row![
            text(title)
                .size(18)
                .font(BOLD)
                .color(self.theme.text)
                .wrapping(text::Wrapping::None)
                .width(Length::Fill),
            self.icon_button(crate::pictures::Icon::X, "Close", 28.0, 18, close, true),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    /// A dialog's parts, 12 px apart in 20 px of padding, in [`dialog`]'s
    /// frame at `width`, kept inside the window: taller than the window
    /// leaves it, they scroll.
    pub fn framed<'a>(
        &'a self,
        body: impl IntoIterator<Item = Element<'a, Message>>,
        width: f32,
        close: Message,
    ) -> Element<'a, Message> {
        let body = scrollable(column(body).spacing(12).padding(20))
            .width(Length::Fill)
            .style(crate::style::scrollbar(&self.theme));
        dialog(&self.theme, body.into(), width, close)
    }

    /// What a dialog found, on the sunken ground: a picture beside them
    /// when given, its name in bold and the lines under it, as the Slint
    /// client's join, event, gift and authorization dialogs show it.
    pub fn found_card<'a>(
        &'a self,
        picture: Option<Element<'a, Message>>,
        name: String,
        lines: impl IntoIterator<Item = String>,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
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
        let shown: Element<'_, Message> = match picture {
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
pub fn buttons<'a>(parts: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
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

/// A dialog over the dimmed window, as the Slint client's: the raised
/// ground in the theme's edge, Slint's shadow, the window dimmed under it
/// ([`outside`]). A click on the dimmed window sends `outside`; nothing
/// under it takes a key.
pub fn dialog<'a>(
    theme: &Theme,
    body: Element<'a, Message>,
    width: f32,
    on_outside: Message,
) -> Element<'a, Message> {
    let (ground, edge) = (theme.raised, theme.edge);
    let card = opaque(
        container(body)
            .width(width)
            .style(move |_| container::Style {
                background: Some(ground.into()),
                border: Border {
                    color: edge,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                shadow: card_shadow(),
                ..Default::default()
            }),
    );
    // Kept inside the window, by its padding, both ways.
    opaque(stack![
        outside(on_outside, true),
        container(card).padding(ROOM / 2.0).center(Length::Fill)
    ])
}

/// How dark the window goes under a dialog over a server's or channel's
/// settings, as the Slint client's (`#000000aa`).
pub const PANEL_DIM: iced::Color = iced::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.667,
};

/// A dialog over a server's or a channel's settings, as the Slint client
/// draws those (Delete Role, Add members, a ban, Security Actions): on the
/// chat's ground with 8 px corners and no edge, `width` across and, when
/// given, `height` tall, the window dimmed by [`PANEL_DIM`] under it. A
/// click on the dimmed window sends `on_outside`; nothing under it takes a
/// key.
pub fn panel<'a>(
    theme: &Theme,
    body: Element<'a, Message>,
    width: f32,
    height: Option<f32>,
    on_outside: Message,
) -> Element<'a, Message> {
    let ground = theme.chat;
    let mut card = container(body)
        .width(width)
        .style(move |_| crate::style::rounded(ground, 8));
    if let Some(height) = height {
        card = card.height(height);
    }
    opaque(stack![
        crate::controls::dimmed(on_outside, PANEL_DIM),
        container(opaque(card)).center(Length::Fill)
    ])
}
