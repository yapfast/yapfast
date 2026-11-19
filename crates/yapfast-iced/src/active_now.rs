//! Discord's Active Now beside the friends list: friends together in a
//! server's voice channel, or in one game party, or each on their own doing
//! something Discord can name, a card each, with how long they've been at
//! it. The cards are made as Discord's `NowPlayingViewStore` makes them,
//! only while the friends list shows.

use crate::widgets::scroll_view;
use std::collections::HashMap;

use iced::{
    Alignment, Element, Length, Padding, Point,
    widget::{Space, button, column, container, row, stack, text},
};
use yapfast_core::{
    application::{AppState, DmScreen},
    chat::{Activity, ActivityKind, ChatRoom, Friend, Presence},
};

use crate::{
    app::{App, Message},
    controls::{bar, placed},
    fonts::{BOLD, SEMIBOLD},
    people::voice_spots,
    pictures::{Face, initials, tint},
    style::{hex, rounded, scrollbar},
    theme::rgb,
    widgets::Pic,
};

/// The conversation's width from which Active Now shows, and its own.
pub const ROOM: f32 = 1000.0;
pub const WIDTH: f32 = 360.0;
/// The most faces a row shows before "+N".
const FACES: usize = 3;
/// Xbox's application, whose games Discord tells apart by name.
const XBOX: &str = "438122941302046720";

/// A card: friends doing one thing, or together in a voice channel.
#[derive(Debug, PartialEq)]
pub struct Card {
    /// "Ann and Bob", "Ann, Bob, and 1 other".
    title: String,
    /// What they're doing, "Dota 2" or "In a Voice Channel", and since
    /// when, in Unix seconds.
    doing: String,
    start: Option<u64>,
    /// The first of them, for the card's avatar, and their presence.
    face: Someone,
    presence: Option<Presence>,
    /// The game's or app's picture, at the card's right.
    icon: Option<String>,
    rows: Vec<CardRow>,
}

