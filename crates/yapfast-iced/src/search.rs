//! Searching messages: the header's box, Enter searching the DM or the
//! channel's server, and what it found beside the conversation: how many,
//! the newest first under their channels' names, a page at a time. A result
//! clicked, or its Jump, shows it; Close, Escape or clearing the box ends
//! the search, and its cards go. Ctrl+F goes to the box (`App::subscription`).

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Rectangle, Task,
    advanced::{
        Shell,
        widget::{Id, Operation, operation::Focusable},
    },
    keyboard::{self, key},
    widget::{column, container, operation, row, text, text_input},
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
    widgets::{At, Hooks, Inner, Stateless, Wrap},
};

/// The panel's width, and the box's at rest and while it holds a search.
pub const WIDTH: f32 = 420.0;
const BOX: f32 = 244.0;
const BOX_USED: f32 = 300.0;
const BOX_HEIGHT: f32 = 32.0;
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
    /// Ctrl+F, or with `channel` off Ctrl+Shift+F: the box takes the keys.
    Focus {
        channel: bool,
    },
    /// Escape in the box, which let go of the keys as it took it.
    Escape,
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
            // `focus`, which the window has to look at first.
            Event::Focus { .. } => {}
            // As in Discord's box: Escape clears what's typed and keeps the
            // keys, and in an empty box lets go of them.
            Event::Escape => {
                if self.text.is_empty() {
                    return Task::none();
                }
                self.text.clear();
                cx.dispatch(Action::Search(String::new()));
                cx.touch();
                return operation::focus(FIELD);
            }
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
    let field = search_field(
        cx,
        FIELD,
        placeholder,
        &searching.text,
        |text| Message::Search(Event::Text(text)),
        Message::Search(Event::Search),
        Message::Search(Event::Clear),
    );
    Wrap::new(field, HeldEscape).into()
}

/// Ctrl+F, as Discord's binds take it: the conversation's search box takes
/// the keys, unless a dialog, a menu or the settings has them. In a
/// server's channel Ctrl+F fills the box with the channel's `in:` first, in
/// place of what it held; Ctrl+Shift+F, or Ctrl+F in a DM, keeps it.
pub fn focus(app: &mut App, channel: bool) -> Task<Message> {
    if app.keys_held() {
        return Task::none();
    }
    let state = app.client.state();
    let Some(room) = state.conversation_that(|can| can.search) else {
        return Task::none();
    };
    // Discord fills a thread or a voice channel's chat in too, but a
    // search's `in:` names only the server's text channels.
    let filter = (channel
        && discord::parse_room(&room.id).is_some()
        && !room.direct
        && !room.voice
        && room.parent.is_none())
    .then(|| format!("in:{} ", in_channel(&room.name)));
    if let Some(filter) = filter {
        app.searching.text = filter;
    }
    operation::focus(FIELD)
}

/// A channel as the box fills it in after `in:`: `#` and its name, in
/// quotes when it has a space, which Discord's text channels don't.
fn in_channel(name: &str) -> String {
    if name.contains(' ') {
        format!("\"#{name}\"")
    } else {
        format!("#{name}")
    }
}

/// Hands on the Escape the box takes while it has the keys
/// ([`Event::Escape`]), which a field otherwise keeps to itself.
struct HeldEscape;

impl Hooks<Message> for HeldEscape {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        _: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_, Message>,
    ) {
        let escape = matches!(
            event,
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Escape),
                ..
            })
        );
        let held = escape && {
            let mut focused = Focused(false);
            child.operate(&mut focused);
            focused.0
        };
        child.update(shell);
        if held {
            shell.publish(Message::Search(Event::Escape));
        }
    }
}

/// Whether a field inside has the keys.
struct Focused(bool);

impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        self.0 |= state.is_focused();
    }
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

/// A search box: Discord's, 244 by 32 px, a sunken field in a faint
/// outline (on Bonfire, Discord's measured inks), wider while it holds
/// something, its icon or Clear at its end. Its width and height are the
/// header's, which another place may set.
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
        ctx.look.glyph(Icon::Search, 16, theme.secondary).into()
    } else {
        ctx.look
            .icon_button(Icon::X, tr("Clear search"), clear)
            .size(24.0, 14)
            .below()
            .into()
    };
    let field = row![input, end].spacing(4).align_y(Alignment::Center);
    let (line, ground) = if theme.bonfire {
        (crate::style::hex(0x303035), crate::style::hex(0x17171a))
    } else {
        (theme.line, theme.sunken)
    };
    container(field)
        .padding(
            Padding::ZERO
                .left(10)
                .right(if value.is_empty() { 7 } else { 4 }),
        )
        .width(if value.is_empty() { BOX } else { BOX_USED })
        .height(BOX_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| crate::style::outlined(ground, line, 8.0))
}
