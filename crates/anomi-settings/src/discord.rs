//! The Discord settings page: four switches for how the account's channels
//! and messages show, the list of those it blocked, and, with a Matrix
//! build, Unlink. Without an account it offers to link one. The Slint side:
//! `DiscordSettings` and `DiscordLinkSettings` (`ui/settings.slint:2195`,
//! `:2110`).
//!
//! The link's own panel (the code to scan, the token) is the sign-in
//! screen's (N09); here the page says what linking does and what it risks,
//! as that panel's first phase does, and asks the window to show the code.

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{column, container, row, svg, text};
use iced::{Border, Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::discord::{BLOCKED_STEP, Blocked, Discord, Intent};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::icon;
use yapfast_lang::{tr, trn};

use crate::kit::{link, switch};

/// Between the parts of the page and of the blocked list's header (`16px`,
/// `12px`), between a person's lines (`2px`) and the list's rows (`8px`).
const SPACING: f32 = 16.0;
const HEADER: f32 = 12.0;
const LINES: f32 = 2.0;
const BLOCKED: f32 = 8.0;
/// A person's picture and the blocked header's disc, and its icon.
const PICTURE: f32 = 40.0;
const GLYPH: f32 = 20.0;
/// A row's vertical padding (`6px`) and the header's top padding (`8px`).
const ROW: f32 = 6.0;
const ABOVE: f32 = 8.0;
/// The risk's icon, the gap after it, and the alpha of its fill and edge
/// (`ui/discord.slint:44`).
const WARNING_ICON: f32 = 18.0;
const WARNING_GAP: f32 = 10.0;
const WARNING_FILL: f32 = 0.1;
const WARNING_EDGE: f32 = 0.35;

pub fn view(page: &Discord, motion: Motion) -> impl View<Intent> + '_ {
    if !page.linked {
        return unlinked(motion);
    }
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if page.matrix_build {
        parts.push(
            row![
                typography::label(tr!(
                    "Unlinking signs Yapfast out of Discord on this computer."
                ))
                .width(Length::Fill)
                .align_y(alignment::Vertical::Center),
                action_button(tr!("Unlink Discord"), Kind::Danger, motion)
                    .leading(Icon::LogOut)
                    .on_press(Intent::Unlink),
            ]
            .spacing(HEADER)
            .align_y(alignment::Vertical::Center)
            .boxed(),
        );
    }
    parts.extend([
        switch(tr!("Show channels you can't access"), page.show_hidden_channels, motion)
            .description(tr!("They appear locked. Server admins hid them on purpose."))
            .on_toggle(Intent::ShowHiddenChannels)
            .view(),
        switch(tr!("Mark messages read on Discord"), page.sync_reads, motion)
            .description(tr!(
                "Channels you read in Yapfast show as read in the Discord apps too."
            ))
            .on_toggle(Intent::SyncReads)
            .view(),
        switch(tr!("Show me as idle when I'm away"), page.show_idle, motion)
            .description(tr!(
                "After 10 minutes without using Yapfast, as in Discord. Either way, your phone gets notifications while you're away."
            ))
            .on_toggle(Intent::ShowIdle)
            .view(),
        switch(tr!("Hide messages from people you blocked"), page.hide_blocked, motion)
            .description(tr!(
                "Leaves them out entirely, rather than showing a line you can open."
            ))
            .on_toggle(Intent::HideBlocked)
            .view(),
    ]);
    // As Discord's Ignore & Block lists them, only while there are any.
    if !page.blocked.is_empty() {
        parts.push(blocked(page, motion));
    }
    column(parts).spacing(SPACING).width(Length::Fill).boxed()
}

/// `DiscordLinkSettings`: what linking does, what it risks, and the way to
/// start it.
fn unlinked(motion: Motion) -> Element<'static, Intent> {
    column![
        typography::label(tr!(
            "Link your Discord account to see its servers and DMs in Yapfast, and join its calls. The login stays on this computer, in the system's credential store."
        ))
        .width(Length::Fill),
        column![
            warning(),
            action_button(tr!("Show the code"), Kind::Primary, motion).on_press(Intent::Link),
        ]
        .spacing(HEADER),
    ]
    .spacing(SPACING)
    .width(Length::Fill)
    .boxed()
}

