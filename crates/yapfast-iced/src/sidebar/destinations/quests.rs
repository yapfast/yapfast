//! The Quests screen: Discord's quests as cards, the first as the wide
//! banner, with what each asks, how far it is and the buttons to accept it.

use iced::{
    Alignment, Border, Color, Element, Length, Point, Size,
    widget::{Column, Space, column, container, progress_bar, row, stack, text},
};
use yapfast_core::{
    clock,
    i18n::{fill, tr},
    storefront::{Quest, QuestRewardKind, QuestStatus, QuestTask, QuestsPage},
};

use super::{
    Event, QuestTab, columns, contain, corners, cover, fade, frame, grid, heading, pill, room,
    status,
};
use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::Ctx,
    fonts::{BOLD, SEMIBOLD},
    pictures::Icon,
    style::round,
};

/// A card's picture strip, and the wide banner's height at most.
const STRIP: f32 = 120.0;
const BANNER_LEAST: f32 = 260.0;
const BANNER_MOST: f32 = 440.0;
/// The least a card is wide, and how many stand across.
const CARD_LEAST: f32 = 300.0;
const CARDS: usize = 3;
/// The tile before a card's words.
const TILE: f32 = 56.0;

pub(super) fn screen<'a>(cx: Ctx<'a>) -> Element<'a, Message> {
    let page: &'a QuestsPage = &cx.state.quests;
    let tab = cx.pages.quests;
    if !page.loaded {
        return status(cx, Icon::Trophy, page.loading, page.error.as_deref());
    }
    let list: &'a [Quest] = match tab {
        QuestTab::All => &page.offered,
        QuestTab::Claimed => &page.claimed,
    };
    if list.is_empty() {
        let (title, words) = match tab {
            QuestTab::All => (
                tr("No quests right now"),
                tr("Discord has no quest for your account at the moment. Check back later."),
            ),
            QuestTab::Claimed => (
                tr("No claimed quests yet"),
                tr("The quests you finish and claim a reward for show up here."),
            ),
        };
        return container(cx.look.empty_screen(
            Icon::Trophy,
            title,
            words,
            Some((tr("Refresh"), Message::Pages(Event::Refresh))),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .into();
    }
    let width = room(cx);
    let mut content = Column::new().spacing(24).width(Length::Fill);
    if let Some(error) = &page.error {
        content = content.push(
            row![
                text(fill(tr("Could not refresh the quests: {}"), &[error]))
                    .size(14)
                    .color(cx.look.theme.danger)
                    .width(Length::Fill),
                action(
                    cx.look.theme,
                    tr("Retry"),
                    Kind::Plain,
                    Some(Message::Pages(Event::Refresh)),
                )
                .small(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );
    }
    let cards: &'a [Quest] = if tab == QuestTab::All {
        content = content.push(banner(cx, &list[0], width));
        &list[1..]
    } else {
        list
    };
    if !cards.is_empty() {
        let (count, each) = columns(width, CARD_LEAST, CARDS);
        let title = match tab {
            QuestTab::All => tr("Available Quests"),
            QuestTab::Claimed => tr("Claimed Quests"),
        };
        content = content.push(heading(cx, title)).push(grid(
            cards.iter().map(|quest| card(cx, quest, each)).collect(),
            count,
        ));
    }
    frame(cx, content.into())
}

/// The pictures the open tab shows.
pub(super) fn pictures(page: &QuestsPage, tab: QuestTab) -> Vec<String> {
    let list = match tab {
        QuestTab::All => &page.offered,
        QuestTab::Claimed => &page.claimed,
    };
    list.iter()
        .take(30)
        .flat_map(|quest| {
            [
                quest.hero.clone(),
                quest.bar.clone(),
                quest.tile.clone(),
                quest.logo.clone(),
                quest
                    .reward
                    .as_ref()
                    .and_then(|reward| reward.picture.clone()),
            ]
        })
        .flatten()
        .collect()
}

/// What the quest's game is called: its game, else the quest.
fn game(quest: &Quest) -> &str {
    if quest.game.is_empty() {
        &quest.name
    } else {
        &quest.game
    }
}

/// What the reward is, as a card's bold line.
fn reward_title(quest: &Quest) -> String {
    match &quest.reward {
        Some(reward) if reward.kind == QuestRewardKind::Orbs && reward.orbs.is_some() => {
            fill(tr("Claim {} Orbs"), &[&reward.orbs.unwrap_or(0)])
        }
        Some(reward) if !reward.name.is_empty() => reward.name.clone(),
        _ => quest.name.clone(),
    }
}

/// What the quest asks, in a sentence.
fn task_text(quest: &Quest) -> String {
    let (game, time) = (game(quest), super::minutes(quest.target));
    match quest.task {
        QuestTask::Play => fill(tr("Play {} for {}."), &[&game, &time]),
        QuestTask::Stream => fill(tr("Stream {} for {}."), &[&game, &time]),
        QuestTask::Watch => fill(tr("Watch the video for {}."), &[&time]),
        QuestTask::Activity => fill(tr("Play {} in an activity for {}."), &[&game, &time]),
        QuestTask::Achievement => fill(tr("Reach the goal in {}."), &[&game]),
    }
}

/// The date the quest ends, as a line, if it says.
fn ends(quest: &Quest) -> Option<String> {
    let seconds = u64::try_from(quest.expires?).ok().filter(|s| *s > 0)?;
    Some(fill(tr("Ends {}"), &[&clock::day(seconds)]))
}

/// The buttons under a quest: its game's page, and what the account does
/// next, by how far it is. `wide` stretches each across its half.
fn actions<'a>(cx: Ctx<'a>, quest: &'a Quest, wide: bool) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let fit = |button: crate::controls::Action<'a, Message>| -> Element<'a, Message> {
        if wide {
            button.wide(36.0).into()
        } else {
            button.into()
        }
    };
    let mut buttons = row![].spacing(8);
    if let Some(link) = &quest.link {
        let label = match (&quest.button, quest.task) {
            (Some(label), _) if !label.is_empty() => label.clone(),
            (_, QuestTask::Watch) => tr("Watch now").to_owned(),
            _ => tr("Play now").to_owned(),
        };
        buttons = buttons.push(fit(action(
            theme,
            label,
            Kind::Plain,
            Some(Message::Link(link.clone(), Point::ORIGIN)),
        )));
    }
    let accepting = cx.state.quests.accepting.contains(&quest.id);
    buttons = buttons.push(match quest.status {
        QuestStatus::Available => fit(action(
            theme,
            if accepting {
                tr("Accepting")
            } else {
                tr("Accept quest")
            },
            Kind::Primary,
            (!accepting).then(|| Message::Pages(Event::Accept(quest.id.clone()))),
        )),
        QuestStatus::Accepted => fit(action(
            theme,
            if quest.target == 0 {
                tr("In progress").to_owned()
            } else {
                fill(tr("{} min left"), &[&quest.minutes_left()])
            },
            Kind::Primary,
            None,
        )),
        // Claiming may ask for a captcha, whose page opens in the browser
        // and waits there until it is solved.
        QuestStatus::Completed => {
            let claiming = cx.state.quests.claiming.contains(&quest.id);
            fit(action(
                theme,
                if claiming {
                    tr("Claiming")
                } else {
                    tr("Claim reward")
                },
                Kind::Success,
                (!claiming).then(|| Message::Pages(Event::Claim(quest.id.clone()))),
            ))
        }
        QuestStatus::Claimed => fit(action(theme, tr("Claimed"), Kind::Plain, None)),
    });
    buttons.into()
}

