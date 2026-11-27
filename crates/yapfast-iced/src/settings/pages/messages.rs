//! Messages: how messages show, the clock and the date.

use iced::{Element, widget::text};

use super::{Pref, message_size, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    controls::choices,
    form::{self, switch},
};

pub(super) const DATE_FORMATS: [&str; 3] = ["Day/Month/Year", "Month/Day/Year", "Year-Month-Day"];

impl<'a> Page<'a> {
    pub(in crate::settings) fn messages_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let twelve = prefs.twelve_hour_clock;
        let preview = self.sample(vec![
            text("Thinking co-op after nine, if people are up for it.")
                .size(message_size(prefs))
                .color(theme.text)
                .into(),
        ]);
        let date = prefs
            .date_format
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(usize::from(twelve));
        let networks = if cfg!(feature = "matrix") && self.linked() {
            "Discord and Matrix show"
        } else if cfg!(feature = "matrix") {
            "Matrix shows"
        } else {
            "Discord shows"
        };
        form::page([
            form::title(theme, "Messages"),
            preview,
            form::group([
                form::heading(theme, "CLOCK"),
                choices(theme, &["24-hour", "12-hour"], usize::from(twelve), |index| {
                    pref(Pref::TwelveHour(index == 1))
                }),
                switch(
                    "Show seconds",
                    prefs.clock_seconds,
                    Some(pref(Pref::Seconds(!prefs.clock_seconds))),
                )
                .description("Message times show them, as 21:41:05.")
                .view(self.cx.look),
                form::heading(theme, "DATE FORMAT"),
                choices(theme, &DATE_FORMATS, date, |index| {
                    pref(Pref::DateFormat(index))
                }),
            ]),
            form::rule(theme),
            self.flip(
                "Show link previews",
                "Cards for links, videos and bots under messages. Linked pictures still show.",
                prefs.link_previews.unwrap_or(true),
                Pref::LinkPreviews,
            ),
            self.flip(
                "Collapse long messages",
                "Long messages and code show their first lines, with Show more to see the rest.",
                self.cx.layout.collapse_long,
                Pref::CollapseLong,
            ),
            self.flip(
                "Show boost gems",
                "A gem by the name of each member boosting their Discord server, in the member list.",
                prefs.boost_gems.unwrap_or(true),
                Pref::BoostGems,
            ),
            switch(
                "Let others see when you're typing",
                prefs.share_typing.unwrap_or(true),
                Some(pref(Pref::ShareTyping(!prefs.share_typing.unwrap_or(true)))),
            )
            .description(format!(
                "{networks} \"typing...\" while you write. Others still show here when they type."
            ))
            .view(self.cx.look),
            self.flip(
                "Mention people you reply to",
                "They get notified of your reply. Switch it for one reply with @ On on the reply bar.",
                prefs.reply_ping.unwrap_or(true),
                Pref::ReplyPing,
            ),
            self.flip(
                "Remove tracking from links you send",
                "Takes out tags like utm_source and fbclid, and share ids from YouTube, Spotify and X. The links still go to the same page.",
                prefs.strip_tracking.unwrap_or(true),
                Pref::StripTracking,
            ),
            self.flip(
                "Peek at channels",
                "Hold Alt over a channel or DM in the list to see its newest messages without opening it or marking it read.",
                self.cx.layout.peek_channels,
                Pref::PeekChannels,
            ),
            self.flip(
                "Search DMs with the quick switcher",
                "The box at the top of the DM list opens the quick switcher, as Discord's \"Find or start a conversation\" does, instead of filtering the list.",
                self.cx.layout.dm_search_switcher,
                Pref::DmSearchSwitcher,
            ),
        ])
    }
}
