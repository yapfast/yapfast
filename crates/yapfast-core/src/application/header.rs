//! What a conversation's header opens: its pinned messages, a search of a
//! DM's messages or a whole server's, and the picker that adds friends to a
//! DM.

use super::*;

/// How many messages a page of search results holds, as Discord sends them.
const SEARCH_PAGE: u32 = 25;

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

    pub(super) fn search(&mut self, query: String) -> Vec<Effect> {
        let query = query.trim().to_owned();
        let (Some(place), false) = (self.state.search_place(), query.is_empty()) else {
            self.state.search = None;
            return vec![];
        };
        self.state.search = Some(Search {
            place: place.clone(),
            query: query.clone(),
            loading: true,
            ..Search::default()
        });
        vec![Effect::Backend(Command::Discord(DiscordCommand::Search {
            place,
            query,
            offset: 0,
        }))]
    }

    pub(super) fn more_results(&mut self) -> Vec<Effect> {
        let Some(search) = self
            .state
            .search
            .as_mut()
            .filter(|search| !search.loading && search.more())
        else {
            return vec![];
        };
        search.loading = true;
        search.error = None;
        vec![Effect::Backend(Command::Discord(DiscordCommand::Search {
            place: search.place.clone(),
            query: search.query.clone(),
            offset: search.next,
        }))]
    }

    /// Takes a page of results for the search on screen, if it's the page
    /// it asked for.
    pub(super) fn found(
        &mut self,
        place: String,
        query: String,
        offset: u32,
        result: Result<(u32, Vec<chat::SearchHit>), String>,
    ) {
        let Some(search) = self.state.search.as_mut().filter(|search| {
            search.loading
                && search.place == place
                && search.query == query
                && search.next == offset
        }) else {
            return;
        };
        search.loading = false;
        match result {
            Ok((total, page)) => {
                // Messages sent since the last page move the rest along.
                for hit in page {
                    if !search
                        .results
                        .iter()
                        .any(|known| known.message.item == hit.message.item)
                    {
                        search.results.push(hit);
                    }
                }
                search.total = total;
                search.next = offset + SEARCH_PAGE;
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
                None => self.state.notice = Some(super::Notice::Error(error)),
            },
        }
    }
}
