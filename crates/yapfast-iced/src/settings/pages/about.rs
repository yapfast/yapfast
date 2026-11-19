//! About: the build, its updates and Yapfast's links.

use iced::{
    Element,
    widget::{column, row, text},
};

use super::{AccountEvent, Pref, account_message, pref};
use crate::settings::{Event, Page, build_tint};
use crate::{
    app::Message,
    controls::{Kind, action},
    fonts::BOLD,
    form::{self, switch},
    pictures::Icon,
};

/// About's links.
pub(super) const WEBSITE: &str = "https://yapfast.com";

pub(super) const GITHUB: &str = "https://github.com/yapfast/yapfast";

pub(super) const DISCORD: &str = "https://yapfast.com/discord-invite";

impl<'a> Page<'a> {
    pub(in crate::settings) fn about_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let build = self.cx.build;
        let prefs = &self.cx.layout.prefs;
        let mut channel = row![
            text(build.channel.as_str())
                .size(13)
                .font(BOLD)
                .color(build_tint(theme, build.tone))
        ];
        if !build.details.is_empty() {
            channel = channel.push(
                text(format!(" · {}", build.details))
                    .size(13)
                    .color(theme.muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        let card = form::card(
            theme,
            form::beside(
                column![
                    text(build.headline.as_str())
                        .size(20)
                        .font(BOLD)
                        .color(theme.text)
                        .wrapping(text::Wrapping::None),
                    channel
                ]
                .spacing(4),
                action(
                    theme,
                    if settings.copied {
                        "Copied"
                    } else {
                        "Copy version"
                    },
                    Kind::Plain,
                    Some(Message::Settings(Event::CopyVersion)),
                ),
            ),
        );
        let mut parts = vec![form::title(theme, "About"), card];
        parts.extend(self.updates_part());
        parts.push(
            switch(
                "Check for updates",
                prefs.check_for_updates,
                Some(pref(Pref::CheckUpdates(!prefs.check_for_updates))),
            )
            .description("Checks for new versions every 6 hours.")
            .fresh(self.fresh("check-updates"))
            .view(self.cx.look),
        );
        parts.push(
            switch(
                "Include nightly builds",
                prefs.update_nightlies,
                prefs
                    .check_for_updates
                    .then(|| pref(Pref::Nightlies(!prefs.update_nightlies))),
            )
            .description("Get the newest features before release. Expect some bugs.")
            .fresh(self.fresh("update-nightlies"))
            .view(self.cx.look),
        );
        parts.push(
            switch("Install updates automatically", false, None)
                .description("Not available yet.")
                .view(self.cx.look),
        );
        parts.push(form::heading(theme, "LINKS"));
        let ink = crate::theme::rgb(theme.text);
        let link = |label, icon, page| -> Element<'a, Message> {
            action(
                theme,
                label,
                Kind::Plain,
                Some(account_message(AccountEvent::Open(page))),
            )
            .icon(self.cx.look.icon(icon, 18, ink))
            .into()
        };
        parts.push(form::buttons([
            link("Website", Icon::Logo, WEBSITE),
            link("GitHub", Icon::GitHub, GITHUB),
            link("Discord", Icon::Discord, DISCORD),
        ]));
        form::page(parts)
    }
}
