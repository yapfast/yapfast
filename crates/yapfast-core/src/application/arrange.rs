//! Dragging a Discord server's channels and categories to move them in its
//! channel list, as Discord's client has it: who may drag what, where a
//! drop on a row lands, and the positions Discord is asked for.

use super::*;
use crate::chat::ChannelPlacement;
#[cfg(feature = "discord")]
use crate::chat::Permission;
#[cfg(feature = "discord")]
use yapfast_discord::marked::{MarkedChannel, MarkedServer};

/// Where a dragged channel or category lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    /// Before or after another of its kind, by room id: a text or a voice
    /// channel, or a category.
    Before(String),
    After(String),
    /// First or last of its kind in a category, by room id, or with none
    /// among the channels outside any.
    First(Option<String>),
    Last(Option<String>),
}

/// Asks whether a channel dragged into a category takes the category's
/// permissions, as Discord's client asks when the channel's matched its
/// old category's, or it had none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveSync {
    /// The channel's and the category's names.
    pub channel: String,
    pub category: String,
    /// The move, sent once answered: the server's id, the channel's room
    /// id, and where everything goes.
    pub(super) server: String,
    pub(super) room: String,
    pub(super) placements: Vec<ChannelPlacement>,
}

/// Which of a server's lists a channel or category sorts in. Text,
/// announcement and forum channels share one, voice and stage channels
/// another, which always lists below the first in a category.
#[cfg(feature = "discord")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Category,
    Text,
    Voice,
}

/// A row of the channel list as dragging sees it: a channel's or
/// category's id, and a channel's category.
#[cfg(feature = "discord")]
#[derive(Clone, Copy)]
struct Row<'a> {
    id: &'a str,
    kind: Kind,
    parent: Option<&'a str>,
}

impl Application {
    /// Whether this account may drag a Discord channel or category, by its
    /// room id, as Discord's client lets it: a category with Manage
    /// Channels in the server, a channel with it in its category, or in
    /// the server for one outside any. Not a thread, nor a channel pinned
    /// to the top, nor anything while the list shows only the channels
    /// followed.
    pub fn draggable(&self, room: &str) -> bool {
        #[cfg(feature = "discord")]
        if let Some((server, Some(id), _)) = self.discord_place(room) {
            return self.drags(server, &kinds(server), id);
        }
        let _ = room;
        false
    }

