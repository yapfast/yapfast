//! Discord polls on messages, shown after Discord's client: the question
//! and its answers to pick, and once this account voted, the poll closed or
//! "Show results" was clicked, each answer's share of the votes, and a
//! click on an answer, or on the votes so far, shows who voted. What this
//! device alone knows of a poll (the answers picked before voting, whether
//! "Show results" was clicked) is kept among `revealed`, as
//! `{item}/pick{id}` and `{item}/results`.

use iced::{
    Alignment, Border, Element, Length, Padding, Task,
    widget::{Space, button, column, container, row, text},
};
use yapfast_core::{
    application::Action,
    chat::{ChatMessage, Poll},
};

use crate::{
    app::{App, Message},
    controls::{Kind, action, lights},
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::rounded,
};

/// The card at its widest.
const WIDTH: f32 = 480.0;
/// An answer's least height, and its emoji's side.
const ANSWER: f32 = 44.0;
const ANSWER_EMOJI: u32 = 24;

/// What a poll's card does.
#[derive(Clone, Debug)]
pub enum Event {
    /// An answer was picked, or unpicked, by its id.
    Pick(String, u64),
    Vote(String),
    RemoveVote(String),
    ShowResults(String),
    BackToVote(String),
    /// End Poll Now, asked about from the message's menu, or not.
    EndPoll,
    KeepPoll,
    /// Who voted, once the results show: for an answer by its id, or (from
    /// the votes so far) the first.
    Voters(String, Option<u64>),
}

/// What the card's button under the answers does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Vote,
    RemoveVote,
    BackToVote,
}

fn results_key(item: &str) -> String {
    format!("{item}/results")
}

pub fn pick_key(item: &str, answer: u64) -> String {
    format!("{item}/pick{answer}")
}

/// "1 vote", "3 votes".
fn votes_text(votes: usize) -> String {
    if votes == 1 {
        "1 vote".to_owned()
    } else {
        format!("{votes} votes")
    }
}

/// How long a poll has left, as Discord's client says it.
fn time_left(seconds: u64) -> String {
    match seconds {
        86_400.. => format!("{}d left", seconds / 86_400),
        3_600.. => format!("{}h left", seconds / 3_600),
        60.. => format!("{}m left", seconds / 60),
        _ => "< 1m left".to_owned(),
    }
}

/// What a poll's card asked for.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Pick(item, answer) => {
            let multiselect = app
                .client
                .state()
                .message(&item)
                .and_then(|message| message.poll.as_ref())
                .is_some_and(|poll| poll.multiselect);
            let key = pick_key(&item, answer);
            if !app.timeline.revealed.remove(&key) {
                if !multiselect {
                    let picks = format!("{item}/pick");
                    app.timeline
                        .revealed
                        .retain(|kept| !kept.starts_with(&picks));
                }
                app.timeline.revealed.insert(key);
            }
            Task::none()
        }
        Event::ShowResults(item) => {
            app.timeline.revealed.insert(results_key(&item));
            Task::none()
        }
        Event::EndPoll => {
            app.client.dispatch(Action::EndPoll);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::KeepPoll => {
            app.client.dispatch(Action::CancelEndPoll);
            Task::batch([app.sync(), crate::composer::refocus(app)])
        }
        Event::BackToVote(item) => {
            app.timeline.revealed.remove(&results_key(&item));
            Task::none()
        }
        Event::Voters(item, answer) => {
            app.client.dispatch(Action::ShowVoters { item, answer });
            Task::batch([
                app.sync(),
                iced::widget::operation::focus(crate::app::NOBODY),
            ])
        }
        Event::Vote(item) => {
            let picks = format!("{item}/pick");
            let answers = app
                .timeline
                .revealed
                .iter()
                .filter_map(|kept| kept.strip_prefix(&picks)?.parse().ok())
                .collect();
            app.client.dispatch(Action::Vote { item, answers });
            app.sync()
        }
        // Taking the vote back starts the poll afresh.
        Event::RemoveVote(item) => {
            let picks = format!("{item}/pick");
            let results = results_key(&item);
            app.timeline
                .revealed
                .retain(|kept| !kept.starts_with(&picks) && *kept != results);
            app.client.dispatch(Action::Vote {
                item,
                answers: Vec::new(),
            });
            app.sync()
        }
    }
}

