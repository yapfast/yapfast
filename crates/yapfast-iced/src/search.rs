//! Searching messages: the header's box, Enter searching the DM or the
//! channel's server, and what it found beside the conversation, as
//! Discord's results show it: how many, a Sort menu (Newest, Oldest, Most
//! Relevant), each run of results in one channel under that channel's
//! name, the words searched for highlighted, results from people blocked
//! or ignored behind a row that shows them, and 25 a page with Discord's
//! pages under them. A result clicked, or its Jump, shows it; Close or
//! clearing the box ends the search, and its cards go. Ctrl+F goes to the
//! box (`App::subscription`).

use crate::widgets::scroll_view;
use std::sync::Arc;

use iced::{
    Alignment, Border, Color, Element, Length, Padding, Task,
    widget::{column, container, operation, row, text, text_input},
};
use yapfast_core::{
    application::{Action, AppState, SEARCH_PAGE, SEARCH_PAGES, Search},
    chat::SearchSort,
    discord,
    search_words::Words,
    yapfast_discord::search,
};

use crate::{
    app::{App, Message},
    controls::{self, Kind, bar},
    ctx::{Ctx, Cx},
    elide::elide,
    field::{self, Draft, Field},
    fonts::BOLD,
    found::Found,
    pictures::Icon,
    style::{fill, scrollbar},
    widgets::{At, Hooks, Stateless, Wrap},
};

/// The panel's width, and the box's at rest and while it holds a search.
pub const WIDTH: f32 = 420.0;
const BOX: f32 = 244.0;
const BOX_USED: f32 = 300.0;
const BOX_HEIGHT: f32 = 32.0;
const HEAD: f32 = 48.0;
pub const FIELD: &str = "header-search";
/// The results' list, which a new page or order scrolls to the top.
const RESULTS: &str = "search-results";

/// The results' list scrolled to its top.
pub fn to_top() -> Task<Message> {
    operation::snap_to(
        iced::widget::Id::new(RESULTS),
        iced::widget::scrollable::RelativeOffset::START,
    )
}

/// The box's text, and the shown page's cards, with the words they
/// highlight.
#[derive(Default)]
pub struct Searching {
    pub text: String,
    /// The box's own copy of the text, which its field edits, and where
    /// its caret is.
    draft: Draft,
    pub(crate) caret: usize,
    /// The box has the keys; its popout is closed until the next edit, as
    /// after a search from it; the popout's row lit by the keys, and the
    /// month its calendar shows.
    pub(crate) focused: bool,
    pub(crate) hidden: bool,
    pub(crate) lit: Option<usize>,
    pub(crate) month: Option<(i32, u32)>,
    /// Where the box was last drawn, which the popout opens under.
    pub(crate) spot: std::cell::Cell<iced::Rectangle>,
    /// The search last kept in the history, and the name the member
    /// search was last asked for.
    recorded: Option<(String, String, SearchSort, u32)>,
    asked: Option<String>,
    /// The Filters dialog, while it's open.
    pub(crate) filters: Option<crate::search_filters::Filters>,
    results: Vec<Result>,
    words: String,
}

/// A result's card, its room, and over the first of a run in one of a
/// server's channels, the channel's heading.
struct Result {
    found: Found,
    room: String,
    heading: Option<Heading>,
}

