//! The Notifications settings page: three switches, each with what it does.
//! The Slint side: `NotificationSettings` (`ui/settings.slint:1786`), a
//! column 16 px apart; the page's title is the frame's bar's.

use iced::Length;
use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::settings::notifications::{Intent, Notifications};
use yapfast_lang::tr;

use crate::kit::switch;

/// Between the switches (`spacing: 16px`).
const SPACING: f32 = 16.0;

/// What decides which messages notify, by the networks in use.
fn description(page: &Notifications) -> &'static str {
    match (page.matrix, page.discord) {
        (true, true) => tr!(
            "For new messages while Yapfast isn't in front. Each Discord server's notification settings and your Matrix notification rules decide which: usually DMs and mentions. Clicking one opens the conversation."
        ),
        (true, false) => tr!(
            "For new messages while Yapfast isn't in front. Your Matrix notification rules decide which: usually DMs and mentions. Clicking one opens the conversation."
        ),
        _ => tr!(
            "For new messages while Yapfast isn't in front. Each Discord server's notification settings decide which: usually DMs and mentions. Clicking one opens the conversation."
        ),
    }
}

pub fn view(page: &Notifications, motion: Motion) -> impl View<Intent> + '_ {
    column![
        switch(tr!("Show desktop notifications"), page.notifications, motion)
            .description(description(page))
            .on_toggle(Intent::Notifications)
            .view(),
        switch(tr!("Show what messages say"), page.text, motion)
            .description(tr!(
                "Off shows only who wrote and where, for a screen others can see."
            ))
            .enabled(page.notifications)
            .on_toggle(Intent::Text)
            .view(),
        switch(tr!("Flash the taskbar"), page.flash_taskbar, motion)
            .description(tr!(
                "For a message that notifies while Yapfast isn't in front. The window's title counts mentions and DMs waiting either way."
            ))
            .on_toggle(Intent::FlashTaskbar)
            .view(),
    ]
    .spacing(SPACING)
    .width(Length::Fill)
}

/// Its shots for `anomi-shots`: `settings/notifications.png` as the Slint app
/// showed it, with the first switch off and with both networks in use.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::notifications::{both, off, page};

    use crate::shots::{GOLDEN, Shot};

    [
        ("notifications", page as fn() -> anomi_vm::Vm),
        ("notifications_off", off),
        ("notifications_both", both),
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
