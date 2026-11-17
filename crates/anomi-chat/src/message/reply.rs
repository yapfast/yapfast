//! The line over a message: the message it replies to, quoted, or the
//! command an app's answer answers. The Slint side: the reply's and the
//! command's lines of `MessageContent` (`ui/messages.slint:3137`-`:3199`).
//! A click on the quote jumps to the quoted message; on who used the
//! command, their card; on a slash command, it goes in the composer.

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, mouse_area, row, text};
use iced::{Alignment, Fill};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::chat::message::{Command, Header, Intent, Message, Reply, Who};
use anomi_vm::chat::{self};
use anomi_vm::menu::Spot;
use anomi_widgets::avatar;
use anomi_widgets::menu::opener;
use yapfast_lang::tr;

use super::author::HEADER;

/// The quoted author's picture (`:3145`, `:3167`).
const AVATAR: f32 = 16.0;
/// The reply's parts 6 px apart, the command's 4 (`:3144`, `:3166`).
const REPLY_GAP: f32 = 6.0;
const COMMAND_GAP: f32 = 4.0;
/// Names and words at 14 px; the quote at 13, as Discord's (`:3150`, `:3153`).
const WORDS: f32 = 14.0;
const QUOTE: f32 = 13.0;

fn ask(intent: Intent) -> chat::Intent {
    chat::Intent::from(intent)
}

/// The line over `message`, when it has one.
pub fn view(message: &Message) -> Option<Element<'_, chat::Intent>> {
    match &message.header {
        Header::None => None,
        Header::Reply(reply) => Some(quote(message, reply)),
        Header::Command(command) => Some(used(message, command)),
    }
}

/// "@Name what they said", the name in their role's colour, the quote
/// lighting up under the pointer.
fn quote<'a>(message: &'a Message, reply: &'a Reply) -> Element<'a, chat::Intent> {
    let mut parts = row![].spacing(REPLY_GAP).align_y(Alignment::Center);
    if !reply.author.is_empty() {
        if !message.compact {
            parts = parts.push(avatar::circle(&reply.avatar, AVATAR).boxed());
        }
        let color = reply.color;
        parts = parts.push(
            text(format!("@{}", reply.author))
                .size(WORDS)
                .wrapping(Wrapping::None)
                .style(move |theme: &Theme| text::Style {
                    color: Some(color.unwrap_or(theme.palette().secondary)),
                    ..text::Style::default()
                })
                .boxed(),
        );
    }
    // The quote takes the button's colour, which lights under the pointer.
    parts = parts.push(
        text(reply.body.as_str())
            .size(QUOTE)
            .wrapping(Wrapping::None)
            .ellipsis(Ellipsis::End)
            .width(Fill)
            .boxed(),
    );
    let key = message.key;
    button(parts.height(HEADER))
        .padding(0)
        .width(Fill)
        .on_press_maybe(reply.jumps.then(|| ask(Intent::Quoted(key))))
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    palette.text
                } else {
                    palette.secondary
                },
                ..button::Style::default()
            }
        })
        .boxed()
}

/// "Ann used /ping on @Bob": who opens their card, the command goes in the
/// composer where it may.
fn used<'a>(message: &'a Message, command: &'a Command) -> Element<'a, chat::Intent> {
    let key = message.key;
    let mut parts = row![].spacing(COMMAND_GAP).align_y(Alignment::Center);
    if !message.compact {
        parts = parts.push(avatar::circle(&command.avatar, AVATAR).boxed());
    }
    let color = command.color;
    let user = text(command.user.as_str())
        .size(WORDS)
        .font(typography::MEDIUM)
        .wrapping(Wrapping::None)
        .style(move |theme: &Theme| text::Style {
            color: Some(color.unwrap_or(theme.palette().secondary)),
            ..text::Style::default()
        });
    parts = parts.push(if command.known {
        let pointing = mouse_area(user).interaction(iced::mouse::Interaction::Pointer);
        opener(pointing)
            .on_press(move |at| ask(Intent::Card(key, Who::Commander, Spot::beside(at))))
            .boxed()
    } else {
        user.boxed()
    });
    parts = parts.push(
        text(tr!("used"))
            .size(WORDS)
            .style(catalog::text::muted)
            .boxed(),
    );
    let name = text(command.name.as_str())
        .size(WORDS)
        .font(typography::MEDIUM)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End);
    parts = parts.push(
        button(name)
            .padding(0)
            .on_press_maybe(command.usable.then(|| ask(Intent::UseCommand(key))))
            .style(|theme: &Theme, status| {
                let palette = theme.palette();
                button::Style {
                    text_color: if matches!(
                        status,
                        button::Status::Hovered | button::Status::Pressed
                    ) {
                        palette.text
                    } else {
                        palette.link
                    },
                    ..button::Style::default()
                }
            })
            .boxed(),
    );
    if let Some(target) = &command.target {
        parts = parts.push(
            text(tr!("on @{}", target))
                .size(WORDS)
                .wrapping(Wrapping::None)
                .ellipsis(Ellipsis::End)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    parts.height(HEADER).boxed()
}
