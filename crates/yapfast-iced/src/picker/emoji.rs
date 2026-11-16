//! The emoji tab, as the Slint client's `EmojiPicker` (`ui/messages.slint`)
//! and `src/emoji.rs` build it: the search field; the rail of sections down
//! its left; the grid, in 56 px buttons under each section's heading, of
//! the Discord account's Favorites, its Frequently Used (or for a reaction
//! the emoji it reacts with most), each server's custom emoji (those this
//! account may not use here greyed with a lock), then Unicode's categories;
//! the pointed emoji with its name at the foot. A search lists the matches
//! alone. Every emoji has a star that keeps it among the favourites, as
//! Discord's settings keep them.
//!
//! Only the tiles near the viewport are built, and only their pictures
//! made, each once, at the size it shows (36 px) from the Twemoji font's
//! PNGs; the tab holds them alone, and lets go of them with itself.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::OnceLock,
};

use iced::{
    Alignment, Element, Length, Rectangle, Size, Task,
    advanced::{graphics::text::cosmic_text, image::Handle},
    widget::{Space, button, column, mouse_area, row, stack},
};
use yapfast_core::{
    application::Action,
    application::Application,
    chat::{CustomEmoji, EmojiSet},
    emoji as names,
};

use super::{
    Event, Picker, Target, View,
    kit::{self, Grid, Scroll},
};
use crate::{
    app::Message,
    controls::lit_style,
    ctx::{Cx, Request},
    paint, pictures,
    pictures::Icon,
    widgets::Pic,
};

const CELL: f32 = 56.0;
const GLYPH: u32 = 36;
const HEADING: f32 = 28.0;
/// The grid's padding beside its cells.
const INSET: f32 = 8.0;

/// A category: its rail icon, heading and emoji.
struct Category {
    icon: &'static str,
    name: &'static str,
    emoji: Vec<&'static str>,
}

/// Every category with the emoji Twemoji draws as one picture, in Unicode's
/// order, up to Unicode 15.0, as the Slint client offers them; worked out
/// once, the first time a picker opens.
fn catalog(system: &mut cosmic_text::FontSystem) -> &'static [Category] {
    static CATALOG: OnceLock<Vec<Category>> = OnceLock::new();
    if let Some(catalog) = CATALOG.get() {
        return catalog;
    }
    let catalog = emojis::Group::iter()
        .map(|group| {
            let (icon, name) = names::category(group);
            let emoji = group
                .emojis()
                .filter(|emoji| {
                    emoji.unicode_version() <= names::NEWEST
                        && paint::has_emoji(system, emoji.as_str())
                })
                .map(emojis::Emoji::as_str)
                .collect();
            Category { icon, name, emoji }
        })
        .collect();
    CATALOG.get_or_init(|| catalog)
}

fn shared_catalog() -> &'static [Category] {
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .expect("font system");
    catalog(system.raw())
}

/// Whether the client draws this Unicode emoji: one of the catalog's, as
/// the Slint client offers only those Twemoji has a picture for.
pub fn drawn(key: &str) -> bool {
    static DRAWN: OnceLock<HashSet<&'static str>> = OnceLock::new();
    DRAWN
        .get_or_init(|| {
            shared_catalog()
                .iter()
                .flat_map(|category| category.emoji.iter().copied())
                .collect()
        })
        .contains(key)
}

