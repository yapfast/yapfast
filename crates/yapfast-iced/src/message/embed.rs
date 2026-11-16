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

use super::{
    EMBED_BAR, EMBED_PADDING, EMBED_PARTS, EMBED_WIDTH, PICTURE_RADIUS, THUMBNAIL, reveal_key,
};

/// What a message whose only content is a card says while link previews
/// are off, as the Slint client says it.
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

/// A link preview or bot card, as the Slint client's `EmbedView`:
/// behind a bar of its colour, its site, author, title (a link to its
/// page), description, fields (inline ones sharing a row), its large
/// picture and its footer.
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
        + crate::select::PartNo::try_from(place * 16)
            .unwrap_or(0)
            .min(EMBED_PARTS - 16);
    let mut card = column![].spacing(8).width(Length::Fill);
    if !embed.provider.is_empty() {
        card = card.push(text(embed.provider.as_str()).size(12).color(theme.muted));
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
        card = card.push(if embed.url.is_empty() {
            Element::from(title.color(theme.text))
        } else {
            button(title.color(theme.link))
                .padding(0)
                .on_press(Message::Link(embed.url.clone(), Point::ORIGIN))
                .style(|_, _| button::Style::default())
                .into()
        });
    }
    if !laid.description.parts.is_empty() && !embed.description.is_empty() {
        card = card.push(ctx.body_view(
            &laid.description,
            index,
            first,
            small(14.0, theme.secondary),
        ));
    }
    // Inline fields share a row, up to three; others have one each.
    let mut fields = column![].spacing(8);
    let mut shared = row![].spacing(8);
    let mut in_row = 0;
    for ((name, value), field) in laid.fields.iter().zip(&embed.fields) {
        let cell = column![
            ctx.body_view(name, index, first, small(14.0, theme.text)),
            ctx.body_view(value, index, first, small(14.0, theme.secondary)),
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
        card = card.push(crate::message::attachments::gallery(
            ctx, index, message, &gallery,
        ));
    }
    if !embed.footer.is_empty() {
        card = card.push(text(embed.footer.as_str()).size(12).color(theme.muted));
    }
    let mut beside = row![card].spacing(16);
    if let Some(at) = thumbnail {
        let attachment = &message.attachments[at];
        let size = media::coming_size(attachment);
        let scale = (THUMBNAIL / size.width.max(size.height)).min(1.0);
        let size = Size::new((size.width * scale).round(), (size.height * scale).round());
        beside = beside.push(
            ctx.shot(
                Shot::new(attachment.preview.as_ref(), size)
                    .fit(Fit::Contain)
                    .radius(PICTURE_RADIUS)
                    .covered(
                        attachment.spoiler
                            && !ctx
                                .lanes
                                .timeline
                                .revealed
                                .contains(&reveal_key(&message.item, at)),
                    ),
            ),
        );
    }
    let bar_ink = embed.color.map_or(theme.edge, hex);
    let ground = theme.sidebar;
    let shown = row![
        controls::bar(bar_ink, EMBED_BAR, Length::Fill),
        container(beside).padding(Padding {
            top: 12.0,
            bottom: 12.0,
            left: 12.0,
            right: 16.0,
        })
    ]
    .height(Length::Shrink);
    container(
        container(shown)
            .max_width(EMBED_WIDTH)
            .clip(true)
            .style(move |_| rounded(ground, 4)),
    )
    .padding(Padding::ZERO.top(4))
    .into()
}
