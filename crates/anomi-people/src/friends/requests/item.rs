//! Someone waiting for an answer: their friend request, answered here, and
//! their message request's message under it. The Slint side: `RequestItem`
//! (`ui/friends.slint:82`).
//!
//! A row draws twice, as iced's `hover` lays one over the other: at rest, and
//! under the pointer (the row's fill, the real Accept and Ignore buttons).

use iced::widget::{column, container, row, stack, text};
use iced::{Alignment, Background, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::people::requests::{Friend, Intent, Request};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::badge::discord_badge;
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::icon::Icon;
use yapfast_lang::tr;

use super::{hovering, message};

/// The row's corners and padding, and the gaps (`ui/friends.slint:104`,
/// `:118`, `:123`).
const RADIUS: f32 = 10.0;
const PAD: f32 = 10.0;
const GAP: f32 = 12.0;
const STACK_GAP: f32 = 8.0;
/// The avatar, and the top line's least height (`:121`, `:124`).
const AVATAR: f32 = metrics::AVATAR_MESSAGE;
/// The name and the line under it, between them 1 px (`:128`-`:133`).
const NAME: f32 = typography::size::TITLE;
const ABOUT: f32 = 13.0;
const LINES_GAP: f32 = 1.0;
/// The Discord mark on the avatar's corner (`:122`).
const MARK: f32 = 14.0;
const MARK_OFFSET: f32 = -2.0;

/// The row of `request`; `spam` when Discord took its message for spam.
pub fn view<'a>(
    request: &'a Request,
    spam: bool,
    marks: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    hovering(move |hovered| {
        let body = body(request, spam, marks, hovered, motion);
        container(body)
            .width(Fill)
            .style(move |theme: &Theme| container::Style {
                background: hovered.then(|| Background::Color(theme.palette().row_hover)),
                border: Border::default().rounded(RADIUS),
                ..container::Style::default()
            })
            .boxed()
    })
}

fn body<'a>(
    request: &'a Request,
    spam: bool,
    marks: bool,
    hovered: bool,
    motion: Motion,
) -> Element<'a, Intent> {
    // The row's hovered fill has no `Ground` yet: the screen's own is the
    // nearest.
    let ring = Ground::Chat;
    let face = portrait(&request.avatar, AVATAR).motion(motion).view();
    let face: Element<'a, Intent> = if marks {
        stack![
            face,
            iced::widget::pin(discord_badge(MARK, ring))
                .x(MARK_OFFSET)
                .y(MARK_OFFSET)
        ]
        .boxed()
    } else {
        face.boxed()
    };
    let mut top = row![names(request, spam)]
        .spacing(STACK_GAP)
        .align_y(Alignment::Center);
    if request.friend == Friend::Asked {
        let accept = action_button(tr!("Accept"), Kind::Primary, motion)
            .leading(Icon::Check)
            .small();
        top = top.push(if hovered {
            accept.on_press(Intent::Accept(request.id.clone())).boxed()
        } else {
            accept.boxed()
        });
    }
    if request.friend != Friend::No {
        let label = if request.friend == Friend::Asked {
            tr!("Ignore")
        } else {
            tr!("Cancel")
        };
        let remove = action_button(label, Kind::Secondary, motion)
            .leading(Icon::X)
            .small();
        top = top.push(if hovered {
            remove.on_press(Intent::Remove(request.id.clone())).boxed()
        } else {
            remove.boxed()
        });
    }
    let mut lines = column![container(top).center_y(AVATAR).width(Fill)].spacing(STACK_GAP);
    if let Some(sent) = &request.message {
        lines = lines.push(message::view(sent, hovered, motion));
    }
    row![container(face).align_y(Alignment::Start), lines.width(Fill)]
        .spacing(GAP)
        .padding(PAD)
        .align_y(Alignment::Start)
        .width(Length::Fill)
        .boxed()
}

/// Their name, and what they asked.
fn names<'a>(request: &'a Request, spam: bool) -> Element<'a, Intent> {
    let weight = if request.unread() || request.friend == Friend::Asked {
        typography::BOLD
    } else {
        typography::MEDIUM
    };
    column![
        text(request.name.as_str())
            .size(NAME)
            .font(weight)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        text(about(request, spam))
            .size(ABOUT)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(move |theme: &Theme| text::Style {
                color: Some(if spam {
                    theme.palette().loud
                } else {
                    theme.palette().muted
                }),
                ..text::Style::default()
            }),
    ]
    .spacing(LINES_GAP)
    .width(Fill)
    .boxed()
}

/// What they want of this account, in words.
fn about(request: &Request, spam: bool) -> &'static str {
    let messaged = request.message.is_some();
    match request.friend {
        _ if spam => tr!("Maybe spam"),
        Friend::Asked if messaged => tr!("Sent you a friend request and a message"),
        Friend::Asked => tr!("Sent you a friend request"),
        Friend::Sent => tr!("You sent a friend request"),
        Friend::No => tr!("Wants to message you"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anomi_vm::people::requests::Message;

    #[test]
    fn a_row_says_what_was_asked() {
        let mut request = Request {
            friend: Friend::Asked,
            ..Request::default()
        };
        assert_eq!(about(&request, false), "Sent you a friend request");
        request.message = Some(Message::default());
        assert_eq!(
            about(&request, false),
            "Sent you a friend request and a message"
        );
        assert_eq!(about(&request, true), "Maybe spam");
        request.friend = Friend::Sent;
        assert_eq!(about(&request, false), "You sent a friend request");
        request.friend = Friend::No;
        assert_eq!(about(&request, false), "Wants to message you");
    }
}
