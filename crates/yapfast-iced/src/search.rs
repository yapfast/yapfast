//! Searching messages: the header's box, Enter searching the DM or the
//! channel's server, and what it found beside the conversation: how many,
//! the newest first under their channels' names, a page at a time. A result
//! clicked, or its Jump, shows it; Close, Escape or clearing the box ends
//! the search, and its cards go.

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{column, container, row, text, text_input},
};
use yapfast_core::{
    application::Action,
    discord,
    i18n::{self, tr},
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, bar},
    ctx::{Ctx, Cx},
    elide::elide,
    fonts::BOLD,
    found::Found,
    pictures::Icon,
    style::{fill, scrollbar},
};

/// The panel's width, and the box's at rest and while it holds a search.
pub const WIDTH: f32 = 420.0;
const BOX: f32 = 240.0;
const BOX_USED: f32 = 300.0;
const BOX_HEIGHT: f32 = 30.0;
const HEAD: f32 = 48.0;
pub const FIELD: &str = "header-search";

/// The box's text, and the results' cards as they came.
#[derive(Default)]
pub struct Searching {
    pub text: String,
    results: Vec<Result>,
}

/// A result's card, its room, and over the first of a run in one of a
/// server's channels, the channel's name.
struct Result {
    found: Found,
    room: String,
    heading: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Event {
    Text(String),
    Search,
    Clear,
    More,
    Jump(String, String),
}

impl Searching {
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            Event::Text(text) => {
                let cleared = text.is_empty() && !self.text.is_empty();
                self.text = text;
                if cleared {
                    cx.dispatch(Action::Search(String::new()));
                }
            }
            Event::Search => {
                let query = self.text.trim().to_owned();
                cx.dispatch(Action::Search(query));
            }
            Event::Clear => {
                self.text.clear();
                cx.dispatch(Action::Search(String::new()));
            }
            Event::More => cx.dispatch(Action::MoreResults),
            Event::Jump(room, item) => cx.dispatch(Action::JumpInRoom { room, item }),
        }
        cx.touch();
        Task::none()
    }
}

/// The header's box: its placeholder, the search typed, Enter searching;
/// the search icon, or Clear once something's typed.
pub fn header_search<'a>(
    searching: &'a Searching,
    placeholder: &str,
    cx: Ctx<'a>,
) -> Element<'a, Message> {
    search_field(
        cx,
        FIELD,
        placeholder,
        &searching.text,
        |text| Message::Search(Event::Text(text)),
        Message::Search(Event::Search),
        Message::Search(Event::Clear),
    )
    .into()
}

/// The messages the results show.
pub fn searching_messages(
    app: &App,
) -> impl Iterator<Item = &std::sync::Arc<yapfast_core::chat::ChatMessage>> {
    app.searching
        .results
        .iter()
        .map(|result| &result.found.message)
}

/// Whether the search's results show beside the conversation.
pub fn search_shown(app: &App) -> bool {
    app.client.state().search_shown().is_some()
}

/// Lets go of the result cards, laid out in the old theme's inks.
pub fn forget_found(app: &mut App) {
    app.searching.results.clear();
}

/// Builds the cards of results not shown yet, keeping those still
/// first: a next page adds to the end.
pub fn search_sync(app: &mut App) {
    let state = app.client.state();
    let Some(search) = state.search_shown() else {
        if !app.searching.results.is_empty() {
            app.searching.results = Vec::new();
        }
        return;
    };
    let hits = &search.results;
    let kept = app
        .searching
        .results
        .iter()
        .zip(hits)
        .take_while(|(shown, hit)| shown.found.message.item == hit.message.item)
        .count();
    if kept == app.searching.results.len() && kept == hits.len() {
        return;
    }
    let server = discord::parse_server(&search.place).is_some();
    let fresh: Vec<_> = hits
        .iter()
        .enumerate()
        .skip(kept)
        .map(|(index, hit)| {
            let starts = index == 0 || hits[index - 1].room != hit.room;
            let heading = state
                .rooms
                .iter()
                .find(|room| room.id == hit.room)
                .filter(|_| server && starts)
                .map(|room| room.name.clone());
            (Arc::new(hit.message.clone()), hit.room.clone(), heading)
        })
        .collect();
    app.searching.results.truncate(kept);
    for (message, room, heading) in fresh {
        let found = crate::found::found(app, message);
        app.searching.results.push(Result {
            found,
            room,
            heading,
        });
    }
}

