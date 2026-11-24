//! The popout under the header's search box, as Discord's opens while the
//! box has the keys. With the box empty: its Filters (from, in for a
//! server, has, mentions) and the place's History, kept for the next start
//! with a button that clears it. With words typed: Search for them first,
//! then the people and channels the word under the caret may be, three of
//! each; without any, the Filters. After a filter's key: that filter's
//! answers, ten at most with the first lit, or for a date a calendar.
//!
//! Up and Down light a row and wrap, Enter takes the lit row or searches,
//! a click takes one. A filter, or a person or channel for the word, goes
//! into the box; an answer, a day or a history row searches at once.

use std::ops::Range;

use chrono::{Datelike, NaiveDate};
use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{Space, button, column, container, mouse_area, rich_text, row, text},
};
use yapfast_core::{
    application::AppState,
    discord, mention,
    yapfast_discord::search::{self, Filter},
};

use crate::{
    app::{App, Message},
    controls::{lit_style, placed},
    ctx::Ctx,
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
    search::{Event, Searching},
};

/// The popout's width, the gap over it, and a row's height.
const WIDTH: f32 = 420.0;
const GAP: f32 = 6.0;
const ROW: f32 = 34.0;
/// How many searches a place keeps, how many answers a filter offers, and
/// how many people or channels the word under the caret may be, of each.
pub const HISTORY: usize = 5;
const ANSWERS: usize = 10;
const SUGGESTED: usize = 3;
/// The first day the calendar offers: Discord's first.
const FIRST_DAY: (i32, u32, u32) = (2015, 5, 15);

/// What the popout shows.
pub enum Shown {
    Rows(Vec<Group>),
    /// A date filter's calendar, its day put in place of `at`.
    Calendar {
        at: Range<usize>,
    },
}

/// Rows under a heading; History's heading has the button that clears it.
pub struct Group {
    heading: Option<&'static str>,
    clears: bool,
    rows: Vec<Row>,
}

#[derive(Clone, Debug)]
pub enum Row {
    /// Search for what's typed.
    SearchFor(String),
    /// A filter's key, put in at the caret.
    Key {
        key: &'static str,
        label: &'static str,
        hint: &'static str,
    },
    /// The word under the caret, `at`, made `filter`'s answer.
    Turn {
        at: Range<usize>,
        filter: Filter,
        value: String,
        face: Face,
    },
    /// A filter's answer in place of what's typed after its key, `at`.
    Answer {
        at: Range<usize>,
        value: String,
        face: Face,
    },
    History(String),
    /// The Filters dialog: More filters at the Filters' end, or Add search
    /// filters before the people and channels a word may be.
    MoreFilters,
    AddFilters,
}

/// How a person's or channel's row looks.
#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    Person {
        name: String,
        username: String,
        avatar: Option<String>,
    },
    Channel {
        icon: Icon,
        name: String,
    },
    Word,
}

/// What the caret is in: an empty box, words, or a filter's answer, by
/// where that answer runs (from its key's end) and what's typed of it.
enum Mode {
    Empty,
    Words(Range<usize>),
    Answer(Filter, Range<usize>, String),
}

fn mode(text: &str, caret: usize, query: &search::Query) -> Mode {
    if text.trim().is_empty() {
        return Mode::Empty;
    }
    let caret = caret.min(text.len());
    // The last key before the caret, with nothing but one word after it.
    let key = query
        .tokens
        .iter()
        .rev()
        .find(|token| matches!(token.kind, search::Kind::Key(_)) && token.end <= caret);
    if let Some(token) = key
        && let search::Kind::Key(filter) = token.kind
    {
        let typed = text[token.end..caret].trim_start();
        if !typed.contains(char::is_whitespace) {
            let end = text[caret..]
                .find(char::is_whitespace)
                .map_or(text.len(), |at| caret + at);
            return Mode::Answer(filter, token.end..end, typed.to_owned());
        }
    }
    let start = text[..caret]
        .rfind(char::is_whitespace)
        .map_or(0, |at| at + 1);
    let end = text[caret..]
        .find(char::is_whitespace)
        .map_or(text.len(), |at| caret + at);
    Mode::Words(start..end)
}