/// A channel's name over its results: its icon and name, and a thread's
/// channel beside it.
struct Heading {
    icon: Icon,
    name: String,
    marks: Vec<std::ops::Range<usize>>,
    parent: Option<(Icon, String)>,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The box's field: its text edited, its caret moved, Enter.
    Field(field::Event),
    Text(String),
    Search,
    Clear,
    /// Results' page, from 0.
    Page(u32),
    /// The Sort menu opened, or one of its orders chosen.
    SortMenu,
    Sort(SearchSort),
    /// The results from people blocked or ignored shown, or hidden again.
    ShowHidden(bool),
    /// A filter added at the end of the box, which then searches, as the
    /// bots' hint does.
    Append(&'static str),
    /// A channel's heading clicked: it opens.
    Open(String),
    Jump(String, String),
    /// Ctrl+F, or with `channel` off Ctrl+Shift+F: the box takes the keys.
    Focus {
        channel: bool,
    },
    /// Escape in the box.
    Escape,
    /// The popout under the box.
    Popout(crate::search_popout::PopoutEvent),
    /// The Filters dialog.
    Filters(crate::search_filters::FiltersEvent),
}

impl Searching {
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        match event {
            // No longer than Discord's box takes.
            Event::Field(field::Event::Edited(text, caret)) => {
                self.caret = caret;
                let text = if text.chars().count() > search::MAX_LENGTH {
                    let text: String = text.chars().take(search::MAX_LENGTH).collect();
                    self.caret = caret.min(text.len());
                    self.draft.replace(&text, self.caret);
                    text
                } else {
                    text
                };
                return self.update(Event::Text(text), cx);
            }
            Event::Field(field::Event::Moved(caret)) => {
                self.caret = caret;
                self.hidden = false;
                self.refresh_lit(cx);
            }
            Event::Field(field::Event::Send) => return self.update(Event::Search, cx),
            Event::Field(field::Event::Focus(focused)) => {
                self.focused = focused;
                self.hidden = false;
                self.refresh_lit(cx);
            }
            // The window's, which has the history (`search_popout`).
            Event::Field(_) | Event::Popout(_) | Event::Filters(_) => {}
            Event::Text(text) => {
                let cleared = text.is_empty() && !self.text.is_empty();
                self.text = text;
                self.hidden = false;
                self.refresh_lit(cx);
                if cleared {
                    cx.dispatch(Action::Search(String::new()));
                }
            }
            Event::Search => {
                let query = self.text.trim().to_owned();
                self.hidden = true;
                cx.dispatch(Action::Search(query));
            }
            Event::Clear => {
                self.text.clear();
                cx.dispatch(Action::Search(String::new()));
            }
            // A new page starts at its top, as Discord's does.
            Event::Page(page) => {
                cx.dispatch(Action::SearchPage(page));
                cx.touch();
                return to_top();
            }
            Event::Sort(sort) => cx.dispatch(Action::SearchSort(sort)),
            // The window opens the menu, which needs its point.
            Event::SortMenu => {}
            Event::ShowHidden(shown) => cx.dispatch(Action::ShowHiddenResults(shown)),
            Event::Append(filter) => {
                if !self.text.ends_with(' ') {
                    self.text.push(' ');
                }
                self.text.push_str(filter);
                self.text.push(' ');
                let query = self.text.trim().to_owned();
                cx.dispatch(Action::Search(query));
            }
            Event::Open(room) => cx.dispatch(Action::Open(room)),
            Event::Jump(room, item) => cx.dispatch(Action::JumpInRoom { room, item }),
            // `focus`, which the window has to look at first.
            Event::Focus { .. } => {}
            // As in Discord's box: Escape clears what's typed and keeps the
            // keys, and in an empty box lets go of them.
            Event::Escape => {
                if self.text.is_empty() {
                    return operation::focus(crate::app::NOBODY);
                }
                self.text.clear();
                self.hidden = false;
                self.refresh_lit(cx);
                cx.dispatch(Action::Search(String::new()));
            }
        }
        cx.touch();
        Task::none()
    }

    /// Puts `text` in the box with the caret at `caret`, as a pick from
    /// the popout does: one undo step.
    pub(crate) fn put(&mut self, text: String, caret: usize) {
        self.draft.replace(&text, caret);
        self.caret = caret;
        self.text = text;
        self.hidden = false;
    }

    /// The popout's row lit afresh, as its rows change; and the member
    /// search asked for the name the popout may offer people for.
    pub(crate) fn refresh_lit(&mut self, cx: &mut Cx<'_>) {
        let state = cx.state();
        let shown = crate::search_popout::shown(self, state, &[]);
        self.lit = crate::search_popout::first_lit(shown.as_ref());
        if !matches!(shown, Some(crate::search_popout::Shown::Calendar { .. })) {
            self.month = None;
        }
        let wanted = crate::search_popout::person_typed(self, state);
        if wanted != self.asked {
            // Only to fill in names: the composer asks again as it's typed in.
            if wanted.is_some() || state.mention_query.is_some() {
                cx.dispatch(Action::MentionQuery(wanted.clone().map(|word| ('@', word))));
            }
            self.asked = wanted;
        }
    }
}

