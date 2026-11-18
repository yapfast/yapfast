//! A message's pictures, videos, files and stickers: the gallery, file
//! cards and stickers of `message_row` (`src/ui/messages.rs:528`-`:662`),
//! laid out as `gallery` lays them (`src/ui/pictures.rs:714`). A preview
//! is the core's own picture, drawn through a handle over its pixels; one on
//! its way keeps its room as a skeleton.

use yapfast_core::chat::{Attachment, ChatMessage, PREVIEW_HEIGHT, PREVIEW_WIDTH, Placement};
use yapfast_lang::tr;

use anomi_vm::chat::attachments::{Attachments, Cell, File, Gallery, Gif, Sticker};
use anomi_vm::chat::video::Video;
use anomi_vm::chat::voice_message::VoiceMessage;
use anomi_vm::kind::Expected;

use super::message::Context;
use crate::pictures;

/// How large Discord draws a message's sticker, whatever its picture's size.
const STICKER_SIZE: u32 = 160;
/// How many bars a voice message's waveform shows, as Discord's client
/// draws it.
const WAVEFORM_BARS: usize = 32;

pub(super) fn attachments(cx: &Context<'_>, message: &ChatMessage, key: u64) -> Attachments {
    let covered = |index: usize| !cx.revealed.contains(&format!("{}/a{index}", message.item));
    Attachments {
        item: key,
        lit: false,
        preview: cx.look.preview,
        undelivered: message.delivery != yapfast_core::chat::Delivery::Sent,
        gallery: gallery(cx, message, key, Placement::Message, &covered),
        files: files(message, key),
        stickers: stickers(message),
    }
}

/// The pictures at `placement` laid out: one at its own size, several as
/// square tiles two to a row (`Gallery::mosaic`).
pub(super) fn gallery(
    cx: &Context<'_>,
    message: &ChatMessage,
    key: u64,
    placement: Placement,
    covered: &dyn Fn(usize) -> bool,
) -> Gallery {
    let shown: Vec<(usize, &Attachment)> = shown(message, placement).collect();
    let cells: Vec<Cell> = shown
        .iter()
        .map(|&(index, attachment)| {
            let (width, height) = size(attachment);
            Cell {
                index,
                name: attachment.name.clone(),
                description: attachment.description.clone().unwrap_or_default(),
                picture: picture(attachment),
                x: 0,
                y: 0,
                width,
                height,
                spoiler: attachment.spoiler && covered(index),
                video: attachment.video.then_some(Video {
                    item: key,
                    index,
                    player: None,
                }),
                gif: attachment.gif.as_ref().map(|gif| Gif {
                    favorite: cx.state.is_favorite_gif(&gif.url),
                }),
            }
        })
        .collect();
    Gallery::mosaic(cells)
}

/// The attachments at `placement` that show as pictures: those with a
/// preview, those whose preview is on its way, and videos, whose poster
/// may never come.
pub(super) fn shown(
    message: &ChatMessage,
    placement: Placement,
) -> impl Iterator<Item = (usize, &Attachment)> {
    message
        .attachments
        .iter()
        .enumerate()
        .filter(move |(_, attachment)| attachment.placement == placement)
        .filter(|(_, attachment)| {
            attachment.preview.is_some() || loading(attachment) || attachment.video
        })
}

/// A picture whose preview is on its way, shown as a skeleton meanwhile.
fn loading(attachment: &Attachment) -> bool {
    (attachment.image || attachment.video)
        && attachment.preview.is_none()
        && !attachment.preview_failed
}

/// A cell's picture: the preview's handle once it came, else a skeleton
/// while it comes, else nothing (a video's poster that never came).
pub(super) fn picture(attachment: &Attachment) -> Expected {
    match attachment.preview.as_ref().and_then(pictures::shown) {
        Some(handle) => Expected::Ready(handle),
        None if loading(attachment) => Expected::Waiting,
        None => Expected::None,
    }
}

