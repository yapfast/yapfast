//! The lines across the top of the conversation, as the Slint client's
//! `Banner`s: the preview's note that its data is sample data, and what
//! the core says needs saying (`AppState::notice`), until dismissed.

use iced::{
    Alignment, Element, Length,
    widget::{container, row, text},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    fonts::MEDIUM,
    pictures::Icon,
    style::fill,
};

impl App {
    /// The core's notice, while there's one.
    pub fn notice_banner(&self) -> Option<Element<'_, Message>> {
        let notice = self.client.state().notice.as_deref()?;
        Some(self.banner(notice, Message::DismissNotice))
    }

    /// The preview's note, which stays.
    pub fn preview_banner(&self) -> Option<Element<'_, Message>> {
        if !self.client.state().preview {
            return None;
        }
        let theme = &self.theme;
        let raised = theme.raised;
        let note = container(
            row![
                self.glyph(Icon::TriangleAlert, 16, theme.accent),
                text("Local preview: sample data. Nothing leaves this computer.")
                    .size(14)
                    .font(MEDIUM)
                    .color(theme.secondary)
                    .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([7, 16])
        .width(Length::Fill)
        .style(move |_| fill(raised))
        .into();
        Some(note)
    }

    /// A notice with its Dismiss button (`dismissed`).
    pub fn banner<'a>(&'a self, words: &'a str, dismissed: Message) -> Element<'a, Message> {
        let theme = &self.theme;
        let raised = theme.raised;
        container(
            row![
                self.glyph(Icon::TriangleAlert, 18, theme.accent),
                text(words).size(15).color(theme.text).width(Length::Fill),
                action(theme, "Dismiss", Kind::Ghost, Some(dismissed)).small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(iced::Padding {
            top: 6.0,
            bottom: 6.0,
            left: 16.0,
            right: 8.0,
        })
        .width(Length::Fill)
        .style(move |_| fill(raised))
        .into()
    }

    pub fn dismiss_notice(&mut self) -> iced::Task<Message> {
        self.client.dispatch(Action::DismissNotice);
        self.sync()
    }
}
