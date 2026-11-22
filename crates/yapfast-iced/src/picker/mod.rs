//! The expression picker: from the composer in a Discord room, a panel of
//! three tabs over the composer (GIFs, Emoji, Stickers, 498 by 486, each
//! grid under the tabs); from the composer elsewhere, the edit or a
//! message's Add Reaction, the emoji alone at its reaction popup's size
//! (408 by 432). The tabs are built from one kit ([`kit`]): the search
//! field, the rail, the grid of tiles of which only those near the viewport
//! are built, the stars and the foot.
//!
//! Each tab holds what it laid out while it shows; another tab showing, or
//! the picker closing, lets go of it and of the pictures it made. The GIFs
//! and stickers are the core's (`application/gifs.rs`,
//! `application/stickers.rs`), which keeps the pictures of the tiles in
//! sight alone, as it's told them.

mod emoji;
mod gifs;
pub mod kit;
mod stickers;

pub use emoji::drawn;
use iced::{
    Color, Element, Length, Padding, Point, Size, Task,
    widget::{column, container, operation, scrollable::AbsoluteOffset, scrollable::Viewport},
};
use yapfast_core::application::Action;

use crate::{
    app::{App, Message},
    composer::Writing,
    ctx::{Ctx, Cx, Focus, Request, Screen, cx},
    field::{self, Draft},
    slot::{Closes, Why},
};
use kit::{FIELD, GRID, Scroll};

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Composer,
    /// The message being edited in its row.
    Edit,
    /// A reaction to the message with this item, the menu's place.
    React(String, Point),
    /// The custom status's emoji, beside where its button was pressed.
    Status(Point),
    /// A poll's answer's emoji, by its place, likewise.
    Poll(usize, Point),
}

/// The expression picker's tabs, as Discord orders them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Gifs,
    Emoji,
    Stickers,
}

#[derive(Clone, Debug)]
pub enum Event {
    Field(field::Event),
    Scrolled(Viewport),
    /// The pointer came onto a tile, by its place, or left it.
    Hover(usize),
    Left(usize),
    Pick(usize),
    /// A tile's star.
    Star(usize),
    /// A section on the rail, by its place.
    Jump(usize),
    /// The pointer came onto a section on the rail, or left it.
    RailHover(usize),
    RailLeft(usize),
    /// A press in the picker that nothing in it took.
    Pressed,
    Tab(Tab),
    /// A plugin's tab chosen, by the plugin's id.
    #[cfg(feature = "plugins")]
    PluginTab(String),
    /// The GIFs' Back, to their home.
    Back,
    /// A trending or suggested term, searched for.
    Term(String),
    /// A press outside it.
    Close,
    /// The search a pause in typing GIFs' search lets go, by its count.
    SearchDue(u64),
    /// Its left edge dragged this far since the press, let go, and double
    /// clicked back to its own width.
    Widen(f32),
    Widened,
    ResetWidth,
}

/// The expression picker's own width, and its height, as Discord's; its
/// left edge drags it wider.
pub const WIDTH: f32 = 498.0;
const HEIGHT: f32 = 486.0;

pub struct Picker {
    pub target: Target,
    pub tab: Tab,
    /// The composer's picker in a Discord room: the three tabs.
    pub tabs: bool,
    pub field: Draft,
    pub scroll: Scroll,
    /// The tile the pointer is on, by its place.
    pub hover: Option<usize>,
    /// The rail's section the pointer is on, which the foot names.
    rail_hover: Option<usize>,
    emoji: Option<emoji::Emoji>,
    gifs: Option<gifs::Gifs>,
    stickers: Option<stickers::Stickers>,
    /// What the layout was built for, so one built again for the same is
    /// skipped.
    built: u64,
    /// The searches typed into the GIFs' field, the last of which goes once
    /// typing pauses.
    typed: u64,
    /// The expression picker's width, and what it was as its edge was
    /// pressed, while it's dragged.
    width: f32,
    widen_from: Option<f32>,
    /// A plugin's tab chosen in place of Yapfast's (`picker.tab`), by the
    /// plugin's id.
    #[cfg(feature = "plugins")]
    pub plugin_tab: Option<String>,
}

impl Picker {
    /// The panel's size: the expression picker's, or the reaction popup's.
    pub fn size(&self) -> Size {
        if self.tabs {
            Size::new(self.width, HEIGHT)
        } else {
            Size::new(408.0, 432.0)
        }
    }

    /// The tab's own grid's width, beside its rail.
    fn grid_width(&self) -> f32 {
        match self.tab {
            Tab::Gifs => self.size().width,
            Tab::Emoji | Tab::Stickers => self.size().width - kit::RAIL,
        }
    }

