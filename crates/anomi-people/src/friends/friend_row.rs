//! A friend in the list: avatar with its presence, name, the line under it
//! (their voice channel, their status or activity, or how they are), and a
//! Message button. The Slint side: `FriendItem` (`ui/friends.slint:170`).
//!
//! A row draws twice, as iced's `hover` lays one over the other: at rest,
//! ruled from the row above, and under the pointer (the hover fill, no rule,
//! the avatar's ring cut from the fill). The one under the pointer answers.

use iced::widget::{button, column, container, hover, row, stack, text};
use iced::{Alignment, Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::kind::Presence;
use anomi_vm::menu::Spot;
use anomi_vm::people::friends::{FriendRow, Intent};
use anomi_widgets::avatar::{Ground, portrait};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::Icon;
use anomi_widgets::menu::opener;
use anomi_widgets::presence::{status_text, voice_spot};
use anomi_widgets::tip::Side;
use yapfast_lang::{text_in, tr};

/// The row's height and radius (`ui/friends.slint:187`).
pub const HEIGHT: f32 = 62.0;
const RADIUS: f32 = 8.0;
/// The row's inset, and the rule's (`:204`, `:208`).
const INSET: f32 = 10.0;
/// The name's size, and the line under it (`:222`, `:244`).
const NAME: f32 = typography::size::BODY + 1.0;
const LINE: f32 = 13.0;
/// The Message button: a 36 px disc with a 20 px glyph (`:249`).
const BUTTON: f32 = 36.0;
const GLYPH: f32 = 20.0;
/// Between the name and the line under it, and between the row's parts.
const LINES_GAP: f32 = 1.0;
const SPEAKER_GAP: f32 = 4.0;

/// The row of `friend`.
pub fn view<'a>(friend: &'a FriendRow, motion: Motion) -> Element<'a, Intent> {
    let rest = stack![body(friend, false, Motion::default()), rule()];
    let over = container(body(friend, true, motion))
        .width(Fill)
        .height(HEIGHT)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().hover)),
            border: Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    let id = friend.id.clone();
    opener(hover(container(rest).width(Fill).height(HEIGHT), over))
        .on_right_press(move |at| Intent::Menu(id.clone(), Spot::pointer(at)))
        .boxed()
}

/// The line that rules a row from the one above it.
fn rule<'a>() -> Element<'a, Intent> {
    container(
        container(iced::widget::space())
            .width(Fill)
            .height(1)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().line)),
                ..container::Style::default()
            }),
    )
    .padding(Padding::ZERO.left(INSET).right(INSET))
    .width(Fill)
    .boxed()
}

/// What the row holds, in its look at rest or under the pointer.
fn body<'a>(friend: &'a FriendRow, hovered: bool, motion: Motion) -> Element<'a, Intent> {
    let ring = if hovered {
        Ground::Active
    } else {
        Ground::Chat
    };
    let face = portrait(&friend.avatar, metrics::AVATAR_ROW)
        .presence(friend.presence)
        .ground(ring)
        .motion(motion)
        .view();
    let sender = icon_button(
        Icon::MessageCircle,
        text_in("FriendItem", "Message"),
        motion,
    )
    .size(BUTTON)
    .glyph(GLYPH)
    .radius(BUTTON / 2.0)
    .fill(|palette| palette.raised);
    // Only the copy under the pointer holds the real button, so Tab stops
    // once on it; the resting copy only draws it.
    let message: Element<'a, Intent> = if hovered {
        sender
            .tip_side(Side::Below)
            .on_press(Intent::Message(friend.id.clone()))
            .view()
            .boxed()
    } else {
        sender.face().boxed()
    };
    let lines = column![
        text(friend.name.as_str())
            .size(NAME)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary),
        under(friend, motion),
    ]
    .spacing(LINES_GAP)
    .width(Fill);
    let content = row![face, lines, message]
        .spacing(metrics::SPACE_M)
        .align_y(Alignment::Center)
        .padding([0.0, INSET]);
    button(container(content).center_y(Length::Fill))
        .padding(0)
        .width(Fill)
        .height(HEIGHT)
        .on_press(Intent::Message(friend.id.clone()))
        .style(catalog::button::bare)
        .boxed()
}

/// What is said under the name: their voice channel in full when nothing
/// else is said, else a speaker before what is, or how they are.
fn under<'a>(friend: &'a FriendRow, motion: Motion) -> Element<'a, Intent> {
    let spot = |compact| {
        friend.voice.as_ref().map(|voice| {
            voice_spot(&voice.spot, Intent::OpenVoice(voice.room.clone()), motion)
                .size(LINE)
                .compact(compact)
                .view()
        })
    };
    if !friend.has_status() {
        if let Some(place) = spot(false) {
            return row![place].boxed();
        }
        return text(presence(friend.presence))
            .size(LINE)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::muted)
            .boxed();
    }
    let mut line = row![].spacing(SPEAKER_GAP).align_y(Alignment::Center);
    if let Some(place) = spot(true) {
        line = line.push(place.boxed());
    }
    line.push(
        status_text(&friend.status, motion)
            .size(LINE)
            .view()
            .boxed(),
    )
    .boxed()
}

/// How they are, in words, when they say nothing else.
fn presence(presence: Presence) -> &'static str {
    match presence {
        Presence::DoNotDisturb => tr!("Do Not Disturb"),
        Presence::Idle => tr!("Idle"),
        Presence::Online | Presence::Mobile => tr!("Online"),
        Presence::None | Presence::Offline => tr!("Offline"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dot_has_its_words() {
        assert_eq!(presence(Presence::Mobile), "Online");
        assert_eq!(presence(Presence::None), "Offline");
        assert_eq!(presence(Presence::DoNotDisturb), "Do Not Disturb");
    }
}
