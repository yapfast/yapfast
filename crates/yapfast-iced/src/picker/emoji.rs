//! The emoji tab: the search field; the rail of sections down its left; the
//! grid, in 56 px buttons under each section's heading, of the Discord
//! account's Favorites, its Frequently Used (or for a reaction the emoji it
//! reacts with most), each server's custom emoji (those this account may
//! not use here greyed with a lock), then Unicode's categories; the pointed
//! emoji with its name at the foot. A search lists the matches alone. Every
//! emoji has a star that keeps it among the favourites, as Discord's
//! settings keep them.
//!
//! Only the tiles near the viewport are built, and only their pictures
//! made, each once, at the size it shows (36 px) from the Twemoji font's
//! PNGs; the tab holds them alone, and lets go of them with itself. The
//! custom emoji's pictures are the core's: those within three screens of
//! what shows are asked for, those in sight first, and the rest let go of
//! as the grid scrolls on, so a large account's emoji cost a few screens
//! of memory however far it goes; leaving the tab lets go of them all.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::OnceLock,
};

use iced::{
    Alignment, Element, Length, Padding, Rectangle, Size, Task,
    advanced::{graphics::text::cosmic_text, image::Handle},
    widget::{Space, button, column, container, mouse_area, row, stack, text},
};
use yapfast_core::{
    application::Action,
    application::Application,
    chat::{ChatServer, CustomEmoji, EmojiSet, SettingsSection},
    emoji as names,
    i18n::{self, tr},
};

use super::{
    Event, Picker, Target, View,
    kit::{self, Grid},
};
use crate::{
    app::Message,
    controls::lit_style,
    ctx::{Cx, Request},
    fonts::SEMIBOLD,
    paint, pictures,
    pictures::Icon,
    widgets::Pic,
};

const CELL: f32 = 48.0;
const GLYPH: u32 = 32;
const HEADING: f32 = 28.0;
/// The grid's padding beside its cells.
const INSET: f32 = 8.0;
/// How many screens either side of what shows the custom emoji's pictures
/// are asked for ahead and kept: a scroll finds them come, and a row
/// further off lets go of its own.
const REACH: f32 = 3.0;

const TONES: [(emojis::SkinTone, &str, &str); 6] = [
    (emojis::SkinTone::Default, "Default", "👏"),
    (emojis::SkinTone::Light, "Light", "👏🏻"),
    (emojis::SkinTone::MediumLight, "Medium light", "👏🏼"),
    (emojis::SkinTone::Medium, "Medium", "👏🏽"),
    (emojis::SkinTone::MediumDark, "Medium dark", "👏🏾"),
    (emojis::SkinTone::Dark, "Dark", "👏🏿"),
];

type Variants = HashMap<&'static str, &'static str>;

/// Build a tone's supported replacements on first use. A base emoji without
/// a Twemoji picture for the chosen tone keeps its default picture.
fn variants(tone: usize, catalog: &[Category]) -> Option<&'static Variants> {
    static VARIANTS: [OnceLock<Variants>; TONES.len()] = [const { OnceLock::new() }; TONES.len()];
    let (tone, _, _) = *TONES.get(tone).filter(|_| tone > 0)?;
    Some(VARIANTS[tone as usize].get_or_init(|| {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        catalog
            .iter()
            .flat_map(|category| category.emoji.iter().copied())
            .filter_map(|key| {
                let variant = emojis::get(key)?.with_skin_tone(tone)?;
                (variant.unicode_version() <= names::NEWEST
                    && paint::has_emoji(system.raw(), variant.as_str()))
                .then_some((key, variant.as_str()))
            })
            .collect()
    }))
}

fn upload_servers(app: &Application) -> impl Iterator<Item = &ChatServer> {
    app.state
        .servers
        .iter()
        .filter(|server| server.discord && !server.unavailable)
        .filter(|server| {
            let rules = app.expression_rules(&server.id);
            rules.create || rules.manage
        })
        .map(AsRef::as_ref)
}

/// A category: its rail icon, heading and emoji.
struct Category {
    icon: &'static str,
    name: &'static str,
    emoji: Vec<&'static str>,
}

/// Every category with the emoji Twemoji draws as one picture, in Unicode's
/// order, up to Unicode 15.0; worked out once, the first time a picker
/// opens.
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

/// Whether the client draws this Unicode emoji: one of the catalog's,
/// which has only those Twemoji has a picture for.
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
    /// The emoji and the index of its server in `Emoji::servers`.
    Custom(CustomEmoji, usize),
}

