//! A message found: a pinned message, a search's result, or the message a
//! thread starts from. Its author's avatar, name in their role colour and
//! when it was sent, its text (eight lines at most), its first pictures
//! smaller, its poll's card (not to be voted in) and its other files'
//! names. A click on it, or on Jump under the pointer, shows the message.

use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Padding,
    widget::{Space, button, column, container, row, stack},
};
use yapfast_core::chat::{ChatMessage, FOUND_PICTURES, Placement};

use crate::{
    app::{App, Message},
    controls::{self, Kind},
    elide::elide,
    fonts::SEMIBOLD,
    markdown::{FIRST_PART, Look},
    media::{Gallery, Shot},
    pictures::{Face, Icon},
    style::hex,
    timeline::{LINE_HEIGHT, Laid},
};

/// Its pictures' gallery at its widest, its text at its most lines, its
/// avatar.
const GALLERY: f32 = 300.0;
const LINES: f32 = 8.0;
const AVATAR: f32 = 36.0;
/// No timeline's row: its text's pieces select nothing.
const NO_ROW: usize = usize::MAX;

/// A message found and its text laid out, made once as it's found.
#[derive(Clone)]
pub struct Found {
    pub message: Arc<ChatMessage>,
    pub body: Arc<Laid>,
    /// When it was sent, as its card says it.
    pub when: String,
}

/// `message` found, laid out once for its card.
pub fn found(app: &App, message: Arc<ChatMessage>) -> Found {
    found_marked(app, message, &yapfast_core::search_words::Words::default())
}

/// `message` found by a search for `words`, which its text highlights.
pub fn found_marked(
    app: &App,
    message: Arc<ChatMessage>,
    words: &yapfast_core::search_words::Words,
) -> Found {
    let revealed = &app.timeline.revealed;
    let mut body = crate::timeline::body_of(
        &message,
        crate::theme::rgb(app.theme.mention),
        crate::theme::rgb(app.theme.text),
        crate::theme::rgb(app.theme.muted),
        |spoiler| revealed.contains(&format!("{}/{spoiler}", message.item)),
    );
    crate::markdown::highlight(&mut body.body, words);
    let now = yapfast_core::clock::now_ms() / 1000;
    let when = if message.timestamp == 0 {
        String::new()
    } else {
        yapfast_core::clock::sent(
            message.timestamp,
            now,
            app.layout.prefs.twelve_hour_clock,
            false,
        )
    };
    Found {
        message,
        body: Arc::new(body),
        when,
    }
}

/// A found message's card; a click on it sends `jump`.
pub fn found_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    found: &'a Found,
    jump: Message,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let message = &found.message;
    let name_ink = message.color.map_or(theme.text, hex);
    let mut words = column![
        container(
            row![
                elide(message.author.clone())
                    .size(15)
                    .font(SEMIBOLD)
                    .color(name_ink),
                elide(found.when.as_str()).size(12).color(theme.muted),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.right(52))
    ]
    .spacing(2)
    .width(Length::Fill);
    if !message.body.trim().is_empty() {
        let body = ctx.body_view(
            &found.body.body,
            NO_ROW,
            FIRST_PART,
            Look {
                size: 15.0,
                line_height: LINE_HEIGHT,
                ink: theme.text,
            },
        );
        words = words.push(container(body).max_height(LINE_HEIGHT * LINES).clip(true));
    }
    let gallery = Gallery::of(
        &message.attachments[..message.attachments.len().min(FOUND_PICTURES + 4)],
        Placement::Message,
        GALLERY,
    );
    let mut shown = Vec::new();
    if !gallery.is_empty() {
        let mut cells = stack![
            Space::new()
                .width(gallery.size.width)
                .height(gallery.size.height)
        ];
        for &(at, cell) in gallery.cells.iter().take(FOUND_PICTURES) {
            shown.push(at);
            let attachment = &message.attachments[at];
            let shot = ctx.shot(
                Shot::new(attachment.preview.as_ref(), cell.size())
                    .radius(crate::message::PICTURE_RADIUS)
                    .covered(attachment.spoiler),
            );
            let shot: Element<'a, Message> = if attachment.video && !attachment.spoiler {
                stack![
                    shot,
                    container(crate::message::attachments::play_badge(ctx)).center(Length::Fill)
                ]
                .into()
            } else {
                shot
            };
            cells = cells.push(controls::placed(shot, cell.x, cell.y));
        }
        words = words.push(container(cells).padding(Padding::ZERO.top(4)));
    }
    // Nothing is done in it: a click on it goes to the card.
    if let Some(poll) = &message.poll {
        words = words.push(crate::polls::found_view(ctx, message, poll));
    }
    let files: Vec<&str> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|(index, attachment)| {
            attachment.placement == Placement::Message && !shown.contains(index)
        })
        .map(|(_, attachment)| attachment.name.as_str())
        .collect();
    if !files.is_empty() {
        words = words.push(
            row![
                ctx.look.glyph(Icon::File, 14, theme.muted),
                elide(files.join(", ")).size(13).color(theme.muted),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    let face = ctx.face(Face::new(
        message.avatar.as_deref(),
        &message.author,
        AVATAR,
    ));
    let content = row![container(face).align_y(Alignment::Start), words]
        .spacing(12)
        .padding(12);
    let (rest, lit, edge) = (theme.chat, theme.hover, theme.line);
    let card = button(content)
        .padding(0)
        .width(Length::Fill)
        .on_press(jump.clone())
        .style(controls::outlined_lit_style(rest, lit, edge, edge, 8.0));
    let jump_button = container(controls::action(theme, "Jump", Kind::Plain, Some(jump)).small())
        .width(Length::Fill)
        .padding(8)
        .align_x(Alignment::End);
    crate::widgets::Hover::new(card, jump_button).into()
}
