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

pub(super) const DATE_FORMATS: [&str; 3] = ["Day/Month/Year", "Month/Day/Year", "Year-Month-Day"];

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
                container(self.cx.avatar(None, "Sam", 40.0, 20.0, 1.0)).align_top(Length::Shrink),
                column![
                    row![
                        text("Sam").size(size).font(BOLD).color(theme.text),
                        text(clock).size(size - 3.0).color(theme.muted)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text("Thinking co-op after nine, if people are up for it.")
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
            "Discord and Matrix show"
        } else if cfg!(feature = "matrix") {
            "Matrix shows"
        } else {
            "Discord shows"
        };
        form::page([
            form::title(theme, "Messages"),
            preview.into(),
            form::group([
                form::heading(theme, "TEXT SIZE"),
                choices(theme, &["Small", "Default", "Large"], text_size, |index| {
                    pref(Pref::TextSize(index))
                }),
            ]),
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
            form::group([
                form::heading(theme, "PLAY GIFS, EMOJI AND STICKERS"),
                choices(
                    theme,
                    &["While Yapfast is focused", "Always", "Never"],
                    animate,
                    |index| pref(Pref::Animate(index)),
                ),
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
            self.split_switches(),
        ])
    }

    /// Split view's switch, once a Discord account is linked, and while
    /// it's on, the switches of its parts under it.
    fn split_switches(self) -> Element<'a, Message> {
        if !self.linked() {
            return iced::widget::Space::new().into();
        }
        let layout = self.cx.layout;
        let mut shown = iced::widget::column![self.flip(
            "Split view",
            "Open a channel or DM beside the open one, with its own messages and composer, from its menu.",
            layout.split_view,
            Pref::SplitView,
        )];
        if layout.split_view {
            let parts = iced::widget::column![
                self.flip(
                    "Drag rows to open",
                    "Drag a channel or DM out of the list onto the right half of the conversation.",
                    layout.split_drag.unwrap_or(true),
                    Pref::SplitDrag,
                ),
                self.flip(
                    "Ctrl+click and middle click to open",
                    "On a channel or DM in the list. Off, Ctrl+click opens it as a click does.",
                    layout.split_click.unwrap_or(true),
                    Pref::SplitClick,
                ),
                self.flip(
                    "Reopen the split on start",
                    "The channel or DM beside comes back the next time Yapfast opens.",
                    layout.split_restore.unwrap_or(true),
                    Pref::SplitRestore,
                ),
                self.flip(
                    "Notification button",
                    "A message's notification offers Open in split view beside Open, where the desktop shows buttons.",
                    layout.split_notify.unwrap_or(true),
                    Pref::SplitNotify,
                ),
                self.flip(
                    "Unread dot",
                    "A dot by the split's name when someone writes there while you type on the other side.",
                    layout.split_dot.unwrap_or(true),
                    Pref::SplitDot,
                ),
            ];
            shown =
                shown.push(iced::widget::container(parts).padding(iced::Padding::ZERO.left(24)));
        }
        shown.into()
    }
}
