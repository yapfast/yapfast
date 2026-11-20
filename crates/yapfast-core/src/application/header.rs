//! What a conversation's header opens: its pinned messages, a search of a
//! DM's messages or a whole server's, and the picker that adds friends to a
//! DM.

use super::*;
#[cfg(feature = "discord")]
use yapfast_discord::search::{MAX_LENGTH, asks_anything};

/// Without Discord nothing is searched: a search is Discord's.
#[cfg(not(feature = "discord"))]
const MAX_LENGTH: usize = 512;

#[cfg(not(feature = "discord"))]
fn asks_anything(_query: &str, _server: bool) -> bool {
    false
}

impl Application {
    pub(super) fn show_pins(&mut self, shown: bool) -> Vec<Effect> {
        self.state.pins = None;
        let Some(room) = self
            .state
            .conversation_that(|can| can.pins)
            // A forum's messages are its posts'.
            .filter(|room| shown && !room.forum)
            .map(|room| room.id.clone())
        else {
            return vec![];
        };
        self.state.add_people = None;
        let mut effects = self.close_threads();
        self.state.pins = Some(Pins {
            room: room.clone(),
            messages: None,
        });
        effects.push(Effect::Backend(Command::Discord(DiscordCommand::Pins {
            room,
        })));
        effects
    }

    pub(super) fn pins_came(&mut self, room: String, result: Result<Vec<ChatMessage>, String>) {
        if let Some(pins) = self.state.pins.as_mut().filter(|pins| pins.room == room) {
            pins.messages = Some(result);
        }
    }

    /// Searches for `query` from the box, the newest first, as each new
    /// search from Discord's box does; or with nothing typed, ends the
    /// search. Discord doesn't search a box holding only filters' keys, nor
    /// past its longest.
    pub(super) fn search(&mut self, query: String) -> Vec<Effect> {
        let query: String = query.trim().chars().take(MAX_LENGTH).collect();
        let (Some(place), false) = (self.state.search_place(), query.is_empty()) else {
            self.state.search = None;
            return vec![];
        };
        if !asks_anything(&query, discord::parse_server(&place).is_some()) {
            return vec![];
        }
        let sort = chat::SearchSort::Newest;
        // The count shown stays while the same search runs again, as the
        // client keeps it for the same query and order.
        let total = self
            .state
            .search
            .as_ref()
            .filter(|known| known.place == place && known.query == query && known.sort == sort)
            .map_or(0, |known| known.total);
        let search = Search {
            place,
            query,
            sort,
            total,
            loading: true,
            ..Search::default()
        };
        let command = search_command(&search);
        self.state.search = Some(search);
        vec![command]
    }

    /// Shows `page` of the results, unless a page is on its way, as the
    /// client's pages wait for the one asked for.
    pub(super) fn search_page(&mut self, page: u32) -> Vec<Effect> {
        let Some(search) = self
            .state
            .search
            .as_mut()
            .filter(|search| !search.loading && page < search.pages().max(1))
        else {
            return vec![];
        };
        search.page = page;
        search.loading = true;
        search.error = None;
        vec![search_command(search)]
    }

    /// Searches again in `sort`'s order from the first page; not while a
    /// search is on its way, as the client's Sort menu waits.
    pub(super) fn search_sort(&mut self, sort: chat::SearchSort) -> Vec<Effect> {
        let Some(search) = self
            .state
            .search
            .as_mut()
            .filter(|search| !search.loading && search.sort != sort)
        else {
            return vec![];
        };
        search.sort = sort;
        search.page = 0;
        search.loading = true;
        search.error = None;
        vec![search_command(search)]
    }

    /// Takes a page of results for the search on screen, if it's the page
    /// it asked for, or Discord's word that it's still indexing.
    pub(super) fn found(
        &mut self,
        place: String,
        query: String,
        sort: chat::SearchSort,
        offset: u32,
        result: Result<chat::Searched, String>,
    ) {
        let Some(search) = self.state.search.as_mut().filter(|search| {
            search.loading
                && search.place == place
                && search.query == query
                && search.sort == sort
                && search.page * SEARCH_PAGE == offset
        }) else {
            return;
        };
        if let Ok(chat::Searched::Indexing { again }) = result {
            search.indexing = true;
            search.loading = again;
            return;
        }
        search.loading = false;
        search.indexing = false;
        match result {
            Ok(chat::Searched::Indexing { .. }) => {}
            Ok(chat::Searched::Page {
                total,
                hits,
                words,
                indexing_older,
            }) => {
                search.indexing_older = indexing_older;
                search.total = total;
                search.words = words;
                search.results = hits;
            }
            Err(error) => search.error = Some(error),
        }
    }

    pub(super) fn open_add_people(&mut self) -> Vec<Effect> {
        let room = self
            .state
            .dm_that(|can| can.group_dms)
            .filter(|_| self.state.discord_online)
            .map(|room| room.id.clone());
        if let Some(room) = room {
            self.state.pins = None;
            self.state.add_people = Some(AddPeople {
                room,
                ..AddPeople::default()
            });
        }
        vec![]
    }

    pub(super) fn choose_person(&mut self, user: String) -> Vec<Effect> {
        let room = self.state.group_room();
        let Some(picker) = self
            .state
            .add_people
            .as_mut()
            .filter(|picker| !picker.adding)
        else {
            return vec![];
        };
        if let Some(index) = picker.chosen.iter().position(|chosen| chosen == &user) {
            picker.chosen.remove(index);
        } else if picker.chosen.len() < room {
            picker.chosen.push(user);
        }
        picker.error = None;
        vec![]
    }

    /// Adds those chosen to the group DM, or with a one-to-one DM, makes a
    /// group of them and its other person, as Discord's client does; the
    /// DM itself stays as it was.
    pub(super) fn add_people(&mut self) -> Vec<Effect> {
        let person = self.state.one_to_one().and_then(|room| room.person.clone());
        let Some(picker) = self
            .state
            .add_people
            .as_mut()
            .filter(|picker| !picker.adding && !picker.chosen.is_empty())
        else {
            return vec![];
        };
        picker.adding = true;
        picker.error = None;
        let command = match person {
            Some(person) => DiscordCommand::AddToGroup {
                room: None,
                users: std::iter::once(person)
                    .chain(picker.chosen.iter().cloned())
                    .collect(),
            },
            None => DiscordCommand::AddToGroup {
                room: Some(picker.room.clone()),
                users: picker.chosen.clone(),
            },
        };
        vec![Effect::Backend(Command::Discord(command))]
    }

    /// A group made opens once the DM list has it.
    pub(super) fn group_changed(&mut self, result: Result<String, String>) {
        match result {
            Ok(room) => {
                if self.state.add_people.take().is_some()
                    && self.state.selected.as_ref() != Some(&room)
                {
                    self.pending_open = Some(room);
                }
            }
            Err(error) => match &mut self.state.add_people {
                Some(picker) => {
                    picker.adding = false;
                    picker.error = Some(error);
                }
                None => self.state.notice = Some(error),
            },
        }
    }
}

/// The command that loads `search`'s page.
fn search_command(search: &Search) -> Effect {
    Effect::Backend(Command::Discord(DiscordCommand::Search {
        place: search.place.clone(),
        query: search.query.clone(),
        sort: search.sort,
        offset: search.page * SEARCH_PAGE,
    }))
}
