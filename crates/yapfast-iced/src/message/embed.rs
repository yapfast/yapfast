//! A link's or a bot's embed, as Discord's client draws one: its colour
//! bar, author, title, text, fields, pictures and footer.

use iced::{
    Color, Element, Length, Padding, Point, Size,
    widget::{button, column, container, row, text},
};
use yapfast_core::chat::{self, ChatMessage, Placement};

use crate::{
    app::Message,
    controls,
    fonts::{MEDIUM, SEMIBOLD},
    markdown::Look,
    media::{self, Fit, Gallery, Shot},
    style::{hex, rounded},
    timeline::EmbedText,
};

use super::{EMBED_BAR, EMBED_PADDING, EMBED_PARTS, EMBED_WIDTH, PICTURE_RADIUS, THUMBNAIL};

/// What a message whose only content is a card says while link previews are
/// off.
pub(super) fn hidden_card(message: &ChatMessage) -> Option<String> {
    let embed = message.embeds.first()?;
    let pictures = message.attachments.iter().any(|attachment| {
        matches!(
            attachment.placement,
            Placement::Message | Placement::Sticker
        )
    });
    if !message.body.trim().is_empty() || pictures {
        return None;
    }
    Some(if embed.title.is_empty() {
        "A card, hidden while link previews are off.".to_owned()
    } else {
        format!(
            "A card, \"{}\", hidden while link previews are off.",
            embed.title
        )
    })
}

/// A link preview or bot card: behind a bar of its colour, its site,
/// author, title (a link to its page), description, fields (inline ones
/// sharing a row), its large picture and its footer.
pub(super) fn embed<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    place: usize,
    embed: &'a chat::Embed,
    laid: &'a EmbedText,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let small = |size: f32, ink: Color| Look {
        size,
        line_height: (size * 1.3).round(),
        ink,
    };
    // Its text isn't selected with the message's.
    let first = EMBED_PARTS
        + crate::piece::PartNo::try_from(place * 16)
            .unwrap_or(0)
            .min(EMBED_PARTS - 16);
    let mut card = column![].spacing(8).width(Length::Fill);
    if !embed.provider.is_empty() {
        card = card.push(
            container(text(embed.provider.as_str()).size(12).color(theme.text))
                .padding(Padding::ZERO.bottom(1)),
        );
    }
    if !embed.author.is_empty() {
        card = card.push(
            text(embed.author.as_str())
                .size(14)
                .font(MEDIUM)
                .color(theme.text),
        );
    }
    if !embed.title.is_empty() {
        let title = text(embed.title.as_str()).size(16).font(SEMIBOLD);
        card = card.push(
            container(if embed.url.is_empty() {
                Element::from(title.color(theme.text))
            } else {
                button(title.color(theme.link))
                    .padding(0)
                    .on_press(Message::Link(embed.url.clone(), Point::ORIGIN))
                    .style(|_, _| button::Style::default())
                    .into()
            })
            .padding(Padding::ZERO.bottom(2)),
        );
    }
    if !laid.description.parts.is_empty() && !embed.description.is_empty() {
        card = card.push(ctx.body_view(&laid.description, index, first, small(14.0, theme.text)));
    }
    // Inline fields share a row, up to three; others have one each.
    let mut fields = column![].spacing(8);
    let mut shared = row![].spacing(8);
    let mut in_row = 0;
    for ((name, value), field) in laid.fields.iter().zip(&embed.fields) {
        let cell = column![
            ctx.body_view(name, index, first, small(14.0, theme.text)),
            ctx.body_view(value, index, first, small(14.0, theme.text)),
        ]
        .spacing(2)
        .width(Length::Fill);
        if !field.inline || in_row == 3 {
            if in_row > 0 {
                fields = fields.push(std::mem::replace(&mut shared, row![].spacing(8)));
                in_row = 0;
            }
            if !field.inline {
                fields = fields.push(cell);
                continue;
            }
        }
        shared = shared.push(cell);
        in_row += 1;
    }
    if in_row > 0 {
        fields = fields.push(shared);
    }
    if !embed.fields.is_empty() {
        card = card.push(fields);
    }
    let thumbnail = message
        .attachments
        .iter()
        .position(|attachment| attachment.placement == Placement::Thumbnail(place));
    // Its padding, and a thumbnail beside the text, take from the room.
    let room = (crate::message::content_width_of(ctx, index).min(EMBED_WIDTH)
        - EMBED_PADDING
        - if thumbnail.is_some() {
            THUMBNAIL + 16.0
        } else {
            0.0
        })
    .max(1.0);
    let gallery = Gallery::of(&message.attachments, Placement::Embed(place), room);
    if !gallery.is_empty() {
        // Discord leaves 16px above a picture, 8 of them the spacing and,
        // under a title, 2 its own.
        card = card.push(
            container(crate::message::attachments::embed_gallery(
                ctx, index, message, &gallery, embed,
            ))
            .padding(Padding::ZERO.top(if embed.title.is_empty() { 8 } else { 5 })),
        );
    }
    if !embed.footer.is_empty() {
        card = card.push(text(embed.footer.as_str()).size(12).color(theme.text));
    }
    let mut beside = row![card].spacing(16);
    if let Some(at) = thumbnail {
        let attachment = &message.attachments[at];
        let size = media::coming_size(attachment);
        let scale = (THUMBNAIL / size.width.max(size.height)).min(1.0);
        let size = Size::new((size.width * scale).round(), (size.height * scale).round());
        let shot = ctx.shot(
            Shot::new(attachment.preview.as_ref(), size)
                .fit(Fit::Contain)
                .radius(PICTURE_RADIUS)
                .covered(crate::message::attachments::covered(ctx, message, at)),
        );
        beside = beside.push(crate::message::attachments::pressed(
            ctx, index, message, at, shot,
        ));
    }
    // Discord's dark card has a 1px line round it, in a step above its
    // ground, that its bar covers on the left.
    let (bar_ink, ground, line) = if theme.bonfire {
        (
            embed.color.map_or(hex(0x3a3a40), hex),
            hex(0x242429),
            Some(hex(0x313137)),
        )
    } else {
        (embed.color.map_or(theme.edge, hex), theme.sidebar, None)
    };
    let inset = if line.is_some() { 1.0 } else { 0.0 };
    let body = container(
        container(beside)
            .padding(Padding {
                top: 10.0,
                bottom: 16.0,
                left: 12.0,
                right: 16.0,
            })
            .style(move |_| rounded(ground, 0)),
    )
    .padding(Padding {
        top: inset,
        bottom: inset,
        right: inset,
        left: 0.0,
    });
    let shown = row![controls::bar(bar_ink, EMBED_BAR, Length::Fill), body].height(Length::Shrink);
    let edge = line.unwrap_or(ground);
    // Discord leaves 3px under an embed, measured against its next message.
    container(
        container(shown)
            .max_width(EMBED_WIDTH)
            .clip(true)
            .style(move |_| rounded(edge, 4)),
    )
    .padding(Padding::ZERO.top(4).bottom(3))
    .into()
}