/// The risk to the account, in a danger-tinted card.
fn warning() -> Element<'static, Intent> {
    let glyph = icon(Icon::TriangleAlert, WARNING_ICON).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().danger),
    });
    let line = text(tr!(
        "Discord doesn't allow apps like Yapfast on personal accounts. Linking puts your account at some risk of being suspended."
    ))
    .size(typography::size::SMALL)
    .width(Length::Fill)
    .style(catalog::text::primary);
    container(row![glyph, line].spacing(WARNING_GAP))
        .padding(metrics::SPACE_M)
        .width(Length::Fill)
        .style(|theme: &Theme| {
            let danger = theme.palette().danger;
            container::Style {
                background: Some(
                    Color {
                        a: WARNING_FILL,
                        ..danger
                    }
                    .into(),
                ),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(Color {
                        a: WARNING_EDGE,
                        ..danger
                    }),
                ..Default::default()
            }
        })
        .boxed()
}

/// The blocked accounts: a header, five of them at a time, and See more.
fn blocked(page: &Discord, motion: Motion) -> Element<'_, Intent> {
    let count = page.blocked.len();
    let shown = page.blocked_shown.max(BLOCKED_STEP);
    let disc = container(icon(Icon::Ban, GLYPH).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().secondary),
    }))
    .center(PICTURE)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().raised.into()),
        border: Border::default().rounded(PICTURE / 2.0),
        ..Default::default()
    });
    let header = row![
        disc,
        column![
            text(tr!("Blocked accounts"))
                .size(typography::size::TITLE)
                .font(typography::SEMIBOLD)
                .style(catalog::text::primary),
            text(trn!("{n} account", "{n} accounts", count))
                .size(typography::size::SMALL)
                .style(catalog::text::secondary),
        ]
        .spacing(LINES),
    ]
    .spacing(HEADER)
    .align_y(alignment::Vertical::Center)
    .padding(iced::Padding::ZERO.top(ABOVE));
    let mut list = column![header].spacing(BLOCKED);
    list = list.push(
        column(
            page.blocked
                .iter()
                .take(shown)
                .map(|person| person_row(person, motion)),
        )
        .boxed(),
    );
    if count > shown {
        list = list.push(
            link(trn!("See {n} more", "See {n} more", count - shown), motion)
                .small(true)
                .on_press(Intent::MoreBlocked)
                .view(),
        );
    }
    list.boxed()
}

/// A blocked person: their picture, their name over their handle, Unblock.
fn person_row(person: &Blocked, motion: Motion) -> Element<'_, Intent> {
    let mut who = column![
        text(person.name.as_str())
            .size(typography::size::TITLE)
            .font(typography::SEMIBOLD)
            .style(catalog::text::primary)
            .width(Length::Fill)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
    ]
    .spacing(LINES)
    .width(Length::Fill);
    if !person.username.is_empty() {
        who = who.push(
            text(person.username.as_str())
                .size(typography::size::SMALL)
                .style(catalog::text::secondary)
                .width(Length::Fill)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .boxed(),
        );
    }
    container(
        row![
            portrait(&person.avatar, PICTURE).view(),
            who,
            action_button(tr!("Unblock"), Kind::Secondary, motion)
                .small()
                .on_press(Intent::Unblock(person.id.clone())),
        ]
        .spacing(HEADER)
        .align_y(alignment::Vertical::Center),
    )
    .padding(iced::Padding::from([ROW, 0.0]))
    .boxed()
}

/// Its shots for `anomi-shots`: `settings/discord.png` as the Slint app
/// showed it, the page with blocked accounts, and the page without an
/// account.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::discord::{page, unlinked, with_blocked};

    use crate::shots::{GOLDEN, Shot};

    [
        ("discord", page as fn() -> anomi_vm::Vm),
        ("discord_blocked", with_blocked),
        ("discord_unlinked", unlinked),
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