/// What the popout shows for the box's text and caret, in `state`'s
/// place, with the place's `history`.
pub fn shown(searching: &Searching, state: &AppState, history: &[String]) -> Option<Shown> {
    let text = searching.text.as_str();
    let place = state.search_place()?;
    let server = discord::parse_server(&place).map(str::to_owned);
    let query = crate::search::parse(text, state);
    let groups = match mode(text, searching.caret, &query) {
        Mode::Empty => {
            let mut groups = vec![filters(server.is_some())];
            if !history.is_empty() {
                groups.push(Group {
                    heading: Some("History"),
                    clears: true,
                    rows: history.iter().cloned().map(Row::History).collect(),
                });
            }
            groups
        }
        Mode::Words(at) => {
            let mut groups = vec![Group {
                heading: None,
                clears: false,
                rows: vec![Row::SearchFor(text.trim().to_owned())],
            }];
            let word = text[at.clone()].trim_start_matches(['@', '#']);
            let mut suggested = Vec::new();
            if !word.is_empty() {
                let people = people(state, word, SUGGESTED);
                let turn = |filter: Filter, (value, face): &(String, Face)| Row::Turn {
                    at: at.clone(),
                    filter,
                    value: value.clone(),
                    face: face.clone(),
                };
                let groups = [
                    ("From User", Filter::From, people.clone()),
                    (
                        "In Channel",
                        Filter::In,
                        server
                            .as_deref()
                            .map(|server| channels(state, server, word, SUGGESTED))
                            .unwrap_or_default(),
                    ),
                    ("Mentions User", Filter::Mentions, people),
                ];
                for (heading, filter, found) in groups {
                    if !found.is_empty() {
                        suggested.push(Group {
                            heading: Some(heading),
                            clears: false,
                            rows: found.iter().map(|found| turn(filter, found)).collect(),
                        });
                    }
                }
            }
            if suggested.is_empty() {
                groups.push(filters(server.is_some()));
            } else {
                groups.push(Group {
                    heading: None,
                    clears: false,
                    rows: vec![Row::AddFilters],
                });
            }
            groups.extend(suggested);
            groups
        }
        Mode::Answer(filter, at, typed) => {
            if matches!(filter, Filter::Before | Filter::After | Filter::During) {
                return Some(Shown::Calendar { at });
            }
            let (heading, found) = answers(filter, &typed, state, server.as_deref());
            if found.is_empty() {
                return None;
            }
            vec![Group {
                heading,
                clears: false,
                rows: found
                    .into_iter()
                    .map(|(value, face)| Row::Answer {
                        at: at.clone(),
                        value,
                        face,
                    })
                    .collect(),
            }]
        }
    };
    Some(Shown::Rows(groups))
}

/// The Filters group: only those the place has (`in:` in a server).
fn filters(server: bool) -> Group {
    let mut rows = vec![Row::Key {
        key: "from:",
        label: "From a specific user",
        hint: "user",
    }];
    if server {
        rows.push(Row::Key {
            key: "in:",
            label: "Sent in a specific channel",
            hint: "channel",
        });
    }
    rows.push(Row::Key {
        key: "has:",
        label: "Includes a specific type of data",
        hint: "link, embed or file",
    });
    rows.push(Row::Key {
        key: "mentions:",
        label: "Mentions a specific user",
        hint: "user",
    });
    rows.push(Row::MoreFilters);
    Group {
        heading: Some("Filters"),
        clears: false,
        rows,
    }
}

/// A filter's answers for what's typed of it, under their heading.
fn answers(
    filter: Filter,
    typed: &str,
    state: &AppState,
    server: Option<&str>,
) -> (Option<&'static str>, Vec<(String, Face)>) {
    let listed = |names: &[&str]| {
        // A `-` first negates, and stays.
        let (not, typed) = match typed.strip_prefix('-') {
            Some(typed) => ("-", typed),
            None => ("", typed),
        };
        let typed = typed.to_lowercase();
        names
            .iter()
            .filter(|name| name.starts_with(&typed))
            .map(|name| (format!("{not}{name}"), Face::Word))
            .collect::<Vec<_>>()
    };
    match filter {
        Filter::From | Filter::Mentions => {
            let word = typed.trim_start_matches('@');
            let mut found = Vec::new();
            if "me".starts_with(&word.to_lowercase()) {
                found.push(("@me".to_owned(), Face::Word));
            }
            found.extend(people(state, word, ANSWERS));
            found.truncate(ANSWERS);
            let heading = if filter == Filter::From {
                "From User"
            } else {
                "Mentions User"
            };
            (Some(heading), found)
        }
        Filter::In => (
            Some("In Channel"),
            server
                .map(|server| channels(state, server, typed, ANSWERS))
                .unwrap_or_default(),
        ),
        Filter::Has => (
            Some("Message Contains"),
            listed(&search::HAS.map(|(name, _)| name)),
        ),
        Filter::AuthorType => (None, listed(&search::AUTHOR_TYPES.map(|(name, _)| name))),
        Filter::Pinned => (None, listed(&["true", "false"])),
        _ => (None, Vec::new()),
    }
}

