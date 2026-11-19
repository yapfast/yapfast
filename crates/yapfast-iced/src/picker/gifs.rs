//! The GIFs tab: a search over Klipy after a quarter second's pause in
//! typing; a home of the Favorites tile, "Trending GIFs" and the trending
//! terms; the GIFs found, in Discord's masonry of columns about 200 px wide.
//! A click sends a GIF, Shift and a click puts it in the composer, and its
//! star keeps it among the favourites. Back, or Escape, goes home; Escape
//! there, or Ctrl+G, closes the picker. Only the pictures in sight and a
//! screen either side are asked of the core, at their tiles' sizes.

use std::hash::{Hash, Hasher};

use iced::{
    Alignment, Color, Element, Length, Padding, Rectangle, Size, Task,
    widget::{Space, button, column, container, mouse_area, row, stack, text, tooltip},
};
use yapfast_core::application::{Action, GifPicture, GifView};

use super::{
    Event, Picker, View,
    kit::{self, Grid, Masonry},
};
use crate::{
    app::Message,
    ctx::{Cx, Focus, Request},
    fonts::{MEDIUM, SEMIBOLD},
    media::{Fit, Shot},
    pictures::Icon,
    style::rounded,
};

/// The home's tiles' height, and how many placeholders a search shows
/// while it loads.
const HOME_TILE: f32 = 110.0;
const PLACEHOLDERS: usize = 16;
/// Grounds for tiles whose picture isn't in yet, in turn.
const GROUNDS: [Color; 3] = [
    Color::from_rgb8(0x3b, 0x3f, 0x63),
    Color::from_rgb8(0x4a, 0x3b, 0x63),
    Color::from_rgb8(0x3b, 0x52, 0x63),
];
/// How long typing pauses before it searches.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(250);
/// Discord's client suggests other terms within this of the grid's end.
const NEAR_END: f32 = 1180.0;

pub enum Item {
    Gif {
        url: String,
        src: String,
        favorite: bool,
    },
    Favorites(String),
    Trending(String),
    Term {
        name: String,
        src: String,
    },
    Placeholder,
    /// Terms to try instead, under a search's end.
    Suggestions,
}

impl Item {
    fn src(&self) -> &str {
        match self {
            Self::Gif { src, .. } | Self::Favorites(src) | Self::Trending(src) => src,
            Self::Term { src, .. } => src,
            Self::Placeholder | Self::Suggestions => "",
        }
    }
}

#[derive(Default)]
pub struct Gifs {
    grid: Grid<Item>,
    note: String,
    /// The pictures last asked for, with their sizes, and whether the grid
    /// was near its end.
    asked: Vec<(String, u32, u32)>,
    near_end: bool,
    /// The core's search as the field was last given it.
    query: String,
}

