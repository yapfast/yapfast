//! Searching messages, as the Slint client's: the header's box
//! (`HeaderSearch`), Enter searching the DM or the channel's server, and
//! what it found beside the conversation (`SearchPanel`): how many, the
//! newest first under their channels' names, a page at a time. A result
//! clicked, or its Jump, shows it; Close, Escape or clearing the box ends
//! the search, and its cards go.

use std::sync::Arc;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{column, container, row, scrollable, text, text_input},
};
use yapfast_core::{application::Action, discord};

use crate::{
    app::{App, Message},
    controls::{self, Kind, bar},
    elide::elide,
    fonts::BOLD,
    found::Found,
    pictures::Icon,
    style::{fill, rounded, scrollbar},
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

impl App {
    pub fn search_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Text(text) => {
                let cleared = text.is_empty() && !self.searching.text.is_empty();
                self.searching.text = text;
                if cleared {
                    self.client.dispatch(Action::Search(String::new()));
                }
            }
            Event::Search => {
                let query = self.searching.text.trim().to_owned();
                self.client.dispatch(Action::Search(query));
            }
            Event::Clear => {
                self.searching.text.clear();
                self.client.dispatch(Action::Search(String::new()));
            }
            Event::More => self.client.dispatch(Action::MoreResults),
            Event::Jump(room, item) => self.client.dispatch(Action::JumpInRoom { room, item }),
        }
        self.sync()
    }

    /// The messages the results show.
    pub fn searching_messages(
        &self,
    ) -> impl Iterator<Item = &std::sync::Arc<yapfast_core::chat::ChatMessage>> {
        self.searching
            .results
            .iter()
            .map(|result| &result.found.message)
    }

    /// Whether the search's results show beside the conversation.
    pub fn search_shown(&self) -> bool {
        self.client.state().search_shown().is_some()
    }

    /// Builds the cards of results not shown yet, keeping those still
    /// first: a next page adds to the end.
    pub fn search_sync(&mut self) {
        let state = self.client.state();
        let Some(search) = state.search_shown() else {
            if !self.searching.results.is_empty() {
                self.searching.results = Vec::new();
            }
            return;
        };
        let hits = &search.results;
        let kept = self
            .searching
            .results
            .iter()
            .zip(hits)
            .take_while(|(shown, hit)| shown.found.message.item == hit.message.item)
            .count();
        if kept == self.searching.results.len() && kept == hits.len() {
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
        self.searching.results.truncate(kept);
        for (message, room, heading) in fresh {
            let found = self.found(message);
            self.searching.results.push(Result {
                found,
                room,
                heading,
            });
        }
    }

    /// The header's box: its placeholder, the search typed, Enter
    /// searching; the search icon, or Clear once something's typed.
    pub fn header_search(&self, placeholder: &str) -> Element<'_, Message> {
        self.search_field(
            FIELD,
            placeholder,
            &self.searching.text,
            |text| Message::Search(Event::Text(text)),
            Message::Search(Event::Search),
            Message::Search(Event::Clear),
        )
    }

    /// A search box as the Slint client's `HeaderSearch`: a sunken field,
    /// wider while it holds something, its icon or Clear at its end.
    pub fn search_field<'a>(
        &'a self,
        id: &'static str,
        placeholder: &str,
        value: &str,
        on_input: impl Fn(String) -> Message + 'a,
        submit: Message,
        clear: Message,
    ) -> Element<'a, Message> {
        let theme = &self.theme;
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
        let end: Element<'_, Message> = if value.is_empty() {
            self.glyph(Icon::Search, 20, theme.secondary).into()
        } else {
            self.icon_button(Icon::X, "Clear search", 24.0, 14, clear, true)
        };
        let field = row![input, end].spacing(4).align_y(Alignment::Center);
        let (line, ground) = (theme.line, theme.sunken);
        container(field)
            .padding(Padding::ZERO.left(8).right(4))
            .width(if value.is_empty() { BOX } else { BOX_USED })
            .height(BOX_HEIGHT)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| container::Style {
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..rounded(ground, 8)
            })
            .into()
    }

    /// What the search found, beside the conversation.
    pub fn search_view(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        let search = state.search_shown()?;
        let theme = &self.theme;
        let server = discord::parse_server(&search.place).is_some();
        let heading = match search {
            search if search.results.is_empty() && search.loading => "Searching...".to_owned(),
            search if search.results.is_empty() && search.error.is_some() => "Search".to_owned(),
            search if search.total == 1 => "1 Result".to_owned(),
            search => format!("{} Results", search.total),
        };
        let note = match &search.error {
            Some(error) => Some(error.as_str()),
            None if !search.loading && search.results.is_empty() => Some(if server {
                "Nothing in this server matches. Try other words."
            } else {
                "Nothing in this conversation matches. Try other words."
            }),
            None => None,
        };
        let head = column![
            container(
                row![
                    container(elide(heading).size(15).font(BOLD).color(theme.text))
                        .width(Length::Fill),
                    self.icon_button(
                        Icon::X,
                        "Close search",
                        28.0,
                        18,
                        Message::Search(Event::Clear),
                        true
                    ),
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
        for result in &self.searching.results {
            let mut entry = column![].spacing(6);
            if let Some(channel) = &result.heading {
                entry = entry.push(
                    row![
                        self.glyph(Icon::Hash, 14, theme.muted),
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
            entry = entry.push(self.found_view(&result.found, jump));
            list = list.push(entry);
        }
        if search.more() {
            let (label, message) = if search.loading {
                ("Loading...", None)
            } else {
                ("Show more", Some(Message::Search(Event::More)))
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
            scrollable(list)
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
}