/// The people `word` may name, by username, as the composer's `@` finds
/// them: the place's members and authors, and those the member search
/// found.
pub(crate) fn people(state: &AppState, word: &str, most: usize) -> Vec<(String, Face)> {
    let mut candidates: Vec<mention::Candidate> = state
        .mention_candidates()
        .into_iter()
        .filter(|candidate| !candidate.group && candidate.username.is_some())
        .collect();
    // A one-to-one DM's other person, by the username their profile gives,
    // else by id, which Discord takes as well.
    if let Some(room) = state.selected_room().filter(|room| room.direct)
        && let Some(person) = &room.person
    {
        let username = [&state.panel_profile, &state.profile]
            .into_iter()
            .flatten()
            .find(|view| view.profile.id == *person && !view.profile.handle.is_empty())
            .map_or_else(
                || discord::sender_user(person).unwrap_or(person).to_owned(),
                |view| view.profile.handle.clone(),
            );
        if !candidates
            .iter()
            .any(|candidate| candidate.username.as_deref() == Some(username.as_str()))
        {
            candidates.push(mention::Candidate {
                name: room.name.clone(),
                username: Some(username),
                mention: String::new(),
                avatar: room.avatar.clone(),
                color: None,
                group: false,
                channel: false,
                voice: false,
            });
        }
    }
    mention::matching(&candidates, word, most)
        .into_iter()
        .filter_map(|candidate| {
            let username = candidate.username.clone()?;
            Some((
                username.clone(),
                Face::Person {
                    name: candidate.name.clone(),
                    username,
                    avatar: candidate.avatar.clone(),
                },
            ))
        })
        .collect()
}

/// The server's channels, voice channels and threads whose names hold
/// `word`, as `in:` writes them; with nothing typed, the open one first.
pub(crate) fn channels(
    state: &AppState,
    server: &str,
    word: &str,
    most: usize,
) -> Vec<(String, Face)> {
    let word = word
        .trim_start_matches('#')
        .trim_matches('"')
        .to_lowercase();
    let mut found: Vec<_> = state
        .rooms
        .iter()
        .filter(|room| {
            !room.direct
                && !room.locked
                && !room.forum
                && discord::parse_room(&room.id).map(|(guild, _)| guild) == Some(server)
                && room.name.to_lowercase().contains(&word)
        })
        .collect();
    if word.is_empty()
        && let Some(open) = found
            .iter()
            .position(|room| state.selected.as_deref() == Some(room.id.as_str()))
    {
        let open = found.remove(open);
        found.insert(0, open);
    }
    found
        .into_iter()
        .take(most)
        .map(|room| {
            (
                crate::search::in_channel(&room.name, room.parent.is_some()),
                Face::Channel {
                    icon: crate::sidebar::room_icon(room),
                    name: room.name.clone(),
                },
            )
        })
        .collect()
}

/// The rows one after another, as the keys step through them.
fn rows(shown: &Shown) -> Vec<&Row> {
    match shown {
        Shown::Rows(groups) => groups.iter().flat_map(|group| &group.rows).collect(),
        Shown::Calendar { .. } => Vec::new(),
    }
}

/// The row lit as the popout changes: the first of a filter's answers,
/// else none.
pub fn first_lit(shown: Option<&Shown>) -> Option<usize> {
    match shown? {
        Shown::Rows(groups) => groups
            .first()
            .and_then(|group| group.rows.first())
            .is_some_and(|row| matches!(row, Row::Answer { .. }))
            .then_some(0),
        Shown::Calendar { .. } => None,
    }
}

