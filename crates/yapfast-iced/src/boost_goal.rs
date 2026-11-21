//! A native summary of the current server's boosts, and the account's own:
//! how many boosts it can give, and a button that gives the server one. The
//! screen keeps only its server id; counts and level follow the shared
//! snapshot, and the account's boosts are the core's `AppState::boosts`.
//! Yapfast sells no boosts, as it has no checkout: they come with Nitro.

use iced::{
    Alignment, Element, Length, Task,
    widget::{column, progress_bar, row, text},
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    i18n::{fill, tr},
    storefront::BoostPage,
};

use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Ctx, Cx, Focus, Request, Screen},
    dialog::buttons,
    fonts::BOLD,
    form,
    slot::{Closes, Why},
    theme::Theme,
};

const WIDTH: f32 = 480.0;
const METER_HEIGHT: f32 = 8.0;
const LEVEL_BOOSTS: [u32; 3] = [2, 7, 14];

#[derive(Clone, Debug)]
pub enum Event {
    Open,
    Close,
    /// Gives the server one of the account's free boosts.
    Boost,
    /// Reads the account's boosts again.
    Reload,
}

pub struct Shown {
    server: String,
}

impl Shown {
    pub fn open(cx: &mut Cx<'_>) -> Option<Self> {
        let Navigation::Server(id) = &cx.state().navigation else {
            return None;
        };
        let server = cx
            .state()
            .servers
            .iter()
            .find(|server| server.id == *id && server.discord)?;
        let shown = Self {
            server: server.id.clone(),
        };
        cx.request(Request::Focus(Focus::Nobody));
        cx.dispatch(Action::LoadBoosts(shown.server.clone()));
        Some(shown)
    }

    /// Navigation away closes the summary rather than showing another
    /// server's numbers under the previous one's name.
    pub fn follows(&self, state: &AppState) -> bool {
        matches!(&state.navigation, Navigation::Server(id) if *id == self.server)
    }

    pub fn update(&self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Close => cx.request(Request::Close(Screen::BoostGoal)),
            Event::Boost => cx.dispatch(Action::BoostServer(self.server.clone())),
            Event::Reload => cx.dispatch(Action::LoadBoosts(self.server.clone())),
            Event::Open => {}
        }
        Task::none()
    }
}

impl Closes for Shown {
    fn close(&mut self, cx: &mut Cx<'_>, why: Why) {
        if why == Why::User {
            cx.request(Request::Focus(Focus::Default));
        }
    }
}

