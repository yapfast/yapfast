//! Messages: how messages show, the clock and the date.

use iced::{
    Alignment, Element, Length,
    widget::{column, container, row, text},
};

use super::{ANIMATES, Pref, message_size, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    controls::choices,
    fonts::BOLD,
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
        let size = message_size(prefs.text_size);
        let twelve = prefs.twelve_hour_clock;
        let clock = format!(
            "{}{}{}",
            if twelve { "9:41" } else { "21:41" },
            if prefs.clock_seconds { ":05" } else { "" },
            if twelve { " PM" } else { "" }
        );
        let edge = theme.line;
        let preview = container(
            row![
                container(self.cx.avatar(None, tr("Sam"), 40.0, 20.0, 1.0))
                    .align_top(Length::Shrink),
                column![
                    row![
                        text(tr("Sam")).size(size).font(BOLD).color(theme.text),
                        text(clock).size(size - 3.0).color(theme.muted)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(tr("Thinking co-op after nine, if people are up for it."))
                        .size(size)
                        .color(theme.text)
                ]
                .spacing(3)
                .width(Length::Fill)
            ]
            .spacing(16),
        )
        .padding(16)
        .width(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            border: crate::style::edge(edge, 8.0),
            ..Default::default()
        });
        let text_size = prefs.text_size.unwrap_or(1).clamp(0, 2) as usize;
        let date = prefs
            .date_format
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(usize::from(twelve));
        let animate = ANIMATES
            .iter()
            .position(|kind| *kind == prefs.animate)
            .unwrap_or(0);
        let networks = if cfg!(feature = "matrix") && self.linked() {
            tr("Discord and Matrix show")
        } else if cfg!(feature = "matrix") {
            tr("Matrix shows")
        } else {
            tr("Discord shows")
        };
        form::page([
            form::title(theme, tr("Messages")),
            preview.into(),
            form::group([
                form::heading(theme, tr("TEXT SIZE")),
                choices(theme, &[tr("Small"), tr("Default"), tr("Large")], text_size, |index| {
                    pref(Pref::TextSize(index))
                }),
            ]),
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
            form::group([
                form::heading(theme, tr("PLAY GIFS, EMOJI AND STICKERS")),
                choices(
                    theme,
                    &[tr("While Yapfast is focused"), tr("Always"), tr("Never")],
                    animate,
                    |index| pref(Pref::Animate(index)),
                ),
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