/// The size a lone preview shows at, in logical pixels: a preview made for
/// a denser screen has more pixels than it shows at; one on its way takes
/// the room the picture's own size says it will.
pub(super) fn size(attachment: &Attachment) -> (u32, u32) {
    if let Some(preview) = &attachment.preview {
        let logical = |side: u32| {
            (u64::from(side) * 100).div_ceil(u64::from(attachment.preview_percent.max(100))) as u32
        };
        return (logical(preview.width), logical(preview.height));
    }
    let Some((width, height)) = attachment.dimensions else {
        return (PREVIEW_WIDTH, 200);
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(PREVIEW_WIDTH) / f64::from(width.max(1)),
            f64::from(PREVIEW_HEIGHT) / f64::from(height.max(1)),
        ),
    );
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (side(width), side(height))
}

/// The files that show as cards: any but a picture, a video, or one on its
/// way; a voice message's recording plays in its card.
fn files(message: &ChatMessage, key: u64) -> Vec<File> {
    message
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, attachment)| {
            attachment.placement == Placement::Message
                && attachment.preview.is_none()
                && !loading(attachment)
                && !attachment.video
        })
        .map(|(index, attachment)| {
            let size = attachment.size.map(yapfast_core::chat::file_size);
            let detail = match (attachment.image, attachment.preview_failed, size) {
                (true, true, Some(size)) => format!("{size} · {}", tr!("Preview unavailable")),
                (true, true, None) => tr!("Preview unavailable").into(),
                (_, _, Some(size)) => size,
                _ => String::new(),
            };
            // A voice message's recording is its first attachment.
            let voice = message.voice.as_ref().filter(|_| index == 0);
            File {
                index,
                name: attachment.name.clone(),
                detail,
                voice: voice.map(|voice| VoiceMessage {
                    item: key,
                    duration: voice_time(voice.duration_ms),
                    waveform: waveform(&voice.waveform),
                    enabled: message.delivery == yapfast_core::chat::Delivery::Sent,
                    speed: "1x".into(),
                    volume: 1.0,
                    ..VoiceMessage::default()
                }),
            }
        })
        .collect()
}

/// A message's stickers, 160 px square, each a skeleton while it comes.
fn stickers(message: &ChatMessage) -> Vec<Sticker> {
    shown(message, Placement::Sticker)
        .map(|(index, attachment)| Sticker {
            index,
            name: attachment.name.clone(),
            picture: picture(attachment),
            width: STICKER_SIZE,
            height: STICKER_SIZE,
        })
        .collect()
}

/// A voice message's waveform as its card's bars, each the loudest of its
/// stretch, from 0 to 1; flat without one (`waveform_bars`).
fn waveform(waveform: &[u8]) -> Vec<f32> {
    (0..WAVEFORM_BARS)
        .map(|bar| {
            let start = bar * waveform.len() / WAVEFORM_BARS;
            let end = ((bar + 1) * waveform.len() / WAVEFORM_BARS).max(start + 1);
            let loudest = waveform.get(start..end.min(waveform.len()));
            f32::from(loudest.into_iter().flatten().copied().max().unwrap_or(0)) / 255.0
        })
        .collect()
}

/// A voice message's length in whole seconds rounded up, as Discord's are
/// (`voice_time`, `src/ui/refresh.rs:5655`).
fn voice_time(ms: u64) -> String {
    let seconds = ms.div_ceil(1000);
    match seconds / 3600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waveform_shows_its_loudest_and_a_length_rounds_up() {
        let bars = waveform(&[0, 255, 0, 0]);
        assert_eq!(bars.len(), WAVEFORM_BARS);
        assert!(bars.contains(&1.0));
        assert_eq!(waveform(&[]), vec![0.0; WAVEFORM_BARS]);
        assert_eq!(voice_time(2_400), "0:03");
        assert_eq!(voice_time(3_600_000), "1:00:00");
    }
}
