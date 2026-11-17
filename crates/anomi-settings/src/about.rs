//! The About settings page, with updates: which build this is and whether a
//! newer release is out. The check is a switch, off until it's turned on; a
//! release installs only when Update and restart is pressed. The Slint side:
//! `AboutSettings` (`ui/settings.slint:1962`) and `src/ui/updates.rs`.

mod bar;

use std::borrow::Cow;

use iced::Length::{Fill, Shrink};
use iced::widget::{column, container, row, text};
use iced::{Border, Center};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::about::{About, Intent, Status};
use anomi_vm::settings::frame::Tone;
use anomi_widgets::brand;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::list::scroll_view;
use anomi_widgets::picture::picture;
use yapfast_lang::tr;

use crate::kit::{heading, switch};

/// Between the page's blocks, and between a heading and what is under it.
const SPACING: f32 = 16.0;
const TIGHT: f32 = 8.0;
/// The build card: its padding and corners, the logo's side and corners.
const CARD_PAD: f32 = 16.0;
const CARD_RADIUS: f32 = 8.0;
const LOGO: f32 = 56.0;
const LOGO_RADIUS: f32 = 14.0;
/// The release notes' box: the room round its text and its tallest height.
const NOTES_PAD: f32 = 12.0;
const NOTES_MAX: f32 = 220.0;

/// A line of explanation, muted unless `ink` says otherwise.
fn line<'a>(
    words: impl text::IntoFragment<'a>,
    ink: fn(&Theme) -> text::Style,
) -> Element<'a, Intent> {
    text(words)
        .size(typography::size::LABEL)
        .style(ink)
        .width(Fill)
        .boxed()
}

/// What the check found, in words; empty when there is nothing to say.
fn status(about: &About) -> Cow<'static, str> {
    match about.status {
        Status::Checking => tr!("Checking for updates…").into(),
        Status::Current => tr!("You're up to date.").into(),
        Status::Available => tr!("Yapfast {} is available.", about.latest).into(),
        Status::Skipped => tr!(
            "You skipped {}. We'll tell you about the next one.",
            about.latest
        )
        .into(),
        Status::Failed => tr!("Couldn't check for updates: {}", about.error).into(),
        Status::Source => {
            tr!("You're running a build from source, so there's nothing to update.").into()
        }
        Status::Idle if about.check => "".into(),
        Status::Idle => tr!("Update checks are turned off.").into(),
    }
}

/// The accent, for a release that is out.
fn accent(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().accent),
        ..text::Style::default()
    }
}

/// The card: the logo, the build and the button that copies it.
fn card(about: &About, motion: Motion) -> Element<'_, Intent> {
    let build = &about.build;
    let tone = build.tone;
    let mut second = row![
        text(&build.channel)
            .size(typography::size::LABEL)
            .font(typography::BOLD)
            .style(move |theme: &Theme| {
                let palette = theme.palette();
                text::Style {
                    color: Some(match tone {
                        Tone::Release => palette.live,
                        Tone::Nightly => palette.accent,
                        Tone::Development => palette.danger_text(),
                    }),
                    ..text::Style::default()
                }
            })
            .wrapping(text::Wrapping::None)
            .boxed(),
    ];
    if !build.details.is_empty() {
        second = second.push(
            text(format!(" · {}", build.details))
                .size(typography::size::LABEL)
                .style(catalog::text::muted)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .boxed(),
        );
    }
    let copy = if about.copied {
        tr!("Copied")
    } else {
        tr!("Copy version")
    };
    container(
        row![
            picture(brand::icon())
                .side(LOGO)
                .radius(LOGO_RADIUS)
                .boxed(),
            column![
                text(&build.headline)
                    .size(typography::size::DIALOG_TITLE)
                    .font(typography::BOLD)
                    .style(catalog::text::primary)
                    .wrapping(text::Wrapping::None)
                    .ellipsis(text::Ellipsis::End),
                second,
            ]
            .spacing(4.0)
            .width(Fill),
            action_button(copy, Kind::Secondary, motion).on_press(Intent::CopyVersion),
        ]
        .spacing(SPACING)
        .align_y(Center),
    )
    .padding(CARD_PAD)
    .width(Fill)
    .style(|theme: &Theme| container::Style {
        background: Some(theme.palette().sidebar.into()),
        border: Border::default().rounded(CARD_RADIUS),
        ..container::Style::default()
    })
    .boxed()
}