/// The header's box: its placeholder, the search typed with its filters
/// as Discord's pills, Enter searching; the search icon, or Clear once
/// something's typed.
pub fn header_search<'a>(
    searching: &'a Searching,
    placeholder: &str,
    cx: Ctx<'a>,
) -> Element<'a, Message> {
    let theme = cx.look.theme;
    // The text set from outside the box: Ctrl+F, Clear, a filter added.
    if searching.draft.text() != searching.text {
        searching
            .draft
            .replace(&searching.text, searching.text.len());
    }
    let (pills, atoms) = pills_of(&searching.text, cx.state);
    let inks = pill_inks(theme);
    let input = Field::new(
        &searching.draft,
        iced::widget::Id::new(FIELD),
        placeholder.into(),
        |event| match event {
            field::Event::Escape => Message::Search(Event::Escape),
            event => Message::Search(Event::Field(event)),
        },
        field::Colors {
            ground: Color::TRANSPARENT,
            text: theme.text,
            placeholder: theme.muted,
            selection: theme.selection,
        },
    )
    .single()
    .size(14.0)
    .line_height(20.0)
    // Room either side for the pills' edges.
    .padding(Padding::from([5, 2]))
    .pills(pills, inks)
    .atoms(atoms)
    .suggesting(
        crate::search_popout::open(searching)
            && crate::search_popout::has_rows(searching, cx.state),
    );
    // Wider while it has the keys or holds a search, as Discord's.
    let used = !searching.text.is_empty() || searching.focused;
    let typed = !searching.text.is_empty();
    let end: Element<'a, Message> = if typed {
        cx.look
            .icon_button(Icon::X, "Clear search", Message::Search(Event::Clear))
            .size(24.0, 14)
            .below()
            .into()
    } else {
        cx.look.glyph(Icon::Search, 16, theme.secondary).into()
    };
    let (line, ground) = if theme.bonfire {
        (crate::style::hex(0x303035), crate::style::hex(0x17171a))
    } else {
        (theme.line, theme.sunken)
    };
    let field = container(row![input, end].spacing(4).align_y(Alignment::Center))
        .padding(Padding::ZERO.left(8).right(if typed { 2 } else { 5 }))
        .width(if used { BOX_USED } else { BOX })
        .height(BOX_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| crate::style::outlined(ground, line, 8.0));
    Wrap::new(field, Placed(&searching.spot)).into()
}

/// Notes where the box is drawn, for the popout under it.
struct Placed<'a>(&'a std::cell::Cell<iced::Rectangle>);

impl Hooks<Message> for Placed<'_> {
    type State = Stateless;

    fn draw(
        &self,
        _: &Stateless,
        renderer: &mut iced::Renderer,
        at: At,
        child: &mut dyn FnMut(&mut iced::Renderer),
    ) {
        self.0.set(at.bounds);
        child(renderer);
    }
}

/// A search as the popout's History shows it: its filters as the box's
/// pills.
pub fn query_view<'a>(ctx: Ctx<'a>, query: &str, size: f32) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let (pills, _) = pills_of(query, ctx.state);
    let inks = pill_inks(theme);
    let mut spans: Vec<iced::advanced::text::Span<'a, Message>> = Vec::new();
    let mut at = 0;
    for (index, pill) in pills.iter().enumerate() {
        if pill.range.start > at {
            spans
                .push(iced::widget::span(query[at..pill.range.start].to_owned()).color(theme.text));
        }
        // A key and its answer as one pill, squared where they meet.
        let joins_before = index > 0 && pills[index - 1].range.end == pill.range.start;
        let joins_after = pills
            .get(index + 1)
            .is_some_and(|next| next.range.start == pill.range.end);
        let round = |joined: bool| if joined { 0.0 } else { 4.0 };
        let radius = iced::border::Radius {
            top_left: round(joins_before),
            bottom_left: round(joins_before),
            top_right: round(joins_after),
            bottom_right: round(joins_after),
        };
        let span = iced::widget::span(query[pill.range.clone()].to_owned())
            .color(theme.text)
            .background(inks.ground)
            .border(Border {
                color: inks.line,
                width: 1.0,
                radius,
            })
            .padding(Padding::from([1, 1]));
        spans.push(if pill.bold { span.font(BOLD) } else { span });
        at = pill.range.end;
    }
    if at < query.len() {
        spans.push(iced::widget::span(query[at..].to_owned()).color(theme.text));
    }
    iced::widget::rich_text(spans)
        .size(size)
        .wrapping(text::Wrapping::None)
        .into()
}

