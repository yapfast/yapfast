//! Messages: how messages show, the clock and the date.

use iced::{Element, widget::text};

use super::{Pref, message_size, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    controls::choices,
    form::{self, switch},
};
use yapfast_core::i18n::{self, later, tr};

pub(super) const DATE_FORMATS: [&str; 3] = [
    later("Day/Month/Year"),
    later("Month/Day/Year"),
    later("Year-Month-Day"),
];

impl<'a> Page<'a> {
    pub(in crate::settings) fn messages_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let prefs = &self.cx.layout.prefs;
        let twelve = prefs.twelve_hour_clock;
        let preview = self.sample(vec![
            text(tr("Thinking co-op after nine, if people are up for it."))
                .size(message_size(prefs))
                .color(theme.text)
                .into(),
        ]);
        let date = prefs
            .date_format
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(usize::from(twelve));
        let networks = if cfg!(feature = "matrix") && self.linked() {
            tr("Discord and Matrix show")
        } else if cfg!(feature = "matrix") {
            tr("Matrix shows")
        } else {
            tr("Discord shows")
        };
        form::page([
            form::title(theme, tr("Messages")),
            preview,
            form::group([
                form::heading(theme, tr("CLOCK")),
                choices(theme, &[tr("24-hour"), tr("12-hour")], usize::from(twelve), |index| {
                    pref(Pref::TwelveHour(index == 1))
                }),
                switch(
                    tr("Show seconds"),
                    prefs.clock_seconds,
                    Some(pref(Pref::Seconds(!prefs.clock_seconds))),
                )
                .description(tr("Message times show them, as 21:41:05."))
                .view(self.cx.look),
                form::heading(theme, tr("DATE FORMAT")),
                choices(theme, &DATE_FORMATS.map(tr), date, |index| {
                    pref(Pref::DateFormat(index))
                }),
            ]),
            form::rule(theme),
            self.flip(
                tr("Show link previews"),
                tr("Cards for links, videos and bots under messages. Linked pictures still show."),
                prefs.link_previews.unwrap_or(true),
                Pref::LinkPreviews,
            ),
            self.flip(
                tr("Collapse long messages"),
                tr("Long messages and code show their first lines, with Show more to see the rest."),
                self.cx.layout.collapse_long,
                Pref::CollapseLong,
            ),
            self.flip(
                tr("Show boost gems"),
                tr("A gem by the name of each member boosting their Discord server, in the member list."),
                prefs.boost_gems.unwrap_or(true),
                Pref::BoostGems,
            ),
            switch(
                tr("Let others see when you're typing"),
                prefs.share_typing.unwrap_or(true),
                Some(pref(Pref::ShareTyping(!prefs.share_typing.unwrap_or(true)))),
            )
            .description(i18n::fill(tr("{} \"typing...\" while you write. Others still show here when they type."), &[&networks]))
            .view(self.cx.look),
            self.flip(
                tr("Mention people you reply to"),
                tr("They get notified of your reply. Switch it for one reply with @ On on the reply bar."),
                prefs.reply_ping.unwrap_or(true),
                Pref::ReplyPing,
            ),
            self.flip(
                tr("Remove tracking from links you send"),
                tr("Takes out tags like utm_source and fbclid, and share ids from YouTube, Spotify and X. The links still go to the same page."),
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
