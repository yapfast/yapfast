//! Notifications: desktop notifications, what they show, and on Windows
//! the taskbar's flash.

use iced::Element;

use super::{Pref, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    form::{self, switch},
};
use yapfast_core::i18n::{self, tr};

impl<'a> Page<'a> {
    pub(in crate::settings) fn notifications_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let on = prefs.notifications.unwrap_or(true);
        let rules = if cfg!(feature = "matrix") && self.linked() {
            tr(
                "Each Discord server's notification settings and your Matrix notification rules decide",
            )
        } else if cfg!(feature = "matrix") {
            tr("Your Matrix notification rules decide")
        } else {
            tr("Each Discord server's notification settings decide")
        };
        let words = prefs.notification_text.unwrap_or(true);
        let mut parts = vec![
            form::title(theme, tr("Notifications")),
            switch(
                tr("Show desktop notifications"),
                on,
                Some(pref(Pref::Notifications(!on))),
            )
            .description(i18n::fill(tr("For new messages while Yapfast isn't in front. {} which: usually DMs and mentions. Clicking one opens the conversation."), &[&rules]))
            .view(self.cx.look),
            switch(
                tr("Show what messages say"),
                words,
                on.then(|| pref(Pref::NotificationText(!words))),
            )
            .description(tr("Off shows only who wrote and where, for a screen others can see."))
            .view(self.cx.look),
        ];
        // Only on Windows, as Discord's client offers it: on Linux an
        // attention request can bring the window forward or switch the
        // desktop to it, which a message mustn't.
        if cfg!(windows) {
            parts.push(self.flip(
                tr("Flash the taskbar"),
                tr("With a notification, until you come back to Yapfast. The window's title counts mentions and DMs waiting either way."),
                prefs.flash_taskbar.unwrap_or(true),
                Pref::FlashTaskbar,
            ));
        }
        form::page(parts)
    }
}