/// The pills' inks: Discord's token inks on Bonfire
/// (`--background-surface-highest`, `--border-muted`).
fn pill_inks(theme: &crate::theme::Theme) -> field::PillInks {
    if theme.bonfire {
        field::PillInks {
            ground: crate::style::hex(0x3f4048),
            line: Color::from_rgba8(152, 152, 159, 0.04),
        }
    } else {
        field::PillInks {
            ground: theme.active,
            line: theme.line,
        }
    }
}

/// `text` read as the box reads it: anyone named counts as someone, as
/// the client's people are those it has met; a channel must be one of the
/// server's.
pub(crate) fn parse(text: &str, state: &AppState) -> search::Query {
    struct Here<'a> {
        server: Option<&'a str>,
        rooms: &'a [Arc<yapfast_core::chat::ChatRoom>],
    }
    impl search::Context for Here<'_> {
        fn has_channels(&self) -> bool {
            self.server.is_some()
        }
        fn channels_named(&self, name: &str) -> Vec<String> {
            self.rooms
                .iter()
                .filter(|room| {
                    !room.direct
                        && room.name == name
                        && discord::parse_room(&room.id).map(|(server, _)| server) == self.server
                })
                .map(|room| room.id.clone())
                .collect()
        }
        fn person(&self, _: &search::Who) -> Option<String> {
            Some(String::new())
        }
    }
    let place = state.search_place();
    let here = Here {
        server: place.as_deref().and_then(discord::parse_server),
        rooms: &state.rooms,
    };
    search::Query::parse(text, &here, &chrono::Local::now())
}

/// The box's pills and what Backspace takes whole, as Discord's box draws
/// them: every filter's key, in semibold, and each answer that checks out,
/// joined to its key; keys and the answers picked from a list (`has:`,
/// `pinned:`, `authorType:`) go whole.
fn pills_of(text: &str, state: &AppState) -> (Vec<field::Pill>, Vec<std::ops::Range<usize>>) {
    if text.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let query = parse(text, state);
    let (mut pills, mut atoms) = (Vec::new(), Vec::new());
    for token in &query.tokens {
        let range = token.start..token.end;
        match &token.kind {
            search::Kind::Text => continue,
            search::Kind::Key(_) => atoms.push(range.clone()),
            search::Kind::Answer(filter, _) => {
                if matches!(
                    filter,
                    search::Filter::Has | search::Filter::Pinned | search::Filter::AuthorType
                ) {
                    atoms.push(range.clone());
                }
            }
        }
        pills.push(field::Pill {
            range,
            bold: matches!(token.kind, search::Kind::Key(_)),
        });
    }
    (pills, atoms)
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
    // Discord fills in a server's channel, voice channel or thread; a
    // channel it can't read only gets the keys.
    let filter =
        (channel && discord::parse_room(&room.id).is_some() && !room.direct && !room.locked)
            .then(|| format!("in:{} ", in_channel(&room.name, room.parent.is_some())));
    if let Some(filter) = filter {
        app.searching.text = filter;
    }
    operation::focus(FIELD)
}

/// A channel as the box fills it in after `in:`, as Discord's client
/// writes it: its name in quotes when it has a space, a quote or a
/// backslash, those escaped, and `#` first unless it's a thread.
pub(crate) fn in_channel(name: &str, thread: bool) -> String {
    let name = if name.contains([' ', '"', '\\']) {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        name.to_owned()
    };
    if thread { name } else { format!("#{name}") }
}

/// Keeps each search as it starts in its place's history, as Discord's
/// box does with every search, a sort or a page too: one already there
/// goes first, one that goes on from the newest takes its place, and five
/// at most are kept.
fn remember_search(app: &mut App) {
    let Some(search) = app
        .client
        .state()
        .search
        .as_ref()
        .filter(|search| search.loading)
    else {
        return;
    };
    let key = (
        search.place.clone(),
        search.query.clone(),
        search.sort,
        search.page,
    );
    if app.searching.recorded.as_ref() == Some(&key) {
        return;
    }
    let (place, query, ..) = key.clone();
    app.searching.recorded = Some(key);
    let kept = app.layout.search_history.entry(place).or_default();
    if !remember(kept, &query) {
        return;
    }
    crate::layout::store(
        app.client.root(),
        "search_history",
        &app.layout.search_history,
    );
}

