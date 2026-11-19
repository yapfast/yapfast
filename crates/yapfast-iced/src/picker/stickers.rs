//! The Stickers tab: the search field; the rail of collections (Favorites,
//! Frequently Used, every server's own stickers, the open server's first,
//! then Discord's default packs); the grid of 96 px tiles in columns under
//! each collection's name, those this account may not send here greyed with
//! a lock; the pointed sticker's name at the foot. A click sends a sticker
//! as a message of its own; its star keeps it among the favourites. Only
//! the pictures in sight and a screen either side are asked of the core.

use std::hash::{Hash, Hasher};

use iced::{
    Alignment, Element, Length, Rectangle, Size, Task,
    widget::{Space, button, column, container, mouse_area, row, stack},
};
use yapfast_core::application::{Action, StickerPicture};

use super::{
    Event, Picker, View,
    kit::{self, Grid},
};
use crate::{
    app::Message,
    controls::lit_style,
    ctx::Cx,
    media::{Fit, Shot},
    pictures::Icon,
    widgets::Pic,
};

/// A tile's side, the gap between them and the grid's edge, as Discord's
/// picker has them, and a collection's heading's height.
const TILE: f32 = 96.0;
const GAP: f32 = 8.0;
const PADDING: f32 = 12.0;
const HEADING: f32 = 28.0;

/// What a collection's heading and rail button show.
pub enum Mark {
    Clock,
    /// A server's icon, by its address.
    Icon(String),
    /// A pack's first sticker's picture, by its address.
    Sticker(String),
}

pub enum Item {
    Heading(String),
    Sticker {
        id: String,
        name: String,
        picture: String,
        usable: bool,
        favorite: bool,
    },
}

#[derive(Default)]
pub struct Stickers {
    grid: Grid<Item, Mark>,
    /// The section each heading heads, by the heading's place.
    headings: std::collections::HashMap<usize, usize>,
    /// The pictures last asked for.
    asked: Vec<String>,
}

impl Stickers {
    pub fn section_top(&self, index: usize) -> Option<f32> {
        self.grid.sections.get(index).map(|section| section.top)
    }

    /// The servers' icons the rail and headings show, from the app's
    /// pictures.
    pub fn icons(&self) -> Vec<String> {
        self.grid
            .sections
            .iter()
            .filter_map(|section| match &section.mark {
                Mark::Icon(url) => Some(url.clone()),
                _ => None,
            })
            .collect()
    }
}

impl Picker {
    pub(super) fn build_stickers(&mut self, cx: &Cx<'_>) {
        let state = cx.state();
        let picker_state = &state.stickers;
        let nitro = cx.client.app.discord_nitro();
        let listed = picker_state.listed(nitro);
        let mut hasher = std::hash::DefaultHasher::new();
        picker_state.query.hash(&mut hasher);
        for collection in &listed {
            (collection.id, collection.name, collection.stickers.len()).hash(&mut hasher);
        }
        for sticker in &picker_state.favorites {
            sticker.id.hash(&mut hasher);
        }
        let signature = hasher.finish();
        let picker = &mut *self;
        if picker.built == signature {
            return;
        }
        picker.built = signature;
        let width = picker.grid_width();
        let Some(tab) = &mut picker.stickers else {
            return;
        };
        let columns = (((width - 2.0 * PADDING + GAP) / (TILE + GAP)).floor() as usize).max(1);
        let favorites = &picker_state.favorites;
        let mut grid = Grid::default();
        let mut headings = std::collections::HashMap::new();
        let mut top = 0.0;
        for collection in listed {
            let start = top;
            let mark = if collection.id == "frequent" {
                Mark::Clock
            } else {
                let server = yapfast_core::discord::server_id(collection.id);
                match state
                    .servers
                    .iter()
                    .find(|known| known.id == server)
                    .and_then(|server| server.icon.clone())
                {
                    Some(url) => Mark::Icon(url),
                    None => Mark::Sticker(
                        collection
                            .stickers
                            .first()
                            .map(|(sticker, _)| sticker.picture.clone())
                            .unwrap_or_default(),
                    ),
                }
            };
            headings.insert(grid.tiles.len(), grid.sections.len());
            grid.place(
                Rectangle::new(iced::Point::new(0.0, top), Size::new(width, HEADING)),
                Item::Heading(collection.name.to_owned()),
            );
            top += HEADING;
            for (index, (sticker, usable)) in collection.stickers.iter().enumerate() {
                let (column, line) = (index % columns, index / columns);
                grid.place(
                    Rectangle::new(
                        iced::Point::new(
                            PADDING + column as f32 * (TILE + GAP),
                            top + line as f32 * (TILE + GAP),
                        ),
                        Size::new(TILE, TILE),
                    ),
                    Item::Sticker {
                        id: sticker.id.clone(),
                        name: sticker.name.clone(),
                        picture: sticker.picture.clone(),
                        usable: *usable,
                        favorite: favorites.iter().any(|known| known.id == sticker.id),
                    },
                );
            }
            top += collection.stickers.len().div_ceil(columns) as f32 * (TILE + GAP) + GAP;
            grid.sections.push(kit::Section {
                top: start,
                end: top,
                mark,
                name: collection.name.to_owned(),
            });
        }
        grid.height = top;
        tab.grid = grid;
        tab.headings = headings;
        picker.hover = None;
    }

