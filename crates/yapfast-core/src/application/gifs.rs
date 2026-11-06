//! The GIF picker, as Discord's client has it beside the emoji: a home of
//! trending terms, a search, the trending GIFs and the account's favourites.
//! A click sends a GIF's page as a message; Shift puts it in the composer.
//! The picker keeps pictures only for the GIFs on screen, or about to be,
//! and none once it closes. A picture whose tile grows, as the picker widens,
//! is fetched again larger, the smaller one showing until it comes.

use super::*;
use crate::chat::{FavoriteGif, Gif, GifCategory};

/// What the picker shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GifView {
    /// The Favorites tile, "Trending GIFs" and the trending terms.
    #[default]
    Home,
    Trending,
    Favorites,
    /// What the search field finds.
    Search,
}

/// A picture the picker shows, by its address.
#[derive(Clone, Debug)]
pub enum GifPicture {
    Loading,
    Shown(Arc<Picture>),
    Failed,
}

/// A picture the picker shows, and the size it was last asked for at.
#[derive(Clone, Debug)]
pub struct AskedPicture {
    pub picture: GifPicture,
    size: (u32, u32),
}

impl AskedPicture {
    /// Its tile has grown past a tenth over the size it was asked for at, and
    /// the picture filled that: a larger one would be sharper. A picture
    /// smaller than its tile keeps its own size, larger or not.
    fn outgrown(&self, width: u32, height: u32) -> bool {
        let GifPicture::Shown(picture) = &self.picture else {
            return false;
        };
        let (asked_width, asked_height) = self.size;
        let filled = picture.width + 1 >= asked_width || picture.height + 1 >= asked_height;
        filled
            && u64::from(width) * 10 > u64::from(asked_width) * 11
            && u64::from(height) * 10 > u64::from(asked_height) * 11
    }
}

#[derive(Clone, Debug, Default)]
pub struct GifPicker {
    pub open: bool,
    pub view: GifView,
    /// The search field: a search, or in Favorites a filter. It stays as it
    /// was when the picker opens again, as Discord's does.
    pub query: String,
    /// The home's trending terms, kept once they came, and a GIF for its
    /// "Trending GIFs" tile.
    pub categories: Vec<GifCategory>,
    pub trending_preview: Option<Gif>,
    pub home_loading: bool,
    pub home_error: Option<String>,
    /// The GIFs found: a search's, or those trending.
    pub results: Vec<Gif>,
    /// What they're for: a search, or `None` for those trending; `None`
    /// outside while nothing was asked.
    results_for: Option<Option<String>>,
    pub loading: bool,
    pub error: Option<String>,
    /// Terms to try instead, for the search shown.
    pub suggestions: Vec<String>,
    suggested: Option<String>,
    pub pictures: HashMap<String, AskedPicture>,
    /// The favourites were asked for, and those shown are Discord's rather
    /// than those kept on disk.
    favorites_asked: bool,
    favorites_fresh: bool,
    /// This start asked for what the picker fetches ahead.
    #[cfg(feature = "discord")]
    prefetched: bool,
    /// Stars used before Discord's favourites came, each adding a GIF or
    /// taking one out: they show at once and reach Discord's once they come,
    /// so a change made elsewhere meanwhile stays.
    starred: Vec<(Gif, bool)>,
}

impl GifPicker {
    /// The search the results are for, once they're a search's.
    fn searched(&self) -> Option<&str> {
        self.results_for.as_ref()?.as_deref()
    }
}

impl AppState {
    /// The favourites the picker lists: the newest first, those whose page
    /// has the filter's letters when there is one, as Discord's client
    /// filters them.
    pub fn shown_favorite_gifs(&self) -> Vec<&FavoriteGif> {
        let squash = |text: &str| -> String {
            text.chars()
                .filter(|c| !matches!(c, '-' | '_' | ' '))
                .flat_map(char::to_lowercase)
                .collect()
        };
        let filter = squash(&self.gifs.query);
        let mut favorites: Vec<&FavoriteGif> = self
            .favorite_gifs
            .iter()
            .flatten()
            .filter(|favorite| filter.is_empty() || squash(&favorite.url).contains(&filter))
            .collect();
        favorites.sort_by_key(|favorite| std::cmp::Reverse(favorite.order));
        favorites
    }

    pub fn is_favorite_gif(&self, url: &str) -> bool {
        self.favorite_gifs
            .iter()
            .flatten()
            .any(|favorite| favorite.url == url)
    }

