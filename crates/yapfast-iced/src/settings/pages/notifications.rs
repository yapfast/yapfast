//! Notifications: desktop notifications, what they show, and the
//! taskbar's flash.

use iced::Element;

use super::{Pref, pref};
use crate::{
    app::{App, Message},
    form::{self, switch},
};

impl App {
    pub(in crate::settings) fn notifications_page(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let prefs = &self.layout.prefs;
        let on = prefs.notifications.unwrap_or(true);
        let rules = if cfg!(feature = "matrix") && self.discord_linked() {
            "Each Discord server's notification settings and your Matrix notification rules decide"
        } else if cfg!(feature = "matrix") {
            "Your Matrix notification rules decide"
        } else {
            "Each Discord server's notification settings decide"
        };
        let words = prefs.notification_text.unwrap_or(true);
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
            .view(self.look()),
            switch(
                "Show what messages say",
                words,
                on.then(|| pref(Pref::NotificationText(!words))),
            )
            .description("Off shows only who wrote and where, for a screen others can see.")
            .view(self.look()),
            self.flip(
                "Flash the taskbar",
                "For a message that notifies while Yapfast isn't in front. The window's title counts mentions and DMs waiting either way.",
                prefs.flash_taskbar.unwrap_or(true),
                Pref::FlashTaskbar,
            ),
        ])
    }
}
