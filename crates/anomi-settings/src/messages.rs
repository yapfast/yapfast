//! The Messages settings page: a sample message as the choices show it, the
//! display, text size, clock, date format and animation choices, and the
//! page's switches. The Slint side: `MessageSettings`
//! (`ui/settings.slint:1634`).

use std::sync::OnceLock;

use iced::widget::{column, container, row, rule, text};
use iced::{Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::kind::Avatar;
use anomi_vm::settings::messages::{Animate, DateFormat, Intent, Messages, TextSize};
use anomi_widgets::avatar::circle;
use yapfast_lang::tr;

use crate::kit::{choices, heading, switch, words};

/// Between the parts of the page (`spacing: 16px`), and a group's heading and
/// its control (`8px`).
const SPACING: f32 = 16.0;
const GROUP: f32 = 8.0;
/// The sample's padding.
const PAD: f32 = 16.0;
/// The time column of the compact display (`ClientStyle.compact-gutter`).
const GUTTER: f32 = 60.0;

pub fn view(page: &Messages, motion: Motion) -> impl View<Intent> + '_ {
    let rows: Vec<Element<'_, Intent>> = vec![
        preview(page),
        group(
            tr!("MESSAGE DISPLAY"),
            words(&[tr!("Cozy"), tr!("Compact")]),
            usize::from(page.compact),
            |i| Intent::Compact(i == 1),
            motion,
        ),
        group(
            tr!("TEXT SIZE"),
            words(&[
                tr!("Small"),
                // "Default" means "Normal" here (Slint's context).
                yapfast_lang::text_in("MessageSettings", "Default"),
                tr!("Large"),
            ]),
            TextSize::ALL
                .iter()
                .position(|s| *s == page.text_size)
                .unwrap_or(1),
            |i| Intent::TextSize(TextSize::ALL[i.min(2)]),
            motion,
        ),
        column![
            heading(tr!("CLOCK")),
            choices(
                words(&[tr!("24-hour"), tr!("12-hour")]),
                usize::from(page.twelve_hour),
                motion,
            )
            .wrap(true)
            .on_pick(|i| Intent::TwelveHour(i == 1))
            .view(),
            switch(tr!("Show seconds"), page.seconds, motion)
                .description(tr!("Message times show them, as 21:41:05."))
                .on_toggle(Intent::Seconds)
                .view(),
        ]
        .spacing(GROUP)
        .boxed(),
        group(
            tr!("DATE FORMAT"),
            words(&[
                tr!("Day/Month/Year"),
                tr!("Month/Day/Year"),
                tr!("Year-Month-Day"),
            ]),
            DateFormat::ALL
                .iter()
                .position(|d| *d == page.date_shown())
                .unwrap_or(0),
            |i| Intent::DateFormat(DateFormat::ALL[i.min(2)]),
            motion,
        ),
        group(
            tr!("PLAY GIFS, EMOJI AND STICKERS"),
            words(&[tr!("While Yapfast is focused"), tr!("Never")]),
            usize::from(page.animate == Animate::Never),
            |i| Intent::Animate(Animate::ALL[i.min(1)]),
            motion,
        ),
        rule::horizontal(1).style(catalog::rule::line).boxed(),
        switch(tr!("Show link previews"), page.link_previews, motion)
            .description(tr!(
                "Cards for links, videos and bots under messages. Linked pictures still show."
            ))
            .on_toggle(Intent::LinkPreviews)
            .view(),
        switch(tr!("Collapse long messages"), page.collapse_long, motion)
            .description(tr!(
                "Long messages and code show their first lines, with Show more to see the rest."
            ))
            .on_toggle(Intent::CollapseLong)
            .view(),
        switch(tr!("Show boost gems"), page.boost_gems, motion)
            .description(tr!(
                "A gem by the name of each member boosting their Discord server, in the member list."
            ))
            .on_toggle(Intent::BoostGems)
            .view(),
        switch(tr!("Let others see when you're typing"), page.share_typing, motion)
            .description(typing(page))
            .on_toggle(Intent::ShareTyping)
            .view(),
        switch(tr!("Mention people you reply to"), page.reply_ping, motion)
            .description(tr!(
                "They get notified of your reply. Switch it for one reply with @ On on the reply bar."
            ))
            .on_toggle(Intent::ReplyPing)
            .view(),
        switch(
            tr!("Remove tracking from links you send"),
            page.strip_tracking,
            motion,
        )
        .description(tr!(
            "Takes out tags like utm_source and fbclid, and share ids from YouTube, Spotify and X. The links still go to the same page."
        ))
        .on_toggle(Intent::StripTracking)
        .view(),
    ];
    column(rows).spacing(SPACING).width(Length::Fill)
}