    /// A tab shows, its field saying the core's search: the GIFs' and the
    /// stickers' are kept while the picker is shut, as Discord's are.
    fn show(&mut self, tab: Tab, query: &str) {
        self.tab = tab;
        self.field.reset(query);
        self.scroll = Scroll::default();
        self.hover = None;
        self.rail_hover = None;
        self.built = 0;
        self.emoji = (tab == Tab::Emoji).then(emoji::Emoji::default);
        self.gifs = (tab == Tab::Gifs).then(gifs::Gifs::default);
        self.stickers = (tab == Tab::Stickers).then(stickers::Stickers::default);
    }
}

impl Picker {
    /// The emoji tab, while it shows.
    #[cfg(test)]
    pub fn emoji_tab(&self) -> Option<&emoji::Emoji> {
        self.emoji.as_ref()
    }

    /// A picker for `target` on `tab`, `width` wide if it has the tabs;
    /// the core opens what the tab shows.
    fn open(target: Target, tab: Tab, tabs: bool, width: f32, cx: &mut Cx<'_>) -> Self {
        let mut picker = Picker {
            target,
            tab,
            tabs,
            field: Draft::default(),
            scroll: Scroll::default(),
            hover: None,
            rail_hover: None,
            emoji: None,
            gifs: None,
            stickers: None,
            built: 0,
            typed: 0,
            width,
            widen_from: None,
            #[cfg(feature = "plugins")]
            plugin_tab: None,
        };
        let query = open_tab(tab, cx);
        picker.show(tab, &query);
        picker
    }

    fn switch_tab(&mut self, tab: Tab, cx: &mut Cx<'_>) -> Task<Message> {
        let old = self.tab;
        if old == tab {
            return Task::none();
        }
        close_tab(old, cx);
        let query = open_tab(tab, cx);
        self.show(tab, &query);
        crate::control_file::trim();
        cx.touch();
        Task::batch([self.scroll_grid(0.0), operation::focus(FIELD)])
    }

    /// Asks the root to close the picker.
    fn ask_close(&self, cx: &mut Cx<'_>) -> Task<Message> {
        cx.request(Request::Close(Screen::Picker));
        Task::none()
    }

    fn scroll_grid(&mut self, top: f32) -> Task<Message> {
        self.scroll.top = top;
        operation::scroll_to(
            GRID,
            AbsoluteOffset {
                x: None,
                y: Some(top),
            },
        )
    }

    /// The tab's layout follows the core, and the pictures in sight are
    /// asked for; called as the window syncs.
    pub fn sync(&mut self, cx: &mut Cx<'_>) {
        match self.tab {
            Tab::Emoji => {
                self.build_emoji(cx);
                self.emoji_reach(cx);
            }
            Tab::Gifs => {
                self.build_gifs(cx);
                self.ask_gifs(cx);
            }
            Tab::Stickers => {
                self.build_stickers(cx);
                self.ask_stickers(cx);
            }
        }
    }

    /// The pictures the picker's rail draws from the app's own: servers'
    /// icons and their first emoji. The emoji grid asks for its own
    /// (`emoji_reach`).
    pub fn pictures(&self) -> Vec<String> {
        match (&self.emoji, &self.stickers) {
            (Some(emoji), _) => emoji.pictures(),
            (_, Some(stickers)) => stickers.icons(),
            _ => Vec::new(),
        }
    }

