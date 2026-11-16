//! What a message carries besides its text: pictures and videos in their
//! gallery, stickers, and files as cards.

use iced::{
    Alignment, Color, Element, Length, Padding, Size,
    widget::{Space, column, container, mouse_area, row, stack, text},
};
use yapfast_core::chat::{self, Attachment, ChatMessage, Placement};

use crate::{
    app::Message,
    controls::placed,
    fonts::MEDIUM,
    media::{self, Fit, Gallery, Shot},
    pictures::Icon,
    style::rounded,
};

use super::{FILE_CARD_WIDTH, FILE_ICON, PICTURE_RADIUS, PLAY_BADGE, STICKER, reveal_key};

/// A message's files shown as cards in `placement`, with their places
/// among its attachments.
pub fn files(
    message: &ChatMessage,
    placement: Placement,
) -> impl Iterator<Item = (usize, &Attachment)> {
    message
        .attachments
        .iter()
        .enumerate()
        .filter(move |(_, attachment)| {
            attachment.placement == placement && !media::pictured(attachment)
        })
}

/// The round play mark over a video's poster, as the Slint client's
/// `VideoCell` before it plays.
pub fn play_badge<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    container(ctx.look.glyph(Icon::PlayMark, 20, Color::WHITE))
        .center(PLAY_BADGE)
        .style(|_| rounded(crate::style::SMOKE, PLAY_BADGE / 2.0))
        .into()
}

/// A message's pictures laid out as its gallery ([`Gallery`]): each in
/// its cell with round corners, a spoiler covered until clicked.
pub fn gallery<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    gallery: &Gallery,
) -> Element<'a, Message> {
    let mut cells = stack![
        Space::new()
            .width(gallery.size.width)
            .height(gallery.size.height)
    ];
    for &(at, cell) in &gallery.cells {
        let attachment = &message.attachments[at];
        let covered = attachment.spoiler
            && !ctx
                .lanes
                .timeline
                .revealed
                .contains(&reveal_key(&message.item, at));
        let shot = ctx.shot(
            Shot::new(attachment.preview.as_ref(), cell.size())
                .radius(PICTURE_RADIUS)
                .covered(covered),
        );
        let shot: Element<'a, Message> = if covered {
            mouse_area(shot)
                .on_press(Message::Timeline(crate::timeline::Event::Reveal(
                    ctx.row_item(index),
                    at,
                )))
                .interaction(iced::mouse::Interaction::Pointer)
                .into()
        } else if attachment.video && !media::loading(attachment) && !ctx.state.preview {
            media_press(
                message,
                at,
                true,
                crate::playback::clip_cell(ctx, message, at, cell.size(), shot),
            )
        } else if attachment.video {
            stack![shot, container(play_badge(ctx)).center(Length::Fill)].into()
        } else if attachment.image {
            let opened = mouse_area(shot)
                .on_press(Message::Image(crate::image_viewer::Event::Open(
                    message.item.clone(),
                    at,
                )))
                .interaction(iced::mouse::Interaction::Pointer);
            media_press(message, at, false, opened)
        } else {
            shot
        };
        cells = cells.push(placed(shot, cell.x, cell.y));
    }
    cells.into()
}

/// A message's picture or video that a right click opens the
/// message's menu on, with what can be done with it.
pub(super) fn media_press<'a>(
    message: &ChatMessage,
    at: usize,
    video: bool,
    shown: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if message.deleted || message.delivery != chat::Delivery::default() {
        return shown.into();
    }
    let item = message.item.clone();
    crate::widgets::RightPress::new(shown, move |_| {
        Message::Image(crate::image_viewer::Event::Menu(item.clone(), at, video))
    })
    .into()
}

/// A message's Discord stickers, each on its own at Discord's size,
/// moving ones playing; none when it has none.
pub(super) fn stickers<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
) -> Option<Element<'a, Message>> {
    let mut stickers = row![].spacing(8);
    let mut any = false;
    for (index, attachment) in message
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, attachment)| attachment.placement == Placement::Sticker)
    {
        any = true;
        let size = Size::new(STICKER, STICKER);
        let shown = match attachment.preview.as_ref() {
            Some(picture) => ctx.shot(Shot::new(Some(picture), size).fit(Fit::Contain).bare()),
            None => ctx.shot(Shot::new(None, size).radius(12.0)),
        };
        // A click opens its card under it.
        let open = Message::Info(crate::info_cards::Event::Sticker(
            message.item.clone(),
            index,
        ));
        let pressable = iced::widget::mouse_area(shown)
            .on_press(open)
            .interaction(iced::mouse::Interaction::Pointer);
        stickers = stickers.push(crate::widgets::Spot::new(pressable, ctx.link_spot).by_press());
    }
    any.then(|| stickers.into())
}

/// A file's card, as the Slint client's `FileCard`: its icon in the
/// accent, its name and size (and why a picture shows no preview), and
/// Save, which asks where.
pub fn file_card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    item: &str,
    index: usize,
    attachment: &'a Attachment,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let size = attachment.size.map(chat::file_size);
    let detail = match (attachment.image, attachment.preview_failed, size) {
        (true, true, Some(size)) => format!("{size} · Preview unavailable"),
        (true, true, None) => "Preview unavailable".to_owned(),
        (_, _, Some(size)) => size,
        _ => String::new(),
    };
    let mut words = column![
        text(attachment.name.as_str())
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
    ];
    if !detail.is_empty() {
        words = words.push(text(detail).size(12).color(theme.muted));
    }
    let card = row![
        ctx.look.glyph(Icon::File, FILE_ICON, theme.accent),
        container(words).width(Length::Fill).clip(true),
        ctx.look
            .icon_button(
                Icon::Download,
                "Save",
                Message::Image(crate::image_viewer::Event::Save(item.to_owned(), index))
            )
            .size(32.0, 20),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let (ground, edge) = (theme.sidebar, theme.edge);
    container(card)
        .padding(Padding {
            top: 12.0,
            bottom: 12.0,
            left: 12.0,
            right: 8.0,
        })
        .max_width(FILE_CARD_WIDTH)
        .style(move |_| crate::style::outlined(ground, edge, 8.0))
        .into()
}