/// The quest's logo over its picture, or its game's name in its place.
fn mark<'a>(cx: Ctx<'a>, quest: &'a Quest, most: f32) -> Element<'a, Message> {
    contain(cx, quest.logo.as_deref(), Size::new(most, 36.0), 0.0).unwrap_or_else(|| {
        text(game(quest))
            .size(18)
            .font(BOLD)
            .color(cx.look.theme.channel_white)
            .into()
    })
}

/// The first quest, as the wide banner: its picture, shaded on the left
/// where its words stand.
fn banner<'a>(cx: Ctx<'a>, quest: &'a Quest, width: f32) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let height = (width * 0.4).clamp(BANNER_LEAST, BANNER_MOST).floor();
    let back = cover(
        cx,
        quest.hero.as_deref(),
        Size::new(width, height),
        [16.0; 4],
        quest.colors,
    );
    let shade = container(Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fade(
                Color {
                    a: 0.78,
                    ..Color::BLACK
                },
                Color::TRANSPARENT,
            )),
            border: Border {
                radius: corners([16.0; 4]),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let text_width = (width * 0.5).max(240.0);
    let words = column![
        text(game(quest).to_uppercase())
            .size(if width < 560.0 { 28 } else { 40 })
            .font(BOLD)
            .color(theme.channel_white),
        text(task_text(quest)).size(15).color(theme.channel_white),
        actions(cx, quest, false),
    ]
    .spacing(14)
    .max_width(text_width);
    let top = row![
        mark(cx, quest, text_width),
        Space::new().width(Length::Fill),
        pill(cx, tr("Sponsored")),
    ]
    .align_y(Alignment::Start);
    let front = column![
        top,
        Space::new().height(Length::Fill),
        row![words, Space::new().width(Length::Fill)]
    ]
    .padding(28);
    stack![back, shade, front].into()
}