/// An emoji the grid shows.
#[derive(Clone, Debug)]
pub enum Face {
    Unicode(&'static str),
    Custom(CustomEmoji),
}

impl Face {
    /// Its key: the emoji, or a custom one's `name:id`.
    fn key(&self) -> String {
        match self {
            Self::Unicode(key) => (*key).to_owned(),
            Self::Custom(emoji) => names::custom_key(emoji),
        }
    }
}

/// What a section's heading and rail button show.
pub enum Mark {
    Star,
    Clock,
    Face(Face),
    /// A server's icon, by its address.
    Icon(String),
}

pub enum Item {
    Heading(String),
    Cell {
        face: Face,
        usable: bool,
        favorite: bool,
    },
}

/// The tab's layout and the pictures it made.
#[derive(Default)]
pub struct Emoji {
    grid: Grid<Item, Mark>,
    /// The section each heading heads, by the heading's place.
    headings: HashMap<usize, usize>,
    pictures: RefCell<HashMap<(&'static str, u32), Handle>>,
    used: RefCell<HashSet<(&'static str, u32)>>,
}

impl Emoji {
    pub fn section_top(&self, index: usize) -> Option<f32> {
        self.grid.sections.get(index).map(|section| section.top)
    }

    /// The custom emoji in sight and the servers' icons on the rail.
    pub fn pictures(&self, scroll: Scroll) -> Vec<String> {
        let mut urls: Vec<String> = self
            .grid
            .sections
            .iter()
            .filter_map(|section| match &section.mark {
                Mark::Icon(url) => Some(url.clone()),
                Mark::Face(Face::Custom(emoji)) => Some(emoji.picture.clone()),
                _ => None,
            })
            .collect();
        for (_, tile) in self.grid.in_sight(scroll) {
            if let Item::Cell {
                face: Face::Custom(emoji),
                ..
            } = &tile.item
            {
                urls.push(emoji.picture.clone());
            }
        }
        urls
    }

    /// A Unicode emoji's picture at `size`, made once while it's in sight.
    fn picture(&self, key: &'static str, size: u32) -> Option<Handle> {
        self.used.borrow_mut().insert((key, size));
        if let Some(handle) = self.pictures.borrow().get(&(key, size)) {
            return Some(handle.clone());
        }
        let canvas = {
            let mut system = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            paint::emoji(system.raw(), key, size)?
        };
        let handle = pictures::handle(canvas);
        self.pictures
            .borrow_mut()
            .insert((key, size), handle.clone());
        Some(handle)
    }
}

/// What the grid is built from.
struct Lists {
    query: String,
    favorites: Vec<(String, bool)>,
    frequent: Vec<(String, bool)>,
    sets: Vec<EmojiSet>,
}

impl Lists {
    fn signature(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        self.query.hash(&mut hasher);
        self.favorites.hash(&mut hasher);
        self.frequent.hash(&mut hasher);
        for set in &self.sets {
            set.id.hash(&mut hasher);
            set.icon.hash(&mut hasher);
            for emoji in &set.emoji {
                (&emoji.id, emoji.usable).hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

impl Picker {
    /// The account's favourites and most used, and the servers' emoji, for
    /// the picker's target.
    fn emoji_lists(&self, app: &Application) -> Lists {
        let query = self.field.text().trim().trim_matches(':').to_lowercase();
        let sets = app.emoji_sets();
        let (favorites, frequent) = if query.is_empty() {
            let by_id: HashMap<&str, &CustomEmoji> = sets
                .iter()
                .flat_map(|set| &set.emoji)
                .map(|emoji| (emoji.id.as_str(), emoji))
                .collect();
            let lists = &app.state.discord_emojis;
            let used = match self.target {
                Target::React(..) => &lists.reactions,
                Target::Composer | Target::Edit | Target::Status(_) | Target::Poll(..) => {
                    &lists.frequent
                }
            };
            let mut frequent = names::discord_keys(used, &by_id);
            for key in app.frequent_reactions(16) {
                if !frequent.iter().any(|(known, _)| *known == key) {
                    frequent.push((key, true));
                }
            }
            (names::discord_keys(&lists.favorites, &by_id), frequent)
        } else {
            Default::default()
        };
        Lists {
            query,
            favorites,
            frequent,
            sets,
        }
    }

    /// Lays the grid out again when what it shows changed, as `emoji::rows`
    /// lays the Slint client's out.
    pub(super) fn build_emoji(&mut self, cx: &mut Cx<'_>) {
        let lists = self.emoji_lists(&cx.client.app);
        let signature = lists.signature();
        let picker = &mut *self;
        if picker.built == signature {
            return;
        }
        picker.built = signature;
        let width = picker.grid_width();
        let Some(tab) = &mut picker.emoji else {
            return;
        };
        let columns = (((width - 2.0 * INSET) / CELL).floor() as usize).max(1);
        let Lists {
            query,
            favorites,
            frequent,
            sets,
        } = lists;
        let starred: HashSet<&str> = favorites.iter().map(|(key, _)| key.as_str()).collect();
        let custom: HashMap<&str, &CustomEmoji> = sets
            .iter()
            .flat_map(|set| &set.emoji)
            .map(|emoji| (emoji.id.as_str(), emoji))
            .collect();
        // A key of the account's lists as the grid shows it.
        let face = |key: &str| match key.rsplit_once(':') {
            Some((_, id)) => custom.get(id).map(|emoji| Face::Custom((*emoji).clone())),
            None => emojis::get(key).map(|emoji| Face::Unicode(emoji.as_str())),
        };
        let mut grid = Grid::default();
        let mut headings = HashMap::new();
        let mut top = 0.0;
        let mut section = |grid: &mut Grid<Item, Mark>,
                           heading: Option<(String, Mark)>,
                           cells: Vec<(Face, bool)>| {
            if cells.is_empty() {
                return;
            }
            let start = top;
            if let Some((name, mark)) = heading {
                headings.insert(grid.tiles.len(), grid.sections.len());
                grid.sections.push(kit::Section {
                    top: start,
                    end: start,
                    mark,
                });
                grid.place(
                    Rectangle::new(iced::Point::new(0.0, top), Size::new(width, HEADING)),
                    Item::Heading(name.to_uppercase()),
                );
                top += HEADING;
            }
            for (index, (face, usable)) in cells.into_iter().enumerate() {
                let (column, line) = (index % columns, index / columns);
                let favorite = starred.contains(face.key().as_str());
                grid.place(
                    Rectangle::new(
                        iced::Point::new(INSET + column as f32 * CELL, top + line as f32 * CELL),
                        Size::new(CELL, CELL),
                    ),
                    Item::Cell {
                        face,
                        usable,
                        favorite,
                    },
                );
            }
            top = grid.height;
            if let Some(last) = grid.sections.last_mut()
                && last.top == start
            {
                last.end = top;
            }
        };
        let catalog = shared_catalog();
        if !query.is_empty() {
            let mut cells: Vec<(Face, bool)> = sets
                .iter()
                .flat_map(|set| &set.emoji)
                .filter(|emoji| emoji.text.to_lowercase().contains(&query))
                .map(|emoji| (Face::Custom(emoji.clone()), emoji.usable))
                .collect();
            cells.extend(
                catalog
                    .iter()
                    .flat_map(|category| category.emoji.iter().copied())
                    .filter(|key| {
                        emojis::get(key).is_some_and(|emoji| names::matches(emoji, &query))
                    })
                    .map(|key| (Face::Unicode(key), true)),
            );
            section(&mut grid, None, cells);
        } else {
            let listed = |list: &[(String, bool)]| -> Vec<(Face, bool)> {
                list.iter()
                    .filter_map(|(key, usable)| Some((face(key)?, *usable)))
                    .collect()
            };
            section(
                &mut grid,
                Some(("Favorites".into(), Mark::Star)),
                listed(&favorites),
            );
            section(
                &mut grid,
                Some(("Frequently Used".into(), Mark::Clock)),
                listed(&frequent),
            );
            for set in &sets {
                let Some(first) = set.emoji.first() else {
                    continue;
                };
                let mark = match &set.icon {
                    Some(url) => Mark::Icon(url.clone()),
                    None => Mark::Face(Face::Custom(first.clone())),
                };
                section(
                    &mut grid,
                    Some((set.server.clone(), mark)),
                    set.emoji
                        .iter()
                        .map(|emoji| (Face::Custom(emoji.clone()), emoji.usable))
                        .collect(),
                );
            }
            for category in catalog {
                section(
                    &mut grid,
                    Some((
                        category.name.into(),
                        Mark::Face(Face::Unicode(category.icon)),
                    )),
                    category
                        .emoji
                        .iter()
                        .map(|key| (Face::Unicode(key), true))
                        .collect(),
                );
            }
        }
        tab.grid = grid;
        tab.headings = headings;
        picker.hover = None;
    }

    pub(super) fn emoji_search(&mut self, _text: String, cx: &mut Cx<'_>) -> Task<Message> {
        self.build_emoji(cx);
        {
            cx.request(Request::Ask);
            self.scroll_grid(0.0)
        }
    }

    fn emoji_cell(&self, index: usize) -> Option<(&Face, bool)> {
        let tab = self.emoji.as_ref()?;
        match &tab.grid.tiles.get(index)?.item {
            Item::Cell { face, usable, .. } => Some((face, *usable)),
            Item::Heading(_) => None,
        }
    }

    pub(super) fn emoji_pick(&mut self, index: usize, cx: &mut Cx<'_>) -> Task<Message> {
        match self.emoji_cell(index) {
            Some((face, true)) => {
                let key = face.key();
                self.place_emoji(key, cx)
            }
            _ => Task::none(),
        }
    }

    /// Enter picks the first match.
    pub(super) fn emoji_first(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        let searching = !self.field.text().trim().is_empty();
        let first = Some(&*self).and_then(|picker| {
            picker
                .emoji
                .as_ref()?
                .grid
                .tiles
                .iter()
                .position(|tile| matches!(tile.item, Item::Cell { usable: true, .. }))
        });
        match first {
            Some(first) if searching => self.emoji_pick(first, cx),
            _ => Task::none(),
        }
    }

    /// The star keeps the emoji among the Discord account's favourites, or
    /// lets it go.
    pub(super) fn emoji_star(&mut self, index: usize, cx: &mut Cx<'_>) {
        let Some(name) = self
            .emoji_cell(index)
            .and_then(|(face, _)| names::discord_name(&face.key()))
        else {
            return;
        };
        cx.dispatch(Action::FavoriteEmoji(name));
    }
}

impl<'a> View<'a> {
    /// An emoji as the grid, rail or foot draws it, `size` across.
    fn face_view(
        &self,
        tab: &Emoji,
        face: &Face,
        side: f32,
        size: u32,
        opacity: f32,
    ) -> Element<'a, Message> {
        let handle = match face {
            Face::Unicode(key) => tab.picture(key, size),
            Face::Custom(emoji) => self.cx.picture(&emoji.picture, size, size, false),
        };
        match handle {
            Some(handle) => Pic::square(handle, size as f32)
                .opacity(opacity)
                .boxed(side)
                .into(),
            None => Space::new().width(side).height(side).into(),
        }
    }

    fn section_mark(self, tab: &Emoji, mark: &Mark, side: f32, size: u32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        match mark {
            Mark::Star => self
                .cx
                .look
                .glyph(
                    Icon::StarFilled,
                    size,
                    crate::style::hex(crate::style::CROWN),
                )
                .boxed(side)
                .into(),
            Mark::Clock => self
                .cx
                .look
                .glyph(Icon::Clock, size, theme.secondary)
                .boxed(side)
                .into(),
            Mark::Face(face) => self.face_view(tab, face, side, size, 1.0),
            Mark::Icon(url) => match self.cx.picture(url, size, size, true) {
                Some(handle) => Pic::square(handle, size as f32).boxed(side).into(),
                None => Space::new().width(side).height(side).into(),
            },
        }
    }

    pub(super) fn emoji_view(self, picker: &'a Picker) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(tab) = &picker.emoji else {
            return Space::new().into();
        };
        tab.used.borrow_mut().clear();
        let field = kit::search(&picker.field, "Find an emoji", theme);
        let searching = !picker.field.text().trim().is_empty();
        let rail = kit::rail(
            tab.grid
                .sections
                .iter()
                .map(|section| self.section_mark(tab, &section.mark, 36.0, 32)),
            (!searching)
                .then(|| tab.grid.current(picker.scroll))
                .flatten(),
            theme,
        );
        let wash = theme.wash(0.12);
        let rows = tab
            .grid
            .rows(picker.scroll, picker.built, |index, item, _| match item {
                Item::Heading(name) => {
                    let mark = tab
                        .headings
                        .get(&index)
                        .and_then(|section| tab.grid.sections.get(*section))
                        .map(|section| self.section_mark(tab, &section.mark, 16.0, 16));
                    kit::heading(mark, name, theme)
                }
                Item::Cell {
                    face,
                    usable,
                    favorite,
                } => {
                    let fade = if *usable { 1.0 } else { 0.4 };
                    let mut face_shown = stack![self.face_view(tab, face, CELL, GLYPH, fade)];
                    if !usable {
                        face_shown = face_shown.push(kit::pinned(
                            self.cx.look.glyph(Icon::Lock, 14, theme.text),
                            Alignment::End,
                            Alignment::End,
                            4.0,
                        ));
                    }
                    let cell = button(face_shown)
                        .padding(0)
                        .width(CELL)
                        .height(CELL)
                        .on_press_maybe(usable.then_some(Message::Picker(Event::Pick(index))))
                        .style(lit_style(None, wash, 6.0));
                    let mut layers = stack![cell];
                    if *favorite || picker.hover == Some(index) {
                        layers = layers.push(kit::pinned(
                            kit::star(
                                self.cx.look,
                                *favorite,
                                18.0,
                                12,
                                Message::Picker(Event::Star(index)),
                            ),
                            Alignment::End,
                            Alignment::Start,
                            0.0,
                        ));
                    }
                    mouse_area(layers)
                        .on_enter(Message::Picker(Event::Hover(index)))
                        .on_exit(Message::Picker(Event::Left(index)))
                        .into()
                }
            });
        let grid: Element<'a, Message> = if tab.grid.tiles.is_empty() {
            kit::note(
                format!("No emoji match \"{}\".", picker.field.text().trim()),
                theme,
            )
        } else {
            kit::grid(rows, theme)
        };
        // The pointed emoji, larger, with its name.
        let pointed = picker
            .hover
            .and_then(|index| match &tab.grid.tiles.get(index)?.item {
                Item::Cell { face, .. } => Some(face),
                Item::Heading(_) => None,
            });
        let foot: Element<'a, Message> = match pointed {
            Some(face) => row![
                self.face_view(tab, face, 30.0, 26, 1.0),
                kit::foot_words(names::title(&face.key()), true, theme)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            None => kit::foot_words(
                match picker.target {
                    Target::Composer | Target::Edit => "Pick an emoji for your message.",
                    Target::React(..) => "Pick an emoji to react with.",
                    Target::Status(_) => "Pick an emoji for your status.",
                    Target::Poll(..) => "Pick an emoji for your message.",
                }
                .into(),
                false,
                theme,
            ),
        };
        let view = column![
            field,
            row![rail, grid].height(Length::Fill),
            kit::foot(foot, 48.0, theme)
        ]
        .height(Length::Fill);
        // Pictures of tiles no longer built go.
        let used = tab.used.borrow();
        tab.pictures
            .borrow_mut()
            .retain(|key, _| used.contains(key));
        view.into()
    }
}