/// Whether a timeline shows a poll still open at `now`, whose time left
/// goes down.
pub fn open_poll(messages: &[std::sync::Arc<ChatMessage>], now: u64) -> bool {
    messages.iter().any(|message| {
        message
            .poll
            .as_ref()
            .is_some_and(|poll| poll.expiry.is_some() && !poll.closed(now))
    })
}

/// A poll's height, for estimates.
pub fn estimate(poll: &Poll) -> f32 {
    16.0 * 2.0 + 24.0 + 20.0 + poll.answers.len() as f32 * (ANSWER + 8.0) + 40.0
}

/// A message's poll's card.
pub fn view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    poll: &'a Poll,
) -> Element<'a, Message> {
    card(ctx, message, poll, true)
}

/// A found message's poll's card (a pinned message's, a search's result's,
/// a thread's start's), which nothing is done in: the votes show only once
/// this account voted or the poll closed, and it sends people to the
/// message to vote.
pub fn found_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    poll: &'a Poll,
) -> Element<'a, Message> {
    card(ctx, message, poll, false)
}

/// A poll's card, `interactive` in the timeline.
fn card<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &'a ChatMessage,
    poll: &'a Poll,
    interactive: bool,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let item = &message.item;
    let now = ctx.minute;
    let closed = poll.closed(now);
    let voted = poll.voted();
    let revealed = &ctx.lanes.timeline.revealed;
    let shown = interactive && revealed.contains(&results_key(item));
    let results = closed || voted || shown;
    let picked = |id: u64| interactive && revealed.contains(&pick_key(item, id));
    let total: usize = poll.answers.iter().map(|answer| answer.votes).sum();
    let most = poll.answers.iter().map(|answer| answer.votes).max();
    let mut card = column![
        text(poll.question.as_str())
            .size(17)
            .font(BOLD)
            .color(theme.text)
    ]
    .spacing(8);
    let prompt = match (closed, poll.multiselect) {
        _ if !interactive => "",
        (true, _) => "",
        (false, true) => "Select one or more answers",
        (false, false) => "Select one answer",
    };
    if !prompt.is_empty() {
        card = card.push(text(prompt).size(13).color(theme.muted));
    }
    for answer in &poll.answers {
        let on = if results {
            answer.mine
        } else {
            picked(answer.id)
        };
        let percent = (answer.votes * 100 + total / 2)
            .checked_div(total)
            .unwrap_or(0);
        let leading = answer.votes > 0 && Some(answer.votes) == most;
        // At least an answer's height, more as its text wraps.
        let mut line = row![Space::new().height(ANSWER - 16.0)]
            .spacing(10)
            .align_y(Alignment::Center);
        if let Some(emoji) = poll_emoji(ctx, message, answer.emoji.as_deref()) {
            line = line.push(emoji);
        }
        line = line.push(
            text(answer.text.as_str())
                .size(15)
                .font(SEMIBOLD)
                .color(theme.text)
                .width(Length::Fill),
        );
        if results {
            if answer.mine {
                line = line.push(ctx.look.glyph(Icon::Check, 16, theme.accent));
            }
            line = line
                .push(
                    text(votes_text(answer.votes))
                        .size(13)
                        .color(theme.secondary),
                )
                .push(
                    text(format!("{percent}%"))
                        .size(15)
                        .font(BOLD)
                        .color(theme.text)
                        .width(44)
                        .align_x(Alignment::End),
                );
        } else {
            line = line.push(poll_mark(ctx, on, poll.multiselect));
        }
        let face = container(line)
            .padding([8, 12])
            .width(Length::Fill)
            .height(Length::Shrink)
            .align_y(Alignment::Center);
        // The share of the votes, filling the answer from its left: its
        // ground up to the share, in one gradient with a hard edge, as
        // a layer under the text would take the card's whole height.
        let face = if results && percent > 0 {
            let fill = if leading {
                theme.accent_soft
            } else {
                theme.wash(0.06)
            };
            let share = percent.min(100) as f32 / 100.0;
            face.style(move |_| container::Style {
                background: Some(iced::Background::Gradient(
                    iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2)
                        .add_stop(0.0, fill)
                        .add_stop(share, fill)
                        // A stop at the same place would replace it.
                        .add_stop((share + 0.001).min(1.0), iced::Color::TRANSPARENT)
                        .add_stop(1.0, iced::Color::TRANSPARENT)
                        .into(),
                )),
                border: crate::style::round(8),
                ..container::Style::default()
            })
        } else {
            face
        };
        let (rest, lit, accent) = (theme.raised, theme.active, theme.accent);
        // Once the results show, an answer shows who voted for it.
        let press = interactive.then(|| {
            Message::Poll(if results {
                Event::Voters(item.clone(), Some(answer.id))
            } else {
                Event::Pick(item.clone(), answer.id)
            })
        });
        card = card.push(
            button(container(face).height(Length::Shrink))
                .padding(0)
                .width(Length::Fill)
                .height(Length::Shrink)
                .on_press_maybe(press)
                .style(move |_, status| button::Style {
                    background: Some(if lights(status) { lit } else { rest }.into()),
                    border: Border {
                        color: accent,
                        width: if on { 2.0 } else { 0.0 },
                        radius: 8.0.into(),
                    },
                    ..button::Style::default()
                }),
        );
    }
    card = card.push(if interactive {
        poll_footer(ctx, item, poll, (closed, voted, shown, results), total)
    } else {
        found_footer(ctx, closed, total)
    });
    container(
        container(card.width(Length::Fill))
            .padding(16)
            .max_width(WIDTH)
            .style({
                let (ground, line) = (theme.sidebar, theme.line);
                move |_| crate::style::outlined(ground, line, 8.0)
            }),
    )
    .padding(Padding::ZERO.top(4))
    .into()
}

