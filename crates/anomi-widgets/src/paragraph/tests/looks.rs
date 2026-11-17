//! The samples drawn, for a reviewer to hold beside the golden screenshots
//! (`chan/history_plain_a.png`, `chan/showcase_emoji_stickers_gif.png`,
//! `chan/spoiler_reply_blocked_linkembed.png`, `chan/bot_embed_card_poll.png`):
//! with `ANOMI_PARAGRAPH_LOOKS` naming a folder, each theme's PNG goes there,
//! at twice the size. The paragraph has no screen of its own until the
//! message (C02) places it, so these stand in for its shots.

use iced::widget::{column, container};
use iced::{Color, Length};

use anomi_vm::fixtures::text as samples;

use super::*;

/// The samples in a column as wide as the golden shots' messages, on the
/// conversation's ground; then an embed's title at 16 pixels, its
/// description and fields at 14 and dimmer, as `EmbedView` shows them
/// (`ui/messages.slint:2131`, `:2139`).
fn showcase<'a>(messages: &'a [Text], embeds: &'a [Text]) -> Element<'a, Asked> {
    let mut list = column(messages.iter().map(|text| linked(text).boxed())).spacing(6);
    if let Some((title, rest)) = embeds.split_first() {
        list = list.push(linked(title).size(16.0).boxed());
        for text in rest {
            list = list.push(
                linked(text)
                    .size(14.0)
                    .color(|palette| palette.secondary)
                    .heading_color(|palette| palette.text)
                    .boxed(),
            );
        }
    }
    container(list.width(672.0))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().chat.into()),
            text_color: Some(Color::WHITE),
            ..container::Style::default()
        })
        .boxed()
}

#[test]
fn the_samples_draw_for_review() {
    let Some(folder) = std::env::var_os("ANOMI_PARAGRAPH_LOOKS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let (messages, embeds) = (samples::messages(), samples::embeds());
    for id in ["bonfire", "catppuccin-latte"] {
        let theme = Theme::built_in(Some(id));
        let view = showcase(&messages, &embeds);
        let mut ui = Simulator::with_size(settings(), Size::new(720.0, 860.0), view);
        let path = folder.join(format!("paragraph-{id}.png"));
        // The simulator writes it under its renderer's name, only when missing.
        let _ = std::fs::remove_file(folder.join(format!("paragraph-{id}-tiny-skia.png")));
        let snapshot = ui.snapshot(&theme).expect("draws");
        assert!(snapshot.matches_image(&path).expect("writes"));
    }
}