    /// Which of the channel list's rows, `shown` by room id from the top,
    /// the row `dragged` may drop on.
    pub fn droppable(&self, shown: &[String], dragged: usize) -> Vec<bool> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = shown.get(dragged).and_then(|room| self.discord_place(room)) {
            let kinds = kinds(server);
            return (0..shown.len())
                .map(|onto| self.lands(server, &kinds, shown, dragged, onto).is_some())
                .collect();
        }
        let _ = dragged;
        vec![false; shown.len()]
    }

    /// Where the row `dragged` of the channel list, `shown` by room id from
    /// the top, lands dropped on the row `onto`, as Discord's client works
    /// it out: a channel takes the place of one of its kind, above it when
    /// dragged up and below it when dragged down; on a category it goes
    /// last in the category above when dragged up, first in that one when
    /// dragged down. A category goes above another when dragged up, and
    /// below the last row of another when dragged down. None where it
    /// can't go.
    pub fn landing(&self, shown: &[String], dragged: usize, onto: usize) -> Option<Landing> {
        #[cfg(feature = "discord")]
        if let Some((server, _, _)) = shown.get(dragged).and_then(|room| self.discord_place(room)) {
            return self.lands(server, &kinds(server), shown, dragged, onto);
        }
        let _ = (shown, dragged, onto);
        None
    }

    /// [`Self::draggable`], by the channel's or category's id in `server`.
    #[cfg(feature = "discord")]
    fn drags(&self, server: &MarkedServer, kinds: &Kinds<'_>, id: &str) -> bool {
        if !self.state.discord_online
            || server.follows_only()
            || server.marks.notifications.of(id).pinned()
        {
            return false;
        }
        match kinds.get(id) {
            Some((Kind::Category, _)) => {
                self.may(&discord::server_id(&server.id), Permission::ManageChannels)
            }
            Some((_, parent)) => self.may(&manager(server, *parent), Permission::ManageChannels),
            None => false,
        }
    }

    /// [`Self::landing`] in `server`, whose channels and categories
    /// `kinds` has.
    #[cfg(feature = "discord")]
    fn lands(
        &self,
        server: &MarkedServer,
        kinds: &Kinds<'_>,
        shown: &[String],
        dragged: usize,
        onto: usize,
    ) -> Option<Landing> {
        let row = |at: usize| {
            let (guild, id) = discord::parse_room(shown.get(at)?)?;
            let &(kind, parent) = kinds.get(id).filter(|_| guild == server.id)?;
            Some(Row { id, kind, parent })
        };
        let room = |id: String| discord::room_id(&server.id, &id);
        let landing = match land(row, dragged, onto)? {
            Landing::Before(id) => Landing::Before(room(id)),
            Landing::After(id) => Landing::After(room(id)),
            Landing::First(id) => Landing::First(id.map(room)),
            Landing::Last(id) => Landing::Last(id.map(room)),
        };
        let (_, id) = discord::parse_room(&shown[dragged])?;
        self.may_land(server, kinds, id, &landing)
            .then_some(landing)
    }

    /// Whether this account may drag the channel or category `id` and drop
    /// it where `landing` says: with Manage Channels in the category it
    /// goes in, or in the server outside any, and able to see a category it
    /// moves into.
    #[cfg(feature = "discord")]
    fn may_land(
        &self,
        server: &MarkedServer,
        kinds: &Kinds<'_>,
        id: &str,
        landing: &Landing,
    ) -> bool {
        let Some(&(kind, parent)) = kinds.get(id) else {
            return false;
        };
        if !self.drags(server, kinds, id) {
            return false;
        }
        let into = match landing {
            // A category goes beside a category, a channel beside a channel.
            Landing::Before(other) | Landing::After(other) => {
                match raw(other).and_then(|other| kinds.get(other)) {
                    Some(&(theirs, parent))
                        if (theirs == Kind::Category) == (kind == Kind::Category) =>
                    {
                        parent
                    }
                    _ => return false,
                }
            }
            Landing::First(category) | Landing::Last(category) => {
                match category.as_deref().map(raw) {
                    _ if kind == Kind::Category => return false,
                    None => None,
                    Some(Some(category))
                        if kinds
                            .get(category)
                            .is_some_and(|(kind, _)| *kind == Kind::Category) =>
                    {
                        Some(category)
                    }
                    Some(_) => return false,
                }
            }
        };
        if kind == Kind::Category {
            return true;
        }
        let place = manager(server, into);
        self.may(&place, Permission::ManageChannels)
            && (into == parent || into.is_none() || self.may(&place, Permission::ViewChannel))
    }

    /// Moves a Discord channel or category, by its room id, where `landing`
    /// says, as dropping it there in the channel list does. A channel moving
    /// into a category whose permissions it could take asks first, as
    /// Discord's client does; see [`MoveSync`].
    #[cfg_attr(
        not(feature = "discord"),
        allow(
            clippy::needless_pass_by_ref_mut,
            reason = "builds with Discord change it"
        )
    )]
    pub(super) fn move_channel(&mut self, room: String, landing: Landing) -> Vec<Effect> {
        #[cfg(feature = "discord")]
        if self.discord_place(&room).is_some_and(|(server, id, _)| {
            id.is_some_and(|id| self.may_land(server, &kinds(server), id, &landing))
        }) && let Some((server, placements)) = self.placements(&room, &landing)
            && !placements.is_empty()
        {
            let into = placements
                .iter()
                .find(|placement| placement.room == room)
                .and_then(|placement| placement.parent.clone())
                .flatten();
            if let Some(category) = into
                && self.asks_sync(&room, &category)
            {
                self.state.move_sync = Some(MoveSync {
                    channel: self.channel_title(&room),
                    category: self.channel_title(&category),
                    server,
                    room,
                    placements,
                });
                return vec![];
            }
            return vec![Effect::Backend(Command::Discord(
                DiscordCommand::MoveChannels {
                    server,
                    placements,
                    lock: None,
                },
            ))];
        }
        let _ = (room, landing);
        vec![]
    }

    /// Answers [`MoveSync`]: the channel moves, taking its new category's
    /// permissions with `sync`; or with none, it stays where it was.
    pub(super) fn answer_move_sync(&mut self, sync: Option<bool>) -> Vec<Effect> {
        let Some(asked) = self.state.move_sync.take() else {
            return vec![];
        };
        let Some(sync) = sync else {
            return vec![];
        };
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::MoveChannels {
                server: asked.server,
                placements: asked.placements,
                lock: sync.then_some(asked.room),
            },
        ))]
    }

    /// Whether moving a channel into `category` asks to take its
    /// permissions, as Discord's client asks: when this account may manage
    /// the permissions of both, the channel's differ from the category's,
    /// and they matched its old category's, or it had none.
    #[cfg(feature = "discord")]
    fn asks_sync(&self, room: &str, category: &str) -> bool {
        self.may(room, Permission::ManageRoles)
            && self.may(category, Permission::ManageRoles)
            && !self.same_overwrites(room, category)
            && self
                .category_of(room)
                .is_none_or(|old| self.same_overwrites(room, &old))
    }

    /// The server's id and where its channels and categories go once
    /// `room` lands where `landing` says, as Discord's client asks for it:
    /// each list numbered again from 0 across the server, in the order it
    /// shows, giving those whose position changes, and the dragged
    /// channel's new category. Those in categories this account may not
    /// manage are left out, as Discord's client leaves them.
    #[cfg(feature = "discord")]
    fn placements(&self, room: &str, landing: &Landing) -> Option<(String, Vec<ChannelPlacement>)> {
        let (server, Some(dragged), _) = self.discord_place(room)? else {
            return None;
        };
        let kinds = kinds(server);
        let &(kind, _) = kinds.get(dragged)?;
        let key = |parent| order(server, parent);
        let mut placed = Vec::new();
        // Each channel whose position changes, in the order it shows.
        let mut numbered = |listed: &[(&str, Option<&str>, i64)],
                            moved: Option<(&str, Option<&str>)>| {
            for (at, &(id, _, position)) in listed.iter().enumerate() {
                let parent = moved
                    .filter(|(which, _)| *which == id)
                    .map(|(_, parent)| parent.map(str::to_owned));
                if position != at as i64 || parent.is_some() {
                    placed.push(yapfast_discord::ready::Placement {
                        id: id.to_owned(),
                        position: at as i64,
                        parent,
                    });
                }
            }
        };
        if kind == Kind::Category {
            let shown = self.categories(&discord::server_id(&server.id));
            let mut categories: Vec<&str> = shown
                .iter()
                .filter_map(|category| raw(&category.id))
                .map(|id| {
                    server
                        .categories
                        .get_key_value(id)
                        .map(|(id, _)| id.as_str())
                })
                .collect::<Option<_>>()?;
            let from = categories.iter().position(|id| *id == dragged)?;
            categories.remove(from);
            let to = match landing {
                Landing::Before(other) => {
                    categories.iter().position(|id| Some(*id) == raw(other))?
                }
                Landing::After(other) => {
                    categories.iter().position(|id| Some(*id) == raw(other))? + 1
                }
                Landing::First(_) | Landing::Last(_) => return None,
            };
            categories.insert(to, server.categories.get_key_value(dragged)?.0.as_str());
            let listed: Vec<_> = categories
                .iter()
                .map(|id| (*id, None, server.categories[*id].position))
                .collect();
            numbered(&listed, None);
            // The channels follow their categories in their new order.
            let rank = |parent: Option<&str>| match parent
                .filter(|id| server.categories.contains_key(*id))
            {
                Some(id) => (
                    true,
                    categories
                        .iter()
                        .position(|known| *known == id)
                        .unwrap_or(usize::MAX),
                ),
                None => (false, 0),
            };
            for mut listed in [list(server.text_channels()), list(server.voice_channels())] {
                listed.sort_by_key(|(_, parent, _)| rank(*parent));
                numbered(&listed, None);
            }
        } else {
            let mut listed = match kind {
                Kind::Voice => list(server.voice_channels()),
                _ => list(server.text_channels()),
            };
            let from = listed.iter().position(|(id, _, _)| *id == dragged)?;
            let (_, was, position) = listed.remove(from);
            let index = |other: &str| listed.iter().position(|(id, _, _)| Some(*id) == raw(other));
            let (to, parent) = match landing {
                Landing::Before(other) => (index(other)?, listed[index(other)?].1),
                Landing::After(other) => (index(other)? + 1, listed[index(other)?].1),
                Landing::First(category) => {
                    let parent = category.as_deref().and_then(raw);
                    (
                        listed.partition_point(|(_, known, _)| key(*known) < key(parent)),
                        parent,
                    )
                }
                Landing::Last(category) => {
                    let parent = category.as_deref().and_then(raw);
                    (
                        listed.partition_point(|(_, known, _)| key(*known) <= key(parent)),
                        parent,
                    )
                }
            };
            let parent = parent.and_then(|id| {
                server
                    .categories
                    .get_key_value(id)
                    .map(|(id, _)| id.as_str())
            });
            listed.insert(to, (dragged, parent, position));
            let moved = (parent != was.filter(|id| server.categories.contains_key(*id)))
                .then_some((dragged, parent));
            numbered(&listed, moved);
        }
        // What this account may not manage stays out, as in Discord's client.
        let placements = placed
            .into_iter()
            .filter(|placement| {
                placement.id == dragged
                    || match kinds.get(placement.id.as_str()) {
                        Some((Kind::Category, _)) => true,
                        Some((_, parent)) => {
                            self.may(&manager(server, *parent), Permission::ManageChannels)
                        }
                        None => false,
                    }
            })
            .map(|placement| ChannelPlacement {
                room: discord::room_id(&server.id, &placement.id),
                position: placement.position,
                parent: placement
                    .parent
                    .map(|parent| parent.map(|id| discord::room_id(&server.id, &id))),
            })
            .collect();
        Some((discord::server_id(&server.id), placements))
    }
}

