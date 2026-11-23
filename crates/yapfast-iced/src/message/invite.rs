//! The card an invite link in a message shows under it, as Discord's
//! client draws it: the server's icon, name and counts with Join, Joined
//! (which opens the server) or "Joining…" while it joins, and Copy link; a
//! card saying the invite is invalid; or a placeholder while it's looked
//! up. The core looks each invite up once a session (`AppState::invite_cards`).

use iced::{
    Alignment, Element, Length, Padding,
    widget::{Space, column, container, row, text},
};
use yapfast_core::{
    application::AppState,
    chat::{ChatMessage, InviteCard},
    emoji::card::grouped,
};

use crate::{
    app::Message,
    controls::{Kind, action},
    elide::elide,
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
    style::rounded,
};

/// Discord's card: at most 432 px wide, its icon 50 px with 16 px corners.
const WIDTH: f32 = 432.0;
const ICON: f32 = 50.0;

/// The invite whose card's Join is joining, by its code.
fn joining(state: &AppState) -> Option<&str> {
    state
        .join
        .as_ref()
        .filter(|join| join.joining && join.from_card)
        .and_then(|join| join.code.as_deref())
}

/// The card of the invite `code` that `message` links; none for an invite
/// to a group DM or to be friends, or one not looked up yet.
pub(super) fn invite_card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    code: &'a str,
) -> Option<Element<'a, Message>> {
    let state = ctx.state;
    let theme = ctx.look.theme;
    let card = state.invite_cards.get(code)?;
    let (header, face, words, end): (String, Element<'a, Message>, Element<'a, Message>, _) =
        match card {
            InviteCard::Other => return None,
            InviteCard::Looking => {
                let ground = theme.raised;
                let skeleton = |width: f32, height: f32, radius: f32| {
                    container(Space::new())
                        .width(width)
                        .height(height)
                        .style(move |_| rounded(ground, radius))
                };
                (
                    "Resolving invite".to_owned(),
                    skeleton(ICON, ICON, 16.0).into(),
                    skeleton(160.0, 16.0, 4.0).into(),
                    None,
                )
            }
            InviteCard::Invalid => {
                let red = theme.danger;
                let tile = container(ctx.look.glyph(Icon::Dm, 24, red))
                    .width(ICON)
                    .height(ICON)
                    .center_x(ICON)
                    .center_y(ICON)
                    .style(move |_| rounded(iced::Color { a: 0.15, ..red }, 12));
                let detail = if message.own {
                    "Try sending a new invite".to_owned()
                } else {
                    format!("Ask {} for a new invite", message.author)
                };
                (
                    if message.own {
                        "You sent an invite, but"
                    } else {
                        "You received an invite, but"
                    }
                    .to_owned(),
                    tile.into(),
                    column![
                        text("Invalid Invite").size(16).font(SEMIBOLD).color(red),
                        text(detail).size(14).color(theme.muted),
                    ]
                    .spacing(2)
                    .into(),
                    None,
                )
            }
            InviteCard::Server(preview) => {
                let joined = state
                    .servers
                    .iter()
                    .any(|server| server.id == preview.server);
                let busy = joining(state) == Some(code);
                let mut counts = row![].spacing(12).align_y(Alignment::Center);
                let dot = |ink: iced::Color| {
                    container(Space::new())
                        .width(8)
                        .height(8)
                        .style(move |_| rounded(ink, 4))
                };
                if let Some(online) = preview.online {
                    counts = counts.push(
                        row![
                            dot(theme.live),
                            text(format!("{} Online", grouped(online)))
                                .size(14)
                                .color(theme.muted)
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    );
                }
                if let Some(members) = preview.members {
                    let noun = if members == 1 { "Member" } else { "Members" };
                    counts = counts.push(
                        row![
                            dot(theme.muted),
                            text(format!("{} {noun}", grouped(members)))
                                .size(14)
                                .color(theme.muted)
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    );
                }
                let copied = ctx.lanes.timeline.copied_invite.as_deref() == Some(code);
                let copy = ctx
                    .look
                    .icon_button(
                        if copied { Icon::Check } else { Icon::Copy },
                        if copied { "Copied" } else { "Copy link" },
                        Message::Timeline(crate::timeline::Event::CopyInvite(code.to_owned())),
                    )
                    .size(36.0, 18);
                let label = if busy {
                    "Joining…"
                } else if joined {
                    "Joined"
                } else {
                    "Join"
                };
                let join = action(
                    theme,
                    label,
                    if joined { Kind::Plain } else { Kind::Success },
                    (!busy).then(|| {
                        Message::Timeline(crate::timeline::Event::JoinInvite(code.to_owned()))
                    }),
                );
                (
                    if message.own {
                        "You sent an invite to join a server"
                    } else {
                        "You've been invited to join a server"
                    }
                    .to_owned(),
                    ctx.avatar(preview.icon.as_deref(), &preview.name, ICON, 16.0, 1.0),
                    column![
                        elide(preview.name.as_str())
                            .size(16)
                            .font(SEMIBOLD)
                            .color(theme.text),
                        counts,
                    ]
                    .spacing(2)
                    .into(),
                    Some(
                        row![copy, Element::from(join)]
                            .spacing(8)
                            .align_y(Alignment::Center),
                    ),
                )
            }
        };
    let mut line = row![face, container(words).width(Length::Fill).center_y(ICON)]
        .spacing(16)
        .align_y(Alignment::Center);
    if let Some(end) = end {
        line = line.push(end);
    }
    let (ground, edge) = (theme.sidebar, theme.line);
    let card = container(
        column![
            elide(header).size(14).font(MEDIUM).color(theme.secondary),
            line,
        ]
        .spacing(12),
    )
    .padding(16)
    .max_width(WIDTH)
    .width(Length::Fill)
    .style(move |_| iced::widget::container::Style {
        border: iced::Border {
            color: edge,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..rounded(ground, 8)
    });
    Some(container(card).padding(Padding::ZERO.top(4)).into())
}