/// The place's searches, newest first.
pub fn history<'a>(app: &'a App, state: &AppState) -> &'a [String] {
    state
        .search_place()
        .and_then(|place| app.layout.search_history.get(&place))
        .map_or(&[], Vec::as_slice)
}

/// Whether the popout has rows for the keys to step through.
pub fn has_rows(searching: &Searching, state: &AppState) -> bool {
    matches!(shown(searching, state, &[]), Some(Shown::Rows(_)))
}

/// The name typed after `from:` or `mentions:`, or the word typed alone,
/// which the member search fills in people for, as Discord's asks the
/// gateway.
pub fn person_typed(searching: &Searching, state: &AppState) -> Option<String> {
    if !open(searching) {
        return None;
    }
    let text = searching.text.as_str();
    let query = crate::search::parse(text, state);
    let word = match mode(text, searching.caret, &query) {
        Mode::Answer(Filter::From | Filter::Mentions, _, typed) => typed,
        Mode::Words(at) => text[at].to_owned(),
        _ => return None,
    };
    let word = word.trim_start_matches('@');
    (!word.is_empty()).then(|| word.to_owned())
}

/// Whether the popout shows: while the box has the keys, until a search
/// runs from it.
pub fn open(searching: &Searching) -> bool {
    searching.focused && !searching.hidden && searching.filters.is_none()
}

/// What the popout's events do.
#[derive(Clone, Debug)]
pub enum PopoutEvent {
    /// A row taken, by its place among them all.
    Pick(usize),
    /// A day of the calendar.
    Day(NaiveDate),
    /// The calendar a month on, or back.
    Month(i32),
    /// History's trash: the place's searches go.
    ClearHistory,
}

/// Up or Down: the next row lit, round to the first or last.
pub fn step(app: &mut App, by: i32) -> Task<Message> {
    let state = app.client.state();
    let count =
        shown(&app.searching, state, history(app, state)).map_or(0, |shown| rows(&shown).len());
    if count == 0 {
        return Task::none();
    }
    let count = count as i32;
    let lit = match app.searching.lit {
        Some(lit) => (lit as i32 + by).rem_euclid(count),
        None if by > 0 => 0,
        None => count - 1,
    };
    app.searching.lit = Some(lit as usize);
    Task::none()
}

/// Enter: the lit row, else a search of what's typed.
pub fn choose(app: &mut App) -> Task<Message> {
    match app.searching.lit {
        Some(index) => event(app, PopoutEvent::Pick(index)),
        None => {
            let mut cx = crate::ctx::cx!(app);
            app.searching.update(Event::Search, &mut cx)
        }
    }
}

pub fn event(app: &mut App, event: PopoutEvent) -> Task<Message> {
    let state = app.client.state();
    let text = app.searching.text.clone();
    let (replace, search) = match event {
        PopoutEvent::Pick(index) => {
            let Some(row) = shown(&app.searching, state, history(app, state))
                .and_then(|shown| rows(&shown).get(index).map(|row| (*row).clone()))
            else {
                return Task::none();
            };
            match row {
                Row::SearchFor(_) => (None, true),
                Row::Key { key, .. } => {
                    let caret = app.searching.caret.min(text.len());
                    let before = &text[..caret];
                    let space = if before.is_empty() || before.ends_with(' ') {
                        ""
                    } else {
                        " "
                    };
                    let put = format!("{before}{space}{key} ");
                    (Some((put, text[caret..].trim_start().to_owned())), false)
                }
                Row::Turn {
                    at, filter, value, ..
                } => {
                    let put = format!("{}{} {value} ", &text[..at.start], filter.key());
                    (Some((put, text[at.end..].trim_start().to_owned())), false)
                }
                Row::Answer { at, value, .. } => {
                    let put = format!("{} {value} ", &text[..at.start]);
                    (Some((put, text[at.end..].trim_start().to_owned())), true)
                }
                Row::History(query) => (Some((query, String::new())), true),
                Row::MoreFilters | Row::AddFilters => {
                    return crate::search_filters::update(
                        app,
                        crate::search_filters::FiltersEvent::Open,
                    );
                }
            }
        }
        PopoutEvent::Day(day) => {
            let Some(Shown::Calendar { at }) = shown(&app.searching, state, history(app, state))
            else {
                return Task::none();
            };
            let put = format!("{} {} ", &text[..at.start], day.format("%Y-%m-%d"));
            (Some((put, text[at.end..].trim_start().to_owned())), true)
        }
        PopoutEvent::Month(by) => {
            let (year, month) = app.searching.month.unwrap_or_else(this_month);
            let months = year * 12 + month as i32 - 1 + by;
            app.searching.month = Some((months.div_euclid(12), months.rem_euclid(12) as u32 + 1));
            return Task::none();
        }
        PopoutEvent::ClearHistory => {
            if let Some(place) = state.search_place() {
                app.layout.search_history.remove(&place);
                crate::layout::store(
                    app.client.root(),
                    "search_history",
                    &app.layout.search_history,
                );
            }
            return Task::none();
        }
    };
    if let Some((before, after)) = replace {
        let text = format!("{before}{after}");
        let text: String = text.chars().take(search::MAX_LENGTH).collect();
        let caret = before.len().min(text.len());
        app.searching.put(text, caret);
    }
    let mut cx = crate::ctx::cx!(app);
    if search {
        app.searching.update(Event::Search, &mut cx)
    } else {
        app.searching.refresh_lit(&mut cx);
        cx.touch();
        Task::none()
    }
}

