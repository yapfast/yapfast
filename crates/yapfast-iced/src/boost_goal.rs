//! A native, read-only summary of the current server's boosts. The screen
//! keeps only its server id; counts and level follow the shared snapshot.
//! Purchases and management go to Discord's official guide in the browser.

use iced::{
    Alignment, Element, Length, Point, Task,
    widget::{column, progress_bar, row, text},
};
use yapfast_core::application::{AppState, Navigation};

use crate::{
    app::Message,
    controls::{Kind, action},
    ctx::{Ctx, Cx, Focus, Request, Screen},
    dialog::buttons,
    fonts::BOLD,
    form,
    slot::{Closes, Why},
};

const WIDTH: f32 = 480.0;
const METER_HEIGHT: f32 = 8.0;
const LEVEL_BOOSTS: [u32; 3] = [2, 7, 14];

/// Verified against Discord's public support documentation. This is a
/// guide, rather than an undocumented checkout or account-settings route.
pub const GUIDE: &str =
    "https://support.discord.com/hc/en-us/articles/360028038352-Server-Boosting-FAQ";

#[derive(Clone, Debug)]
pub enum Event {
    Open,
    Close,
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
        Some(shown)
    }

    /// Navigation away closes the summary rather than showing another
    /// server's numbers under the previous one's name.
    pub fn follows(&self, state: &AppState) -> bool {
        matches!(&state.navigation, Navigation::Server(id) if *id == self.server)
    }

    pub fn update(&self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        if let Event::Close = event {
            cx.request(Request::Close(Screen::BoostGoal));
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
    body.push(form::note(
        theme,
        "For purchases and boost management, open Discord's official guide in your browser.",
    ));
    body.push(buttons([
        action(theme, "Done", Kind::Ghost, Some(close.clone())).into(),
        action(
            theme,
            "Purchase and management guide",
            Kind::Primary,
            Some(Message::Link(GUIDE.to_owned(), Point::ORIGIN)),
        )
        .into(),
    ]));
    cx.look.framed(body, WIDTH, close)
}
