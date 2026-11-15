//! The conversation column: what takes the timeline's place (the friends
//! screens, browsing, the forum, a call) and what stands beside it
//! (search, the thread, the member list, the profile panel).

use iced::{
    Element, Length,
    widget::{column, row},
};

use crate::{
    app::{App, Message},
    controls::bar,
};

/// The navigation's width beside the conversation: the rail and the
/// sidebar.
pub const NAVIGATION: f32 = crate::rail::WIDTH + crate::sidebar::WIDTH;

impl App {
    /// The conversation column: the header, the timeline and the composer,
    /// with a server's member list beside them; or a voice channel's call.
    pub fn conversation(&self) -> Element<'_, Message> {
        if self.voice_panel() {
            return self.call_view();
        }
        let outline = self.theme.outline_chat;
        let line = bar(outline, Length::Fill, 1);
        if let Some(screen) = self.dm_screen() {
            let page = match screen {
                yapfast_core::application::DmScreen::Friends => self.friends_screen(),
                yapfast_core::application::DmScreen::Requests => self.requests_screen(),
            };
            return column![self.friends_header(screen), line, page]
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        if let Some(screen) = self.browse_channels() {
            return screen;
        }
        let mut chat = column(self.banners())
            .width(Length::Fill)
            .height(Length::Fill);
        // A DM's call shows above its messages.
        if let Some(call) = self.dm_call_view() {
            chat = chat.push(call);
        }
        let chat = match self.forum_view() {
            Some(forum) => chat.push(forum),
            None => chat.push(self.timeline()).push(self.composer_view()),
        };
        let mut body = row![chat];
        if let Some(search) = self.search_view() {
            body = body.push(search);
        } else if let Some(aside) = self.aside_view() {
            body = body.push(aside);
        } else if self.members_shown() {
            body = body.push(self.members_view());
        } else if self.panel_shown()
            && let Some(panel) = self.profile_panel_view()
        {
            body = body.push(panel);
        }
        column![self.header_view(), line, body.height(Length::Fill)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The conversation column's left edge: the navigation's right.
    pub fn chat_left(&self) -> f32 {
        NAVIGATION
    }

    /// The conversation column's right edge: the member list's left.
    pub fn chat_right(&self) -> f32 {
        self.window.width
            - if self.search_shown() {
                crate::search::WIDTH
            } else if self.aside_shown() {
                self.aside_width()
            } else if self.members_shown() {
                crate::members::WIDTH
            } else if self.panel_shown() {
                crate::profile::PANEL_WIDTH
            } else {
                0.0
            }
    }
}
