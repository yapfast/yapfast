//! A server's Engagement page: where system messages go and which, how
//! members who set nothing are notified, and where idle members move. Task
//! X02's (docs/iced/tasks/settings.md). The Slint side: `EngagementSection`
//! (`ui/server_settings.slint:159`).

use iced::widget::{column, row};
use iced::{Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::settings::engagement::{Engagement, Intent, Switch};
use yapfast_lang::tr;

use crate::kit::{choices, heading, picker, switch, words};

/// The gap between the page's parts, and inside one.
const GAP: f32 = 16.0;
const TIGHT: f32 = 8.0;

/// A line of explanation under a heading.
fn label(words: &str) -> Element<'_, Intent> {
    typography::label(words).width(Length::Fill).boxed()
}

/// One of the system messages' switches: on only with a system channel.
fn toggle<'a>(
    words: &'a str,
    on: bool,
    which: Switch,
    enabled: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut one = switch(words, on, motion).enabled(enabled);
    if enabled {
        one = one.on_toggle(move |_| Intent::Toggle(which));
    }
    one.view()
}

/// A list of channels or timeouts, off while Discord takes a change.
fn channels<'a, S: AsRef<str>>(
    options: &'a [S],
    chosen: usize,
    enabled: bool,
    pick: fn(usize) -> Intent,
) -> Element<'a, Intent> {
    picker(options, chosen, enabled.then_some(pick))
}

pub fn view(page: &Engagement, motion: Motion) -> impl View<Intent> + '_ {
    let ready = !page.saving;
    let system = page.system_channel > 0;
    let on = ready && system;
    let timeouts = words(&[
        tr!("1 minute"),
        tr!("5 minutes"),
        tr!("15 minutes"),
        tr!("30 minutes"),
        tr!("1 hour"),
    ]);

    let mut notifications = choices(
        words(&[tr!("All Messages"), tr!("Only @mentions")]),
        page.notifications,
        motion,
    );
    if ready {
        notifications = notifications.on_pick(Intent::Notifications);
    }

    column![
        column![
            heading(tr!("SYSTEM MESSAGES CHANNEL")),
            label(tr!(
                "This is the channel we send system event messages to. These can be turned off at any time."
            )),
            channels(
                &page.system_channels,
                page.system_channel,
                ready,
                Intent::SystemChannel
            ),
        ]
        .spacing(TIGHT),
        toggle(
            tr!("Send a random welcome message when someone joins this server."),
            page.welcome,
            Switch::Welcome,
            on,
            motion
        ),
        toggle(
            tr!("Prompt members to reply to welcome messages with a sticker."),
            page.welcome_sticker,
            Switch::WelcomeSticker,
            on,
            motion
        ),
        toggle(
            tr!("Send a message when someone boosts this server."),
            page.boosted,
            Switch::Boosted,
            on,
            motion
        ),
        toggle(
            tr!("Send helpful tips for server setup."),
            page.setup_tips,
            Switch::SetupTips,
            on,
            motion
        ),
        column![
            heading(tr!("DEFAULT NOTIFICATION SETTINGS")),
            label(tr!(
                "This will determine whether members who have not explicitly set their notification settings receive a notification for every message sent in this server or not."
            )),
            notifications.view(),
        ]
        .spacing(TIGHT),
        column![
            heading(tr!("INACTIVE CHANNEL")),
            label(tr!(
                "Automatically move members to this channel and mute them when they have been idle for longer than the inactive timeout. This does not affect browsers."
            )),
            row![
                // Slint's stretch 2 to 1 shares only the room left over the
                // pickers' own widths: 428 and 252 px on the page.
                iced::widget::container(channels(
                    &page.afk_channels,
                    page.afk_channel,
                    ready,
                    Intent::AfkChannel
                ))
                .width(Length::FillPortion(17)),
                iced::widget::container(channels(
                    timeouts,
                    page.afk_timeout,
                    ready && page.afk_channel > 0,
                    Intent::AfkTimeout
                ))
                .width(Length::FillPortion(10)),
            ]
            .spacing(GAP)
            .align_y(alignment::Vertical::Center),
        ]
        .spacing(TIGHT),
    ]
    .spacing(GAP)
}

/// Its shots for `anomi-shots`: the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::engagement::{page, unset};
    vec![
        Shot {
            area: "ssettings",
            name: "engagement",
            size: GOLDEN,
            theme: "bonfire",
            vm: page,
        },
        Shot {
            area: "ssettings",
            name: "engagement_unset",
            size: GOLDEN,
            theme: "bonfire",
            vm: unset,
        },
    ]
}
