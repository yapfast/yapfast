//! Notifications: desktop notifications, what they show, and the
//! taskbar's flash.

use iced::Element;

use super::{Pref, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    form::{self, switch},
};

impl<'a> Page<'a> {
    pub(in crate::settings) fn notifications_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let on = prefs.notifications.unwrap_or(true);
        let rules = if cfg!(feature = "matrix") && self.linked() {
            "Each Discord server's notification settings and your Matrix notification rules decide"
        } else if cfg!(feature = "matrix") {
            "Your Matrix notification rules decide"
        } else {
            "Each Discord server's notification settings decide"
        };
        let words = prefs.notification_text.unwrap_or(true);
        let replies = prefs.reply_titles;
        form::page([
            form::title(theme, "Notifications"),
            switch(
                "Show desktop notifications",
                on,
                Some(pref(Pref::Notifications(!on))),
            )
            .description(format!(
                "For new messages while Yapfast isn't in front. {rules} which: usually DMs and mentions. Clicking one opens the conversation."
            ))
            .view(self.cx.look),
            switch(
                "Show what messages say",
                words,
                on.then(|| pref(Pref::NotificationText(!words))),
            )
            .description("Off shows only who wrote and where, for a screen others can see.")
            .view(self.cx.look),
            switch(
                "Say whom a reply answers",
                replies,
                on.then(|| pref(Pref::ReplyTitles(!replies))),
            )
            .description("A reply's title says \"Sam replied to you\", or names whom it answers. Discord's own notifications don't.")
            .view(self.cx.look),
            self.flip(
                "Flash the taskbar",
                "For a message that notifies while Yapfast isn't in front. The window's title counts mentions and DMs waiting either way.",
                prefs.flash_taskbar.unwrap_or(true),
                Pref::FlashTaskbar,
            ),
        ])
    }
}