/// Each of a server's channels and categories by id, with which list it
/// sorts in and a channel's category.
#[cfg(feature = "discord")]
type Kinds<'a> = HashMap<&'a str, (Kind, Option<&'a str>)>;

#[cfg(feature = "discord")]
fn kinds(server: &MarkedServer) -> Kinds<'_> {
    let mut kinds = HashMap::with_capacity(
        server.text_channels.len() + server.voice_channels.len() + server.categories.len(),
    );
    for (kind, channels) in [
        (Kind::Text, &server.text_channels),
        (Kind::Voice, &server.voice_channels),
    ] {
        for channel in channels {
            kinds.insert(channel.id.as_str(), (kind, channel.parent.as_deref()));
        }
    }
    for id in server.categories.keys() {
        kinds.insert(id.as_str(), (Kind::Category, None));
    }
    kinds
}

/// A channel's or category's id, by its room id.
#[cfg(feature = "discord")]
fn raw(room: &str) -> Option<&str> {
    discord::parse_room(room).map(|(_, id)| id)
}

/// Where a channel's category comes as the list orders by it: those
/// outside any first, then each category by position.
#[cfg(feature = "discord")]
fn order<'a>(server: &MarkedServer, parent: Option<&'a str>) -> (bool, i64, &'a str) {
    match parent.and_then(|id| Some((id, server.categories.get(id)?))) {
        Some((id, category)) => (true, category.position, id),
        None => (false, 0, ""),
    }
}