/// What the typing switch says, naming the networks in use.
fn typing(page: &Messages) -> &'static str {
    if page.matrix && page.discord {
        tr!(
            "Discord and Matrix show \"typing...\" while you write. Others still show here when they type."
        )
    } else if page.matrix {
        tr!("Matrix shows \"typing...\" while you write. Others still show here when they type.")
    } else {
        tr!("Discord shows \"typing...\" while you write. Others still show here when they type.")
    }
}

/// A heading over a row of choices.
fn group<'a>(
    title: &'a str,
    options: &'static [&'static str],
    current: usize,
    pick: impl Fn(usize) -> Intent + 'a,
    motion: Motion,
) -> Element<'a, Intent> {
    column![
        heading(title),
        choices(options, current, motion)
            .wrap(true)
            .on_pick(pick)
            .view(),
    ]
    .spacing(GROUP)
    .boxed()
}

/// A message as the chosen size, display and clock show it.
fn preview(page: &Messages) -> Element<'_, Intent> {
    let size = page.text_size.px();
    let seconds = if page.seconds { ":05" } else { "" };
    let clock = if page.twelve_hour { "9:41" } else { "21:41" };
    let said = tr!("Thinking co-op after nine, if people are up for it.");
    let body = |words: &'static str| text(words).size(size).style(catalog::text::primary);
    let name = text("Sam")
        .size(size)
        .font(typography::BOLD)
        .style(catalog::text::primary);
    let inside: Element<'_, Intent> = if page.compact {
        let time = text(format!("{clock}{seconds}"))
            .size(typography::size::CAPTION)
            .font(typography::MEDIUM)
            .style(catalog::text::muted)
            .width(GUTTER)
            .align_x(alignment::Horizontal::Right);
        row![time, row![name, body(said).width(Length::Fill)].spacing(8)]
            .spacing(8)
            .boxed()
    } else {
        let pm = if page.twelve_hour { " PM" } else { "" };
        static SAM: OnceLock<Avatar> = OnceLock::new();
        let avatar = SAM.get_or_init(|| Avatar {
            initial: "S".into(),
            tint: Color::from_rgb8(0xc0, 0x67, 0x9a),
            picture: None,
        });
        let stamp = text(format!("{clock}{seconds}{pm}"))
            .size(size - 3.0)
            .style(catalog::text::muted);
        row![
            column![circle(avatar, metrics::AVATAR_MESSAGE)],
            column![
                row![name, stamp]
                    .spacing(8)
                    .align_y(alignment::Vertical::Center),
                body(said)
            ]
            .spacing(3)
            .width(Length::Fill),
        ]
        .spacing(16)
        .boxed()
    };
    container(inside)
        .padding(PAD)
        .width(Length::Fill)
        .style(|theme: &Theme| container::Style {
            border: iced::Border::default()
                .rounded(metrics::RADIUS_CARD)
                .width(1)
                .color(theme.palette().line),
            ..container::Style::default()
        })
        .boxed()
}

/// Its shots for `anomi-shots`: `settings/messages.png` as the Slint app
/// showed it, and the compact display.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::messages::{compact, messages, with};

    use crate::shots::{GOLDEN, Shot};

    fn compact_vm() -> anomi_vm::Vm {
        with(compact())
    }
    [
        ("messages", messages as fn() -> anomi_vm::Vm),
        ("messages_compact", compact_vm),
    ]
    .into_iter()
    .map(|(name, vm)| Shot {
        area: "settings",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    })
    .collect()
}
