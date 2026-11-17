//! A poll on its message (`PollView`, `ui/messages.slint:690`): the question
//! and its answers to pick, and once this account voted, the poll closed or
//! "Show results" was pressed, each answer's share of the votes; under them
//! the votes, the time left and the buttons, and the confirmation of ending
//! it early.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Background, Border, Fill, Length};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::chat::poll::{Action, Intent, Poll};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use super::answer;
use super::ends::ends;

/// The card's widest (`max-width: 480px`), its inside padding and the gap
/// between its parts, and the room above it (`padding-top: 4px`).
const WIDTH: f32 = 480.0;
const PAD: f32 = 16.0;
const GAP: f32 = 8.0;
const ABOVE: f32 = 4.0;
const QUESTION: f32 = 17.0;
const PROMPT: f32 = typography::size::LABEL;
const LINE: f32 = typography::size::SMALL;
/// The foot's and the ending's room above them (`padding-top: 4px`).
const FOOT_ABOVE: f32 = 4.0;

pub(super) fn view(poll: &Poll, motion: Motion) -> Element<'_, Intent> {
    let mut parts = column![
        text(poll.question.as_str())
            .size(QUESTION)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::Word)
            .style(catalog::text::primary)
            .boxed()
    ]
    .spacing(GAP);
    if !poll.prompt.is_empty() {
        parts = parts.push(
            text(poll.prompt.as_str())
                .size(PROMPT)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    for one in &poll.answers {
        parts = parts.push(answer::view(poll, one));
    }
    parts = parts.push(foot(poll, motion));
    if poll.interactive && poll.confirming {
        parts = parts.push(confirm(poll, motion));
    }
    let card = container(parts.padding(PAD))
        .width(Fill)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.sidebar)),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(palette.line),
                ..container::Style::default()
            }
        });
    container(container(card).width(Length::Fill.max(WIDTH)))
        .padding(iced::Padding::ZERO.top(ABOVE))
        .boxed()
}

/// The votes and the time left, then the buttons: on one line when they fit,
/// otherwise the buttons go to a line of their own so the count and the time
/// are never cut short.
fn foot(poll: &Poll, motion: Motion) -> Element<'_, Intent> {
    let mut info = row![total(poll)].spacing(GAP).align_y(Alignment::Center);
    if !poll.left.is_empty() {
        info = info.push(
            text(format!("\u{b7}  {}", poll.left))
                .size(LINE)
                .wrapping(text::Wrapping::None)
                .style(catalog::text::muted)
                .boxed(),
        );
    }
    let mut buttons = row![].spacing(GAP).align_y(Alignment::Center);
    if poll.can_show_results {
        buttons = buttons.push(
            action_button(tr!("Show results"), Kind::Ghost, motion)
                .small()
                .on_press(Intent::ShowResults(poll.key)),
        );
    }
    if poll.action != Action::None {
        let (words, kind) = match poll.action {
            Action::Vote => (tr!("Vote"), Kind::Primary),
            Action::RemoveVote => (tr!("Remove Vote"), Kind::Secondary),
            _ => (tr!("Go back to vote"), Kind::Secondary),
        };
        let go = action_button(words, kind, motion)
            .small()
            .enabled(poll.action_enabled);
        buttons = buttons.push(if poll.action_enabled {
            go.on_press(Intent::Act(poll.key, poll.action))
        } else {
            go
        });
    }
    container(ends(info.boxed(), buttons.boxed(), GAP))
        .padding(iced::Padding::ZERO.top(FOOT_ABOVE))
        .boxed()
}

/// The votes so far, which show who voted once the results do.
fn total(poll: &Poll) -> Element<'_, Intent> {
    let words = text(poll.total.as_str())
        .size(LINE)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    if poll.interactive && poll.results {
        button(words)
            .padding(0)
            .on_press(Intent::Voters(poll.key, None))
            .style(|theme: &Theme, status| {
                let palette = theme.palette();
                let lit = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    text_color: if lit { palette.text } else { palette.secondary },
                    ..button::Style::default()
                }
            })
            .boxed()
    } else {
        words.style(catalog::text::secondary).boxed()
    }
}

/// Closing it before its time, once made sure of.
fn confirm(poll: &Poll, motion: Motion) -> Element<'_, Intent> {
    let rule = container(space())
        .width(Fill)
        .height(1)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().line)),
            ..container::Style::default()
        });
    column![
        rule,
        text(tr!(
            "End the poll now? This will close it immediately and reveal the results."
        ))
        .size(LINE)
        .width(Length::Fill)
        .wrapping(text::Wrapping::Word)
        .style(catalog::text::secondary),
        row![
            space().width(Fill),
            action_button(tr!("Cancel"), Kind::Ghost, motion)
                .small()
                .on_press(Intent::CancelEnd(poll.key)),
            action_button(tr!("End Poll Now"), Kind::Danger, motion)
                .small()
                .on_press(Intent::End(poll.key)),
        ]
        .spacing(GAP),
    ]
    .spacing(GAP)
    .padding(iced::Padding::ZERO.top(FOOT_ABOVE))
    .boxed()
}