/// `query` put in `kept`, newest first: whether that changed it.
fn remember(kept: &mut Vec<String>, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() || kept.first().is_some_and(|newest| newest == query) {
        return false;
    }
    if let Some(at) = kept.iter().position(|old| old == query) {
        let old = kept.remove(at);
        kept.insert(0, old);
    } else if kept
        .first()
        .is_some_and(|newest| query.starts_with(newest.as_str()))
    {
        query.clone_into(&mut kept[0]);
    } else {
        kept.insert(0, query.to_owned());
        kept.truncate(crate::search_popout::HISTORY);
    }
    true
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

/// Builds the cards of the page shown, keeping those that are the same.
pub fn search_sync(app: &mut App) {
    remember_search(app);
    let state = app.client.state();
    let Some(search) = state.search_shown() else {
        if !app.searching.results.is_empty() {
            app.searching.results = Vec::new();
        }
        return;
    };
    let hits = &search.results;
    let same = app.searching.results.len() == hits.len()
        && app
            .searching
            .results
            .iter()
            .zip(hits)
            .all(|(shown, hit)| shown.found.message.item == hit.message.item);
    if same && app.searching.words == search.words {
        return;
    }
    let words = Words::of(&search.words);
    let server = discord::parse_server(&search.place).is_some();
    let results: Vec<Result> = hits
        .iter()
        .enumerate()
        .map(|(index, hit)| {
            let starts = index == 0 || hits[index - 1].room != hit.room;
            let parent_of = |parent: &String| {
                let parent = state.rooms.iter().find(|room| room.id == *parent)?;
                Some((crate::sidebar::room_icon(parent), parent.name.clone()))
            };
            // A listed channel, or a thread Discord named with the results.
            let listed = state.rooms.iter().find(|room| room.id == hit.room);
            let heading = match (listed, &hit.thread) {
                _ if !server || !starts => None,
                (Some(room), _) => Some(Heading {
                    icon: crate::sidebar::room_icon(room),
                    marks: words.marks(&room.name),
                    name: room.name.clone(),
                    parent: room.parent.as_ref().and_then(parent_of),
                }),
                (None, Some((name, parent))) => Some(Heading {
                    icon: Icon::Thread,
                    marks: words.marks(name),
                    name: name.clone(),
                    parent: parent.as_ref().and_then(parent_of),
                }),
                (None, None) => None,
            };
            Result {
                found: crate::found::found_marked(app, Arc::new(hit.message.clone()), &words),
                room: hit.room.clone(),
                heading,
            }
        })
        .collect();
    let words = search.words.clone();
    app.searching.words = words;
    app.searching.results = results;
}

/// Opens the results' Sort menu at `at`, the current order ticked.
pub fn open_sort_menu(app: &mut App, at: iced::Point) -> Task<Message> {
    use crate::menus::{Menu, MenuAct, MenuRow};
    let Some(search) = app.client.state().search_shown() else {
        return Task::none();
    };
    let rows = [
        (SearchSort::Newest, "Newest", "newest"),
        (SearchSort::Oldest, "Oldest", "oldest"),
        (SearchSort::Relevant, "Most Relevant", "relevant"),
    ]
    .into_iter()
    .map(|(sort, label, key)| MenuRow {
        check: if search.sort == sort {
            crate::controls::Check::Current
        } else {
            crate::controls::Check::None
        },
        ..MenuRow::plain(MenuAct::SearchSort, label).key(key)
    })
    .collect();
    app.win.menu = Some(Menu::new(at, rows));
    operation::focus(crate::app::NOBODY)
}

/// The order a Sort menu row's key names.
pub fn sort_of(key: &str) -> SearchSort {
    match key {
        "oldest" => SearchSort::Oldest,
        "relevant" => SearchSort::Relevant,
        _ => SearchSort::Newest,
    }
}

/// A page button of Discord's pages, or a gap between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageButton {
    Back {
        disabled: bool,
    },
    /// A page, from 1, and whether it's the one shown.
    Page(u32, bool),
    Gap,
    Next {
        disabled: bool,
    },
}

