//! `MentionList`'s rows: members, then roles, each under its heading, or
//! channels (`ui/composer.slint:119`-`:161`).

use iced::widget::{container, svg, text};
use iced::{Border, Color, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::suggestions::{Intent, Mark, Mention};
use anomi_vm::icon::Icon;
use anomi_widgets::avatar::circle;
use anomi_widgets::icon::icon;

use super::heading;
use super::row::{choice, mark};

/// A person's avatar and a group's round mark are 24 px; a sigil is 13 px
/// bold and a voice channel's speaker 14 px.
const MARK: f32 = 24.0;
const SIGIL: f32 = 13.0;
const SPEAKER: f32 = 14.0;
/// A group's mark is its colour at this alpha (`with-alpha(0.2)`).
const TINT: f32 = 0.2;

pub fn rows(mentions: &[Mention], chosen: usize) -> Vec<Element<'_, Intent>> {
    let mut out = Vec::with_capacity(mentions.len());
    for (index, mention) in mentions.iter().enumerate() {
        if !mention.heading.is_empty() {
            out.push(heading(mention.heading.as_str()));
        }
        out.push(choice(
            index,
            index == chosen,
            [face(mention), name(mention), detail(mention)],
        ));
    }
    out
}

/// The mark before the name: an avatar, or a round tinted mark.
/// A round 24 px mark in the colour's tint.
fn round(color: Option<Color>, inside: Element<'_, Intent>) -> Element<'_, Intent> {
    container(inside)
        .center(MARK)
        .style(move |theme: &Theme| {
            let base = color.unwrap_or(theme.palette().muted);
            container::Style {
                background: Some(base.scale_alpha(TINT).into()),
                border: Border::default().rounded(MARK / 2.0),
                ..container::Style::default()
            }
        })
        .boxed()
}

fn face(mention: &Mention) -> Element<'_, Intent> {
    let color = mention.color;
    match &mention.mark {
        Mark::Person(avatar) => mark(circle(avatar, MARK).boxed(), MARK),
        Mark::Group(sigil) => round(
            color,
            text(sigil.as_str())
                .size(SIGIL)
                .font(typography::BOLD)
                .style(move |theme: &Theme| text::Style {
                    color: Some(color.unwrap_or(theme.palette().secondary)),
                    ..text::Style::default()
                })
                .boxed(),
        ),
        Mark::Voice => round(
            color,
            icon(Icon::Volume, SPEAKER)
                .style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().secondary),
                })
                .boxed(),
        ),
    }
}

/// The name, in its role's colour where it has one.
fn name(mention: &Mention) -> Element<'_, Intent> {
    let color: Option<Color> = mention.color;
    text(mention.name.as_str())
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(move |theme: &Theme| text::Style {
            color: Some(color.unwrap_or(theme.palette().text)),
            ..text::Style::default()
        })
        .boxed()
}

/// Their username, or what a group reaches, filling the rest.
fn detail(mention: &Mention) -> Element<'_, Intent> {
    text(mention.detail.as_str())
        .size(typography::size::LABEL)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Fill)
        .style(catalog::text::muted)
        .boxed()
}