#[derive(Debug, PartialEq)]
struct Someone {
    id: String,
    name: String,
    avatar: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RowKind {
    /// The voice channel they're in: its server, the channel, who's there.
    Voice,
    /// A game several friends play: the game, how many.
    Game,
    /// What one of them is doing in full, and how long since it started.
    Detail,
}

#[derive(Debug, PartialEq)]
struct CardRow {
    kind: RowKind,
    title: String,
    detail: String,
    /// Its picture, and the name its initial and tint come from without.
    icon: Option<String>,
    named: String,
    start: Option<u64>,
    faces: Vec<Someone>,
    more: usize,
    /// A voice channel's room, which a click opens.
    room: Option<String>,
}

/// Makes the cards again while the friends list shows, and lets go of
/// them when it doesn't.
pub fn active_now_sync(app: &mut App) {
    if crate::friends::dm_screen(app) != Some(DmScreen::Friends) || !active_now_fits(app.ctx()) {
        app.active_now.clear();
        app.active_now_made = None;
        return;
    }
    // Made from the friends and the voice channels' people alone.
    let state = app.client.state();
    let made = Some((state.people_changes, state.list_changes));
    if app.active_now_made == made {
        return;
    }
    app.active_now_made = made;
    let cards = cards(app.client.state());
    if cards != app.active_now {
        app.active_now = cards;
    }
}

/// Whether a card shows a time that moves each minute.
pub fn active_now_minutes(app: &App) -> bool {
    app.active_now.iter().any(|card| card.start.is_some())
}

pub fn active_now_pictures(app: &App) -> Vec<String> {
    let mut urls = Vec::new();
    for card in &app.active_now {
        urls.extend(card.face.avatar.clone());
        urls.extend(card.icon.clone());
        for row in &card.rows {
            urls.extend(row.icon.clone());
            urls.extend(row.faces.iter().filter_map(|face| face.avatar.clone()));
        }
    }
    urls
}

/// "13m", "1h", "2d", as Discord's Active Now times a game.
fn since(seconds: u64) -> String {
    match seconds {
        seconds if seconds < 3600 => format!("{}m", (seconds / 60).max(1)),
        seconds if seconds < 86400 => format!("{}h", seconds / 3600),
        seconds => format!("{}d", seconds / 86400),
    }
}

/// "01:19:45 elapsed".
fn elapsed(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02} elapsed",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// "A", "A and B", "A, B, and 1 other", "A, B, and 4 others".
fn names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [one, two] => format!("{one} and {two}"),
        [one, two, rest @ ..] => {
            let others = if rest.len() == 1 { "other" } else { "others" };
            format!("{one}, {two}, and {} {others}", rest.len())
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Group<'a> {
    Voice(&'a str),
    Party(&'a str),
    Person(&'a str),
}

/// Discord's order: cards of more than one person first, then those in
/// voice, then those doing something, Spotify's after the rest, then by
/// name.
type Order = (
    std::cmp::Reverse<bool>,
    std::cmp::Reverse<bool>,
    std::cmp::Reverse<bool>,
    bool,
    String,
);

/// One application's activity in a card, and the friends at it.
struct Doing<'a> {
    activity: &'a Activity,
    friends: Vec<&'a Friend>,
}

/// The voice channel someone's in: its room, its name and its server's.
type Spot<'a> = (&'a str, &'a str, &'a str);

/// The cards: friends in one voice channel share a card, then those in one
/// game party, and everyone else has their own. Each friend counts by
/// their first activity, and a card needs a voice channel or an activity
/// Discord can name.
fn cards(state: &AppState) -> Vec<Card> {
    let spots = voice_spots(state);
    let mut groups: Vec<Vec<&Friend>> = Vec::new();
    let mut index: HashMap<Group<'_>, usize> = HashMap::new();
    for friend in &state.friends {
        let group = match spots.get(friend.id.as_str()) {
            Some(spot) => Group::Voice(spot.0),
            None => match friend
                .activities
                .first()
                .and_then(|activity| activity.party_id.as_deref())
            {
                Some(party) => Group::Party(party),
                None => Group::Person(&friend.id),
            },
        };
        match index.get(&group) {
            Some(&at) => groups[at].push(friend),
            None => {
                index.insert(group, groups.len());
                groups.push(vec![friend]);
            }
        }
    }
    let mut cards: Vec<(Order, Card)> = groups
        .iter()
        .filter_map(|friends| {
            let room = spots.get(friends[0].id.as_str()).and_then(|spot| {
                let room = state.rooms.iter().find(|room| room.id == spot.0)?;
                Some((*spot, &**room))
            });
            group_card(state, room, friends)
        })
        .collect();
    cards.sort_by(|(one, _), (other, _)| one.cmp(other));
    cards.into_iter().map(|(_, card)| card).collect()
}

fn group_card(
    state: &AppState,
    room: Option<(Spot<'_>, &ChatRoom)>,
    friends: &[&Friend],
) -> Option<(Order, Card)> {
    // Each application once, with the friends at it, the latest started
    // first.
    let mut doing: Vec<(String, Doing<'_>)> = Vec::new();
    let mut spotify = false;
    for &friend in friends {
        let Some(activity) = friend.activities.first() else {
            continue;
        };
        let Some(application) = application(activity) else {
            continue;
        };
        spotify = activity.spotify();
        match doing.iter_mut().find(|(each, _)| *each == application) {
            Some((_, at)) => at.friends.push(friend),
            None => doing.push((
                application,
                Doing {
                    activity,
                    friends: vec![friend],
                },
            )),
        }
    }
    if room.is_none() && doing.is_empty() {
        return None;
    }
    let mut doing: Vec<Doing<'_>> = doing.into_iter().map(|(_, doing)| doing).collect();
    doing.sort_by(|one, other| {
        other
            .activity
            .start
            .unwrap_or_default()
            .cmp(&one.activity.start.unwrap_or_default())
            .then_with(|| one.activity.name.cmp(&other.activity.name))
    });
    let people = room.map_or(friends.len(), |(_, room)| {
        room.people.len().max(friends.len())
    });
    let order = (
        std::cmp::Reverse(people > 1),
        std::cmp::Reverse(room.is_some()),
        std::cmp::Reverse(!doing.is_empty()),
        spotify,
        friends
            .iter()
            .map(|friend| friend.name.to_lowercase())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let card = match room {
        Some((spot, room)) => voice_card(state, spot, room, friends, doing.first()),
        None => doing_card(friends, &doing),
    };
    Some((order, card))
}

/// What Discord's Active Now tells an activity by: Spotify, a Twitch or
/// YouTube stream, its application, or an Xbox game's name. Anything else,
/// a Hang or a game without an application, has no card.
fn application(activity: &Activity) -> Option<String> {
    if activity.spotify() {
        return Some("spotify".to_owned());
    }
    if activity.kind == ActivityKind::Streaming
        && let Some(url) = activity.url.as_deref().filter(|url| stream_link(url))
    {
        return Some(format!("url:{url}"));
    }
    match activity.application_id.as_deref()? {
        XBOX => Some(format!("name:{}", activity.name)),
        id => Some(id.to_owned()),
    }
}

/// A Twitch channel or a YouTube video, as Discord's
/// `^https?://(?:www\.)?(twitch\.tv/(.+)|youtube.com/watch\?v=)`.
fn stream_link(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.strip_prefix("twitch.tv/")
        .is_some_and(|name| !name.is_empty())
        || rest.starts_with("youtube.com/watch?v=")
}

/// Friends in one voice channel: the server, the channel and who's in it,
/// and what the one latest at it is doing.
fn voice_card(
    state: &AppState,
    (room_id, channel, server_name): Spot<'_>,
    room: &ChatRoom,
    friends: &[&Friend],
    doing: Option<&Doing<'_>>,
) -> Card {
    let server = state
        .servers
        .iter()
        .find(|server| server.channels.iter().any(|each| each == room_id));
    let (faces, more) = faces(
        room.people
            .iter()
            .map(|person| someone(&person.id, &person.name, person.avatar.as_deref())),
    );
    let mut rows = vec![CardRow {
        kind: RowKind::Voice,
        title: server_name.to_owned(),
        detail: channel.to_owned(),
        icon: server.and_then(|server| server.icon.clone()),
        named: server_name.to_owned(),
        start: None,
        faces,
        more,
        room: Some(room_id.to_owned()),
    }];
    let activity = doing.map(|doing| doing.activity);
    rows.extend(
        activity
            .filter(|activity| has_details(activity))
            .map(detail_row),
    );
    card(
        friends,
        activity.map_or("In a Voice Channel".to_owned(), self::doing),
        activity
            .and_then(|activity| activity.start)
            .map(|ms| ms / 1000),
        activity.and_then(|activity| activity.large_image.as_deref()),
        rows,
    )
}

/// Friends in no voice channel: one, or a game party, and what they're
/// doing.
fn doing_card(friends: &[&Friend], doing: &[Doing<'_>]) -> Card {
    let first = &doing[0];
    let activity = first.activity;
    let what = self::doing(activity);
    let mut rows: Vec<CardRow> = doing
        .iter()
        .map(|doing| doing.activity)
        .filter(|activity| has_details(activity))
        .take(2)
        .map(detail_row)
        .collect();
    if first.friends.len() > 1 {
        let (faces, more) = faces(
            first
                .friends
                .iter()
                .map(|friend| someone(&friend.id, &friend.name, friend.avatar.as_deref())),
        );
        rows.push(CardRow {
            kind: RowKind::Game,
            title: what.clone(),
            detail: format!("{} People", first.friends.len()),
            icon: activity.large_image.as_deref().map(str::to_owned),
            named: what.clone(),
            start: None,
            faces,
            more,
            room: None,
        });
    }
    card(
        friends,
        what,
        activity.start.map(|ms| ms / 1000),
        activity.large_image.as_deref(),
        rows,
    )
}

/// A card headed by its friends' names and the first one's face.
fn card(
    friends: &[&Friend],
    doing: String,
    start: Option<u64>,
    icon: Option<&str>,
    rows: Vec<CardRow>,
) -> Card {
    let first = friends[0];
    let names: Vec<&str> = friends.iter().map(|friend| friend.name.as_str()).collect();
    Card {
        title: self::names(&names),
        doing,
        start,
        face: someone(&first.id, &first.name, first.avatar.as_deref()),
        presence: first.presence,
        icon: icon.map(str::to_owned),
        rows,
    }
}

/// How a card says it: "Dota 2", "Listening to Spotify".
fn doing(activity: &Activity) -> String {
    match activity.kind {
        ActivityKind::Playing => activity.name.to_string(),
        _ => activity.line().unwrap_or_else(|| activity.name.to_string()),
    }
}

/// The activity's own row, when it says more than its name: Rich
/// Presence's details and state, such as Medal's "Clipping Dota 2".
fn has_details(activity: &Activity) -> bool {
    activity.details.is_some() || activity.state.is_some()
}

fn detail_row(activity: &Activity) -> CardRow {
    let title = activity.details.as_deref().unwrap_or(&activity.name);
    CardRow {
        kind: RowKind::Detail,
        title: title.to_owned(),
        detail: activity
            .state
            .as_deref()
            .map_or_else(|| format!("with {}", activity.name), str::to_owned),
        icon: activity
            .small_image
            .as_deref()
            .or(activity.large_image.as_deref())
            .map(str::to_owned),
        named: activity.name.to_string(),
        start: activity.start.map(|ms| ms / 1000),
        faces: Vec::new(),
        more: 0,
        room: None,
    }
}

fn someone(id: &str, name: &str, avatar: Option<&str>) -> Someone {
    Someone {
        id: id.to_owned(),
        name: name.to_owned(),
        avatar: avatar.map(str::to_owned),
    }
}

/// A face row of people, and how many more there are.
fn faces(people: impl Iterator<Item = Someone>) -> (Vec<Someone>, usize) {
    let mut all: Vec<_> = people.collect();
    let more = all.len().saturating_sub(FACES);
    all.truncate(FACES);
    (all, more)
}

/// The column beside the friends list.
pub fn view<'a>(ctx: crate::ctx::Ctx<'a>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    // Discord's cards run from 17 px in to 16 px short of the right, and a
    // dark theme's column is a shade under the chat's.
    let mut page = column![text("Active Now").size(20).font(BOLD).color(theme.text)]
        .spacing(10)
        .padding(Padding {
            top: 13.0,
            left: 17.0,
            right: 16.0,
            bottom: 16.0,
        })
        .height(Length::Fill);
    if ctx.active_now.is_empty() {
        page = page.push(
            column![
                text("It's quiet for now...")
                    .size(16)
                    .font(BOLD)
                    .color(theme.text)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                text(
                    "When a friend starts an activity \u{2014} like playing a game or hanging out on voice \u{2014} we'll show it here!"
                )
                .size(13)
                .color(theme.muted)
                .width(Length::Fill)
                .align_x(Alignment::Center),
            ]
            .spacing(6)
            .padding(Padding {
                top: 8.0,
                ..Padding::ZERO
            }),
        );
    } else {
        let now = yapfast_core::clock::now_ms() / 1000;
        let mut cards = column![].spacing(12);
        for card in ctx.active_now {
            cards = cards.push(active_card(ctx, card, now));
        }
        page = page.push(
            scroll_view(cards)
                .height(Length::Fill)
                .style(scrollbar(theme)),
        );
    }
    let (panel, line) = if theme.bonfire {
        (hex(0x1a1a1e), hex(0x28282d))
    } else {
        (theme.panel, theme.line)
    };
    row![
        bar(line, 1, Length::Fill),
        container(page)
            .width(WIDTH - 1.0)
            .height(Length::Fill)
            .style(move |_| rounded(panel, 0))
    ]
    .into()
}

fn active_card<'a>(ctx: crate::ctx::Ctx<'a>, card: &'a Card, now: u64) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let since = card
        .start
        .map(|start| format!(" \u{2013} {}", since(now.saturating_sub(start))))
        .unwrap_or_default();
    // Discord's dark Active Now card: a lighter head over a darker body,
    // joined by a step of colour rather than a line.
    let (ground, head_ground, edge) = if theme.bonfire {
        (hex(0x202024), hex(0x29292d), hex(0x2e2e32))
    } else {
        (theme.raised, theme.raised, theme.edge)
    };
    let face = Face::new(card.face.avatar.as_deref(), &card.face.name, 32.0);
    let face = ctx.face(match card.presence {
        // The ring takes the head's colour at rest.
        Some(presence) => face.dot(Some((presence, rgb(head_ground), Point::new(20.0, 20.0)))),
        None => face,
    });
    let mut head = row![
        face,
        column![
            text(card.title.as_str())
                .size(15)
                .font(BOLD)
                .color(theme.text)
                .wrapping(text::Wrapping::None),
            text(format!("{}{since}", card.doing))
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        ]
        .width(Length::Fill),
    ]
    .spacing(12)
    .padding(Padding::ZERO.left(17).right(16))
    .height(69)
    .align_y(Alignment::Center);
    if let Some(icon) = &card.icon {
        head = head.push(row_icon(ctx, Some(icon), "", false, 24.0));
    }
    let hover = theme.hover;
    let head = button(head)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Friends(crate::friends::Event::Menu(
            card.face.id.clone(),
        )))
        .style(crate::controls::lit_style(
            theme.bonfire.then_some(head_ground),
            hover,
            0.0,
        ));
    let mut body = column![head];
    for entry in &card.rows {
        body = body.push(active_row(ctx, entry));
    }
    container(body)
        .width(Length::Fill)
        .clip(true)
        .style(move |_| crate::style::outlined(ground, edge, 16.0))
        .into()
}

fn active_row<'a>(ctx: crate::ctx::Ctx<'a>, entry: &'a CardRow) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut words = column![
        text(entry.title.as_str())
            .size(14)
            .font(SEMIBOLD)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
    ]
    .width(Length::Fill);
    if !entry.detail.is_empty() {
        words = words.push(
            text(entry.detail.as_str())
                .size(12)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        );
    }
    // Its time moves by itself, each second: no message, no view.
    if entry.kind == RowKind::Detail
        && let Some(start) = entry.start
    {
        words = words.push(
            crate::widgets::live_text(std::time::Duration::from_secs(1), move || {
                elapsed((yapfast_core::clock::now_ms() / 1000).saturating_sub(start))
            })
            .size(12)
            .color(theme.muted),
        );
    }
    let mut line = row![
        row_icon(
            ctx,
            entry.icon.as_deref(),
            &entry.named,
            entry.kind == RowKind::Voice,
            32.0
        ),
        words
    ]
    .spacing(12)
    .padding(Padding {
        top: 16.0,
        left: 17.0,
        right: 16.0,
        bottom: 16.0,
    })
    .align_y(Alignment::Center);
    if !entry.faces.is_empty() || entry.more > 0 {
        line = line.push(faces_row(ctx, &entry.faces, entry.more));
    }
    // At least 83 px tall, taller for a third line.
    let line = line.push(Space::new().width(0).height(51));
    let Some(room) = &entry.room else {
        return line.into();
    };
    let hover = theme.hover;
    button(line)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Side(crate::sidebar::Event::Open(room.clone())))
        .style(crate::controls::lit_style(None, hover, 0.0))
        .into()
}