/// The votes so far, the time left, and the card's buttons.
fn poll_footer<'a>(
    ctx: crate::ctx::Ctx<'a>,
    item: &str,
    poll: &Poll,
    (closed, voted, shown, results): (bool, bool, bool, bool),
    total: usize,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let left = if closed {
        "Poll closed".to_owned()
    } else {
        poll.expiry
            .map(|expiry| time_left(expiry.saturating_sub(ctx.minute)))
            .unwrap_or_default()
    };
    // The votes so far, which show who voted once the results do.
    let votes = button(text(votes_text(total)).size(14))
        .padding(0)
        .on_press_maybe(results.then(|| Message::Poll(Event::Voters(item.to_owned(), None))))
        .style(crate::controls::ink_style(theme.secondary, theme.text));
    let mut footer = row![votes]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding::ZERO.top(4));
    if !left.is_empty() {
        footer = footer.push(
            text(format!("\u{00b7}  {left}"))
                .size(14)
                .color(theme.muted),
        );
    }
    footer = footer.push(Space::new().width(Length::Fill));
    if !results {
        footer = footer.push(
            action(
                theme,
                "Show results",
                Kind::Ghost,
                Some(Message::Poll(Event::ShowResults(item.to_owned()))),
            )
            .small(),
        );
    }
    let act = if closed {
        None
    } else if voted {
        Some(Act::RemoveVote)
    } else if shown {
        Some(Act::BackToVote)
    } else {
        Some(Act::Vote)
    };
    if let Some(act) = act {
        let any_picked = poll.answers.iter().any(|answer| {
            ctx.lanes
                .timeline
                .revealed
                .contains(&pick_key(item, answer.id))
        });
        let enabled = voted || shown || any_picked;
        let (label, kind, event) = match act {
            Act::Vote => ("Vote", Kind::Primary, Event::Vote(item.to_owned())),
            Act::RemoveVote => (
                "Remove Vote",
                Kind::Plain,
                Event::RemoveVote(item.to_owned()),
            ),
            Act::BackToVote => (
                "Go back to vote",
                Kind::Plain,
                Event::BackToVote(item.to_owned()),
            ),
        };
        footer = footer
            .push(action(theme, label, kind, enabled.then_some(Message::Poll(event))).small());
    }
    // Closing it before its time, once made sure of from the message's
    // menu.
    if ctx.state.confirm_end_poll.as_deref() != Some(item) {
        return footer.into();
    }
    let ask = column![
        crate::controls::bar(theme.line, Length::Fill, 1),
        text("End the poll now? This will close it immediately and reveal the results.")
            .size(14)
            .color(theme.secondary),
        row![
            Space::new().width(Length::Fill),
            action(
                theme,
                "Cancel",
                Kind::Ghost,
                Some(Message::Poll(Event::KeepPoll))
            )
            .small(),
            action(
                theme,
                "End Poll Now",
                Kind::Danger,
                Some(Message::Poll(Event::EndPoll))
            )
            .small(),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .padding(Padding::ZERO.top(4));
    column![footer, ask].into()
}

/// A found poll's foot: the votes and "Poll closed" once it closed, before
/// that where to vote.
fn found_footer<'a>(ctx: crate::ctx::Ctx<'a>, closed: bool, total: usize) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut footer = row![]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding(Padding::ZERO.top(4));
    if closed {
        footer = footer
            .push(text(votes_text(total)).size(14).color(theme.secondary))
            .push(text("\u{00b7}  Poll closed").size(14).color(theme.muted));
    } else {
        footer = footer.push(
            text("View original message to interact")
                .size(14)
                .color(theme.secondary),
        );
    }
    footer.into()
}