/// What the search found, beside the conversation.
pub fn view<'a>(searching: &'a Searching, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = cx.state;
    let search = state.search_shown()?;
    let theme = cx.look.theme;
    let server = discord::parse_server(&search.place).is_some();
    let heading = match search {
        search if search.results.is_empty() && search.loading => tr("Searching...").to_owned(),
        search if search.results.is_empty() && search.error.is_some() => tr("Search").to_owned(),
        search => i18n::trn("{n} Result", "{n} Results", search.total as usize),
    };
    let note = match &search.error {
        Some(error) => Some(error.as_str()),
        None if !search.loading && search.results.is_empty() => Some(if server {
            tr("Nothing in this server matches. Try other words.")
        } else {
            tr("Nothing in this conversation matches. Try other words.")
        }),
        None => None,
    };
    let head = column![
        container(
            row![
                container(elide(heading).size(15).font(BOLD).color(theme.text)).width(Length::Fill),
                cx.look
                    .icon_button(Icon::X, tr("Close search"), Message::Search(Event::Clear))
                    .below(),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(16).right(10))
        .height(HEAD - 1.0)
        .align_y(Alignment::Center),
        bar(theme.line, Length::Fill, 1),
    ];
    let mut list = column![].spacing(8).padding(12);
    for result in &searching.results {
        let mut entry = column![].spacing(6);
        if let Some(channel) = &result.heading {
            entry = entry.push(
                row![
                    cx.look.glyph(Icon::Hash, 14, theme.muted),
                    elide(channel.as_str())
                        .size(13)
                        .font(BOLD)
                        .color(theme.secondary),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
                .padding(Padding::ZERO.top(6)),
            );
        }
        let jump = Message::Search(Event::Jump(
            result.room.clone(),
            result.found.message.item.clone(),
        ));
        entry = entry.push(crate::found::found_view(cx, &result.found, jump));
        list = list.push(entry);
    }
    if search.more() {
        let (label, message) = if search.loading {
            (tr("Loading..."), None)
        } else {
            (tr("Show more"), Some(Message::Search(Event::More)))
        };
        list = list.push(
            container(controls::action(theme, label, Kind::Plain, message).small())
                .padding(Padding::ZERO.top(4))
                .center_x(Length::Fill),
        );
    }
    let mut body = column![head];
    if let Some(note) = note {
        body = body.push(container(text(note).size(14).color(theme.secondary)).padding(16));
    }
    body = body.push(
        scroll_view(list)
            .height(Length::Fill)
            .style(scrollbar(theme)),
    );
    let ground = theme.sidebar;
    Some(
        row![
            bar(theme.edge, 1, Length::Fill),
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| fill(ground)),
        ]
        .width(WIDTH)
        .height(Length::Fill)
        .into(),
    )
}

/// A search box: a sunken field, wider while it holds something, its icon
/// or Clear at its end. Its width and height are the header's, which
/// another place may set.
pub fn search_field<'a>(
    ctx: crate::ctx::Ctx<'a>,
    id: &'static str,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    submit: Message,
    clear: Message,
) -> container::Container<'a, Message> {
    let theme = ctx.look.theme;
    let (ink, hint, selection) = (theme.text, theme.muted, theme.selection);
    let input = text_input(placeholder, value)
        .id(iced::widget::Id::new(id))
        .size(14)
        .padding(0)
        .on_input(on_input)
        .on_submit(submit)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: ink,
            placeholder: hint,
            value: ink,
            selection,
        });
    let end: Element<'a, Message> = if value.is_empty() {
        ctx.look.glyph(Icon::Search, 20, theme.secondary).into()
    } else {
        ctx.look
            .icon_button(Icon::X, tr("Clear search"), clear)
            .size(24.0, 14)
            .below()
            .into()
    };
    let field = row![input, end].spacing(4).align_y(Alignment::Center);
    let (line, ground) = (theme.line, theme.sunken);
    container(field)
        .padding(Padding::ZERO.left(8).right(4))
        .width(if value.is_empty() { BOX } else { BOX_USED })
        .height(BOX_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| crate::style::outlined(ground, line, 8.0))
}