impl Picker {
    pub(super) fn build_gifs(&mut self, cx: &Cx<'_>) {
        let state = cx.state();
        let gifs = &state.gifs;
        let mut hasher = std::hash::DefaultHasher::new();
        (gifs.view as u8, &gifs.query, gifs.loading, &gifs.error).hash(&mut hasher);
        (gifs.home_loading, &gifs.home_error, gifs.categories.len()).hash(&mut hasher);
        gifs.results.len().hash(&mut hasher);
        gifs.results.first().map(|gif| &gif.url).hash(&mut hasher);
        gifs.trending_preview
            .as_ref()
            .map(|gif| &gif.src)
            .hash(&mut hasher);
        gifs.suggestions.hash(&mut hasher);
        state.favorite_gifs_changed.hash(&mut hasher);
        state.favorite_gifs.as_ref().map(Vec::len).hash(&mut hasher);
        let signature = hasher.finish();
        let picker = &mut *self;
        let width = picker.grid_width();
        let Some(tab) = &mut picker.gifs else {
            return;
        };
        if tab.query != gifs.query {
            tab.query.clone_from(&gifs.query);
            if picker.field.text() != gifs.query {
                picker.field.reset(&gifs.query);
            }
        }
        if picker.built == signature {
            return;
        }
        picker.built = signature;
        let mut masonry = Masonry::new(width - 2.0 * Masonry::GUTTER);
        let column = masonry.width;
        let mut grid = Grid::default();
        let mut note = String::new();
        let mut tile = |item: Item, height: f32| {
            let (x, y) = masonry.place(height);
            grid.place(
                Rectangle::new(iced::Point::new(x, y), Size::new(masonry.width, height)),
                item,
            );
        };
        let ratio = |width: u32, height: u32| {
            if width == 0 || height == 0 {
                1.0
            } else {
                height as f32 / width as f32
            }
        };
        let placeholders = |count: usize, tile: &mut dyn FnMut(Item, f32)| {
            for index in 0..count {
                // Heights that vary, the same each time.
                tile(Item::Placeholder, 120.0 + ((index * 37 + 11) % 100) as f32);
            }
        };
        match gifs.view {
            GifView::Home => {
                if gifs.categories.is_empty() && gifs.home_loading {
                    for _ in 0..8 {
                        tile(Item::Placeholder, HOME_TILE);
                    }
                } else {
                    let newest = state
                        .shown_favorite_gifs()
                        .first()
                        .map(|favorite| favorite.src.clone());
                    tile(Item::Favorites(newest.unwrap_or_default()), HOME_TILE);
                    let trending = gifs.trending_preview.as_ref().map(|gif| gif.src.clone());
                    tile(Item::Trending(trending.unwrap_or_default()), HOME_TILE);
                    for category in &gifs.categories {
                        tile(
                            Item::Term {
                                name: category.name.clone(),
                                src: category.src.clone(),
                            },
                            HOME_TILE,
                        );
                    }
                    if let Some(error) = &gifs.home_error {
                        note.clone_from(error);
                    }
                }
            }
            GifView::Trending | GifView::Search => {
                if gifs.loading {
                    placeholders(PLACEHOLDERS, &mut tile);
                } else if let Some(error) = &gifs.error {
                    note.clone_from(error);
                } else if gifs.results.is_empty() {
                    note = "No GIFs match your search.".into();
                }
                for gif in gifs.results.iter().filter(|_| !gifs.loading) {
                    tile(
                        Item::Gif {
                            url: gif.url.clone(),
                            src: gif.src.clone(),
                            favorite: state.is_favorite_gif(&gif.url),
                        },
                        column * ratio(gif.width, gif.height),
                    );
                }
            }
            GifView::Favorites => match &state.favorite_gifs {
                None => placeholders(PLACEHOLDERS / 2, &mut tile),
                Some(all) => {
                    let shown = state.shown_favorite_gifs();
                    if all.is_empty() {
                        note = "Click the star in the corner of a GIF to favorite it. \
                                Favorites will show up here!"
                            .into();
                    } else if shown.is_empty() {
                        note = "No favorite GIFs match your search.".into();
                    }
                    for favorite in shown {
                        tile(
                            Item::Gif {
                                url: favorite.url.clone(),
                                src: favorite.src.clone(),
                                favorite: true,
                            },
                            column * ratio(favorite.width, favorite.height),
                        );
                    }
                }
            },
        }
        if gifs.view == GifView::Search && !gifs.suggestions.is_empty() {
            let top = if note.is_empty() {
                masonry.bottom() + 4.0
            } else {
                96.0
            };
            grid.place(
                Rectangle::new(
                    iced::Point::new(Masonry::GUTTER, top),
                    Size::new(width - 2.0 * Masonry::GUTTER, 84.0),
                ),
                Item::Suggestions,
            );
        }
        grid.height = grid.height.max(masonry.bottom());
        tab.grid = grid;
        tab.note = note;
        picker.hover = None;
    }

    /// Asks for the pictures of the tiles in sight and a screen either
    /// side, letting go of the rest, and says whether the grid is near its
    /// end.
    pub(super) fn ask_gifs(&mut self, cx: &mut Cx<'_>) {
        let picker = &mut *self;
        let scroll = picker.scroll;
        let Some(tab) = &mut picker.gifs else {
            return;
        };
        let mut asked: Vec<(String, u32, u32)> = Vec::new();
        for (_, tile) in tab.grid.in_sight(scroll) {
            let src = tile.item.src();
            if src.is_empty() || asked.iter().any(|(known, _, _)| known == src) {
                continue;
            }
            // A home tile crops its picture to the column's width.
            let height = match tile.item {
                Item::Gif { .. } => tile.at.height,
                _ => tile.at.height.max(tile.at.width),
            };
            asked.push((src.to_owned(), tile.at.width as u32, height as u32));
        }
        let near_end = scroll.top + scroll.height >= tab.grid.height - NEAR_END;
        if asked == tab.asked && near_end == tab.near_end {
            return;
        }
        tab.asked.clone_from(&asked);
        tab.near_end = near_end;
        cx.client.dispatch(Action::GifsShown {
            shown: asked,
            near_end,
        });
    }