    /// Tells the core the pictures of the tiles in sight, and the packs'
    /// first stickers the rail shows; it lets go of the rest.
    pub(super) fn ask_stickers(&mut self, cx: &mut Cx<'_>) {
        let picker = &mut *self;
        let scroll = picker.scroll;
        let Some(tab) = &mut picker.stickers else {
            return;
        };
        let mut asked: Vec<String> = Vec::new();
        for section in &tab.grid.sections {
            if let Mark::Sticker(url) = &section.mark
                && !url.is_empty()
                && !asked.contains(url)
            {
                asked.push(url.clone());
            }
        }
        for (_, tile) in tab.grid.in_sight(scroll) {
            if let Item::Sticker { picture, .. } = &tile.item
                && !asked.contains(picture)
            {
                asked.push(picture.clone());
            }
        }
        if asked == tab.asked {
            return;
        }
        tab.asked.clone_from(&asked);
        cx.client.dispatch(Action::StickersShown(asked));
    }

    pub(super) fn stickers_search(&mut self, text: String, cx: &mut Cx<'_>) -> Task<Message> {
        cx.dispatch(Action::SearchStickers(text));
        self.scroll_grid(0.0)
    }

    fn sticker_item(&self, index: usize) -> Option<&Item> {
        Some(&self.stickers.as_ref()?.grid.tiles.get(index)?.item)
    }

    pub(super) fn sticker_pick(&self, index: usize, cx: &mut Cx<'_>) -> Task<Message> {
        let Some(Item::Sticker {
            id, usable: true, ..
        }) = self.sticker_item(index)
        else {
            return Task::none();
        };
        let id = id.clone();
        cx.dispatch(Action::PickSticker(id));
        self.ask_close(cx)
    }

    /// Enter sends the first sticker the search shows.
    pub(super) fn sticker_first(&self, cx: &mut Cx<'_>) -> Task<Message> {
        let first = Some(self).and_then(|picker| {
            picker
                .stickers
                .as_ref()?
                .grid
                .tiles
                .iter()
                .position(|tile| matches!(tile.item, Item::Sticker { usable: true, .. }))
        });
        match first {
            Some(first) => self.sticker_pick(first, cx),
            None => Task::none(),
        }
    }

    pub(super) fn sticker_star(&self, index: usize, cx: &mut Cx<'_>) {
        if let Some(Item::Sticker { id, .. }) = self.sticker_item(index) {
            let id = id.clone();
            cx.dispatch(Action::FavoriteSticker(id));
        }
    }
}