pub fn view<'a>(shown: &'a Shown, cx: Ctx<'a>) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let close = Message::BoostGoal(Event::Close);
    let mut body = vec![
        row![
            text("Server boost progress")
                .size(18)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
            action(theme, "Close", Kind::Ghost, Some(close.clone())),
        ]
        .align_y(Alignment::Center)
        .into(),
    ];
    let server = cx
        .state
        .servers
        .iter()
        .find(|server| server.id == shown.server && server.discord);
    if let Some(server) = server {
        body.push(form::note(theme, server.name.as_str()));
    }
    match server {
        Some(server)
            if cx.state.discord_online
                && !server.unavailable
                && server.premium_tier.unwrap_or(0) <= 3 =>
        {
            let tier = server.premium_tier.unwrap_or(0);
            body.push(
                text(format!("Reported level: {tier}"))
                    .size(24)
                    .font(BOLD)
                    .color(theme.text)
                    .into(),
            );
            if let Some(boosts) = server.boosts {
                body.push(form::note(theme, format!("Reported boosts: {boosts}")));
                let target = LEVEL_BOOSTS[usize::from(tier.min(2))];
                body.push(
                    progress_bar(0.0..=target as f32, boosts.min(target) as f32)
                        .girth(METER_HEIGHT)
                        .style(move |_| progress_bar::Style {
                            background: theme.field.into(),
                            bar: theme.accent.into(),
                            border: crate::style::round(METER_HEIGHT / 2.0),
                        })
                        .into(),
                );
                let progress = if tier == 3 {
                    "Highest standard boost level reached.".to_owned()
                } else if boosts < target {
                    let remaining = target - boosts;
                    let unit = if remaining == 1 { "boost" } else { "boosts" };
                    format!(
                        "{remaining} {unit} below the Level {} threshold ({target}).",
                        tier + 1
                    )
                } else {
                    "Total boosts meet the next threshold. Allocations and grace periods may affect the reported level.".to_owned()
                };
                body.push(form::note(theme, progress));
            } else {
                body.push(form::note(
                    theme,
                    "Boost count unavailable. This server is not sharing its progress bar.",
                ));
            }
            body.push(form::heading(theme, "STANDARD LEVEL THRESHOLDS"));
            body.push(
                column(LEVEL_BOOSTS.into_iter().enumerate().map(|(index, boosts)| {
                    row![
                        text(format!("Level {}", index + 1))
                            .color(theme.text)
                            .width(Length::Fill),
                        text(format!("{boosts} boosts")).color(theme.secondary),
                    ]
                    .into()
                }))
                .spacing(8)
                .into(),
            );
            body.push(form::note(
                theme,
                "Totals may include boosts allocated to separate perks. Only server managers can change those allocations in Discord.",
            ));
        }
        _ => body.push(form::note(
            theme,
            "Boost data is unavailable for this server. No current progress can be shown.",
        )),
    }
    let page = &cx.state.boosts;
    let mine = server.is_some()
        && cx.state.discord_online
        && page.server.as_deref() == Some(shown.server.as_str());
    let mut end: Vec<Element<'a, Message>> =
        vec![action(theme, tr("Done"), Kind::Ghost, Some(close.clone())).into()];
    if mine {
        body.extend(your_boosts(theme, page));
        if page.error.is_some() && !page.loading {
            end.push(
                action(
                    theme,
                    tr("Retry"),
                    Kind::Plain,
                    Some(Message::BoostGoal(Event::Reload)),
                )
                .into(),
            );
        }
        if page.slots.is_some_and(|slots| slots.free > 0) {
            end.push(
                action(
                    theme,
                    if page.applying {
                        tr("Boosting")
                    } else {
                        tr("Boost this server")
                    },
                    Kind::Primary,
                    (!page.applying).then_some(Message::BoostGoal(Event::Boost)),
                )
                .into(),
            );
        }
    }
    body.push(buttons(end));
    cx.look.framed(body, WIDTH, close)
}

/// What the account's boosts say under the server's progress: what is
/// being read, how many it can give, how many it has given, and why a try
/// failed.
fn your_boosts<'a>(theme: &Theme, page: &BoostPage) -> Vec<Element<'a, Message>> {
    let mut parts: Vec<Element<'a, Message>> = vec![form::heading(theme, tr("YOUR BOOSTS"))];
    match page.slots {
        None if page.loading => parts.push(form::note(theme, tr("Reading your boosts..."))),
        None => {}
        Some(slots) => {
            if slots.here > 0 {
                parts.push(form::note(
                    theme,
                    match slots.here {
                        1 => tr("You boost this server with 1 boost.").to_owned(),
                        count => fill(tr("You boost this server with {} boosts."), &[&count]),
                    },
                ));
            }
            parts.push(form::note(
                theme,
                match slots.free {
                    0 if slots.here + slots.elsewhere + slots.resting == 0 => {
                        tr("You have no boosts to give. Boosts come with Nitro.").to_owned()
                    }
                    0 => tr("You have no free boost to give.").to_owned(),
                    1 => tr("You have 1 boost to give.").to_owned(),
                    count => fill(tr("You have {} boosts to give."), &[&count]),
                },
            ));
            if slots.resting > 0 {
                parts.push(form::note(
                    theme,
                    match slots.resting {
                        1 => tr("1 more boost is resting after it was moved.").to_owned(),
                        count => fill(
                            tr("{} more boosts are resting after they were moved."),
                            &[&count],
                        ),
                    },
                ));
            }
        }
    }
    if page.given {
        parts.push(form::note_in(tr("Your boost was added."), theme.accent));
    }
    if let Some(error) = &page.error {
        parts.push(form::error(theme, error.clone()).into());
    }
    parts
}
