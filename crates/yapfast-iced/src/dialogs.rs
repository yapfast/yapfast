//! Dialogs over the dimmed window, as the Slint client's (`ui/dialogs.slint`):
//! the frame every dialog is drawn in ([`dialog`]), and Delete's
//! confirmation. While one is open nothing under it takes a click or a key.

use iced::{
    Border, Element, Length, Task,
    keyboard::key,
    widget::{Space, column, container, opaque, row, stack, text},
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
    opaque(stack![
        outside(on_outside, true),
        container(card).center(Length::Fill)
    ])
}