/// A quest's card: its picture with its mark, who stands behind it and
/// until when, its reward and task, how far it is, and its buttons.
fn card<'a>(cx: Ctx<'a>, quest: &'a Quest, width: f32) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let ground = theme.raised;
    let strip = stack![
        cover(
            cx,
            quest.bar.as_deref().or(quest.hero.as_deref()),
            Size::new(width, STRIP),
            [12.0, 12.0, 0.0, 0.0],
            quest.colors,
        ),
        container(mark(cx, quest, width * 0.6))
            .padding(12)
            .width(Length::Fill)
            .height(STRIP)
            .align_y(Alignment::End),
    ];
    let by = if quest.publisher.is_empty() {
        tr("Sponsored").to_owned()
    } else {
        fill(tr("Sponsored by {}"), &[&quest.publisher])
    };
    let mut credit = row![text(by).size(12).color(theme.secondary).width(Length::Fill)]
        .align_y(Alignment::Center);
    if let Some(ends) = ends(quest) {
        credit = credit.push(text(ends).size(12).color(theme.secondary));
    }
    let tile_url = quest
        .reward
        .as_ref()
        .and_then(|reward| reward.picture.as_deref())
        .or(quest.tile.as_deref());
    let words = column![
        text(quest.name.to_uppercase())
            .size(11)
            .font(SEMIBOLD)
            .color(theme.link),
        text(reward_title(quest))
            .size(14)
            .font(BOLD)
            .color(theme.text),
        text(task_text(quest)).size(12).color(theme.secondary),
    ]
    .spacing(2)
    .width(Length::Fill);
    let about = row![
        cover(cx, tile_url, Size::new(TILE, TILE), [8.0; 4], quest.colors),
        words
    ]
    .spacing(12);
    let mut body = Column::new()
        .spacing(12)
        .padding(12)
        .push(credit)
        .push(about);
    if quest.status == QuestStatus::Accepted {
        body = body.push(
            progress_bar(0.0..=1.0, quest.fraction())
                .girth(6.0)
                .style(move |_| progress_bar::Style {
                    background: theme.field.into(),
                    bar: theme.accent.into(),
                    border: round(3),
                }),
        );
    }
    body = body.push(actions(cx, quest, true));
    container(column![strip, body])
        .width(width)
        .style(move |_| container::Style {
            background: Some(ground.into()),
            border: round(12),
            ..container::Style::default()
        })
        .into()
}
