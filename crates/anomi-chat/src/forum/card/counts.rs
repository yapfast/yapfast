//! The card's last line: its top reaction, the replies and the time
//! (`ui/forum.slint:166`-`:188`).

use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::forum::{Intent, Post, Reaction};
use anomi_vm::icon::Icon;

use super::{CHIP_RADIUS, CHIP_SIDE, COUNTS_LINE, GLYPH, INNER, glyph, mark};

/// The top reaction, the replies and the time (`:166`-`:188`).
pub(super) fn counts<'a>(post: &'a Post, motion: Motion) -> impl View<Intent> + 'a {
    let small = |content: &'a str| {
        text(content)
            .size(typography::size::LABEL)
            .wrapping(text::Wrapping::None)
    };
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    if let Some(reaction) = &post.reaction {
        items.push(chip(reaction, motion).boxed());
    }
    items.push(glyph(Icon::MessageCircle));
    items.push(
        small(&post.replies)
            .font(typography::SEMIBOLD)
            .style(catalog::text::muted)
            .boxed(),
    );
    items.push(small("\u{2022}").style(catalog::text::faint).boxed());
    items.push(small(&post.time).style(catalog::text::muted).boxed());
    row(items)
        .spacing(metrics::SPACE_S)
        .align_y(Alignment::Center)
        .height(COUNTS_LINE)
}

/// The reaction's chip (`:169`-`:179`).
fn chip<'a>(reaction: &'a Reaction, motion: Motion) -> impl View<Intent> + 'a {
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    items.extend(mark(&reaction.mark, GLYPH, motion));
    if !reaction.count.is_empty() {
        items.push(
            text(reaction.count.as_str())
                .size(typography::size::LABEL)
                .font(typography::SEMIBOLD)
                .style(catalog::text::secondary)
                .boxed(),
        );
    }
    container(row(items).spacing(INNER).align_y(Alignment::Center))
        .padding([0.0, CHIP_SIDE])
        .height(COUNTS_LINE)
        .center_y(COUNTS_LINE)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.active)),
                border: Border {
                    color: palette.line,
                    width: 1.0,
                    radius: CHIP_RADIUS.into(),
                },
                ..container::Style::default()
            }
        })
}
