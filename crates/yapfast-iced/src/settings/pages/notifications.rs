//! Notifications: desktop notifications, what they show, and the
//! taskbar's flash.

use iced::Element;

use super::{Pref, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    form::{self, switch},
};
use yapfast_core::i18n::tr;

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
        form::page([
            form::title(theme, tr("Notifications")),
            switch(
                tr("Show desktop notifications"),
                on,
                Some(pref(Pref::Notifications(!on))),
            )
            .description(format!(
                "For new messages while Yapfast isn't in front. {rules} which: usually DMs and mentions. Clicking one opens the conversation."
            ))
            .view(self.cx.look),
            switch(
                tr("Show what messages say"),
                words,
                on.then(|| pref(Pref::NotificationText(!words))),
            )
            .description(tr("Off shows only who wrote and where, for a screen others can see."))
            .view(self.cx.look),
            self.flip(
                tr("Flash the taskbar"),
                tr("For a message that notifies while Yapfast isn't in front. The window's title counts mentions and DMs waiting either way."),
                prefs.flash_taskbar.unwrap_or(true),
                Pref::FlashTaskbar,
            ),
        ])
    }
}
