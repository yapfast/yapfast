//! A server tag's card, as Discord's: a click on a tag beside a name in a
//! message or on a profile opens the card of the server the tag is from,
//! under the press. It shows the tag, and the server's name and icon when
//! this account is in it, while Discord's answer loads
//! (`Action::OpenServerTag`, `AppState::tag_server`); then the server's
//! profile: its counts, when it was made and what it says of itself. A
//! server that shows its profile only to its members says so, one that
//! failed to load offers to try again, and Go to Server shows for a server
//! this account is in. A click outside or Escape closes it; the member
//! list's tags don't open it, as in Discord's client.

use iced::{
    Alignment, Element, Length, Point, Size, Task, mouse,
    widget::{button, column, container, mouse_area, opaque, row, text},
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    chat::ServerTag,
};

use crate::{
    app::Message,
    controls::{self, Kind, lights},
    ctx::{Ctx, Cx, Focus, Request, Screen},
    elide::elide,
    fonts::{BOLD, SEMIBOLD},
    info_cards::frame,
    looks::Chip,
    media::{Fit, Shot},
    pictures::Icon,
    slot::{Closes, Why},
    style::rounded,
    widgets::Place,
};

/// The card's width, as Discord's, and the server's icon.
const WIDTH: f32 = 300.0;
const ICON: f32 = 64.0;

#[derive(Clone, Debug)]
pub enum Event {
    /// A tag clicked: its server's id, its letters and its badge.
    Open {
        server: String,
        text: String,
        badge: Option<String>,
    },
    /// "Unable to load server profile" clicked: Discord is asked again.
    Retry,
    /// Go to Server, or the server's name.
    Go,
    Close,
}

/// The card open: the tag's server, its letters and badge, and where it
/// was clicked.
pub struct Shown {
    pub server: String,
    text: String,
    badge: Option<String>,
    at: Point,
}

/// What the card shows, as Discord's client does, worked out from the
/// core's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    /// The server's name, empty while it isn't known, and the letter its
    /// icon's place shows without one.
    pub name: String,
    pub initial: String,
    pub icon: Option<String>,
    /// "1,200 Online" and "4,644 Members", when Discord counts them.
    pub online: Option<String>,
    pub members: Option<String>,
    /// When it was made: "Est. Nov 2026".
    pub established: Option<String>,
    pub description: String,
    /// Why there's no more to show, when Discord didn't show the server.
    pub note: &'static str,
    /// Discord hasn't answered yet; the server shows its profile only to
    /// its members; or it failed to load, which a click tries again.
    pub loading: bool,
    pub limited: bool,
    pub failed: bool,
    /// This account is in the server, which Go to Server opens.
    pub joined: bool,
}

/// The card for `shown` as the core's state has it.
pub fn card(shown: &Shown, state: &AppState) -> Card {
    let joined = state
        .servers
        .iter()
        .find(|server| server.id == shown.server && !server.unavailable);
    let answer = state
        .tag_server
        .as_ref()
        .filter(|(server, _)| *server == shown.server)
        .map(|(_, answer)| answer);
    let profile = answer.and_then(|answer| answer.as_ref().ok()?.as_ref());
    let (limited, failed) = (
        matches!(answer, Some(Ok(None))),
        matches!(answer, Some(Err(_))),
    );
    let known = joined.map(|server| server.name.as_str());
    let name = match (profile, limited, failed) {
        (Some(profile), ..) => profile.name.as_str(),
        (None, true, _) => known.unwrap_or("Private Server"),
        (None, _, true) => known.unwrap_or("Unknown Server"),
        _ => known.unwrap_or_default(),
    };
    let icon = profile
        .and_then(|profile| profile.icon.clone())
        .or_else(|| joined.and_then(|server| server.icon.clone()));
    let count = |count: Option<u64>, one: &str, many: &str| {
        count.map(|count| {
            let words = if count == 1 { one } else { many };
            format!("{} {words}", yapfast_core::emoji::card::grouped(count))
        })
    };
    Card {
        initial: if name.is_empty() {
            shown
                .text
                .chars()
                .next()
                .map(String::from)
                .unwrap_or_default()
        } else {
            crate::pictures::initials(name)
        },
        name: name.to_owned(),
        icon,
        online: count(
            profile.and_then(|profile| profile.online),
            "Online",
            "Online",
        ),
        members: count(
            profile.and_then(|profile| profile.members),
            "Member",
            "Members",
        ),
        established: profile
            .map(|profile| yapfast_core::clock::month(profile.created_ms / 1000))
            .filter(|month| !month.is_empty())
            .map(|month| format!("Est. {month}")),
        description: profile.map_or_else(String::new, |profile| profile.description.clone()),
        note: match (limited, failed) {
            (true, _) => "The server has limited who can see this profile.",
            (_, true) => "Unable to load server profile",
            _ => "",
        },
        loading: answer.is_none(),
        limited,
        failed,
        joined: joined.is_some(),
    }
}