/// Discord's pages for `count` pages with `selected` shown (both from 1):
/// Back, the first page and a gap when far from it, `visible` pages
/// around the selected one, a gap and the last page when far from it,
/// Next.
pub fn page_buttons(selected: u32, count: u32, visible: u32) -> Vec<PageButton> {
    let (before, after) = (visible.div_ceil(2), visible / 2);
    let (mut first, mut last) = if selected <= before {
        (1, visible)
    } else if selected + after > count {
        ((count + 1).saturating_sub(visible), count)
    } else {
        (selected + 1 - before, selected + after)
    };
    first = first.max(1);
    last = last.min(count);
    let mut buttons = vec![PageButton::Back {
        disabled: selected == 1,
    }];
    let mut end = Vec::new();
    if first > 1 {
        buttons.extend([PageButton::Page(1, false), PageButton::Gap]);
        first += 2;
    }
    if last < count {
        end.extend([PageButton::Gap, PageButton::Page(count, false)]);
        last = last.saturating_sub(2);
    }
    buttons.extend((first..=last).map(|page| PageButton::Page(page, page == selected)));
    buttons.extend(end);
    buttons.push(PageButton::Next {
        disabled: selected == count,
    });
    buttons
}

/// The heading over the results: how many, while searching or indexing
/// what it says instead.
fn count(search: &Search) -> String {
    match search {
        search if search.loading && search.total == 0 => "Searching…".to_owned(),
        search if search.total == 0 => "No Results".to_owned(),
        search if search.total == 1 => "1 Result".to_owned(),
        search => format!("{} Results", search.total),
    }
}

/// Whose results are hidden until shown: from someone blocked, or
/// ignored.
fn hidden_why(cx: Ctx<'_>, message: &yapfast_core::chat::ChatMessage) -> Option<bool> {
    let user = discord::sender_user(&message.sender)?;
    if cx.state.is_blocked(user) {
        Some(true)
    } else {
        cx.state
            .discord_ignored
            .iter()
            .any(|ignored| ignored == user)
            .then_some(false)
    }
}