/// A row's picture: a server's icon, or a game's; else its initial on
/// its tint.
fn row_icon<'a>(
    ctx: crate::ctx::Ctx<'a>,
    url: Option<&str>,
    named: &str,
    round: bool,
    side: f32,
) -> Element<'a, Message> {
    let radius = if round { side / 2.0 } else { 8.0 };
    if let Some(picture) = url.and_then(|url| ctx.picture(url, side as u32, side as u32, true)) {
        return Pic::square(picture, side).radius(radius).into();
    }
    if named.is_empty() {
        return Space::new().width(side).height(side).into();
    }
    let ground = hex(tint(named));
    container(
        text(initials(named))
            .size(13)
            .font(BOLD)
            .color(iced::Color::WHITE),
    )
    .center(side)
    .style(move |_| rounded(ground, radius))
    .into()
}

/// Overlapping faces, and how many more.
fn faces_row<'a>(
    ctx: crate::ctx::Ctx<'a>,
    faces: &'a [Someone],
    more: usize,
) -> Element<'a, Message> {
    let raised = ctx.look.theme.raised;
    let count = faces.len() + usize::from(more > 0);
    let width = count as f32 * 20.0 + 6.0;
    let mut layers = stack![Space::new().width(width).height(26)];
    for (index, face) in faces.iter().enumerate() {
        layers = layers.push(placed(
            container(ctx.avatar(face.avatar.as_deref(), &face.name, 22.0, 11.0, 1.0))
                .center(26)
                .style(move |_| rounded(raised, 13)),
            index as f32 * 20.0,
            0.0,
        ));
    }
    if more > 0 {
        let (sunken, secondary) = (ctx.look.theme.sunken, ctx.look.theme.secondary);
        layers = layers.push(placed(
            container(
                container(
                    text(format!("+{more}"))
                        .size(10)
                        .font(BOLD)
                        .color(secondary),
                )
                .center(22)
                .style(move |_| rounded(sunken, 11)),
            )
            .center(26)
            .style(move |_| rounded(raised, 13)),
            faces.len() as f32 * 20.0,
            0.0,
        ));
    }
    layers.into()
}