/// A server's text or voice channels this account may see, in the order
/// the list shows them: each one's id, category and position.
#[cfg(feature = "discord")]
fn list<'a>(
    channels: impl Iterator<Item = MarkedChannel<'a>>,
) -> Vec<(&'a str, Option<&'a str>, i64)> {
    channels
        .filter(|channel| channel.marks.access != yapfast_discord::ready::Access::Hidden)
        .map(|MarkedChannel { channel, .. }| {
            (
                channel.id.as_str(),
                channel.parent.as_deref(),
                channel.position,
            )
        })
        .collect()
}

/// Where Discord's client asks whether this account may manage a channel
/// in `parent`: the category, or the server for one outside any.
#[cfg(feature = "discord")]
fn manager(server: &MarkedServer, parent: Option<&str>) -> String {
    match parent.filter(|id| server.categories.contains_key(*id)) {
        Some(id) => discord::room_id(&server.id, id),
        None => discord::server_id(&server.id),
    }
}

/// Where the row `dragged` lands on the row `onto`, by channel and
/// category ids rather than room ids; see [`Application::landing`].
#[cfg(feature = "discord")]
fn land<'a>(
    row: impl Fn(usize) -> Option<Row<'a>>,
    dragged: usize,
    onto: usize,
) -> Option<Landing> {
    use Kind::{Category, Text, Voice};
    if dragged == onto {
        return None;
    }
    let (moving, hovered) = (row(dragged)?, row(onto)?);
    let up = onto < dragged;
    let above = onto.checked_sub(1).and_then(&row);
    let below = row(onto + 1);
    let owned = |id: &str| id.to_owned();
    match (moving.kind, hovered.kind) {
        (Category, Category) if up => Some(Landing::Before(owned(hovered.id))),
        // Dragged down, a category goes below the last row of another: the
        // category itself, empty or collapsed, or its last channel.
        (Category, Category) => below
            .is_none_or(|next| next.kind == Category)
            .then(|| Landing::After(owned(hovered.id))),
        (Category, _) if up => None,
        (Category, _) => {
            let category = hovered.parent?;
            below
                .is_none_or(|next| next.parent != Some(category))
                .then(|| Landing::After(owned(category)))
                .filter(|_| category != moving.id)
        }
        // Dragged up onto a category, a channel goes last in the one above,
        // or among those outside any; dragged up onto the first row, first
        // of all.
        (_, Category) if up => match above {
            None => Some(Landing::First(None)),
            Some(previous) if previous.kind == Category => {
                Some(Landing::Last(Some(owned(previous.id))))
            }
            Some(previous)
                if previous.kind == moving.kind
                    || moving.kind == Voice && previous.kind == Text =>
            {
                Some(Landing::Last(previous.parent.map(owned)))
            }
            Some(_) => None,
        },
        // Dragged down onto a category, first in it, unless its first row
        // is of the other kind and lists the other way round.
        (_, Category) => match below {
            Some(next)
                if next.kind != Category
                    && next.parent == Some(hovered.id)
                    && next.kind != moving.kind
                    && !(moving.kind == Text && next.kind == Voice) =>
            {
                None
            }
            _ => Some(Landing::First(Some(owned(hovered.id)))),
        },
        (Text, Text) | (Voice, Voice) => Some(if up {
            Landing::Before(owned(hovered.id))
        } else {
            Landing::After(owned(hovered.id))
        }),
        // A text channel dragged up onto its category's first voice channel
        // goes last of its text channels; a voice channel dragged down onto
        // the last text channel, first of its voice channels.
        (Text, Voice) if up => above
            .is_none_or(|previous| previous.kind != Voice)
            .then(|| Landing::Last(hovered.parent.map(owned))),
        (Voice, Text) if !up => below
            .is_none_or(|next| next.kind != Text)
            .then(|| Landing::First(hovered.parent.map(owned))),
        _ => None,
    }
}
