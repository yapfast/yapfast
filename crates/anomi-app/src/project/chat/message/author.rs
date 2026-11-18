//! Who wrote a message and the line over it: the author's picture, name,
//! style and tag (`message_row`'s author fields and `Layout::restyle`'s
//! cosmetics), the message it replies to or the command it answers.

use iced::Color;
use yapfast_core::chat::{ChatMessage, NoticeKind};
use yapfast_core::discord;
use yapfast_lang::tr;

use anomi_vm::chat::message::{Author, Command, Header, Notice, Reply};
use anomi_vm::kind::{Avatar, NameStyle, Tag};

use super::Context;
use crate::pictures;
use crate::project::rows;

/// The palette's neutral grey, a blocked person's disc: it names no one.
fn grey() -> Color {
    Color::from_rgb8(0x64, 0x6b, 0x7a)
}

pub(super) fn author(cx: &Context<'_>, message: &ChatMessage, blocked: bool) -> Author {
    let state = cx.state;
    if blocked {
        return Author {
            name: tr!("Blocked user").to_owned(),
            avatar: Avatar {
                initial: "?".into(),
                tint: grey(),
                picture: None,
            },
            ..Author::default()
        };
    }
    let cosmetics = cx.look.cosmetics;
    let style = cosmetics
        .style(message.style.as_deref())
        .and_then(|style| NameStyle::new(style.font, style.effect, style.colors()))
        // In a server, an author's name shows its style's font alone.
        .map(|style| {
            if cx.look.in_server {
                style.font_only()
            } else {
                style
            }
        });
    Author {
        name: message.author.clone(),
        avatar: rows::avatar(
            state,
            &message.author,
            &message.author,
            message.avatar.as_deref(),
        ),
        decoration: pictures::find(state, cosmetics.decoration(message.decoration.as_deref())),
        color: message.color.map(rows::rgb),
        style,
        tag: cosmetics.tag(message.tag.as_ref()).map(|tag| Tag {
            text: tag.text.to_string(),
            badge: rows::expected(state, tag.badge.as_deref()),
            server: tag.guild.map(|guild| guild.to_string()).unwrap_or_default(),
        }),
        bot: message.bot,
        known: !message.sender.is_empty(),
    }
}

/// The message it replies to, or the command it answers when it quotes
/// none.
pub(super) fn header(cx: &Context<'_>, message: &ChatMessage, blocked: bool) -> Header {
    let state = cx.state;
    if blocked {
        return Header::None;
    }
    if let Some(reply) = &message.reply {
        return Header::Reply(Box::new(Reply {
            author: reply.author.clone(),
            avatar: rows::avatar(state, &reply.author, &reply.author, reply.avatar.as_deref()),
            color: reply.color.map(rows::rgb),
            body: reply.body.clone(),
            jumps: reply.item.as_deref().is_some_and(|item| !item.is_empty()),
        }));
    }
    let Some(used) = &message.used else {
        return Header::None;
    };
    Header::Command(Box::new(Command {
        user: used.user.clone(),
        avatar: rows::avatar(state, &used.user, &used.user, used.avatar.as_deref()),
        color: used.color.map(rows::rgb),
        known: discord::sender_user(&used.sender).is_some_and(|user| !user.is_empty()),
        name: used.name.clone(),
        usable: used.slash.is_some() && cx.look.writes,
        target: used.target.clone(),
    }))
}

/// The icon a notice shows in place of its author's picture.
pub(super) fn notice(message: &ChatMessage) -> Option<Notice> {
    Some(match &message.notice.as_ref()?.kind {
        NoticeKind::Pinned(_) => Notice::Pin,
        NoticeKind::Joined => Notice::Join,
        NoticeKind::ThreadStarted => Notice::Thread,
        NoticeKind::Call => Notice::Call,
        NoticeKind::PollResult => Notice::Poll,
    })
}