impl Face {
    /// Its key: the emoji, or a custom one's `name:id`.
    fn key(&self) -> String {
        match self {
            Self::Unicode(key) => (*key).to_owned(),
            Self::Custom(emoji, _) => names::custom_key(emoji),
        }
    }
}

/// What a section's heading and rail button show.
pub enum Mark {
    Star,
    Clock,
    Popular,
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
    servers: Vec<(String, Option<String>)>,
    choices: Option<Choices>,
    /// The section each heading heads, by the heading's place.
    headings: HashMap<usize, usize>,
    /// The sections folded shut, by name.
    folded: HashSet<String>,
    pictures: RefCell<HashMap<(&'static str, u32), Handle>>,
    used: RefCell<HashSet<(&'static str, u32)>>,
    /// The animated custom emoji playing in the grid's cells, which go
    /// with the tab, before the heap is given back.
    players: crate::animated::Players,
    /// The layout and the tiles, from and to, whose custom emoji's
    /// pictures were last asked for and kept.
    reached: Option<(u64, usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choices {
    Tones,
    UploadServer,
}

impl Emoji {
    /// The grid's players.
    #[cfg(test)]
    pub fn players(&self) -> &crate::animated::Players {
        &self.players
    }

    /// What the folded sections add to the layout's signature.
    fn fold_signature(&self) -> u64 {
        self.folded
            .iter()
            .map(|name| {
                let mut hasher = std::hash::DefaultHasher::new();
                name.hash(&mut hasher);
                hasher.finish()
            })
            .fold(0, |all, one| all ^ one)
    }

    pub fn section_top(&self, index: usize) -> Option<f32> {
        self.grid.sections.get(index).map(|section| section.top)
    }

    /// The servers' icons and first emoji the rail shows, all at once.
    pub fn pictures(&self) -> Vec<String> {
        self.servers
            .iter()
            .filter_map(|(_, icon)| icon.clone())
            .chain(
                self.grid
                    .sections
                    .iter()
                    .filter_map(|section| match &section.mark {
                        Mark::Face(Face::Custom(emoji, _)) => Some(emoji.picture.clone()),
                        _ => None,
                    }),
            )
            .collect()
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
    popular: Option<(usize, Vec<String>)>,
    sets: Vec<EmojiSet>,
}

impl Lists {
    fn signature(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        self.query.hash(&mut hasher);
        self.favorites.hash(&mut hasher);
        self.frequent.hash(&mut hasher);
        self.popular.hash(&mut hasher);
        for set in &self.sets {
            set.id.hash(&mut hasher);
            set.server.hash(&mut hasher);
            set.icon.hash(&mut hasher);
            for emoji in &set.emoji {
                (&emoji.id, emoji.usable).hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

impl Picker {
    /// The name its foot shows at rest: the first emoji's, as Discord's.
    #[cfg(test)]
    pub(crate) fn resting_name(&self) -> Option<String> {
        let tab = self.emoji.as_ref()?;
        tab.grid.tiles.iter().find_map(|tile| match &tile.item {
            Item::Cell { face, .. } => Some(names::title(&face.key())),
            Item::Heading(_) => None,
        })
    }

    /// The account's favourites and most used, and the servers' emoji, for
    /// the picker's target.
    fn emoji_lists(&self, app: &Application) -> Lists {
        let query = self.field.text().trim().trim_matches(':').to_lowercase();
        let sets = app.emoji_sets();
        let popular = query
            .is_empty()
            .then(|| {
                let server = app.state.emoji_server_context()?;
                let ranked = app
                    .state
                    .popular_emojis
                    .get(server)
                    .filter(|ids| !ids.is_empty())?;
                let guild = yapfast_core::discord::parse_server(server)?;
                let index = sets.iter().position(|set| set.id == guild)?;
                Some((index, ranked.clone()))
            })
            .flatten();
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
            popular,
            sets,
        }
    }

    /// Lays the grid out again when what it shows changed.
    pub(super) fn build_emoji(&mut self, cx: &Cx<'_>) {
        let lists = self.emoji_lists(&cx.client.app);
        let tone = cx.layout.emoji_skin_tone;
        let signature = lists.signature()
            ^ self
                .emoji
                .as_ref()
                .map_or(0, Emoji::fold_signature)
                .rotate_left(17)
            ^ (tone as u64).rotate_left(27);
        let picker = &mut *self;
        if picker.built == signature {
            return;
        }
        picker.built = signature;
        let width = picker.grid_width();
        let Some(tab) = &mut picker.emoji else {
            return;
        };
        let folded = tab.folded.clone();
        let columns = (((width - 2.0 * INSET) / CELL).floor() as usize).max(1);
        let Lists {
            query,
            favorites,
            frequent,
            popular,
            sets,
        } = lists;
        let catalog = shared_catalog();
        let variants = variants(tone, catalog);
        let preferred = |key: &'static str| {
            variants
                .and_then(|map| map.get(key).copied())
                .unwrap_or(key)
        };
        let starred: HashSet<&str> = favorites
            .iter()
            .map(|(key, _)| {
                variants
                    .and_then(|map| map.get(key.as_str()).copied())
                    .unwrap_or(key)
            })
            .collect();
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
            let shut = heading
                .as_ref()
                .is_some_and(|(name, _)| folded.contains(name));
            if let Some((name, mark)) = heading {
                headings.insert(grid.tiles.len(), grid.sections.len());
                let heading = name.clone();
                grid.sections.push(kit::Section {
                    top: start,
                    end: start,
                    mark,
                    name,
                });
                grid.place(
                    Rectangle::new(iced::Point::new(0.0, top), Size::new(width, HEADING)),
                    Item::Heading(heading),
                );
                top += HEADING;
            }
            for (index, (face, usable)) in cells.into_iter().enumerate() {
                if shut {
                    break;
                }
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
        if !query.is_empty() {
            let mut cells: Vec<(Face, bool)> = sets
                .iter()
                .enumerate()
                .flat_map(|(server, set)| set.emoji.iter().map(move |emoji| (emoji, server)))
                .filter(|(emoji, _)| emoji.text.to_lowercase().contains(&query))
                .map(|(emoji, server)| (Face::Custom(emoji.clone(), server), emoji.usable))
                .collect();
            cells.extend(
                catalog
                    .iter()
                    .flat_map(|category| category.emoji.iter().copied())
                    .filter(|key| {
                        emojis::get(key).is_some_and(|emoji| names::matches(emoji, &query))
                    })
                    .map(|key| (Face::Unicode(preferred(key)), true)),
            );
            section(&mut grid, None, cells);
        } else {
            let custom: HashMap<_, _> = sets
                .iter()
                .enumerate()
                .flat_map(|(server, set)| {
                    set.emoji
                        .iter()
                        .map(move |emoji| (emoji.id.as_str(), (emoji, server)))
                })
                .collect();
            // A key of the account's lists as the grid shows it.
            let face = |key: &str| match key.rsplit_once(':') {
                Some((_, id)) => custom
                    .get(id)
                    .map(|&(emoji, server)| Face::Custom(emoji.clone(), server)),
                None => emojis::get(key).map(|emoji| Face::Unicode(preferred(emoji.as_str()))),
            };
            let listed = |list: &[(String, bool)]| -> Vec<(Face, bool)> {
                list.iter()
                    .filter_map(|(key, usable)| Some((face(key)?, *usable)))
                    .collect()
            };
            if let Some((server, ranked)) = popular {
                let set = &sets[server];
                let cells = ranked
                    .iter()
                    .filter_map(|id| custom.get(id.as_str()).copied())
                    .filter(|(_, source)| *source == server)
                    .map(|(emoji, source)| (Face::Custom(emoji.clone(), source), emoji.usable))
                    .collect();
                section(
                    &mut grid,
                    Some((format!("Popular in {}", set.server), Mark::Popular)),
                    cells,
                );
            }
            section(
                &mut grid,
                Some((tr("Favorites").into(), Mark::Star)),
                listed(&favorites),
            );
            section(
                &mut grid,
                Some((tr("Frequently Used").into(), Mark::Clock)),
                listed(&frequent),
            );
            for (server, set) in sets.iter().enumerate() {
                let Some(first) = set.emoji.first() else {
                    continue;
                };
                let mark = match &set.icon {
                    Some(url) => Mark::Icon(url.clone()),
                    None => Mark::Face(Face::Custom(first.clone(), server)),
                };
                section(
                    &mut grid,
                    Some((set.server.clone(), mark)),
                    set.emoji
                        .iter()
                        .map(|emoji| (Face::Custom(emoji.clone(), server), emoji.usable))
                        .collect(),
                );
            }
            for category in catalog {
                section(
                    &mut grid,
                    Some((
                        tr(category.name).into(),
                        Mark::Face(Face::Unicode(category.icon)),
                    )),
                    category
                        .emoji
                        .iter()
                        .map(|key| (Face::Unicode(preferred(key)), true))
                        .collect(),
                );
            }
        }
        tab.grid = grid;
        tab.servers = sets.into_iter().map(|set| (set.server, set.icon)).collect();
        tab.headings = headings;
        picker.hover = None;
    }

    pub(super) fn emoji_search(&mut self, _text: String, cx: &mut Cx<'_>) -> Task<Message> {
        self.build_emoji(cx);
        let task = self.scroll_grid(0.0);
        self.emoji_reach(cx);
        task
    }

    pub(super) fn emoji_toggle_tones(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        self.emoji_choices(Some(Choices::Tones), cx)
    }

    pub(super) fn emoji_tone(&mut self, tone: usize, cx: &mut Cx<'_>) -> Task<Message> {
        if tone < TONES.len()
            && self
                .emoji
                .as_ref()
                .is_some_and(|tab| tab.choices == Some(Choices::Tones))
        {
            cx.layout.emoji_skin_tone = tone;
            cx.keep("emoji_skin_tone", tone);
            self.build_emoji(cx);
            self.emoji_reach(cx);
            return self.emoji_choices(None, cx);
        }
        Task::none()
    }

    pub(super) fn emoji_add(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        if self.emoji.is_none() || self.target != Target::Composer {
            return Task::none();
        }
        let (id, multiple) = {
            let mut servers = upload_servers(&cx.client.app);
            let Some(server) = servers.next() else {
                return Task::none();
            };
            (server.id.clone(), servers.next().is_some())
        };
        if multiple {
            self.emoji_choices(Some(Choices::UploadServer), cx)
        } else {
            self.emoji_open_server(id, cx)
        }
    }

    pub(super) fn emoji_open_server(&self, server: String, cx: &mut Cx<'_>) -> Task<Message> {
        if self.emoji.is_some()
            && self.target == Target::Composer
            && upload_servers(&cx.client.app).any(|choice| choice.id == server)
        {
            cx.dispatch(Action::OpenServerEmojiSettings(server.clone()));
            if cx
                .state()
                .server_settings
                .as_ref()
                .is_some_and(|page| page.server == server && page.section == SettingsSection::Emoji)
            {
                return self.ask_close(cx);
            }
        }
        Task::none()
    }

    pub(super) fn emoji_escape(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        if self.emoji.as_ref().is_some_and(|tab| tab.choices.is_some()) {
            self.emoji_choices(None, cx)
        } else {
            self.ask_close(cx)
        }
    }

    fn emoji_choices(&mut self, choices: Option<Choices>, cx: &mut Cx<'_>) -> Task<Message> {
        if let Some(tab) = &mut self.emoji {
            tab.choices = if tab.choices == choices {
                None
            } else {
                choices
            };
            cx.request(Request::Focus(crate::ctx::Focus::Id(kit::FIELD)));
            cx.touch();
        }
        Task::none()
    }

    /// The custom emoji's pictures within [`REACH`] screens of what the
    /// grid shows: those missing are asked for, the nearest first and
    /// those in sight before all, ahead of what was asked for before; the
    /// others the picker had go, with those the rail shows kept. Only when
    /// the tiles within reach changed, so a scroll within a row asks
    /// nothing.
    pub(super) fn emoji_reach(&mut self, cx: &mut Cx<'_>) {
        let (scroll, built) = (self.scroll, self.built);
        let Some(tab) = &mut self.emoji else {
            return;
        };
        let (top, bottom) = (scroll.top, scroll.top + scroll.height);
        let reach = REACH * scroll.height;
        // The tiles go down the grid in their order.
        let tiles = &tab.grid.tiles;
        let from = tiles.partition_point(|tile| tile.at.y + tile.at.height < top - reach);
        let to = tiles.partition_point(|tile| tile.at.y <= bottom + reach);
        if tab.reached == Some((built, from, to)) {
            return;
        }
        tab.reached = Some((built, from, to));
        let mut near: Vec<(f32, &str)> = tiles[from..to.max(from)]
            .iter()
            .filter_map(|tile| match &tile.item {
                Item::Cell {
                    face: Face::Custom(emoji, _),
                    ..
                } => {
                    let away = (top - tile.at.y - tile.at.height)
                        .max(tile.at.y - bottom)
                        .max(0.0);
                    Some((away, emoji.picture.as_str()))
                }
                _ => None,
            })
            .collect();
        // Stable, so those in sight keep the grid's order.
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let pictures = &cx.state().pictures;
        let missing: Vec<String> = near
            .iter()
            .filter(|(_, url)| !url.is_empty() && !pictures.contains_key(*url))
            .map(|(_, url)| (*url).to_owned())
            .collect();
        let mut keep: Vec<String> = near.into_iter().map(|(_, url)| url.to_owned()).collect();
        keep.extend(tab.pictures());
        cx.request(Request::KeepEmoji {
            near: keep,
            first: missing,
        });
    }

    /// Folds a section shut or opens it again, and lays the grid out anew.
    pub(super) fn emoji_fold(&mut self, section: usize, cx: &mut Cx<'_>) {
        let Some(tab) = &mut self.emoji else {
            return;
        };
        let Some(name) = tab.grid.sections.get(section).map(|s| s.name.clone()) else {
            return;
        };
        if !tab.folded.remove(&name) {
            tab.folded.insert(name);
        }
        self.build_emoji(cx);
        self.emoji_reach(cx);
    }

    fn emoji_cell(&self, index: usize) -> Option<(&Face, bool)> {
        let tab = self.emoji.as_ref()?;
        match &tab.grid.tiles.get(index)?.item {
            Item::Cell { face, usable, .. } => Some((face, *usable)),
            Item::Heading(_) => None,
        }
    }

    pub(super) fn emoji_pick(&self, index: usize, cx: &mut Cx<'_>) -> Task<Message> {
        match self.emoji_cell(index) {
            Some((face, true)) => {
                let key = face.key();
                self.place_emoji(key, cx)
            }
            _ => Task::none(),
        }
    }

    /// Enter picks the first match.
    pub(super) fn emoji_first(&self, cx: &mut Cx<'_>) -> Task<Message> {
        let searching = !self.field.text().trim().is_empty();
        let first = Some(self).and_then(|picker| {
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
    pub(super) fn emoji_star(&self, index: usize, cx: &mut Cx<'_>) {
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
            Face::Custom(emoji, _) => self.cx.picture(&emoji.picture, size, size, false),
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
            Mark::Popular => self
                .cx
                .look
                .glyph(Icon::TrendingUp, size, theme.secondary)
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
        tab.players.begin();
        let wash = theme.wash(0.12);
        let selected_tone = self.cx.layout.emoji_skin_tone;
        let (_, _, hand) = TONES.get(selected_tone).unwrap_or(&TONES[0]);
        let skin = button(
            row![
                self.face_view(tab, &Face::Unicode(hand), 22.0, 18, 1.0),
                text("Skin tone").size(12),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([0, 8])
        .height(32)
        .on_press(Message::Picker(Event::ToggleSkinTones))
        .style(lit_style(Some(theme.active), wash, 6.0));
        let mut fields = row![
            container(kit::search_field(
                &picker.field,
                tr("Find the perfect emoji"),
                self.cx.look
            ))
            .width(Length::Fill),
            skin,
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if picker.target == Target::Composer && upload_servers(self.cx.app).next().is_some() {
            fields = fields.push(
                button(text(tr("Add Emoji")).size(12).font(SEMIBOLD))
                    .padding([0, 8])
                    .height(32)
                    .on_press(Message::Picker(Event::AddEmoji))
                    .style(lit_style(Some(theme.active), wash, 6.0)),
            );
        }
        let field = container(fields)
            .padding(Padding::from(10).bottom(6))
            .width(Length::Fill);
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
        let rows = tab
            .grid
            .rows(picker.scroll, picker.built, |index, item, _| match item {
                Item::Heading(name) => {
                    let section = tab.headings.get(&index).copied();
                    let info = section.and_then(|section| tab.grid.sections.get(section));
                    let mark = info.map(|info| self.section_mark(tab, &info.mark, 16.0, 16));
                    let folded = tab.folded.contains(name);
                    kit::fold_heading(
                        mark,
                        name,
                        folded,
                        Message::Picker(Event::Fold(section.unwrap_or(0))),
                        self.cx.look,
                    )
                }
                Item::Cell {
                    face,
                    usable,
                    favorite,
                } => {
                    let fade = if *usable { 1.0 } else { 0.4 };
                    // An animated custom emoji plays in its cell, greyed
                    // or not; the rail and the foot show its still.
                    let playing = match face {
                        Face::Custom(emoji, _) => {
                            self.cx.playing_emoji(&emoji.picture, GLYPH, &tab.players)
                        }
                        Face::Unicode(_) => None,
                    };
                    let shown = match playing {
                        Some(playing) => container(playing.opacity(fade)).center(CELL).into(),
                        None => self.face_view(tab, face, CELL, GLYPH, fade),
                    };
                    let mut face_shown = stack![shown];
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
                i18n::fill(tr("No emoji match \"{}\"."), &[&picker.field.text().trim()]),
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
        // Else the rail's section under the pointer, by its mark and name.
        let section = picker
            .rail_hover
            .and_then(|index| tab.grid.sections.get(index));
        // Else the first emoji, as Discord's foot shows one at rest.
        let first = tab.grid.tiles.iter().find_map(|tile| match &tile.item {
            Item::Cell { face, .. } => Some(face),
            Item::Heading(_) => None,
        });
        let named = |face: &'a Face| -> Element<'a, Message> {
            let mut words = column![kit::foot_words(names::title(&face.key()), true, theme)];
            let mut icon = None;
            if let Face::Custom(_, server) = face
                && let Some((name, url)) = tab.servers.get(*server)
            {
                words = words.push(
                    text(i18n::fill(tr("from {}"), &[name]))
                        .size(12)
                        .color(theme.muted)
                        .wrapping(text::Wrapping::None),
                );
                icon = url
                    .as_deref()
                    .and_then(|url| self.cx.picture(url, 22, 22, true));
            }
            let mut details = row![
                self.face_view(tab, face, 30.0, 26, 1.0),
                words.width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            if let Some(handle) = icon {
                details = details.push(Pic::square(handle, 22.0));
            }
            details.width(Length::Fill).into()
        };
        let shown: Element<'a, Message> = match (pointed, section) {
            (Some(face), _) => named(face),
            (None, Some(section)) => row![
                self.section_mark(tab, &section.mark, 30.0, 26),
                kit::foot_words(section.name.clone(), true, theme)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            (None, None) => match first {
                Some(face) => named(face),
                None => kit::foot_words(
                    match picker.target {
                        Target::Composer | Target::Edit => tr("Pick an emoji for your message."),
                        Target::React(..) => tr("Pick an emoji to react with."),
                        Target::Status(_) => tr("Pick an emoji for your status."),
                        Target::Poll(..) => tr("Pick an emoji for your message."),
                    }
                    .into(),
                    false,
                    theme,
                ),
            },
        };
        let foot: Element<'a, Message> = row![
            self.cx
                .look
                .glyph(Icon::Smile, 22, theme.secondary)
                .boxed(32.0),
            shown
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into();
        let mut view = column![field];
        if let Some(choices) = tab.choices {
            view = view.push(self.emoji_choice_view(tab, choices));
        }
        let view = view
            .push(row![rail, grid].height(Length::Fill))
            .push(kit::foot(foot, 48.0, theme));
        // Pictures of tiles no longer built go, and their players.
        tab.players.end();
        let used = tab.used.borrow();
        tab.pictures
            .borrow_mut()
            .retain(|key, _| used.contains(key));
        view.height(Length::Fill).into()
    }

    fn emoji_choice_view(self, tab: &Emoji, choices: Choices) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let wash = theme.wash(0.12);
        let content: Element<'a, Message> = match choices {
            Choices::Tones => row(TONES.iter().enumerate().map(|(index, &(_, label, hand))| {
                let selected = self.cx.layout.emoji_skin_tone == index;
                button(
                    column![
                        self.face_view(tab, &Face::Unicode(hand), 24.0, 22, 1.0),
                        text(label).size(10),
                    ]
                    .spacing(2)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([4, 1])
                .on_press(Message::Picker(Event::SkinTone(index)))
                .style(lit_style(selected.then_some(theme.active), wash, 6.0))
                .into()
            }))
            .width(Length::Fill)
            .spacing(4)
            .into(),
            Choices::UploadServer => {
                let servers = column(upload_servers(self.cx.app).map(|server| {
                    button(text(server.name.as_str()).size(14))
                        .width(Length::Fill)
                        .padding([5, 8])
                        .on_press(Message::Picker(Event::EmojiServer(server.id.clone())))
                        .style(lit_style(None, wash, 6.0))
                        .into()
                }));
                column![
                    text("Choose a server to upload an emoji").size(12),
                    crate::widgets::scroll_view(servers).height(Length::Shrink),
                ]
                .spacing(4)
                .max_width(320)
                .into()
            }
        };
        container(content)
            .max_height(120)
            .width(Length::Fill)
            .padding([4, 10])
            .style(move |_| crate::style::fill(theme.panel))
            .into()
    }
}
