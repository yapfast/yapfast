//! What the sections end with: a line saying an update is available and the
//! two lines that say which build is running, such as a stable release or a
//! nightly, so a report says whether it has a feature; a click on them
//! copies the build as one line. The Slint side: `UpdateNotice` and
//! `BuildStamp` (`ui/yapfast/build-info.slint:78`, `:108`).

use iced::widget::{column, container, rule, stack, text};
use iced::{Color, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::settings::frame::{Build, Frame, Intent, Tone};
use yapfast_lang::tr;

use crate::kit::pressable;

/// The notice's height and the inset of what's inside it, its ground's
/// corners and its text (`:80`, `:87`, `:93`).
const NOTICE: f32 = 36.0;
const NOTICE_SIDES: f32 = 16.0;
const NOTICE_TOP: f32 = 4.0;
const NOTICE_RADIUS: f32 = 4.0;
const NOTICE_PAD: f32 = 12.0;
/// The accent's ground at rest and under the pointer (`:88`).
const NOTICE_REST: f32 = 0.14;
const NOTICE_HOT: f32 = 0.22;
/// The stamp's height, its inner part's insets and corners (`:112`, `:126`).
const STAMP: f32 = 62.0;
const STAMP_SIDES: f32 = 16.0;
const STAMP_TOP: f32 = 8.0;
const STAMP_RADIUS: f32 = 4.0;
/// The words' insets and the gap between the two lines (`:129`).
const WORDS_SIDES: f32 = 8.0;
const WORDS_TOP: f32 = 4.0;
const LINES: f32 = 4.0;
const SIZE: f32 = typography::size::CAPTION;

/// The notice: where `latest` is the version waiting. A click goes to
/// About, which says more.
pub fn update<'a>(latest: &str, motion: Motion) -> Element<'a, Intent> {
    use crate::kit::{Ink, Leaf};
    let mut leaf = Leaf::new(
        tr!("Update available · {}", latest),
        motion,
        |theme: &Theme, lit: f32| {
            let palette = theme.palette();
            let alpha = NOTICE_REST + (NOTICE_HOT - NOTICE_REST) * lit;
            Ink {
                fill: palette.accent.scale_alpha(alpha),
                text: palette.accent,
                glyph: palette.accent,
            }
        },
    )
    .on_press(Intent::Choose(anomi_vm::settings::frame::Page::About));
    leaf.size = typography::size::LABEL;
    leaf.font = typography::BOLD;
    leaf.pad = NOTICE_PAD;
    leaf.height = NOTICE - 2.0 * NOTICE_TOP;
    leaf.stretch = true;
    leaf.radius = NOTICE_RADIUS;
    container(leaf.boxed())
        .padding(Padding {
            top: NOTICE_TOP,
            bottom: NOTICE_TOP,
            left: NOTICE_SIDES,
            right: NOTICE_SIDES,
        })
        .width(Length::Fill)
        .height(NOTICE)
        .boxed()
}

/// The build's two lines, none while the build isn't known.
pub fn stamp<'a>(frame: &'a Frame, motion: Motion) -> Option<Element<'a, Intent>> {
    let build = frame
        .build
        .as_ref()
        .filter(|build| !build.headline.is_empty())?;
    let tint = move |theme: &Theme| {
        let palette = theme.palette();
        match build.tone {
            Tone::Release => palette.live,
            Tone::Nightly => palette.accent,
            Tone::Development => palette.danger_text(),
        }
    };
    let words = column![headline(build), second(build, frame.copied, tint)]
        .spacing(LINES)
        .padding(Padding {
            left: WORDS_SIDES,
            right: WORDS_SIDES,
            top: WORDS_TOP,
            bottom: 0.0,
        });
    let button = pressable(
        words.boxed(),
        build.headline.clone(),
        Intent::Copy(build.summary.clone()),
        STAMP_RADIUS,
        |theme: &Theme, lit: f32| {
            let palette = theme.palette();
            crate::kit::mix(palette.muted, palette.secondary, lit)
        },
        motion,
    );
    let inner = container(button).height(Length::Fill).padding(Padding {
        left: STAMP_SIDES,
        right: STAMP_SIDES,
        top: STAMP_TOP,
        bottom: STAMP_TOP,
    });
    let line = container(rule::horizontal(1).style(catalog::rule::line))
        .width(Length::Fill)
        .padding(Padding {
            left: STAMP_SIDES,
            right: STAMP_SIDES,
            ..Padding::ZERO
        });
    Some(
        container(stack![inner, line])
            .width(Length::Fill)
            .height(STAMP)
            .clip(true)
            .boxed(),
    )
}

fn headline(build: &Build) -> Element<'_, Intent> {
    text(build.headline.as_str())
        .size(SIZE)
        .wrapping(text::Wrapping::None)
        .style(catalog::text::inherit)
        .boxed()
}

/// "Development build · 2026-11-16", or "Copied" for a moment after a click.
fn second<'a>(
    build: &'a Build,
    copied: bool,
    tint: impl Fn(&Theme) -> Color + 'a,
) -> Element<'a, Intent> {
    let name = if copied {
        text(tr!("Copied")).style(catalog::text::primary as fn(&Theme) -> text::Style)
    } else {
        text(build.channel.as_str()).style(move |theme: &Theme| text::Style {
            color: Some(tint(theme)),
            selection: None,
        })
    }
    .size(SIZE)
    .font(typography::BOLD)
    .wrapping(text::Wrapping::None);
    let mut parts: Vec<Element<'a, Intent>> = vec![name.boxed()];
    if !copied && !build.details.is_empty() {
        parts.push(
            text(format!(" · {}", build.details))
                .size(SIZE)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::inherit)
                .boxed(),
        );
    }
    iced::widget::row(parts).boxed()
}
