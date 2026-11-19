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
    fonts::{BOLD, MEDIUM},
    media::{self, Fit, Gallery, Shot},
    pictures::Icon,
    style::rounded,
};

use super::{FILE_CARD_WIDTH, FILE_ICON, PLAY_BADGE, STICKER, reveal_key};

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

/// The round play mark over a video's poster, before it plays.
pub fn play_badge<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    container(ctx.look.glyph(Icon::PlayMark, 20, Color::WHITE))
        .center(PLAY_BADGE)
        .style(|_| rounded(crate::style::SMOKE, PLAY_BADGE / 2.0))
        .into()
}

/// A message's pictures laid out as its gallery ([`Gallery`]): each in
/// its cell with round corners, a spoiler covered until clicked; on the
/// message under the pointer, a Save button on each, and a star on a GIF
/// under the pointer.
pub fn gallery<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    gallery: &Gallery,
) -> Element<'a, Message> {
    cells(ctx, index, message, gallery, None)
}

/// A link preview's pictures laid out as [`gallery`] does, without Save
/// or a star: a video's poster (one that doesn't play here, such as
/// YouTube's) shows a play mark and opens the video's page.
pub fn embed_gallery<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    gallery: &Gallery,
    embed: &'a chat::Embed,
) -> Element<'a, Message> {
    cells(ctx, index, message, gallery, Some(embed))
}

fn cells<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &'a ChatMessage,
    gallery: &Gallery,
    embed: Option<&'a chat::Embed>,
) -> Element<'a, Message> {
    let mut cells = stack![
        Space::new()
            .width(gallery.size.width)
            .height(gallery.size.height)
    ];
    let page = embed
        .filter(|embed| embed.video && !embed.url.is_empty())
        .map(|embed| embed.url.as_str());
    // Save and the star are for the message's own pictures, once sent.
    let own = embed.is_none()
        && !ctx.state.preview
        && !message.deleted
        && message.delivery == chat::Delivery::default();
    let pointed = own && ctx.pointed.message.as_deref() == Some(message.item.as_str());
    for &(at, cell) in &gallery.cells {
        let attachment = &message.attachments[at];
        let covered = covered(ctx, message, at);
        let shot = ctx.shot(
            Shot::new(attachment.preview.as_ref(), cell.size())
                .radius(CELL_RADIUS)
                .covered(covered),
        );
        let mut shot: Element<'a, Message> = if covered {
            pressed(ctx, index, message, at, shot)
        } else if attachment.video && !media::loading(attachment) && !ctx.state.preview {
            media_press(
                message,
                at,
                true,
                crate::playback::clip_cell(ctx, message, at, cell.size(), shot),
            )
        } else if attachment.video {
            stack![shot, container(play_badge(ctx)).center(Length::Fill)].into()
        } else if let Some(page) = page {
            let poster = mouse_area(stack![
                shot,
                container(play_badge(ctx)).center(Length::Fill)
            ])
            .on_press(Message::Link(page.to_owned(), iced::Point::ORIGIN))
            .interaction(iced::mouse::Interaction::Pointer);
            media_press(message, at, false, poster)
        } else {
            pressed(ctx, index, message, at, shot)
        };
        if pointed && !covered {
            let save = ctx
                .look
                .icon_button(
                    Icon::Download,
                    format!("Save {}", attachment.name),
                    Message::Image(crate::image_viewer::Event::Save(message.item.clone(), at)),
                )
                .size(SAVE_BUTTON, 20)
                .ink(Color::WHITE)
                .fill(crate::style::SMOKE);
            shot = stack![
                shot,
                container(save)
                    .padding(PICTURE_INSET)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            ]
            .into();
        }
        // Discord marks an animated picture's corner with GIF while it
        // isn't playing (animations off, or the window not in front) and
        // the message isn't pointed at, when the favourite star takes its
        // place.
        if !ctx.plays
            && !pointed
            && !covered
            && !attachment.video
            && !media::loading(attachment)
            && attachment.gif.is_some()
        {
            shot = stack![shot, container(gif_badge()).padding(6)].into();
        }
        // Built for the message under the pointer alone: the star looks
        // its GIF up among the favourites.
        if own
            && pointed
            && !covered
            && let Some(gif) = &attachment.gif
        {
            shot = iced::widget::hover(shot, favorite_star(ctx, message, at, gif));
        }
        cells = cells.push(placed(shot, cell.x, cell.y));
    }
    cells.into()
}

/// A Save button's side over a picture, and its inset from the corner.
const SAVE_BUTTON: f32 = 32.0;
const PICTURE_INSET: f32 = 8.0;
/// A gallery picture's corners, as Discord's client rounds them.
const CELL_RADIUS: f32 = 4.0;

/// The mark in an animated picture's top left corner: "GIF" on a dark
/// pill, 29 by 22.
fn gif_badge<'a>() -> Element<'a, Message> {
    container(text("GIF").size(13).font(BOLD).color(Color::WHITE))
        .center_x(29)
        .center_y(22)
        .style(|_| rounded(Color::from_rgba8(0x00, 0x02, 0x05, 0xb3 as f32 / 255.0), 4))
        .into()
}

/// A message's GIF's star in its top left corner, filled while it's among
/// the favourite GIFs: a click adds it or takes it out. The pointer coming
/// onto it asks for the favourites, if they haven't come.
fn favorite_star<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &ChatMessage,
    at: usize,
    gif: &chat::Gif,
) -> Element<'a, Message> {
    use crate::image_viewer::Event;
    let theme = ctx.look.theme;
    let (icon, ink, label) = if ctx.state.is_favorite_gif(&gif.url) {
        (
            Icon::StarFilled,
            crate::style::hex(crate::style::CROWN),
            "Remove from Favorites",
        )
    } else {
        (Icon::Star, theme.secondary, "Add to Favorites")
    };
    let star = ctx
        .look
        .face_button(
            ctx.look.glyph(icon, 20, ink),
            SAVE_BUTTON,
            label,
            Message::Image(Event::FavoriteGif(message.item.clone(), at)),
        )
        .fill(theme.panel)
        .radius(5.0);
    container(mouse_area(star).on_enter(Message::Image(Event::WantFavorites)))
        .padding(PICTURE_INSET)
        .into()
}

/// Whether a message's attachment `at` is a spoiler not yet uncovered.
pub fn covered(ctx: crate::ctx::Ctx<'_>, message: &ChatMessage, at: usize) -> bool {
    message.attachments[at].spoiler
        && !ctx
            .lanes
            .timeline
            .revealed
            .contains(&reveal_key(&message.item, at))
}

/// A message's picture `at`, drawn as `shot`: while covered a click
/// uncovers it; else a click shows it large, and a right click opens the
/// message's menu with the picture's rows.
pub fn pressed<'a>(
    ctx: crate::ctx::Ctx<'a>,
    index: usize,
    message: &ChatMessage,
    at: usize,
    shot: Element<'a, Message>,
) -> Element<'a, Message> {
    if covered(ctx, message, at) {
        mouse_area(shot)
            .on_press(Message::Timeline(crate::timeline::Event::Reveal(
                ctx.row_item(index),
                at,
            )))
            .interaction(iced::mouse::Interaction::Pointer)
            .into()
    } else if message.attachments[at].image {
        let opened = mouse_area(shot)
            .on_press(Message::Image(crate::image_viewer::Event::Open(
                message.item.clone(),
                at,
            )))
            .interaction(iced::mouse::Interaction::Pointer);
        media_press(message, at, false, opened)
    } else {
        shot
    }
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

/// A file's card: its icon in the accent, its name and size (and why a
/// picture shows no preview), and Save, which asks where.
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