fn this_month() -> (i32, u32) {
    let today = chrono::Local::now().date_naive();
    (today.year(), today.month())
}

/// The avatars the popout's people show.
pub fn pictures(app: &App) -> Vec<String> {
    if !open(&app.searching) {
        return Vec::new();
    }
    let state = app.client.state();
    let Some(Shown::Rows(groups)) = shown(&app.searching, state, history(app, state)) else {
        return Vec::new();
    };
    groups
        .into_iter()
        .flat_map(|group| group.rows)
        .filter_map(|row| match row {
            Row::Turn {
                face: Face::Person { avatar, .. },
                ..
            }
            | Row::Answer {
                face: Face::Person { avatar, .. },
                ..
            } => avatar,
            _ => None,
        })
        .collect()
}

/// The popout, under the box, its right edge level with the box's.
pub fn view<'a>(ctx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let searching = ctx.searching;
    let spot = searching.spot.get();
    if !open(searching) || spot.width == 0.0 {
        return None;
    }
    let history = ctx
        .state
        .search_place()
        .and_then(|place| ctx.layout.search_history.get(&place))
        .map_or(&[][..], Vec::as_slice);
    let body = match shown(searching, ctx.state, history)? {
        Shown::Rows(groups) => rows_view(ctx, &groups, searching.lit),
        Shown::Calendar { .. } => calendar(
            ctx,
            searching.month.unwrap_or_else(this_month),
            |day| Message::Search(Event::Popout(PopoutEvent::Day(day))),
            |by| Message::Search(Event::Popout(PopoutEvent::Month(by))),
        ),
    };
    let theme = ctx.look.theme;
    let (ground, edge) = (theme.raised, theme.edge);
    let window = ctx.look.window;
    let width = WIDTH.min(window.width - 16.0);
    let x = (spot.x + spot.width - width).clamp(8.0, (window.width - width - 8.0).max(8.0));
    let top = spot.y + spot.height + GAP;
    let panel = container(body)
        .padding(8)
        .width(width)
        .max_height((window.height - top - 8.0).max(ROW))
        .style(move |_| crate::style::panel(ground, edge, 8.0));
    // A press on the panel keeps the keys in the box.
    let panel = mouse_area(panel).on_press(Message::Nothing);
    Some(placed(panel, x, top))
}

fn rows_view<'a>(ctx: Ctx<'a>, groups: &[Group], lit: Option<usize>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut list = column![].spacing(2);
    let mut index = 0;
    for (number, group) in groups.iter().enumerate() {
        if number > 0 {
            list = list.push(Space::new().height(6));
        }
        if let Some(heading) = group.heading {
            let mut head = row![
                text(heading)
                    .size(12)
                    .font(SEMIBOLD)
                    .color(theme.secondary)
                    .width(Length::Fill)
            ]
            .align_y(Alignment::Center)
            .height(24)
            .padding(Padding::ZERO.left(8).right(4));
            if group.clears {
                head = head.push(
                    ctx.look
                        .icon_button(
                            Icon::Trash,
                            "Clear Search History",
                            Message::Search(Event::Popout(PopoutEvent::ClearHistory)),
                        )
                        .size(20.0, 14)
                        .below(),
                );
            }
            list = list.push(head);
        }
        for row in &group.rows {
            list = list.push(row_view(ctx, row, index, lit == Some(index)));
            index += 1;
        }
    }
    crate::widgets::scroll_view(list).into()
}