    /// The GIF picker is offered: in a Discord room this account can write in.
    pub fn can_pick_gifs(&self) -> bool {
        !self.preview
            && self.selected.as_deref().is_some_and(|room| {
                Network::of(room) == Network::Discord && self.can_write_in(room)
            })
    }
}

impl Application {
    pub(super) fn open_gifs(&mut self) -> Vec<Effect> {
        if !self.state.can_pick_gifs() {
            return vec![];
        }
        let picker = &mut self.state.gifs;
        picker.open = true;
        let mut effects = vec![];
        if picker.categories.is_empty() && !picker.home_loading {
            picker.home_loading = true;
            picker.home_error = None;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GifCategories,
            )));
        }
        // A search that failed tries again.
        if picker.error.is_some()
            && let Some(asked) = picker.results_for.clone()
        {
            effects.extend(self.ask_gifs(asked));
        }
        effects.extend(self.load_favorite_gifs());
        effects
    }

    /// Asks for the favourite GIFs and the picker's home once, at a start, so
    /// their pictures are fetched ahead before the picker is ever opened.
    /// Opening the picker finds both answered, or tries again if one failed.
    #[cfg(feature = "discord")]
    pub(super) fn prefetch_gifs(&mut self) -> Vec<Effect> {
        if self.state.preview || std::mem::replace(&mut self.state.gifs.prefetched, true) {
            return vec![];
        }
        // The home's answer first: it is one small request, and its terms' pictures
        // are what the home draws. The favourites' list is the larger call, and
        // the GIFs it leads to take the longest to fetch, so it follows.
        let mut effects = vec![];
        if self.state.gifs.categories.is_empty() && !self.state.gifs.home_loading {
            self.state.gifs.home_loading = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GifCategories,
            )));
        }
        effects.extend(self.load_favorite_gifs());
        effects
    }

    /// Asks for the favourites once, as opening the picker or pointing at a
    /// GIF's star does first.
    pub(super) fn load_favorite_gifs(&mut self) -> Vec<Effect> {
        if self.state.gifs.favorites_fresh || self.state.gifs.favorites_asked || self.state.preview
        {
            return vec![];
        }
        self.state.gifs.favorites_asked = true;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::FavoriteGifs,
        ))]
    }

    /// Closes the picker, letting go of its pictures.
    pub(super) fn close_gifs(&mut self) -> Vec<Effect> {
        self.state.gifs.open = false;
        self.state.gifs.pictures = HashMap::new();
        vec![]
    }

    /// The search field changed, after a pause in typing: search for it,
    /// or in Favorites filter them.
    pub(super) fn search_gifs(&mut self, query: String) -> Vec<Effect> {
        let picker = &mut self.state.gifs;
        picker.query = query.clone();
        if picker.view == GifView::Favorites {
            return vec![];
        }
        let query = query.trim().to_owned();
        if query.is_empty() {
            picker.view = GifView::Home;
            return vec![];
        }
        picker.view = GifView::Search;
        if picker.searched() == Some(query.as_str()) && picker.error.is_none() {
            return vec![];
        }
        self.ask_gifs(Some(query))
    }

    /// Shows one of the picker's views; the home empties the search.
    pub(super) fn show_gifs(&mut self, view: GifView) -> Vec<Effect> {
        let picker = &mut self.state.gifs;
        picker.view = view;
        match view {
            GifView::Home | GifView::Favorites => {
                picker.query.clear();
                vec![]
            }
            GifView::Trending => {
                picker.query.clear();
                if picker.results_for == Some(None) && picker.error.is_none() {
                    vec![]
                } else {
                    self.ask_gifs(None)
                }
            }
            GifView::Search => vec![],
        }
    }

    /// A trending term's tile: search for it at once.
    pub(super) fn search_gif_category(&mut self, name: String) -> Vec<Effect> {
        let picker = &mut self.state.gifs;
        picker.view = GifView::Search;
        picker.query = name.clone();
        if picker.searched() == Some(name.as_str()) && picker.error.is_none() {
            return vec![];
        }
        self.ask_gifs(Some(name))
    }

    fn ask_gifs(&mut self, query: Option<String>) -> Vec<Effect> {
        let picker = &mut self.state.gifs;
        picker.results.clear();
        picker.results_for = Some(query.clone());
        picker.loading = true;
        picker.error = None;
        picker.suggestions.clear();
        picker.suggested = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::Gifs(
            query,
        )))]
    }

    /// The pictures of the GIFs on screen or about to be, each with the size
    /// to decode it at; the rest are let go. `near_end` says the list is
    /// scrolled near its end, where a search suggests other terms.
    pub(super) fn gifs_shown(
        &mut self,
        shown: Vec<(String, u32, u32)>,
        near_end: bool,
    ) -> Vec<Effect> {
        let picker = &mut self.state.gifs;
        if !picker.open {
            return vec![];
        }
        picker
            .pictures
            .retain(|url, _| shown.iter().any(|(shown, _, _)| shown == url));
        let mut effects = vec![];
        for (url, width, height) in shown {
            if url.is_empty() {
                continue;
            }
            match picker.pictures.get_mut(&url) {
                Some(asked) if asked.outgrown(width, height) => asked.size = (width, height),
                Some(_) => continue,
                None => {
                    let asked = AskedPicture {
                        picture: GifPicture::Loading,
                        size: (width, height),
                    };
                    picker.pictures.insert(url.clone(), asked);
                }
            }
            let favorite = self
                .state
                .favorite_gifs
                .iter()
                .flatten()
                .any(|favorite| favorite.src == url);
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GifPicture {
                    url,
                    width,
                    height,
                    favorite,
                },
            )));
        }
        if near_end
            && picker.view == GifView::Search
            && !picker.loading
            && let Some(query) = picker.searched().map(str::to_owned)
            && picker.suggested.as_deref() != Some(query.as_str())
        {
            picker.suggested = Some(query.clone());
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GifSuggestions(query),
            )));
        }
        effects
    }

    /// A GIF clicked, by its page: sends it as a message on its own, keeping
    /// a reply, or with `insert` puts it in the composer. The picker closes.
    pub(super) fn pick_gif(&mut self, url: String, insert: bool) -> Vec<Effect> {
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        if !self.state.can_pick_gifs() {
            return vec![];
        }
        let picked = self
            .state
            .gifs
            .results
            .iter()
            .find(|gif| gif.url == url)
            .and_then(|gif| gif.id.clone());
        let query = (self.state.gifs.view == GifView::Search)
            .then(|| self.state.gifs.searched().map(str::to_owned))
            .flatten();
        let mut effects = self.close_gifs();
        if insert {
            let draft = self.state.draft();
            let space = if draft.is_empty() || draft.ends_with(' ') {
                ""
            } else {
                " "
            };
            let body = format!("{draft}{space}{url}");
            effects.extend(self.set_draft(body));
            return effects;
        }
        let (reply, quiet_reply) = match self.state.compose.get(&room) {
            Some(Compose::Reply { item, ping, .. }) => (Some(item.clone()), !ping),
            _ => (None, false),
        };
        let options = chat::SendOptions {
            quiet_reply,
            silent: false,
        };
        effects.extend(self.jump_to_present());
        effects.push(Effect::Backend(Command::Send {
            room,
            body: url,
            reply,
            options,
            files: vec![],
        }));
        if let (Some(id), Some(query)) = (picked, query) {
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GifPicked { id, query },
            )));
        }
        effects
    }

    /// Adds a GIF in the picker to the favourites, by its page, or takes
    /// it out.
    pub(super) fn favorite_gif(&mut self, url: String) -> Vec<Effect> {
        let gif = self
            .state
            .gifs
            .results
            .iter()
            .find(|gif| gif.url == url)
            .cloned()
            .unwrap_or(Gif {
                url,
                ..Default::default()
            });
        self.star_gif(gif)
    }

    /// The same for a message's GIF, by item and attachment.
    pub(super) fn favorite_message_gif(&mut self, item: String, index: usize) -> Vec<Effect> {
        let Some(gif) = self
            .state
            .message(&item)
            .and_then(|message| message.attachments.get(index))
            .and_then(|attachment| attachment.gif.clone())
        else {
            return vec![];
        };
        self.star_gif(gif)
    }

    /// Adds a GIF to the favourites or takes it out, keeping them all on
    /// Discord; before Discord's have come, once they do.
    fn star_gif(&mut self, gif: Gif) -> Vec<Effect> {
        let add = !self.state.is_favorite_gif(&gif.url);
        if add && gif.src.is_empty() {
            return vec![];
        }
        if let Some(favorites) = &mut self.state.favorite_gifs
            && star(favorites, &gif, add)
        {
            self.state.favorite_gifs_changed += 1;
        }
        if !self.state.gifs.favorites_fresh {
            self.state.gifs.starred.push((gif, add));
            return self.load_favorite_gifs();
        }
        let favorites = self.state.favorite_gifs.clone().unwrap_or_default();
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SetFavoriteGifs(favorites),
        ))]
    }

    pub(super) fn gif_categories_came(
        &mut self,
        result: Result<(Vec<GifCategory>, Option<Gif>), String>,
    ) {
        let picker = &mut self.state.gifs;
        picker.home_loading = false;
        match result {
            Ok((categories, preview)) => {
                picker.categories = categories;
                picker.trending_preview = preview;
            }
            Err(error) => picker.home_error = Some(error),
        }
    }

    pub(super) fn gifs_came(&mut self, query: Option<String>, result: Result<Vec<Gif>, String>) {
        let picker = &mut self.state.gifs;
        if picker.results_for.as_ref() != Some(&query) {
            return;
        }
        picker.loading = false;
        match result {
            Ok(gifs) => picker.results = gifs,
            Err(error) => picker.error = Some(error),
        }
    }

    pub(super) fn gif_suggestions_came(&mut self, query: String, terms: Vec<String>) {
        let picker = &mut self.state.gifs;
        if picker.searched() == Some(query.as_str()) {
            picker.suggestions = terms;
        }
    }

    pub(super) fn favorite_gifs_came(
        &mut self,
        result: Result<Vec<FavoriteGif>, String>,
    ) -> Vec<Effect> {
        self.state.gifs.favorites_asked = false;
        match result {
            Ok(mut favorites) => {
                let picker = &mut self.state.gifs;
                picker.favorites_fresh = true;
                let mut starred = false;
                for (gif, add) in std::mem::take(&mut picker.starred) {
                    starred |= star(&mut favorites, &gif, add);
                }
                self.state.favorite_gifs = Some(favorites);
                self.state.favorite_gifs_changed += 1;
                let mut effects: Vec<Effect> = self.prefetch_favorites().into_iter().collect();
                if starred {
                    let favorites = self.state.favorite_gifs.clone().unwrap_or_default();
                    effects.push(Effect::Backend(Command::Discord(
                        DiscordCommand::SetFavoriteGifs(favorites),
                    )));
                }
                effects
            }
            // The stars used meanwhile wait for the next try.
            Err(error) => {
                tracing::warn!(%error, "could not load the favourite GIFs");
                vec![]
            }
        }
    }

    /// Shows the favourites kept on disk, with the stars used since, until
    /// Discord's come.
    pub(super) fn kept_favorite_gifs_came(&mut self, mut favorites: Vec<FavoriteGif>) {
        let picker = &self.state.gifs;
        if picker.favorites_fresh {
            return;
        }
        for (gif, add) in &picker.starred {
            star(&mut favorites, gif, *add);
        }
        self.state.favorite_gifs = Some(favorites);
        self.state.favorite_gifs_changed += 1;
    }

    /// Fetches ahead what's cheap to keep of Discord's list of favourite GIFs,
    /// on Discord's worker, keeping the cache to the list as it does. Nothing
    /// is held: the files go to the disk, and a tile decodes one when it shows
    /// it. The list kept on disk may be out of date, so it doesn't ask.
    fn prefetch_favorites(&self) -> Option<Effect> {
        let mut seen = HashSet::new();
        let urls = self
            .state
            .favorite_gifs
            .as_ref()?
            .iter()
            .filter(|favorite| !favorite.src.is_empty() && seen.insert(favorite.src.as_str()))
            .map(|favorite| favorite.src.clone())
            .collect();
        Some(Effect::Backend(Command::Discord(
            DiscordCommand::PrefetchFavorites(urls),
        )))
    }

    pub(super) fn gif_picture_came(&mut self, url: String, picture: Option<Arc<Picture>>) {
        let Some(asked) = self.state.gifs.pictures.get_mut(&url) else {
            return;
        };
        match (picture, &asked.picture) {
            // A larger picture that didn't come, or one no larger than that
            // shown, as an earlier ask's coming late, leaves it.
            (None, GifPicture::Shown(_)) => {}
            (Some(picture), GifPicture::Shown(shown)) if picture.width <= shown.width => {}
            (picture, _) => {
                asked.picture = picture.map_or(GifPicture::Failed, GifPicture::Shown);
            }
        }
    }
}

/// Adds `gif` to `favorites`, or takes it out; whether that changed them.
fn star(favorites: &mut Vec<FavoriteGif>, gif: &Gif, add: bool) -> bool {
    match (
        favorites
            .iter()
            .position(|favorite| favorite.url == gif.url),
        add,
    ) {
        (None, true) => {
            let added = FavoriteGif::of(gif, favorites);
            favorites.push(added);
            true
        }
        (Some(at), false) => {
            favorites.remove(at);
            true
        }
        _ => false,
    }
}