/// Under UPDATES: what is being installed, what is found, and the buttons.
fn updates(about: &About, motion: Motion) -> Element<'_, Intent> {
    let mut parts: Vec<Element<'_, Intent>> = vec![heading(tr!("UPDATES")).boxed()];
    let available = about.status == Status::Available;
    if !about.releases {
        parts.push(line(
            tr!(
                "This is {0}, a fork of {1}. The fork has no releases: it's updated by building it again from its source code, so {0} doesn't look for updates and downloads nothing.",
                about.product,
                "Yapfast"
            ),
            catalog::text::muted,
        ));
    }
    if about.releases && !about.manager.is_empty() {
        parts.push(line(
            tr!(
                "You installed Yapfast with {}, so update it there.",
                about.manager
            ),
            catalog::text::muted,
        ));
    }
    let said = status(about);
    if about.releases && !said.is_empty() {
        let ink: fn(&Theme) -> text::Style = match about.status {
            Status::Available => accent,
            Status::Failed => catalog::text::danger,
            _ => catalog::text::muted,
        };
        parts.push(line(said, ink));
    }
    if let Some(install) = &about.installing {
        let mut block: Vec<Element<'_, Intent>> = vec![
            text(&install.text)
                .size(typography::size::LABEL)
                .style(catalog::text::primary)
                .boxed(),
            bar::bar(install.progress.unwrap_or(0.0), motion).boxed(),
        ];
        if install.can_cancel {
            block.push(
                action_button(tr!("Cancel"), Kind::Secondary, motion)
                    .on_press(Intent::CancelInstall)
                    .boxed(),
            );
        }
        parts.push(column(block).spacing(TIGHT).boxed());
    } else {
        if !about.install_error.is_empty() {
            parts.push(line(
                tr!("Couldn't update: {}", about.install_error),
                catalog::text::danger,
            ));
        }
        if available && !about.install_note.is_empty() {
            parts.push(line(&about.install_note, catalog::text::muted));
        }
        if about.releases {
            parts.push(buttons(about, motion));
        }
    }
    column(parts).spacing(TIGHT).width(Fill).boxed()
}

/// The buttons, which take a second line in a narrow window.
fn buttons(about: &About, motion: Motion) -> Element<'_, Intent> {
    let available = about.status == Status::Available;
    let mut buttons: Vec<Element<'_, Intent>> = Vec::new();
    if available && about.can_install {
        buttons.push(
            action_button(tr!("Update and restart"), Kind::Primary, motion)
                .leading(Icon::Download)
                .on_press(Intent::Install)
                .boxed(),
        );
    }
    if available || about.status == Status::Skipped {
        let kind = if available && !about.can_install {
            Kind::Primary
        } else {
            Kind::Secondary
        };
        buttons.push(
            action_button(tr!("View release"), kind, motion)
                .leading(Icon::ExternalLink)
                .on_press(Intent::ViewRelease)
                .boxed(),
        );
    }
    if available {
        buttons.push(
            action_button(tr!("Skip this version"), Kind::Secondary, motion)
                .on_press(Intent::SkipVersion)
                .boxed(),
        );
    }
    buttons.push(
        action_button(tr!("Check now"), Kind::Secondary, motion)
            .leading(Icon::RefreshCw)
            .enabled(about.status != Status::Checking)
            .on_press(Intent::CheckNow)
            .boxed(),
    );
    row(buttons)
        .spacing(TIGHT)
        .wrap()
        .vertical_spacing(TIGHT)
        .boxed()
}

/// The release's notes in a box that scrolls past its tallest height.
fn notes(about: &About, motion: Motion) -> Element<'_, Intent> {
    let words = text(&about.notes)
        .size(typography::size::LABEL)
        .style(catalog::text::secondary)
        .width(Fill);
    let title = tr!("WHAT'S NEW IN {}", about.latest.to_uppercase());
    column![
        heading(title).boxed(),
        container(
            scroll_view(container(words).padding(NOTES_PAD), motion).height(Shrink.max(NOTES_MAX))
        )
        .width(Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sidebar.into()),
            border: Border::default().rounded(CARD_RADIUS),
            ..container::Style::default()
        }),
    ]
    .spacing(TIGHT)
    .boxed()
}

pub fn view(about: &About, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts: Vec<Element<'_, Intent>> = vec![card(about, motion), updates(about, motion)];
    let shows_notes = matches!(about.status, Status::Available | Status::Skipped);
    if shows_notes && !about.notes.is_empty() {
        parts.push(notes(about, motion));
    }
    if about.releases {
        parts.push(
            switch(tr!("Check for updates"), about.check, motion)
                .description(tr!("Checks for new versions every 6 hours."))
                .fresh(about.fresh_check)
                .on_toggle(Intent::Check)
                .view(),
        );
        parts.push(
            switch(tr!("Include nightly builds"), about.nightlies, motion)
                .description(tr!(
                    "Get the newest features before release. Expect some bugs."
                ))
                .fresh(about.fresh_nightlies)
                .enabled(about.check)
                .on_toggle(Intent::Nightlies)
                .view(),
        );
        parts.push(
            switch(tr!("Install updates automatically"), false, motion)
                .description(tr!("Not available yet."))
                .enabled(false)
                .view(),
        );
    }
    let links = row![
        action_button("GitHub", Kind::Secondary, motion)
            .leading(Icon::Github)
            .on_press(Intent::OpenGithub)
            .boxed(),
        action_button(&about.original, Kind::Secondary, motion)
            .leading(Icon::ExternalLink)
            .on_press(Intent::OpenWebsite)
            .boxed(),
    ];
    parts.push(
        column![
            heading(tr!("LINKS")).boxed(),
            links.spacing(TIGHT).wrap().vertical_spacing(TIGHT).boxed(),
        ]
        .spacing(TIGHT)
        .boxed(),
    );
    column(parts).spacing(SPACING).width(Fill)
}

/// Its shots for `anomi-shots`: `settings/about.png` as the Slint app showed
/// it (a fork without releases), and a release build with an update ready, one
/// downloading and one that failed to check.
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::fixtures::settings::about::{
        page, page_available, page_failed, page_installing, page_releases,
    };

    use crate::shots::{GOLDEN, Shot};

    [
        ("about", page as fn() -> anomi_vm::Vm),
        ("about_releases", page_releases),
        ("about_available", page_available),
        ("about_installing", page_installing),
        ("about_failed", page_failed),
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