/// What the search found, beside the conversation.
pub fn view<'a>(searching: &'a Searching, cx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let state = cx.state;
    let search = state.search_shown()?;
    let theme = cx.look.theme;
    let server = discord::parse_server(&search.place).is_some();
    let mut title = row![elide(count(search)).size(15).font(BOLD).color(theme.text)]
        .spacing(8)
        .align_y(Alignment::Center);
    if search.indexing_older && !search.loading {
        // Discord links its help on indexing; this says what it says.
        let indexing = elide("Indexing").size(13).color(theme.link);
        title = title.push(
            iced::widget::tooltip(
                indexing,
                controls::tip(
                    theme,
                    format!(
                        "{} results indexed so far. We’re still indexing older messages.",
                        search.total
                    ),
                ),
                iced::widget::tooltip::Position::Bottom,
            )
            .gap(6),
        );
    }
    let sort = cx
        .look
        .icon_button(Icon::ArrowsUpDown, "Sort", Message::Search(Event::SortMenu))
        .below();
    // Discord's Filters button, with how many the search has.
    let set = parse(&search.query, cx.state).filters();
    let filters = controls::action(
        theme,
        if set == 0 {
            "Filters".to_owned()
        } else {
            format!("Filters ({set})")
        },
        Kind::Ghost,
        Some(Message::Search(Event::Filters(
            crate::search_filters::FiltersEvent::Open,
        ))),
    )
    .small();
    let head = column![
        container(
            row![
                container(title).width(Length::Fill),
                filters,
                sort,
                cx.look
                    .icon_button(Icon::X, "Close search", Message::Search(Event::Clear))
                    .below(),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding(Padding::ZERO.left(16).right(10))
        .height(HEAD - 1.0)
        .align_y(Alignment::Center),
        bar(theme.line, Length::Fill, 1),
    ];
    let note = match &search.error {
        Some(_) => Some("We dropped the magnifying glass. Can you try searching again?"),
        // Discord's words while it indexes, before it finds anything.
        None if search.indexing && search.results.is_empty() => Some(if server {
            "Before searching, we need to index this server. Give us a bit."
        } else {
            "Before searching, we need to index this DM. Give us a bit."
        }),
        None if !search.loading && search.results.is_empty() => {
            Some("We searched far and wide. Unfortunately, no results were found.")
        }
        None => None,
    };
    let mut list = column![].spacing(8).padding(12);
    if search.page + 1 >= SEARCH_PAGES {
        list = list.push(notice(
            cx,
            "We can only return up to 400 pages of results. Try making your search more specific!",
        ));
    }
    let card = crate::style::rounded(theme.chat, 8.0);
    if bots_mostly(search) {
        list = list.push(
            container(
                column![
                    text("Want to filter out search results from bots?")
                        .size(14)
                        .color(theme.text),
                    link(
                        cx,
                        "Try adding authorType: user to your search.",
                        Message::Search(Event::Append("authorType: user")),
                    ),
                ]
                .spacing(2),
            )
            .padding(12)
            .width(Length::Fill)
            .style(move |_| card),
        );
    }
    let (mut blocked, mut ignored) = (0, 0);
    for result in &searching.results {
        let hidden = hidden_why(cx, &result.found.message);
        match hidden {
            Some(true) => blocked += 1,
            Some(false) => ignored += 1,
            None => {}
        }
        if hidden.is_some() && !search.show_hidden {
            continue;
        }
        let mut entry = column![].spacing(6);
        if let Some(heading) = &result.heading {
            entry = entry.push(channel_heading(cx, heading, &result.room));
        }
        let jump = Message::Search(Event::Jump(
            result.room.clone(),
            result.found.message.item.clone(),
        ));
        entry = entry.push(crate::found::found_view(cx, &result.found, jump));
        list = list.push(entry);
    }
    if blocked + ignored > 0 {
        let hidden = blocked + ignored;
        // Discord's words, plurals and all; its sentence about the blocked
        // has straight apostrophes.
        let (results, messages) = if hidden == 1 {
            ("1 result".to_owned(), "1 message".to_owned())
        } else {
            (format!("{hidden} results"), format!("{hidden} messages"))
        };
        let label = match (search.show_hidden, blocked, ignored) {
            (true, _, 0) => format!("Hide {messages} from blocked users"),
            (true, 0, _) => format!("Hide {messages} from ignored users"),
            (true, ..) => format!("Hide {hidden} messages from blocked and ignored users"),
            (false, _, 0) => format!("We've hidden {results} from users you've blocked."),
            (false, 0, _) => format!("We’ve hidden {results} from users you’ve ignored."),
            (false, ..) => {
                format!("We’ve hidden {hidden} messages from users you’ve blocked and ignored.")
            }
        };
        list = list.push(
            container(link(
                cx,
                label,
                Message::Search(Event::ShowHidden(!search.show_hidden)),
            ))
            .center_x(Length::Fill),
        );
    }
    if search.total > SEARCH_PAGE {
        list = list.push(pages(cx, search));
    }
    let mut body = column![head];
    if let Some(note) = note {
        body = body.push(container(text(note).size(14).color(theme.secondary)).padding(16));
    }
    body = body.push(
        scroll_view(list)
            .id(iced::widget::Id::new(RESULTS))
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

/// Discord's hint about bots' results: on the first page, of ten results
/// or more, three in four or more from bots or webhooks, and no
/// `authorType:` asked for yet.
fn bots_mostly(search: &Search) -> bool {
    let bots = search.results.iter().filter(|hit| hit.message.bot).count();
    search.page == 0
        && search.results.len() >= 10
        && bots * 4 >= search.results.len() * 3
        && !search.query.to_lowercase().contains("authortype:")
}

/// A warning over the results.
fn notice<'a>(cx: Ctx<'a>, words: &'a str) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let card = crate::style::rounded(theme.chat, 8.0);
    container(text(words).size(14).color(theme.text))
        .padding(12)
        .width(Length::Fill)
        .style(move |_| card)
        .into()
}

/// Words in the link colour that do something when clicked.
fn link<'a>(cx: Ctx<'a>, words: impl Into<String>, message: Message) -> Element<'a, Message> {
    let ink = cx.look.theme.link;
    iced::widget::button(text(words.into()).size(14).color(ink))
        .padding(0)
        .style(iced::widget::button::text)
        .on_press(message)
        .into()
}