/// Whether the Friends screen is wide enough for the column: narrower,
/// it isn't drawn, so its cards aren't made, timed or fetched for.
pub fn active_now_fits(ctx: crate::ctx::Ctx<'_>) -> bool {
    ctx.look.window.width - crate::conversation::navigation(ctx.layout) >= ROOM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_shorten_as_discords_active_now_does() {
        assert_eq!(names(&["A"]), "A");
        assert_eq!(names(&["A", "B"]), "A and B");
        assert_eq!(names(&["A", "B", "C"]), "A, B, and 1 other");
        assert_eq!(names(&["A", "B", "C", "D", "E"]), "A, B, and 3 others");
    }

    /// Discord's NowPlayingViewStore: a card per party or per person, never
    /// per game; a Hang or a game without an application has none; parties
    /// first, Spotify last, then by name.
    #[test]
    fn active_now_groups_and_orders_cards_as_discords_does() {
        let friend = |id: &str, name: &str, activity: serde_json::Value| Friend {
            id: id.into(),
            name: name.into(),
            decoration: None,
            nickname: None,
            avatar: None,
            presence: Some(Presence::Online),
            activity: None,
            activities: yapfast_core::yapfast_discord::activity::activities(&serde_json::json!([
                activity
            ])),
            since: None,
        };
        let game = |party: Option<&str>| {
            let mut game =
                serde_json::json!({"type": 0, "name": "Dota 2", "application_id": "570"});
            if let Some(party) = party {
                game["party"] = serde_json::json!({"id": party, "size": [2, 5]});
            }
            game
        };
        let mut state = AppState::default();
        state.friends = vec![
            friend(
                "1",
                "Eve",
                serde_json::json!({"type": 2, "name": "Spotify", "details": "Song", "state": "Band", "party": {"id": "spotify:1"}}),
            ),
            friend("2", "bob", game(None)),
            friend("3", "Ann", game(None)),
            friend("4", "Cara", game(Some("p1"))),
            friend("5", "Dan", game(Some("p1"))),
            friend(
                "6",
                "Finn",
                serde_json::json!({"type": 6, "name": "Hang Status", "state": "custom", "details": "brb"}),
            ),
            friend(
                "7",
                "Gus",
                serde_json::json!({"type": 0, "name": "Solitaire"}),
            ),
        ];
        let cards = cards(&state);
        let titles: Vec<&str> = cards.iter().map(|card| card.title.as_str()).collect();
        assert_eq!(titles, ["Cara and Dan", "Ann", "bob", "Eve"]);
        assert_eq!(cards[0].doing, "Dota 2");
        assert!(cards[0].rows.iter().any(|row| row.detail == "2 People"));
        assert_eq!(cards[3].doing, "Listening to Spotify");
    }

    #[test]
    fn a_stream_counts_only_from_twitch_or_youtube_as_in_discord() {
        assert!(stream_link("https://www.twitch.tv/someone"));
        assert!(stream_link("http://youtube.com/watch?v=abc"));
        assert!(!stream_link("https://twitch.tv/"));
        assert!(!stream_link("https://kick.com/someone"));
    }

    #[test]
    fn times_read_as_discords_active_now() {
        assert_eq!(since(30), "1m");
        assert_eq!(since(13 * 60), "13m");
        assert_eq!(since(2 * 3600), "2h");
        assert_eq!(since(3 * 86400), "3d");
        assert_eq!(elapsed(4785), "01:19:45 elapsed");
    }
}