/// A radio button, or for several answers a checkbox, picked or not.
fn poll_mark<'a>(ctx: crate::ctx::Ctx<'a>, on: bool, multiselect: bool) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (muted, accent) = (theme.muted, theme.accent);
    let radius = if multiselect { 5.0 } else { 10.0 };
    let inner: Element<'a, Message> = match (on, multiselect) {
        (true, true) => ctx.look.glyph(Icon::Check, 14, theme.on_accent).into(),
        (true, false) => {
            let dot = theme.on_accent;
            container(Space::new())
                .width(8)
                .height(8)
                .style(move |_| rounded(dot, 4))
                .into()
        }
        (false, _) => Space::new().into(),
    };
    container(inner)
        .center(20)
        .style(move |_| container::Style {
            background: on.then(|| accent.into()),
            border: Border {
                color: muted,
                width: if on { 0.0 } else { 2.0 },
                radius: radius.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// An answer's emoji: a Unicode one's picture, or a custom one's.
fn poll_emoji<'a>(
    ctx: crate::ctx::Ctx<'a>,
    message: &ChatMessage,
    key: Option<&str>,
) -> Option<Element<'a, Message>> {
    let key = key?;
    let picture = match emojis::get(key) {
        Some(emoji) => ctx.emoji_picture(emoji.as_str(), ANSWER_EMOJI),
        None => message
            .emoji
            .iter()
            .find(|custom| custom.text == key)
            .and_then(|custom| ctx.picture(&custom.picture, ANSWER_EMOJI, ANSWER_EMOJI, false)),
    };
    Some(match picture {
        Some(picture) => crate::widgets::Pic::square(picture, ANSWER_EMOJI as f32).into(),
        None => text(key.to_owned()).size(18).into(),
    })
}