/// A tag's chip, which opens its server's card when Discord said which
/// server it's from.
pub fn opens<'a>(chip: Element<'a, Message>, tag: &ServerTag) -> Element<'a, Message> {
    let Some(guild) = tag.guild else {
        return chip;
    };
    mouse_area(chip)
        .interaction(mouse::Interaction::Pointer)
        .on_press(Message::TagCard(Event::Open {
            server: yapfast_core::discord::server_id(&guild.to_string()),
            text: tag.text.to_string(),
            badge: tag.badge.as_deref().map(str::to_owned),
        }))
        .into()
}

impl Shown {
    /// The card of `server`'s tag, clicked at `at`: Discord is asked first,
    /// so a card that failed before loads afresh.
    pub fn open(
        server: String,
        text: String,
        badge: Option<String>,
        at: Point,
        cx: &mut Cx<'_>,
    ) -> Self {
        cx.dispatch(Action::OpenServerTag(server.clone()));
        Self {
            server,
            text,
            badge,
            at,
        }
    }

    /// What's done on the card; a tag clicked is the root's, which knows
    /// the press (`App::tag_card_event`).
    pub fn update(&self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Retry => cx.dispatch(Action::OpenServerTag(self.server.clone())),
            Event::Go => {
                cx.dispatch(Action::CloseProfile);
                cx.dispatch(Action::Navigate(Navigation::Server(self.server.clone())));
                cx.request(Request::Close(Screen::TagCard));
            }
            Event::Close => cx.request(Request::Close(Screen::TagCard)),
            Event::Open { .. } => {}
        }
        Task::none()
    }

    /// The pictures the card shows: the server's icon and the tag's badge.
    pub fn pictures(&self, state: &AppState) -> Vec<String> {
        card(self, state)
            .icon
            .into_iter()
            .chain(self.badge.clone())
            .collect()
    }
}

impl Closes for Shown {
    const LEAVES: bool = true;

    /// The keys go back to where they were, unless the window went.
    fn close(&mut self, cx: &mut Cx<'_>, why: Why) {
        if why != Why::Window {
            cx.request(Request::Focus(Focus::Default));
        }
        cx.touch();
    }
}

/// The card over a layer that closes it on a click outside, under the
/// press that opened it.
pub fn layers<'a>(shown: &'a Shown, cx: Ctx<'a>) -> [Element<'a, Message>; 2] {
    let at = Point::new(shown.at.x - 8.0, shown.at.y + 12.0);
    [
        controls::outside(Message::TagCard(Event::Close), false),
        Place::new(
            crate::widgets::motion::grow(opaque(view(shown, cx))),
            at,
            8.0,
        )
        .into(),
    ]
}