impl<'a> View<'a> {
    fn sticker_mark(self, mark: &Mark, side: f32) -> Element<'a, Message> {
        let size = side as u32;
        let state = self.cx.state;
        match mark {
            Mark::Clock => self
                .cx
                .look
                .glyph(Icon::Clock, size, self.cx.look.theme.secondary)
                .boxed(side)
                .into(),
            Mark::Icon(url) => match self.cx.picture(url, size, size, true) {
                Some(handle) => Pic::square(handle, side).into(),
                None => Space::new().width(side).height(side).into(),
            },
            Mark::Sticker(url) => {
                let shown = match state.stickers.pictures.get(url) {
                    Some(StickerPicture::Shown(picture)) => Some(picture),
                    _ => None,
                };
                self.cx.shot(
                    Shot::new(shown, Size::new(side, side))
                        .fit(Fit::Contain)
                        .bare(),
                )
            }
        }
    }

    pub(super) fn stickers_view(self, picker: &'a Picker) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(tab) = &picker.stickers else {
            return Space::new().into();
        };
        let state = self.cx.state;
        let field = kit::search(&picker.field, "Find the perfect sticker", theme);
        let rail = kit::rail(
            tab.grid
                .sections
                .iter()
                .map(|section| self.sticker_mark(&section.mark, 32.0)),
            tab.grid.current(picker.scroll),
            theme,
        );
        let hover = theme.hover;
        let rows = tab
            .grid
            .rows(picker.scroll, picker.built, |index, item, _| match item {
                Item::Heading(name) => {
                    let mark = tab
                        .headings
                        .get(&index)
                        .and_then(|section| tab.grid.sections.get(*section))
                        .map(|section| self.sticker_mark(&section.mark, 16.0));
                    container(kit::heading(mark, name, theme))
                        .padding(iced::Padding::ZERO.left(4))
                        .into()
                }
                Item::Sticker {
                    picture,
                    usable,
                    favorite,
                    ..
                } => {
                    let shown = match state.stickers.pictures.get(picture) {
                        Some(StickerPicture::Shown(picture)) => Some(picture),
                        _ => None,
                    };
                    let mut face = stack![
                        self.cx.shot(
                            Shot::new(shown, Size::new(TILE, TILE))
                                .fit(Fit::Contain)
                                .bare()
                        )
                    ];
                    if !usable {
                        // Greyed, as at 40%: the panel's ground over it.
                        let veil = crate::style::faded(theme.raised, 0.6);
                        face = face.push(
                            container(Space::new())
                                .width(TILE)
                                .height(TILE)
                                .style(move |_| crate::style::fill(veil)),
                        );
                        face = face.push(kit::pinned(
                            self.cx.look.glyph(Icon::Lock, 14, theme.text),
                            Alignment::End,
                            Alignment::End,
                            4.0,
                        ));
                    }
                    let tile = button(face)
                        .padding(0)
                        .width(TILE)
                        .height(TILE)
                        .on_press_maybe(usable.then_some(Message::Picker(Event::Pick(index))))
                        .style(lit_style(None, hover, 8.0));
                    let mut layers = stack![tile];
                    if picker.hover == Some(index) {
                        layers = layers.push(kit::pinned(
                            kit::star(
                                self.cx.look,
                                *favorite,
                                24.0,
                                16,
                                Message::Picker(Event::Star(index)),
                            ),
                            Alignment::End,
                            Alignment::Start,
                            4.0,
                        ));
                    }
                    mouse_area(layers)
                        .on_enter(Message::Picker(Event::Hover(index)))
                        .on_exit(Message::Picker(Event::Left(index)))
                        .into()
                }
            });
        let grid: Element<'a, Message> = if tab.grid.tiles.is_empty() {
            let words = state.stickers.error.clone().unwrap_or_else(|| {
                if state.stickers.query.trim().is_empty() {
                    String::new()
                } else {
                    "No stickers match your search.".into()
                }
            });
            kit::note(words, theme)
        } else {
            kit::grid(rows, theme)
        };
        let pointed = picker
            .hover
            .and_then(|index| match &tab.grid.tiles.get(index)?.item {
                Item::Sticker { name, .. } => Some(name.clone()),
                Item::Heading(_) => None,
            });
        // Else the collection on the rail under the pointer.
        let pointed = pointed.or_else(|| {
            let section = tab.grid.sections.get(picker.rail_hover?)?;
            Some(section.name.clone())
        });
        let foot = match pointed {
            Some(name) => kit::foot_words(name, true, theme),
            None => kit::foot_words("Click a sticker to send it.".into(), false, theme),
        };
        column![
            field,
            row![rail, grid].height(Length::Fill),
            kit::foot(foot, 46.0, theme)
        ]
        .height(Length::Fill)
        .into()
    }
}
