//! A server's profile page: its name, icon and description. Task X01's
//! (docs/iced/tasks/settings.md). The Slint side: `ServerProfileSection`
//! (`ui/server_settings.slint:100`).

use iced::widget::{column, row, text};
use iced::{Alignment, Length};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::settings::server::{Intent, Server};
use anomi_widgets::avatar::portrait;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

use crate::kit::{area, heading};

/// The icon's side, the gap beside it and between the lines of the page.
const ICON: f32 = 80.0;
const BESIDE: f32 = 16.0;
const GAP: f32 = 16.0;
const TIGHT: f32 = 8.0;

/// A line of explanation under a heading or beside a picture (Slint's
/// `Label`: 13 px, muted).
fn label<'a>(words: &'a str) -> Element<'a, Intent> {
    text(words)
        .size(typography::size::HEADING)
        .style(catalog::text::muted)
        .width(Length::Fill)
        .boxed()
}

pub fn view(server: &Server, motion: Motion) -> Element<'_, Intent> {
    let profile = &server.profile;
    let ready = !server.saving;

    let mut name = field(tr!("Server name"), profile.name.as_str()).motion(motion);
    if ready {
        name = name.on_input(Intent::Name);
    }

    let hint = if profile.reading {
        tr!("Reading the picture...")
    } else {
        tr!("We recommend an image of at least 512x512.")
    };
    let mut buttons = row![
        action_button(tr!("Change Server Icon"), Kind::Secondary, motion)
            .enabled(ready && !profile.reading)
            .on_press(Intent::PickIcon)
            .boxed()
    ]
    .spacing(TIGHT);
    if profile.has_icon {
        buttons = buttons.push(
            action_button(tr!("Remove Icon"), Kind::Ghost, motion)
                .enabled(ready)
                .on_press(Intent::RemoveIcon)
                .boxed(),
        );
    }
    let icon = row![
        portrait(&profile.icon, ICON).motion(motion).view(),
        column![label(hint), buttons]
            .spacing(TIGHT)
            .width(Length::Fill)
    ]
    .spacing(BESIDE)
    .align_y(Alignment::Center);

    let mut description = area(profile.description.as_str(), motion)
        .placeholder(tr!("Tell people what your server is about."))
        .left(Some(profile.description_left));
    if ready {
        description = description.on_edit(Intent::Description);
    }

    column![
        label(tr!(
            "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages."
        )),
        column![heading(tr!("ServerProfileSection" => "NAME")), name.view()].spacing(TIGHT),
        column![heading(tr!("ICON")), icon].spacing(TIGHT),
        column![heading(tr!("DESCRIPTION")), description.view()].spacing(TIGHT),
    ]
    .spacing(GAP)
    .boxed()
}