/// The card: the server's icon beside its name and the tag, its counts,
/// when it was made and its description, and a way in when this account is
/// in it. What Discord hasn't said yet loads.
fn view<'a>(shown: &'a Shown, cx: Ctx<'a>) -> Element<'a, Message> {
    let card = card(shown, cx.state);
    let theme = cx.look.theme;
    let raised = theme.raised;
    let picture = card
        .icon
        .as_deref()
        .and_then(|url| cx.state.pictures.get(url));
    let icon: Element<'a, Message> = match picture {
        Some(picture) => cx.shot(Shot {
            picture: Some(picture),
            size: Size::new(ICON, ICON),
            fit: Fit::Cover,
            radius: 18.0,
            covered: false,
            ground: true,
        }),
        None => {
            let inside: Element<'a, Message> = if card.limited || card.failed {
                let mark = if card.failed {
                    Icon::TriangleAlert
                } else {
                    Icon::Lock
                };
                cx.look.glyph(mark, 24, theme.secondary).into()
            } else {
                text(card.initial.clone())
                    .size(22)
                    .font(BOLD)
                    .color(theme.text)
                    .into()
            };
            container(inside)
                .center(ICON)
                .style(move |_| rounded(raised, 18))
                .into()
        }
    };
    let mut names = column![].spacing(4).width(Length::Fill);
    if !card.name.is_empty() {
        let name = elide(card.name.clone())
            .size(16)
            .font(SEMIBOLD)
            .color(theme.text);
        names = names.push(if card.joined {
            Element::from(
                mouse_area(name)
                    .interaction(mouse::Interaction::Pointer)
                    .on_press(Message::TagCard(Event::Go)),
            )
        } else {
            name.into()
        });
    } else if card.loading {
        names = names.push(controls::bar(raised, 140, 16).style(move |_| rounded(raised, 4)));
    }
    names = names.push(cx.tag_chip(
        &shown.text,
        shown.badge.as_deref(),
        Chip {
            size: 12.0,
            font: SEMIBOLD,
            height: 20.0,
            pad: 6.0,
            radius: 4.0,
            fill: theme.raised,
            ink: theme.text,
        },
        1.0,
    ));
    let mut body = column![row![icon, names].spacing(12).align_y(Alignment::Center)]
        .spacing(12)
        .padding(16);
    let count = |dot: iced::Color, words: String| {
        row![
            controls::bar(dot, 8, 8).style(move |_| rounded(dot, 4)),
            text(words).size(13).color(theme.secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    if card.online.is_some() || card.members.is_some() {
        let mut counts = row![].spacing(14);
        if let Some(online) = card.online.clone() {
            counts = counts.push(count(theme.live, online));
        }
        if let Some(members) = card.members.clone() {
            counts = counts.push(count(theme.faint, members));
        }
        body = body.push(counts);
    }
    if card.loading {
        body =
            body.push(controls::bar(raised, Length::Fill, 14).style(move |_| rounded(raised, 4)));
    }
    if let Some(established) = card.established.clone() {
        body = body.push(text(established).size(13).color(theme.secondary));
    }
    if !card.description.is_empty() {
        body = body.push(text(card.description.clone()).size(14).color(theme.text));
    }
    if card.failed {
        // A failure's note tries again when clicked, as in Discord's client.
        let (rest, lit) = (theme.muted, theme.text);
        body = body.push(
            button(text(card.note).size(13))
                .padding(0)
                .on_press(Message::TagCard(Event::Retry))
                .style(move |_, status| button::Style {
                    text_color: if lights(status) { lit } else { rest },
                    ..Default::default()
                }),
        );
    } else if !card.note.is_empty() {
        body = body.push(text(card.note).size(13).color(theme.muted));
    }
    if card.joined {
        body = body.push(
            controls::action(
                theme,
                "Go to Server",
                Kind::Primary,
                Some(Message::TagCard(Event::Go)),
            )
            .wide(36.0),
        );
    }
    frame(theme, theme.sidebar, WIDTH, 12.0, body).into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iced::Point;
    use yapfast_core::{
        application::AppState,
        chat::{ChatServer, TagServer},
    };

    use super::{Shown, card};

    fn shown() -> Shown {
        Shown {
            server: "discord:10".into(),
            text: "UFO".into(),
            badge: Some("https://cdn.discordapp.com/clan-badges/10/abc.png?size=32".into()),
            at: Point::ORIGIN,
        }
    }

    #[test]
    fn a_tags_card_fills_in_as_discord_answers() {
        let mut state = AppState::default();
        let tag = shown();
        // Before Discord answers, the tag shows alone, its letter in the
        // icon's place, and the rest loads.
        let asked = card(&tag, &state);
        assert_eq!((asked.name.as_str(), asked.initial.as_str()), ("", "U"));
        assert_eq!((asked.online.as_deref(), asked.note), (None, ""));
        assert!(asked.loading);
        assert!(!asked.joined, "no way in to a server this account isn't in");
        let answer = |result| Some(("discord:10".to_owned(), result));
        state.tag_server = answer(Ok(Some(TagServer {
            name: "Aliens".into(),
            icon: Some("https://cdn.discordapp.com/icons/10/abc.png".into()),
            description: "We come in peace".into(),
            members: Some(4644),
            online: Some(1),
            // 2026-11-05, as the server's id says.
            created_ms: 1_793_880_000_000,
        })));
        let answered = card(&tag, &state);
        assert_eq!(
            (answered.name.as_str(), answered.initial.as_str()),
            ("Aliens", "A")
        );
        assert_eq!(
            answered.icon.as_deref(),
            Some("https://cdn.discordapp.com/icons/10/abc.png")
        );
        assert_eq!(answered.online.as_deref(), Some("1 Online"));
        assert_eq!(
            answered.members.as_deref(),
            Some("4,644 Members"),
            "grouped as Discord writes it"
        );
        let established = answered.established.clone().expect("when it was made");
        assert!(established.starts_with("Est. ") && established.ends_with(" 2026"));
        assert_eq!(answered.description, "We come in peace");
        assert!(!answered.loading && !answered.joined);
        // Another server's answer isn't this card's.
        state.tag_server = Some(("discord:11".into(), Ok(None)));
        assert!(card(&tag, &state).loading);
        // A server that shows its profile only to its members says so, as
        // Discord's client does; one that failed to load offers to try again.
        state.tag_server = answer(Ok(None));
        let limited = card(&tag, &state);
        assert_eq!(limited.name, "Private Server");
        assert_eq!(
            limited.note,
            "The server has limited who can see this profile."
        );
        assert!(limited.limited && !limited.failed);
        state.tag_server = answer(Err("Discord is out of reach".into()));
        let failed = card(&tag, &state);
        assert_eq!(failed.name, "Unknown Server");
        assert_eq!(failed.note, "Unable to load server profile");
        assert!(failed.failed && !failed.limited);
    }

    #[test]
    fn a_tag_of_a_server_this_account_is_in_names_it_and_offers_a_way_in() {
        let mut state = AppState::default();
        state.servers.push(Arc::new(ChatServer {
            id: "discord:10".into(),
            name: "Crew".into(),
            ..Default::default()
        }));
        let crew = card(&shown(), &state);
        assert_eq!(crew.name, "Crew", "known before Discord answers");
        assert!(crew.joined && crew.loading);
        // A private answer keeps the name the account knows.
        state.tag_server = Some(("discord:10".into(), Ok(None)));
        assert_eq!(card(&shown(), &state).name, "Crew");
    }
}