    /// What's done in the picker; its edge dragged is the root's, which
    /// knows the room it has (`picker::picker_event`).
    pub fn update(&mut self, event: Event, cx: &mut Cx<'_>) -> Task<Message> {
        let tab = self.tab;
        // A click anywhere in the picker leaves the keys with its search,
        // so typing goes on searching; a pick that closes it gives them
        // back after.
        if matches!(
            event,
            Event::Pick(_)
                | Event::Star(_)
                | Event::Jump(_)
                | Event::Back
                | Event::Term(_)
                | Event::Pressed
        ) {
            cx.request(Request::Focus(Focus::Id(FIELD)));
        }
        match event {
            Event::Tab(tab) => {
                #[cfg(feature = "plugins")]
                {
                    self.plugin_tab = None;
                }
                self.switch_tab(tab, cx)
            }
            #[cfg(feature = "plugins")]
            Event::PluginTab(plugin) => {
                self.plugin_tab = Some(plugin);
                Task::none()
            }
            Event::Scrolled(viewport) => {
                self.scroll = Scroll {
                    top: viewport.absolute_offset().y,
                    height: viewport.bounds().height,
                };
                match tab {
                    Tab::Gifs => self.ask_gifs(cx),
                    Tab::Stickers => self.ask_stickers(cx),
                    Tab::Emoji => self.emoji_reach(cx),
                }
                Task::none()
            }
            Event::Hover(index) => {
                self.hover = Some(index);
                Task::none()
            }
            Event::Left(index) => {
                if self.hover == Some(index) {
                    self.hover = None;
                }
                Task::none()
            }
            Event::Jump(index) => {
                // The sticker picker's foot names the collection again only
                // once the pointer comes back onto it.
                if tab == Tab::Stickers {
                    self.rail_hover = None;
                }
                self.jump(index, cx)
            }
            Event::RailHover(index) => {
                self.rail_hover = Some(index);
                Task::none()
            }
            Event::RailLeft(index) => {
                if self.rail_hover == Some(index) {
                    self.rail_hover = None;
                }
                Task::none()
            }
            Event::Pressed => Task::none(),
            Event::Field(field::Event::Escape) => match tab {
                Tab::Gifs => self.gifs_escape(cx),
                _ => self.ask_close(cx),
            },
            Event::Field(field::Event::Edited(text, _)) => match tab {
                Tab::Emoji => self.emoji_search(text, cx),
                Tab::Gifs => self.gifs_typed(text, cx),
                Tab::Stickers => self.stickers_search(text, cx),
            },
            Event::Field(field::Event::Send) => match tab {
                Tab::Emoji => self.emoji_first(cx),
                Tab::Gifs => self.gifs_search_now(cx),
                Tab::Stickers => self.sticker_first(cx),
            },
            Event::Field(_) => Task::none(),
            Event::Pick(index) => match tab {
                Tab::Emoji => self.emoji_pick(index, cx),
                Tab::Gifs => self.gif_pick(index, cx),
                Tab::Stickers => self.sticker_pick(index, cx),
            },
            Event::Star(index) => {
                match tab {
                    Tab::Emoji => self.emoji_star(index, cx),
                    Tab::Gifs => self.gif_star(index, cx),
                    Tab::Stickers => self.sticker_star(index, cx),
                }
                cx.touch();
                Task::none()
            }
            Event::Back => self.gifs_back(cx),
            Event::Close => self.ask_close(cx),
            Event::Term(term) => self.gif_term(term, cx),
            Event::SearchDue(typed) => self.gifs_due(typed, cx),
            Event::Widened => {
                // Kept for the next start, if the drag changed it.
                if self.widen_from.take() != Some(self.width) {
                    cx.layout.picker_width = Some(self.width);
                    cx.keep("picker_width", self.width);
                }
                Task::none()
            }
            Event::ResetWidth => {
                self.widen_from = None;
                self.width = WIDTH;
                self.built = 0;
                cx.layout.picker_width = Some(WIDTH);
                cx.keep("picker_width", WIDTH);
                self.sync(cx);
                Task::none()
            }
            Event::Widen(_) => Task::none(),
        }
    }

    /// Its left edge dragged `by` since the press, in `room`.
    pub fn widen(&mut self, by: f32, room: f32, cx: &mut Cx<'_>) {
        let from = *self.widen_from.get_or_insert(self.width);
        self.width = (from - by).clamp(WIDTH, room);
        self.built = 0;
        self.sync(cx);
    }

    /// The rail's section, by its place: the grid jumps to its top,
    /// leaving a search first.
    fn jump(&mut self, index: usize, cx: &mut Cx<'_>) -> Task<Message> {
        let mut tasks = Vec::new();
        if !self.field.text().is_empty() {
            self.field.reset("");
            match self.tab {
                Tab::Emoji => tasks.push(self.emoji_search(String::new(), cx)),
                Tab::Stickers => tasks.push(self.stickers_search(String::new(), cx)),
                Tab::Gifs => {}
            }
        }
        let top = match self.tab {
            Tab::Emoji => self.emoji.as_ref().and_then(|tab| tab.section_top(index)),
            Tab::Stickers => self
                .stickers
                .as_ref()
                .and_then(|tab| tab.section_top(index)),
            Tab::Gifs => None,
        };
        tasks.push(self.scroll_grid(top.unwrap_or(0.0)));
        if self.tab == Tab::Emoji {
            self.emoji_reach(cx);
        }
        Task::batch(tasks)
    }

    /// An emoji goes where the picker was opened for, which the root
    /// places, and the picker closes.
    fn place_emoji(&self, key: String, cx: &mut Cx<'_>) -> Task<Message> {
        cx.request(Request::PlaceEmoji {
            target: self.target.clone(),
            key,
        });
        self.ask_close(cx)
    }
}

