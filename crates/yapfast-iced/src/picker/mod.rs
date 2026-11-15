//! The expression picker, as the Slint client's: from the composer in a
//! Discord room, a panel of three tabs over the composer (GIFs, Emoji,
//! Stickers, 498 by 486, each grid under the tabs); from the composer
//! elsewhere, the edit or a message's Add Reaction, the emoji alone at its
//! reaction popup's size (408 by 432). The tabs are built from one kit
//! ([`kit`]): the search field, the rail, the grid of tiles of which only
//! those near the viewport are built, the stars and the foot.
//!
//! Each tab holds what it laid out while it shows; another tab showing, or
//! the picker closing, lets go of it and of the pictures it made, as the
//! Slint client's pickers do. The GIFs and stickers are the core's
//! (`application/gifs.rs`, `application/stickers.rs`), which keeps the
//! pictures of the tiles in sight alone, as it's told them.

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
    field::{self, Draft},
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
    Tab(Tab),
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
        self.built = 0;
        self.emoji = (tab == Tab::Emoji).then(emoji::Emoji::default);
        self.gifs = (tab == Tab::Gifs).then(gifs::Gifs::default);
        self.stickers = (tab == Tab::Stickers).then(stickers::Stickers::default);
    }
}

impl App {
    /// Opens the picker for the composer, the edit or a reaction, on a tab;
    /// the same again closes it.
    pub fn open_picker(&mut self, target: Target, tab: Tab) -> Task<Message> {
        self.menu = None;
        let tabs = target == Target::Composer && self.client.state().can_pick_gifs();
        let tab = if tabs { tab } else { Tab::Emoji };
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target == target && picker.tab == tab)
        {
            return self.close_picker();
        }
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.target == target)
        {
            return self.switch_tab(tab);
        }
        let close = self.close_picker();
        let mut picker = Picker {
            target,
            tab,
            tabs,
            field: Draft::default(),
            scroll: Scroll::default(),
            hover: None,
            emoji: None,
            gifs: None,
            stickers: None,
            built: 0,
            typed: 0,
            width: WIDTH,
            widen_from: None,
        };
        if tabs {
            picker.width = self
                .layout
                .picker_width
                .unwrap_or(WIDTH)
                .clamp(WIDTH, self.picker_room());
        }
        let query = self.open_tab(tab);
        picker.show(tab, &query);
        self.picker = Some(picker);
        Task::batch([close, self.sync(), operation::focus(FIELD)])
    }

    /// The core opens what a tab shows, and says what its search is.
    fn open_tab(&mut self, tab: Tab) -> String {
        match tab {
            Tab::Gifs => {
                self.client.dispatch(Action::OpenGifs);
                self.client.state().gifs.query.clone()
            }
            Tab::Stickers => {
                self.client.dispatch(Action::OpenStickers);
                self.client.state().stickers.query.clone()
            }
            Tab::Emoji => String::new(),
        }
    }

    /// The core lets go of what a tab showed.
    fn close_tab(&mut self, tab: Tab) {
        match tab {
            Tab::Gifs => self.client.dispatch(Action::CloseGifs),
            Tab::Stickers => self.client.dispatch(Action::CloseStickers),
            Tab::Emoji => {}
        }
    }

    fn switch_tab(&mut self, tab: Tab) -> Task<Message> {
        let Some(old) = self.picker.as_ref().map(|picker| picker.tab) else {
            return Task::none();
        };
        if old == tab {
            return Task::none();
        }
        self.close_tab(old);
        let query = self.open_tab(tab);
        if let Some(picker) = &mut self.picker {
            picker.show(tab, &query);
        }
        crate::control::trim();
        Task::batch([self.sync(), self.scroll_grid(0.0), operation::focus(FIELD)])
    }

    /// The picker closes and lets go of its pictures, and the heap they
    /// took goes back, as the Slint client's `let_go_after_picker` does.
    pub fn close_picker(&mut self) -> Task<Message> {
        let Some(picker) = self.picker.take() else {
            return Task::none();
        };
        self.close_tab(picker.tab);
        drop(picker);
        crate::control::trim();
        Task::batch([self.refocus(), self.sync()])
    }

    fn scroll_grid(&mut self, top: f32) -> Task<Message> {
        if let Some(picker) = &mut self.picker {
            picker.scroll.top = top;
        }
        operation::scroll_to(
            GRID,
            AbsoluteOffset {
                x: None,
                y: Some(top),
            },
        )
    }

    /// The tab's layout follows the core, and the pictures in sight are
    /// asked for; called from `sync`.
    pub fn picker_sync(&mut self) {
        let Some(tab) = self.picker.as_ref().map(|picker| picker.tab) else {
            return;
        };
        match tab {
            Tab::Emoji => self.build_emoji(),
            Tab::Gifs => {
                self.build_gifs();
                self.ask_gifs();
            }
            Tab::Stickers => {
                self.build_stickers();
                self.ask_stickers();
            }
        }
    }

    /// The pictures the picker draws from the app's own: custom emoji and
    /// servers' icons.
    pub fn picker_pictures(&self) -> Vec<String> {
        let Some(picker) = &self.picker else {
            return Vec::new();
        };
        match (&picker.emoji, &picker.stickers) {
            (Some(emoji), _) => emoji.pictures(picker.scroll),
            (_, Some(stickers)) => stickers.icons(),
            _ => Vec::new(),
        }
    }

    pub fn picker_view(&self) -> Option<Element<'_, Message>> {
        let picker = self.picker.as_ref()?;
        let body = match picker.tab {
            Tab::Emoji => self.emoji_view(picker),
            Tab::Gifs => self.gifs_view(picker),
            Tab::Stickers => self.stickers_view(picker),
        };
        let content: Element<'_, Message> = if picker.tabs {
            // Its left edge drags it wider, as Discord's: the right edge and
            // the height stay. A double click puts it back.
            let theme = &self.theme;
            let edge = crate::widgets::Splitter::new(
                Color::TRANSPARENT,
                theme.line,
                theme.faint,
                |by| Message::Picker(Event::Widen(by)),
                Message::Picker(Event::Widened),
                Message::Picker(Event::ResetWidth),
            );
            iced::widget::stack![
                column![kit::tabs(theme, picker.tab), body].height(Length::Fill),
                container(edge)
                    .padding(Padding::ZERO.left(3))
                    .height(Length::Fill),
            ]
            .into()
        } else {
            body
        };
        Some(kit::frame(content, picker.size(), &self.theme))
    }

    pub fn picker_event(&mut self, event: Event) -> Task<Message> {
        let Some(picker) = &mut self.picker else {
            return Task::none();
        };
        let tab = picker.tab;
        match event {
            Event::Tab(tab) => self.switch_tab(tab),
            Event::Scrolled(viewport) => {
                picker.scroll = Scroll {
                    top: viewport.absolute_offset().y,
                    height: viewport.bounds().height,
                };
                match tab {
                    Tab::Gifs => self.ask_gifs(),
                    Tab::Stickers => self.ask_stickers(),
                    Tab::Emoji => return self.ask(),
                }
                Task::none()
            }
            Event::Hover(index) => {
                picker.hover = Some(index);
                Task::none()
            }
            Event::Left(index) => {
                if picker.hover == Some(index) {
                    picker.hover = None;
                }
                Task::none()
            }
            Event::Jump(index) => self.jump(index),
            Event::Field(field::Event::Escape) => match tab {
                Tab::Gifs => self.gifs_escape(),
                _ => self.close_picker(),
            },
            Event::Field(field::Event::Gifs) => self.close_picker(),
            Event::Field(field::Event::Edited(text, _)) => match tab {
                Tab::Emoji => self.emoji_search(text),
                Tab::Gifs => self.gifs_typed(text),
                Tab::Stickers => self.stickers_search(text),
            },
            Event::Field(field::Event::Send) => match tab {
                Tab::Emoji => self.emoji_first(),
                Tab::Gifs => self.gifs_search_now(),
                Tab::Stickers => self.sticker_first(),
            },
            Event::Field(_) => Task::none(),
            Event::Pick(index) => match tab {
                Tab::Emoji => self.emoji_pick(index),
                Tab::Gifs => self.gif_pick(index),
                Tab::Stickers => self.sticker_pick(index),
            },
            Event::Star(index) => {
                match tab {
                    Tab::Emoji => self.emoji_star(index),
                    Tab::Gifs => self.gif_star(index),
                    Tab::Stickers => self.sticker_star(index),
                }
                self.sync()
            }
            Event::Back => self.gifs_back(),
            Event::Close => self.close_picker(),
            Event::Term(term) => self.gif_term(term),
            Event::SearchDue(typed) => self.gifs_due(typed),
            Event::Widen(by) => {
                let room = self.picker_room();
                if let Some(picker) = &mut self.picker {
                    let from = *picker.widen_from.get_or_insert(picker.width);
                    picker.width = (from - by).clamp(WIDTH, room);
                    picker.built = 0;
                }
                self.picker_sync();
                Task::none()
            }
            Event::Widened => {
                let Some(picker) = &mut self.picker else {
                    return Task::none();
                };
                // Kept for the next start, if the drag changed it.
                if picker.widen_from.take() != Some(picker.width) {
                    self.layout.picker_width = Some(picker.width);
                    crate::layout::store(self.client.root(), "picker_width", picker.width);
                }
                Task::none()
            }
            Event::ResetWidth => {
                if let Some(picker) = &mut self.picker {
                    picker.widen_from = None;
                    picker.width = WIDTH;
                    picker.built = 0;
                }
                self.layout.picker_width = Some(WIDTH);
                crate::layout::store(self.client.root(), "picker_width", WIDTH);
                self.picker_sync();
                Task::none()
            }
        }
    }

    /// How wide the expression picker may grow: from its right edge, over
    /// the emoji button, to 8 px right of the server rail, or 8 px from the
    /// window's own edge if that comes first, as the Slint client's.
    pub fn picker_room(&self) -> f32 {
        let right = self.chat_right() - 8.0 - 4.0;
        (self.window.width - 88.0).min(right - 80.0).max(WIDTH)
    }

    /// The rail's section, by its place: the grid jumps to its top,
    /// leaving a search first.
    fn jump(&mut self, index: usize) -> Task<Message> {
        let Some(picker) = &mut self.picker else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        if !picker.field.text().is_empty() {
            picker.field.reset("");
            match picker.tab {
                Tab::Emoji => tasks.push(self.emoji_search(String::new())),
                Tab::Stickers => tasks.push(self.stickers_search(String::new())),
                Tab::Gifs => {}
            }
        }
        let Some(picker) = &self.picker else {
            return Task::none();
        };
        let top = match picker.tab {
            Tab::Emoji => picker.emoji.as_ref().and_then(|tab| tab.section_top(index)),
            Tab::Stickers => picker
                .stickers
                .as_ref()
                .and_then(|tab| tab.section_top(index)),
            Tab::Gifs => None,
        };
        tasks.push(self.scroll_grid(top.unwrap_or(0.0)));
        Task::batch(tasks)
    }

    /// An emoji goes where the picker was opened for: the composer's text,
    /// the edit's, or a reaction.
    fn place_emoji(&mut self, key: String) -> Task<Message> {
        let Some(target) = self.picker.as_ref().map(|picker| picker.target.clone()) else {
            return Task::none();
        };
        let close = self.close_picker();
        let text = yapfast_core::emoji::composer_text(&key);
        match target {
            Target::Composer => {
                self.insert(Writing::Draft, &text);
                close
            }
            Target::Edit => {
                self.insert(Writing::Edit, &text);
                close
            }
            Target::React(item, _) => {
                self.client.dispatch(Action::React { item, key });
                Task::batch([close, self.sync()])
            }
            Target::Status(_) => {
                self.status_emoji(key);
                close
            }
            Target::Poll(index, _) => {
                self.poll_draft_emoji(index, key);
                Task::batch([close, self.sync()])
            }
        }
    }
}