/// A run of results' channel: its icon and name, the words searched for
/// highlighted, and for a thread its channel beside it. A click opens it.
fn channel_heading<'a>(cx: Ctx<'a>, heading: &'a Heading, room: &str) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let mut spans: Vec<iced::advanced::text::Span<'a, Message>> = Vec::new();
    let mut at = 0;
    for mark in &heading.marks {
        spans.push(iced::widget::span(&heading.name[at..mark.start]));
        spans.push(
            iced::widget::span(&heading.name[mark.clone()])
                .background(crate::markdown::SEARCH_HIGHLIGHT)
                .border(crate::style::round(2)),
        );
        at = mark.end;
    }
    spans.push(iced::widget::span(&heading.name[at..]));
    let name = iced::widget::rich_text(spans)
        .size(14)
        .font(BOLD)
        .color(theme.text)
        .wrapping(iced::widget::text::Wrapping::None);
    let mut line = row![cx.look.glyph(heading.icon, 16, theme.muted), name]
        .spacing(4)
        .align_y(Alignment::Center);
    if let Some((icon, parent)) = &heading.parent {
        let chip = crate::style::rounded(theme.sunken, 8.0);
        line = line.push(
            container(
                row![
                    cx.look.glyph(*icon, 12, theme.muted),
                    elide(parent.as_str()).size(12).color(theme.secondary),
                ]
                .spacing(2)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([1, 6]))
            .style(move |_| chip),
        );
    }
    iced::widget::button(line)
        .padding(Padding::ZERO.top(6).left(4))
        .style(iced::widget::button::text)
        .on_press(Message::Search(Event::Open(room.to_owned())))
        .into()
}

/// Discord's pages under the results: Back, numbers, Next.
fn pages<'a>(cx: Ctx<'a>, search: &Search) -> Element<'a, Message> {
    let theme = cx.look.theme;
    let selected = search.page + 1;
    let mut line = row![].spacing(4).align_y(Alignment::Center);
    let wait = search.loading;
    for button in page_buttons(selected, search.pages(), 5) {
        let shown: Element<'a, Message> = match button {
            PageButton::Back { disabled } | PageButton::Next { disabled } => {
                let back = matches!(button, PageButton::Back { .. });
                let (label, to) = if back {
                    ("Back", selected.saturating_sub(2))
                } else {
                    ("Next", selected)
                };
                controls::action(
                    theme,
                    label,
                    Kind::Plain,
                    (!disabled && !wait).then(|| Message::Search(Event::Page(to))),
                )
                .small()
                .into()
            }
            PageButton::Page(page, current) => {
                let kind = if current { Kind::Primary } else { Kind::Plain };
                controls::action(
                    theme,
                    page.to_string(),
                    kind,
                    (!current && !wait).then(|| Message::Search(Event::Page(page - 1))),
                )
                .small()
                .into()
            }
            PageButton::Gap => text("…").size(14).color(theme.muted).into(),
        };
        line = line.push(shown);
    }
    container(line)
        .padding(Padding::from([12, 0]))
        .center_x(Length::Fill)
        .into()
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
            .icon_button(Icon::X, "Clear search", clear)
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

#[cfg(test)]
mod history_tests {
    use super::remember;

    #[test]
    fn history_keeps_five_newest_first_as_discords_box() {
        let mut kept = Vec::new();
        for query in [
            "trip",
            "trip notes",
            "plan",
            "from: bob",
            "has: link",
            "pinned: true",
        ] {
            assert!(remember(&mut kept, query));
        }
        // "trip notes" went on from "trip", which it took the place of.
        assert_eq!(
            kept,
            [
                "pinned: true",
                "has: link",
                "from: bob",
                "plan",
                "trip notes"
            ]
        );
        // One already kept goes first; the newest again changes nothing.
        assert!(remember(&mut kept, "plan"));
        assert_eq!(kept[0], "plan");
        assert!(!remember(&mut kept, " plan "));
        assert_eq!(kept.len(), 5);
    }
}