    /// Typing searches after a pause; in Favorites it filters at once.
    pub(super) fn gifs_typed(&mut self, text: String, cx: &mut Cx<'_>) -> Task<Message> {
        if cx.state().gifs.view == GifView::Favorites {
            cx.dispatch(Action::SearchGifs(text));
            return Task::none();
        }
        let picker = &mut *self;
        picker.typed += 1;
        crate::after(PAUSE, Message::Picker(Event::SearchDue(picker.typed)))
    }

    pub(super) fn gifs_due(&mut self, typed: u64, cx: &mut Cx<'_>) -> Task<Message> {
        if self.typed != typed {
            return Task::none();
        }
        self.gifs_search_now(cx)
    }

    pub(super) fn gifs_search_now(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        let picker = &mut *self;
        picker.typed += 1;
        let text = picker.field.text();
        if let Some(tab) = &mut picker.gifs {
            tab.query.clone_from(&text);
        }
        cx.dispatch(Action::SearchGifs(text));
        // A search changes the view, and the header with it (Back comes or
        // goes, Trending's title goes): the field keeps the keys.
        cx.request(Request::Focus(Focus::Id(super::kit::FIELD)));
        self.scroll_grid(0.0)
    }

    pub(super) fn gifs_back(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        {
            let picker = &mut *self;
            picker.field.reset("");
            picker.typed += 1;
        }
        cx.dispatch(Action::ShowGifs(GifView::Home));
        self.scroll_grid(0.0)
    }

    pub(super) fn gifs_escape(&mut self, cx: &mut Cx<'_>) -> Task<Message> {
        if cx.state().gifs.view == GifView::Home {
            self.ask_close(cx)
        } else {
            self.gifs_back(cx)
        }
    }

    fn gif_item(&self, index: usize) -> Option<&Item> {
        Some(&self.gifs.as_ref()?.grid.tiles.get(index)?.item)
    }

    pub(super) fn gif_pick(&mut self, index: usize, cx: &mut Cx<'_>) -> Task<Message> {
        let action = match self.gif_item(index) {
            Some(Item::Gif { url, .. }) => {
                let insert = cx.modifiers.shift();
                let action = Action::PickGif {
                    url: url.clone(),
                    insert,
                };
                cx.dispatch(action);
                return self.ask_close(cx);
            }
            Some(Item::Favorites(_)) => Action::ShowGifs(GifView::Favorites),
            Some(Item::Trending(_)) => Action::ShowGifs(GifView::Trending),
            Some(Item::Term { name, .. }) => Action::SearchGifCategory(name.clone()),
            _ => return Task::none(),
        };
        cx.dispatch(action);
        self.scroll_grid(0.0)
    }

    /// A trending term, or one suggested: search for it at once.
    pub(super) fn gif_term(&mut self, term: String, cx: &mut Cx<'_>) -> Task<Message> {
        cx.dispatch(Action::SearchGifCategory(term));
        self.scroll_grid(0.0)
    }

    pub(super) fn gif_star(&self, index: usize, cx: &mut Cx<'_>) {
        if let Some(Item::Gif { url, .. }) = self.gif_item(index) {
            let url = url.clone();
            cx.dispatch(Action::FavoriteGif(url));
        }
    }
}