impl Closes for Picker {
    /// The core lets go of what its tab showed, the heap its pictures took
    /// goes back, and the keys go back unless the window went.
    fn close(&mut self, cx: &mut Cx<'_>, why: Why) {
        if why == Why::Window && self.tab == Tab::Emoji {
            // No settle follows the window going: the core lets go itself.
            cx.dispatch(Action::UnloadEmojiPictures);
        } else {
            close_tab(self.tab, cx);
        }
        // What its tabs held goes before the heap is given back.
        self.emoji = None;
        self.gifs = None;
        self.stickers = None;
        crate::control_file::trim();
        if why != Why::Window {
            cx.request(Request::Focus(Focus::Default));
        }
        cx.touch();
    }
}

/// The core opens what a tab shows, and says what its search is. The
/// emoji's: the account's custom emoji pictures go to the disk cache,
/// once a session, behind those the grid asks for, so a scroll reads them
/// from there.
fn open_tab(tab: Tab, cx: &mut Cx<'_>) -> String {
    match tab {
        Tab::Gifs => {
            cx.dispatch(Action::OpenGifs);
            cx.state().gifs.query.clone()
        }
        Tab::Stickers => {
            cx.dispatch(Action::OpenStickers);
            cx.state().stickers.query.clone()
        }
        Tab::Emoji => {
            cx.dispatch(Action::CacheEmojiPictures);
            String::new()
        }
    }
}

/// The core lets go of what a tab showed: the emoji's custom pictures go
/// but those the window shows elsewhere, as the disk cache keeps them.
fn close_tab(tab: Tab, cx: &mut Cx<'_>) {
    match tab {
        Tab::Gifs => cx.dispatch(Action::CloseGifs),
        Tab::Stickers => cx.dispatch(Action::CloseStickers),
        Tab::Emoji => cx.request(Request::KeepEmoji {
            near: Vec::new(),
            first: Vec::new(),
        }),
    }
}

/// Of the custom emoji pictures in memory, those `near` what the emoji
/// picker shows stay, with those the rest of the window shows (a status's
/// emoji, a profile's), which would otherwise go and be read again at
/// once; the core keeps the messages' own. Of the near ones still to
/// come, those not asked for yet are, `first` in its order, ahead of
/// what was asked for before.
///
/// This runs for each row the grid scrolls into. Only the pictures the
/// core holds change, which the views read as they draw, so the window's
/// own images of those let go go here and nothing else is made again.
pub fn keep_emoji(app: &mut App, mut near: Vec<String>, mut first: Vec<String>) {
    // A picker opened in the closed one's place keeps what's near with
    // its own first reach, where letting go now would read it all again.
    let closing = near.is_empty() && first.is_empty();
    if closing
        && app
            .win
            .picker
            .get()
            .is_some_and(|picker| picker.tab == Tab::Emoji)
    {
        return;
    }
    first.retain(|url| !app.client.app.picture_requested(url));
    if !first.is_empty() {
        app.client.dispatch(Action::LoadPicturesFirst(first));
    }
    near.extend(app.pictures_shown(false));
    app.client.dispatch(Action::KeepEmojiPictures(near));
    app.win.pictures.prune();
}

/// Opens the picker for the composer, the edit or a reaction, on a tab;
/// the same again closes it.
pub fn open_picker(app: &mut App, target: Target, tab: Tab) -> Task<Message> {
    app.win.menu = None;
    let tabs = target == Target::Composer && app.client.state().can_pick_gifs();
    let tab = if tabs { tab } else { Tab::Emoji };
    let width = if tabs {
        app.layout
            .picker_width
            .unwrap_or(WIDTH)
            .clamp(WIDTH, picker_room(app))
    } else {
        WIDTH
    };
    let mut cx = cx!(app);
    let task = match app.win.picker.get_mut() {
        Some(picker) if picker.target == target && picker.tab == tab => {
            app.win.picker.close(&mut cx, Why::User);
            Task::none()
        }
        Some(picker) if picker.target == target => picker.switch_tab(tab, &mut cx),
        _ => {
            app.win.picker.close(&mut cx, Why::User);
            app.win
                .picker
                .open(|| Picker::open(target, tab, tabs, width, &mut cx));
            cx.touch();
            // After the keys the closed one gave back.
            cx.request(Request::Focus(Focus::Id(FIELD)));
            Task::none()
        }
    };
    Task::batch([task, app.settle()])
}

/// The picker closes, if it's open.
pub fn close_picker(app: &mut App) -> Task<Message> {
    let mut cx = cx!(app);
    app.win.picker.close(&mut cx, Why::User);
    app.settle()
}