fn row_view<'a>(ctx: Ctx<'a>, row: &Row, index: usize, lit: bool) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let label = |label: String| {
        text(label)
            .size(14)
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None)
    };
    let face: Element<'a, Message> = match row {
        Row::SearchFor(query) => rich_text::<Message, _, _, _>(vec![
            iced::widget::span("Search for ").color(theme.text),
            iced::widget::span(query.clone())
                .font(SEMIBOLD)
                .color(theme.text),
        ])
        .size(14)
        .wrapping(text::Wrapping::None)
        .into(),
        Row::Key {
            key,
            label: name,
            hint,
        } => row![
            container(label((*name).to_owned()))
                .width(Length::Fill)
                .clip(true),
            rich_text::<Message, _, _, _>(vec![
                iced::widget::span(*key)
                    .font(SEMIBOLD)
                    .color(theme.secondary),
                iced::widget::span(format!(" {hint}")).color(theme.muted),
            ])
            .size(13)
            .wrapping(text::Wrapping::None),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        Row::Turn { face, filter, .. } => face_view(ctx, face, Some(filter.key())),
        Row::Answer { face, value, .. } => match face {
            Face::Word => label(value.clone()).into(),
            face => face_view(ctx, face, None),
        },
        Row::MoreFilters => row![
            container(label("More filters".to_owned()))
                .width(Length::Fill)
                .clip(true),
            text("dates, author type, and more")
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        Row::AddFilters => row![
            ctx.look.glyph(Icon::Plus, 14, theme.secondary),
            label("Add search filters".to_owned()),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        Row::History(query) => row![
            ctx.look.glyph(Icon::Search, 14, theme.muted),
            crate::search::query_view(ctx, query, 14.0),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    };
    let (active, hover) = (theme.active, theme.hover);
    button(container(face).height(ROW).align_y(Alignment::Center))
        .padding(Padding::ZERO.left(8).right(8))
        .width(Length::Fill)
        .clip(true)
        .on_press(Message::Search(Event::Popout(PopoutEvent::Pick(index))))
        .style(lit_style(
            lit.then_some(active),
            if lit { active } else { hover },
            4.0,
        ))
        .into()
}

/// A person's or channel's row: avatar or icon, the name, and muted after
/// it the username, or with a word made a filter, the filter's key.
fn face_view<'a>(ctx: Ctx<'a>, face: &Face, key: Option<&'static str>) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (mark, name, detail): (Element<'a, Message>, String, Option<String>) = match face {
        Face::Person {
            name,
            username,
            avatar,
        } => (
            ctx.avatar(avatar.as_deref(), name, 20.0, 10.0, 1.0),
            name.clone(),
            Some(username.clone()),
        ),
        Face::Channel { icon, name } => (
            ctx.look.glyph(*icon, 16, theme.muted).into(),
            name.clone(),
            None,
        ),
        Face::Word => (Space::new().into(), String::new(), None),
    };
    let detail = match key {
        Some(key) => Some(format!("{key} {}", detail.unwrap_or_else(|| name.clone()))),
        None => detail,
    };
    let mut line = row![
        mark,
        text(name)
            .size(14)
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(detail) = detail {
        line = line.push(
            text(detail)
                .size(13)
                .color(theme.muted)
                .wrapping(text::Wrapping::None),
        );
    }
    line.into()
}

/// A month's days, Sunday first, each from Discord's first day to today
/// a button that searches; arrows to the months before and after.
pub(crate) fn calendar<'a>(
    ctx: Ctx<'a>,
    (year, month): (i32, u32),
    on_day: impl Fn(NaiveDate) -> Message + 'a,
    on_month: impl Fn(i32) -> Message + 'a,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let today = chrono::Local::now().date_naive();
    let first_day = NaiveDate::from_ymd_opt(FIRST_DAY.0, FIRST_DAY.1, FIRST_DAY.2)
        .expect("Discord's first day");
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Space::new().into();
    };
    let arrow = |icon, by: i32, shown: bool| -> Element<'a, Message> {
        if shown {
            ctx.look
                .icon_button(
                    icon,
                    if by < 0 {
                        "Previous month"
                    } else {
                        "Next month"
                    },
                    on_month(by),
                )
                .size(24.0, 14)
                .below()
                .into()
        } else {
            Space::new().width(24).into()
        }
    };
    let head = row![
        arrow(
            Icon::ChevronLeft,
            -1,
            (year, month) > (first_day.year(), first_day.month())
        ),
        text(first.format("%B %Y").to_string())
            .size(14)
            .font(SEMIBOLD)
            .color(theme.text)
            .width(Length::Fill)
            .align_x(Alignment::Center),
        arrow(
            Icon::ChevronRight,
            1,
            (year, month) < (today.year(), today.month())
        ),
    ]
    .align_y(Alignment::Center);
    let cell = 40.0;
    let names = row(["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].map(|name| {
        text(name)
            .size(12)
            .font(SEMIBOLD)
            .color(theme.muted)
            .width(cell)
            .align_x(Alignment::Center)
            .into()
    }));
    let mut weeks = column![names].spacing(2);
    let lead = first.weekday().num_days_from_sunday() as i64;
    let start = first - chrono::Duration::days(lead);
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap_or(first);
    for week in 0..6 {
        if start + chrono::Duration::days(week * 7) >= next {
            break;
        }
        let mut days = row![];
        for weekday in 0..7 {
            let day = start + chrono::Duration::days(week * 7 + weekday);
            let shown = day.month() == month;
            let open = shown && day >= first_day && day <= today;
            let ink = if !shown {
                Color::TRANSPARENT
            } else if open {
                theme.text
            } else {
                theme.faint
            };
            let label = text(day.day().to_string())
                .size(13)
                .font(if day == today { SEMIBOLD } else { MEDIUM })
                .color(ink)
                .width(Length::Fill)
                .align_x(Alignment::Center);
            let (active, accent) = (theme.hover, theme.accent);
            let mut cell_button = button(label)
                .padding(Padding::from([6, 0]))
                .width(cell)
                .style(lit_style(
                    (day == today && shown).then_some(crate::style::faded(accent, 0.25)),
                    active,
                    4.0,
                ));
            if open {
                cell_button = cell_button.on_press(on_day(day));
            }
            days = days.push(cell_button);
        }
        weeks = weeks.push(days);
    }
    column![head, container(weeks).center_x(Length::Fill)]
        .spacing(6)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode_of(text: &str, caret: usize) -> Mode {
        let state = AppState::default();
        mode(text, caret, &crate::search::parse(text, &state))
    }

    #[test]
    fn the_caret_picks_the_popouts_mode() {
        assert!(matches!(mode_of("", 0), Mode::Empty));
        assert!(matches!(mode_of("   ", 2), Mode::Empty));
        assert!(
            matches!(mode_of("from:", 5), Mode::Answer(Filter::From, at, typed) if at == (5..5) && typed.is_empty())
        );
        assert!(
            matches!(mode_of("from: ca", 8), Mode::Answer(Filter::From, at, typed) if at == (5..8) && typed == "ca")
        );
        // Within the answer, which runs on to the word's end.
        assert!(
            matches!(mode_of("has: lin plan", 7), Mode::Answer(Filter::Has, at, typed) if at == (4..8) && typed == "li")
        );
        // Past the answer, words again.
        assert!(matches!(mode_of("from: carol hi", 14), Mode::Words(at) if at == (12..14)));
        assert!(matches!(mode_of("trip car", 8), Mode::Words(at) if at == (5..8)));
    }

    #[test]
    fn listed_answers_keep_a_negation() {
        let state = AppState::default();
        let (heading, found) = answers(Filter::Has, "-li", &state, None);
        assert_eq!(heading, Some("Message Contains"));
        let values: Vec<_> = found.iter().map(|(value, _)| value.as_str()).collect();
        assert_eq!(values, ["-link"]);
        let (_, found) = answers(Filter::Has, "", &state, None);
        let values: Vec<_> = found.iter().map(|(value, _)| value.as_str()).collect();
        assert_eq!(
            values,
            [
                "image", "video", "link", "file", "embed", "sound", "poll", "sticker", "forward"
            ]
        );
        let (_, found) = answers(Filter::From, "m", &state, None);
        assert_eq!(found.first().map(|(value, _)| value.as_str()), Some("@me"));
    }
}