impl<'a> View<'a> {
    pub(super) fn gifs_view(self, picker: &'a Picker) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let Some(tab) = &picker.gifs else {
            return Space::new().into();
        };
        let state = self.cx.state;
        let view = state.gifs.view;
        // The header: Back off the home, the field (in Trending, its title).
        let mut header = row![].spacing(8).align_y(Alignment::Center);
        if view != GifView::Home {
            header = header.push(
                self.cx
                    .look
                    .icon_button(Icon::ChevronLeft, "Back", Message::Picker(Event::Back))
                    .size(34.0, 20),
            );
        }
        let placeholder = if view == GifView::Favorites {
            "Search Favorites"
        } else {
            "Search Klipy"
        };
        let field = kit::search_field(&picker.field, placeholder, theme);
        if view == GifView::Trending {
            let raised = theme.raised;
            let title = container(
                text("Trending GIFs")
                    .size(16)
                    .font(crate::fonts::BOLD)
                    .color(theme.text),
            )
            .height(34)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .style(move |_| crate::style::fill(raised));
            // The field stays under the title, out of sight and out of the
            // pointer's way, so typing still searches.
            header = header.push(stack![
                container(field).padding(2).height(34).clip(true),
                iced::widget::opaque(title)
            ]);
        } else {
            header = header.push(field);
        }
        let header = container(header).padding(Padding::ZERO.left(16).right(16).bottom(12));
        let pictures = &state.gifs.pictures;
        let rows = tab
            .grid
            .rows(picker.scroll, picker.built, |index, item, size| {
                self.gif_tile(picker, index, item, size, pictures)
            });
        let body: Element<'a, Message> = if tab.note.is_empty() {
            kit::grid(rows, theme)
        } else {
            stack![kit::note(tab.note.clone(), theme), kit::grid(rows, theme)].into()
        };
        column![header, body].height(Length::Fill).into()
    }

    fn gif_tile(
        self,
        picker: &'a Picker,
        index: usize,
        item: &'a Item,
        size: Size,
        pictures: &'a std::collections::HashMap<String, yapfast_core::application::AskedPicture>,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        if let Item::Suggestions = item {
            return self.gif_suggestions();
        }
        let shown = match pictures.get(item.src()).map(|asked| &asked.picture) {
            Some(GifPicture::Shown(picture)) => Some(picture),
            _ => None,
        };
        let ground = GROUNDS[index % GROUNDS.len()];
        let mut layers = stack![
            container(Space::new())
                .width(size.width)
                .height(size.height)
                .style(move |_| rounded(ground, 5))
        ];
        if shown.is_some() {
            layers = layers.push(
                self.cx
                    .shot(Shot::new(shown, size).fit(Fit::Cover).radius(5.0).bare()),
            );
        }
        let hovered = picker.hover == Some(index);
        // The home's tiles are labelled over a dimmed picture, dimmer under
        // the pointer.
        let label = match item {
            Item::Favorites(_) => Some((Some(Icon::StarFilled), "Favorites".to_owned())),
            Item::Trending(_) => Some((Some(Icon::TrendingUp), "Trending GIFs".to_owned())),
            Item::Term { name, .. } => Some((None, name.clone())),
            _ => None,
        };
        if let Some((icon, words)) = label {
            let dim = if matches!(item, Item::Favorites(_)) {
                crate::style::faded(theme.accent, 0.8)
            } else {
                Color::from_rgba8(0, 0, 0, 0.5)
            };
            let dim = if hovered {
                crate::theme::mix(Color::BLACK, dim, 0.31)
            } else {
                dim
            };
            let mut line = row![].spacing(6).align_y(Alignment::Center);
            if let Some(icon) = icon {
                line = line.push(self.cx.look.glyph(icon, 20, Color::WHITE));
            }
            line = line.push(
                text(words)
                    .size(16)
                    .font(SEMIBOLD)
                    .color(Color::WHITE)
                    .wrapping(text::Wrapping::None),
            );
            layers = layers.push(
                container(line)
                    .center(Length::Fill)
                    .clip(true)
                    .style(move |_| rounded(dim, 5)),
            );
        }
        let pressable = !matches!(item, Item::Placeholder);
        let tile = button(layers)
            .padding(0)
            .width(size.width)
            .height(size.height)
            .on_press_maybe(pressable.then_some(Message::Picker(Event::Pick(index))))
            .style(|_, _| button::Style::default());
        let mut layers = stack![tile];
        if let Item::Gif { favorite, .. } = item
            && hovered
        {
            let tip = if *favorite {
                "Remove from Favorites"
            } else {
                "Add to Favorites"
            };
            let star = kit::star(
                self.cx.look,
                *favorite,
                32.0,
                20,
                Message::Picker(Event::Star(index)),
            );
            layers = layers.push(kit::pinned(
                tooltip(
                    star,
                    crate::controls::tip(theme, tip),
                    tooltip::Position::Top,
                )
                .gap(6),
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

    /// Terms to try instead, under a search that found few.
    fn gif_suggestions(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut terms = row![].spacing(8);
        for term in &self.cx.state.gifs.suggestions {
            let (rest, lit) = (theme.sunken, theme.hover);
            terms = terms.push(
                button(
                    container(text(term.clone()).size(13).font(MEDIUM).color(theme.text))
                        .height(Length::Fill)
                        .align_y(Alignment::Center),
                )
                .padding([0, 10])
                .height(28)
                .on_press(Message::Picker(Event::Term(term.clone())))
                .style(crate::controls::lit_style(Some(rest), lit, 4.0)),
            );
        }
        column![
            text("Your perfect GIF is in another castle. Try the suggested keywords below!")
                .size(14)
                .color(theme.secondary)
                .align_x(Alignment::Center)
                .width(Length::Fill),
            container(terms.wrap()).center_x(Length::Fill)
        ]
        .spacing(10)
        .into()
    }
}