/// Ctrl+G with the GIFs showing closes the picker, as it opened it; the
/// one-line search passes the key on to the window, whose keys come here
/// first.
pub fn gifs_key(
    app: &mut App,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Task<Message>> {
    let gifs = app
        .win
        .picker
        .get()
        .is_some_and(|picker| picker.tab == Tab::Gifs);
    let ctrl_g = modifiers.command()
        && !modifiers.shift()
        && !modifiers.alt()
        && matches!(key.as_ref(), iced::keyboard::Key::Character(g) if g.eq_ignore_ascii_case("g"));
    (gifs && ctrl_g).then(|| close_picker(app))
}

/// The picker's: its edge dragged widens it in the room it has.
pub fn picker_event(app: &mut App, event: Event) -> Task<Message> {
    let room = picker_room(app);
    let mut cx = cx!(app);
    let Some(picker) = app.win.picker.get_mut() else {
        return Task::none();
    };
    match event {
        Event::Widen(by) => {
            picker.widen(by, room, &mut cx);
            Task::none()
        }
        event => picker.update(event, &mut cx),
    }
}

/// An emoji picked goes where the picker was opened for: the
/// composer's text, the edit's, a reaction, the custom status or a
/// poll's answer.
pub fn place_emoji(app: &mut App, target: Target, key: String) -> Task<Message> {
    let text = yapfast_core::emoji::composer_text(&key);
    match target {
        Target::Composer => crate::composer::insert(app, Writing::Draft, &text),
        Target::Edit => crate::composer::insert(app, Writing::Edit, &text),
        Target::React(item, _) => {
            app.client.dispatch(Action::React { item, key });
            app.dirty = true;
        }
        Target::Status(_) => app.status.emoji(key),
        Target::Poll(index, _) => {
            crate::switcher::poll_draft_emoji(app, index, key);
            app.dirty = true;
        }
    }
    Task::none()
}

/// How wide the expression picker may grow: from its right edge, over
/// the emoji button, to 8 px right of the server rail, or 8 px from the
/// window's own edge if that comes first.
pub fn picker_room(app: &App) -> f32 {
    let right = crate::conversation::chat_right(app) - 8.0 - 4.0;
    (app.window.width - 88.0).min(right - 80.0).max(WIDTH)
}

/// The picker, framed, with its tabs when it has them.
pub fn view<'a>(picker: &'a Picker, cx: Ctx<'a>) -> Element<'a, Message> {
    View { s: picker, cx }.view()
}

/// The picker's state and what it reads, as its tabs draw them.
#[derive(Clone, Copy)]
struct View<'a> {
    s: &'a Picker,
    cx: Ctx<'a>,
}

impl<'a> View<'a> {
    fn view(self) -> Element<'a, Message> {
        let picker = self.s;
        let body = match picker.tab {
            Tab::Emoji => self.emoji_view(picker),
            Tab::Gifs => self.gifs_view(picker),
            Tab::Stickers => self.stickers_view(picker),
        };
        // A plugin's tab, while one is chosen and its plugin still draws it.
        #[cfg(feature = "plugins")]
        let (body, plugin_tab) = {
            let subject = self
                .cx
                .state
                .selected_room()
                .map_or("", |room| room.id.as_str());
            match picker
                .plugin_tab
                .as_deref()
                .and_then(|plugin| crate::plugin_slots::picker_tab(self.cx, plugin, subject))
            {
                Some(drawn) => (drawn, picker.plugin_tab.as_deref()),
                None => (body, None),
            }
        };
        #[cfg(not(feature = "plugins"))]
        let plugin_tab: Option<&str> = None;
        let theme = self.cx.look.theme;
        let content: Element<'a, Message> = if picker.tabs {
            // Its left edge drags it wider, as Discord's: the right edge and
            // the height stay. A double click puts it back.
            let edge = crate::widgets::Splitter::new(
                Color::TRANSPARENT,
                theme.line,
                theme.faint,
                |by| Message::Picker(Event::Widen(by)),
                Message::Picker(Event::Widened),
                Message::Picker(Event::ResetWidth),
            );
            iced::widget::stack![
                column![kit::tabs(self.cx, picker.tab, plugin_tab), body].height(Length::Fill),
                container(edge)
                    .padding(Padding::ZERO.left(3))
                    .height(Length::Fill),
            ]
            .into()
        } else {
            body
        };
        // A press nothing in it took gives the keys back to the search.
        iced::widget::mouse_area(kit::frame(content, picker.size(), theme))
            .on_press(Message::Picker(Event::Pressed))
            .into()
    }
}
