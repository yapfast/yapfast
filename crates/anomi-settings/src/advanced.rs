//! The Advanced settings page: switches for how the program runs, with the
//! restart a changed GPU setting waits for. The Slint side:
//! `AdvancedSettings` (`ui/settings.slint:1881`), a column 16 px apart; the
//! page's title is the frame's bar's.

use iced::Length::Fill;
use iced::widget::{column, row, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::advanced::{Advanced, Intent};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use crate::kit::switch;

/// Between the switches (`spacing: 16px`).
const SPACING: f32 = 16.0;
/// Between the restart line's words and its button (`spacing: 12px`).
const RESTART_GAP: f32 = 12.0;

/// The line and button that appear while the GPU choice differs from what runs.
fn restart(page: &Advanced, motion: Motion) -> Element<'_, Intent> {
    let words = if page.gpu_rendering {
        tr!("Yapfast switches to the GPU when it restarts. Restarting leaves any call.")
    } else {
        tr!("Yapfast switches to the processor when it restarts. Restarting leaves any call.")
    };
    row![
        text(words)
            .size(typography::size::LABEL)
            .style(catalog::text::muted)
            .width(Fill),
        action_button(tr!("Restart Yapfast"), Kind::Secondary, motion)
            .leading(Icon::RefreshCw)
            .on_press(Intent::Restart),
    ]
    .spacing(RESTART_GAP)
    .align_y(iced::Center)
    .boxed()
}

pub fn view(page: &Advanced, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = Vec::new();
    if page.autostart_available {
        parts.push(
            switch(tr!("Start when you sign in"), page.start_on_sign_in, motion)
                .description(tr!(
                    "Yapfast starts in the system tray when you sign in to your computer, so messages and calls reach you."
                ))
                .on_toggle(Intent::StartOnSignIn)
                .view(),
        );
    }
    if page.discord_links_available {
        parts.push(
            switch(tr!("Open Discord links in Yapfast"), page.discord_links, motion)
                .description(tr!(
                    "discord:// links from other apps and shortcuts open here instead of Discord. Turning it off gives them back to the app that had them."
                ))
                .on_toggle(Intent::DiscordLinks)
                .view(),
        );
    }
    let tray = if page.tray_available {
        tr!(
            "Closing the window keeps Yapfast running in the system tray, with any call. Quit from the tray icon's menu."
        )
    } else {
        tr!("No system tray is running, so closing the window quits Yapfast.")
    };
    parts.push(
        switch(tr!("Close to tray"), page.close_to_tray, motion)
            .description(tray)
            .on_toggle(Intent::CloseToTray)
            .view(),
    );
    parts.push(
        switch(tr!("Allow multiple windows"), page.multiple_windows, motion)
            .description(tr!(
                "Starting Yapfast while it runs opens another Yapfast, with its own window and tray icon. Off, it brings the open window forward."
            ))
            .on_toggle(Intent::MultipleWindows)
            .view(),
    );
    parts.push(
        switch(tr!("Render with the GPU"), page.gpu_rendering, motion)
            .description(tr!(
                "Hardware acceleration. Off, Yapfast draws on the processor, which uses about half the memory."
            ))
            .on_toggle(Intent::GpuRendering)
            .view(),
    );
    if page.gpu_rendering != page.gpu_running {
        parts.push(restart(page, motion));
    }
    parts.push(
        switch(tr!("Encode streams with the GPU"), page.gpu_encoding, motion)
            .description(tr!(
                "Screen sharing uses the graphics card's video encoder, which takes almost no processor time and looks better at low bitrates. Off, or without one, Yapfast encodes on the processor."
            ))
            .on_toggle(Intent::GpuEncoding)
            .view(),
    );
    parts.push(
        switch(tr!("Developer Mode"), page.developer_mode, motion)
            .description(tr!(
                "Menus offer Copy ID for servers, channels and people, which helps when you write apps that use the Discord API."
            ))
            .on_toggle(Intent::DeveloperMode)
            .view(),
    );
    parts.push(
        text(tr!(
            "Icons from Solar by 480 Design (CC BY 4.0), many of them adapted, and from Lucide (ISC)."
        ))
        .size(typography::size::CAPTION)
        .style(catalog::text::faint)
        .boxed(),
    );
    column(parts).spacing(SPACING).width(Fill)
}

/// Its shots for `anomi-shots`: `settings/advanced.png` as the Slint app
/// showed it, with the restart line and without the system's extras.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::advanced::{bare, page, restart};

    use crate::shots::{GOLDEN, Shot};

    [
        ("advanced", page as fn() -> anomi_vm::Vm),
        ("advanced_restart", restart),
        ("advanced_bare", bare),
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
