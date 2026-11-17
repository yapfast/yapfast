//! A server's Boost Perks page: the progress bar's switch, the banner and
//! the invite background, each at the level that unlocks it. Task X02's
//! (docs/iced/tasks/settings.md). The Slint side: `BoostSection` and
//! `BoostBackground` (`ui/server_settings.slint:280`, `:237`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Border, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::boosts::{Background, Boosts, Intent, Picture};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::picture::{Fit, picture};
use yapfast_lang::tr;

use crate::kit::{heading, switch};

/// A background's well: 240 by 135, the 16 by 9 it is shown at.
const WIDTH: f32 = 240.0;
const HEIGHT: f32 = 135.0;
const RADIUS: f32 = 8.0;
/// The gaps: between the page's parts, inside one, beside the well.
const GAP: f32 = 16.0;
const TIGHT: f32 = 8.0;
/// The room kept beside the words in an empty well.
const INSET: f32 = 16.0;

/// The well: the picture covering it, or words on the sunken ground.
fn well<'a>(shown: &'a Picture, motion: Motion) -> Element<'a, Intent> {
    let inside: Element<'a, Intent> = match &shown.picture {
        Some(handle) => picture(handle.clone())
            .width(WIDTH)
            .height(HEIGHT)
            .radius(RADIUS)
            .fit(Fit::Cover)
            .motion(motion)
            .boxed(),
        None => {
            let words: &str = if shown.locked.is_empty() {
                tr!("No image")
            } else {
                &shown.locked
            };
            container(
                text(words)
                    .size(typography::size::LABEL)
                    .font(typography::SEMIBOLD)
                    .style(catalog::text::muted)
                    .align_x(alignment::Horizontal::Center)
                    .width(Length::Fill),
            )
            .padding([0.0, INSET / 2.0])
            .center_y(Length::Fill)
            .boxed()
        }
    };
    container(inside)
        .width(WIDTH)
        .height(HEIGHT)
        .clip(true)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.sunken.into()),
                border: Border::default()
                    .rounded(RADIUS)
                    .width(1)
                    .color(palette.line),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// A background: its heading, what it asks for, the well, and Upload and
/// Remove beside it.
fn background<'a>(
    which: Background,
    heading_text: &'a str,
    words: &'a str,
    shown: &'a Picture,
    enabled: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut buttons = column![
        action_button(tr!("Upload Background"), Kind::Secondary, motion)
            .enabled(enabled && shown.locked.is_empty())
            .on_press(Intent::Pick(which))
            .boxed()
    ]
    .spacing(TIGHT);
    if shown.shown {
        buttons = buttons.push(
            action_button(tr!("Remove Background"), Kind::Ghost, motion)
                .enabled(enabled)
                .on_press(Intent::Remove(which))
                .boxed(),
        );
    }
    column![
        heading(heading_text),
        typography::label(words).width(Length::Fill),
        row![well(shown, motion), buttons]
            .spacing(GAP)
            .align_y(Alignment::Center),
    ]
    .spacing(TIGHT)
    .boxed()
}

pub fn view(page: &Boosts, motion: Motion) -> impl View<Intent> + '_ {
    let mut bar = switch(tr!("Show Boost progress bar"), page.progress_bar, motion)
        .description(tr!(
            "This progress bar will display in your channel list, attached to your server name (or server banner if you have one set)."
        ))
        .enabled(!page.saving);
    if !page.saving {
        bar = bar.on_toggle(|_| Intent::ToggleProgressBar);
    }
    let ready = !page.saving && !page.reading;
    column![
        bar.view(),
        background(
            Background::Banner,
            tr!("SERVER BANNER BACKGROUND"),
            tr!("The recommended minimum size is 960x540 and recommended aspect ratio is 16:9."),
            &page.banner,
            ready,
            motion
        ),
        background(
            Background::Invite,
            tr!("SERVER INVITE BACKGROUND"),
            tr!(
                "Servers who reach Boosting Level 1 and above get unique invite embeds with custom Server Invite Backgrounds. The recommended minimum size is 1920x1080 and recommended aspect ratio is 16:9."
            ),
            &page.invite,
            ready,
            motion
        ),
    ]
    .spacing(GAP)
}

/// Its shots for `anomi-shots`: the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::settings::boosts::{level_two, page};
    vec![
        Shot {
            area: "ssettings",
            name: "boosts",
            size: GOLDEN,
            theme: "bonfire",
            vm: page,
        },
        Shot {
            area: "ssettings",
            name: "boosts_level_two",
            size: GOLDEN,
            theme: "bonfire",
            vm: level_two,
        },
    ]
}
