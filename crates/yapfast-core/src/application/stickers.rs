//! The sticker picker, as Discord's client has it beside the GIFs and the
//! emoji: every server's stickers, Discord's default packs, and the account's
//! favorites and most sent. A click sends the sticker as a message of its own.
//!
//! Every server's stickers show, as the emoji picker shows every server's
//! emoji: the open server's list first, since they're usable there without
//! Nitro, and the rest greyed with a lock until Nitro. The default packs are
//! kept on disk for a day and the pictures are fetched ahead into the disk
//! cache at a start; the pictures decoded for the tiles are held only while
//! the picker is open.

use super::*;
#[cfg(feature = "discord")]
use crate::chat::Sticker;
use crate::chat::StickerCollection;

/// A picture the picker shows, by its address.
#[derive(Clone, Debug)]
pub enum StickerPicture {
    Loading,
    Shown(Arc<Picture>),
    Failed,
}

/// A sticker this device sent, and how often, which Frequently Used lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StickerUse {
    pub sticker: Sticker,
    pub uses: u32,
}

/// How many stickers Frequently Used offers, as Discord's client does.
pub const FREQUENT_STICKERS: usize = 8;

/// A server's own stickers, by its id.
#[derive(Clone, Debug, Default)]
pub struct ServerStickers {
    pub id: String,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, Debug, Default)]
pub struct StickerPicker {
    pub open: bool,
    /// The open room's server: its stickers are usable without Nitro and list
    /// first. A DM has none.
    pub server_for: Option<String>,
    /// Every server's own stickers that came, as the snapshot or Discord gave
    /// them.
    pub servers: Vec<ServerStickers>,
    /// What the search field says: the stickers whose names hold it list.
    pub query: String,
    /// The stickers this device sent, the most used first: Frequently Used.
    pub uses: Vec<StickerUse>,
    /// The stickers this device keeps, which Favorites lists first.
    pub favorites: Vec<Sticker>,
    /// Discord's default packs: those kept on disk until its answer comes.
    pub packs: Vec<StickerCollection>,
    /// Discord is being asked for the packs.
    pub loading: bool,
    /// Why the packs couldn't be fetched.
    pub error: Option<String>,
    /// The pictures of the tiles shown, and how to show them.
    pub pictures: HashMap<String, StickerPicture>,
    /// The collections whose pictures were fetched ahead, by id.
    prefetched: HashSet<String>,
    /// Discord was asked for the packs, so a snapshot doesn't ask again;
    /// opening the picker does, which retries what failed.
    pub(super) asked: bool,
    /// The servers asked for their own stickers this session.
    pub(super) asked_servers: HashSet<String>,
}

impl StickerPicker {
    /// The collections the picker lists: Favorites, Frequently Used, then
    /// every server's own stickers — the open server's first — and Discord's
    /// default packs. A search keeps the stickers whose names hold it. A
    /// sticker this account may not use where it is looking is `usable`
    /// false, which the picker shows greyed with a lock.
    pub fn collections(&self, nitro: bool) -> Vec<StickerCollection> {
        let search = self.query.trim().to_lowercase();
        let here = self.server_for.as_deref();
        // The open server's sticker ids: its stickers are the ones a picker
        // offers without Nitro, wherever they are listed.
        let here_ids: HashSet<&str> = here
            .and_then(|here| self.servers.iter().find(|server| server.id == here))
            .map(|server| {
                server
                    .stickers
                    .iter()
                    .map(|sticker| sticker.id.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let mut collections = vec![];
        let mut push = |collection: &StickerCollection, usable: &dyn Fn(&Sticker) -> bool| {
            let stickers: Vec<Sticker> = collection
                .stickers
                .iter()
                .filter(|sticker| {
                    search.is_empty() || sticker.name.to_lowercase().contains(&search)
                })
                .map(|sticker| Sticker {
                    usable: usable(sticker),
                    ..sticker.clone()
                })
                .collect();
            if stickers.is_empty() {
                return;
            }
            collections.push(StickerCollection {
                id: collection.id.clone(),
                name: collection.name.clone(),
                stickers,
            });
        };
        // What this device keeps and sends is usable where its server's
        // stickers are, or with Nitro.
        let usable = |sticker: &Sticker| nitro || here_ids.contains(sticker.id.as_str());
        for collection in [self.favorite_collection(), self.frequent()]
            .into_iter()
            .flatten()
        {
            push(&collection, &usable);
        }
        let mut servers: Vec<&ServerStickers> = self
            .servers
            .iter()
            .filter(|server| !server.stickers.is_empty())
            .collect();
        servers
            .sort_by_key(|server| (here != Some(server.id.as_str()), server.name.to_lowercase()));
        for server in servers {
            let collection = StickerCollection {
                id: server.id.clone(),
                name: server.name.clone(),
                stickers: server.stickers.clone(),
            };
            push(&collection, &|_| nitro || here == Some(server.id.as_str()));
        }
        // Discord's packs are Nitro's.
        for pack in &self.packs {
            push(pack, &|_| nitro);
        }
        collections
    }

    /// The stickers this device keeps, which Favorites lists first.
    fn favorite_collection(&self) -> Option<StickerCollection> {
        (!self.favorites.is_empty()).then(|| StickerCollection {
            id: "favorites".into(),
            name: "Favorites".into(),
            stickers: self.favorites.clone(),
        })
    }

    /// The stickers this device sent, the most used first.
    fn frequent(&self) -> Option<StickerCollection> {
        let mut used: Vec<&StickerUse> = self.uses.iter().filter(|used| used.uses > 0).collect();
        used.sort_by(|a, b| {
            b.uses
                .cmp(&a.uses)
                .then_with(|| a.sticker.name.cmp(&b.sticker.name))
        });
        let mut stickers: Vec<Sticker> = used
            .into_iter()
            .take(FREQUENT_STICKERS)
            .map(|used| used.sticker.clone())
            .collect();
        if stickers.is_empty() {
            stickers = self
                .packs
                .iter()
                .flat_map(|pack| &pack.stickers)
                .take(FREQUENT_STICKERS)
                .cloned()
                .collect();
        }
        (!stickers.is_empty()).then(|| StickerCollection {
            id: "frequent".into(),
            name: "Frequently Used".into(),
            stickers,
        })
    }
}

impl AppState {
    /// The sticker picker is offered: in a Discord room this account can
    /// write in, as the GIFs are.
    pub fn can_pick_stickers(&self) -> bool {
        self.can_pick_gifs()
    }
}

impl Application {
    pub(super) fn open_stickers(&mut self) -> Vec<Effect> {
        // The stickers load even before the room says this account may write
        // in it: a channel's permissions come with its snapshot, and the
        // picker can open before it. The button that opens it is offered only
        // where it may.
        self.follow_sticker_server();
        self.state.stickers.open = true;
        self.state.stickers.asked = false;
        let mut effects = self.load_sticker_packs();
        effects.extend(self.load_guild_stickers());
        effects.extend(self.prefetch_stickers());
        effects
    }

    /// Closes the picker, letting go of the pictures it decoded.
    pub(super) fn close_stickers(&mut self) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        picker.open = false;
        picker.pictures.clear();
        vec![]
    }

    /// Takes the open room's server, and the stickers the snapshot carries of
    /// every server, so they list before Discord answers.
    pub(super) fn follow_sticker_server(&mut self) {
        self.state.stickers.server_for = self.state.selected.as_deref().and_then(room_server);
        for (id, name, stickers) in self.snapshot_stickers() {
            let picker = &mut self.state.stickers;
            match picker.servers.iter_mut().find(|server| server.id == id) {
                Some(server) => {
                    server.name = name;
                    if server.stickers.is_empty() {
                        server.stickers = stickers;
                    }
                }
                None => picker.servers.push(ServerStickers { id, name, stickers }),
            }
        }
    }

    /// Every server this account is in, its name and the stickers the
    /// snapshot gave it, by id.
    #[cfg(feature = "discord")]
    fn snapshot_stickers(&self) -> Vec<(String, String, Vec<Sticker>)> {
        let Some(snapshot) = &self.discord else {
            return vec![];
        };
        snapshot
            .servers
            .iter()
            .map(|server| {
                let stickers = server
                    .stickers
                    .iter()
                    .map(|sticker| Sticker {
                        picture: yapfast_discord::ready::sticker_at(
                            &sticker.id,
                            sticker.format,
                            yapfast_discord::stickers::TILE_SIZE,
                        ),
                        id: sticker.id.clone(),
                        name: sticker.name.clone(),
                        format: sticker.format,
                        usable: true,
                    })
                    .collect();
                (server.id.clone(), server.name.clone(), stickers)
            })
            .collect()
    }

    #[cfg(not(feature = "discord"))]
    fn snapshot_stickers(&self) -> Vec<(String, String, Vec<Sticker>)> {
        vec![]
    }

    /// The name of the server `id`, as the snapshot has it.
    fn server_name(&self, id: &str) -> String {
        self.snapshot_stickers()
            .into_iter()
            .find(|(server, _, _)| server == id)
            .map(|(_, name, _)| name)
            .unwrap_or_default()
    }

    /// Asks Discord for its default sticker packs, unless Yapfast has them
    /// for this session or is asking already.
    pub(super) fn load_sticker_packs(&mut self) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        if picker.loading || picker.asked || !picker.packs.is_empty() {
            return vec![];
        }
        picker.asked = true;
        picker.loading = true;
        picker.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::StickerPacks,
        ))]
    }

    /// Asks Discord for every server's own stickers it hasn't asked for this
    /// session: each server's list is its own request, answered from the disk
    /// when it came within a day.
    pub(super) fn load_guild_stickers(&mut self) -> Vec<Effect> {
        let mut effects = vec![];
        for (server, _, _) in self.snapshot_stickers() {
            if !self.state.stickers.asked_servers.insert(server.clone()) {
                continue;
            }
            tracing::info!(server = %server, "asking for a server's stickers");
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::GuildStickers { server },
            )));
        }
        effects
    }

    /// The default packs as Discord answered them.
    pub(super) fn sticker_packs_came(
        &mut self,
        result: Result<Vec<StickerCollection>, String>,
    ) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        picker.loading = false;
        match result {
            Ok(packs) => {
                picker.packs = packs;
                picker.error = None;
            }
            // What Yapfast kept stays, if anything did.
            Err(error) => picker.error = Some(error),
        }
        self.prefetch_stickers()
    }

    /// The default packs as Yapfast kept them, shown until Discord's come.
    pub(super) fn kept_sticker_packs_came(&mut self, packs: Vec<StickerCollection>) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        picker.loading = false;
        if !picker.packs.is_empty() {
            return vec![];
        }
        picker.packs = packs;
        // A start shows these without Discord's answer, so their pictures are
        // fetched ahead here too.
        self.prefetch_stickers()
    }

    /// A server's own stickers as Yapfast kept them, shown until Discord's
    /// answer comes.
    pub(super) fn kept_guild_stickers_came(&mut self, server: String, stickers: Vec<Sticker>) {
        if stickers.is_empty() {
            return;
        }
        let name = self.server_name(&server);
        let picker = &mut self.state.stickers;
        match picker.servers.iter_mut().find(|known| known.id == server) {
            // What the snapshot carried may be newer than the disk.
            Some(known) if !known.stickers.is_empty() => {}
            Some(known) => known.stickers = stickers,
            None => picker.servers.push(ServerStickers {
                id: server,
                name,
                stickers,
            }),
        }
    }

    /// A server's own stickers as Discord answered them.
    pub(super) fn guild_stickers_came(
        &mut self,
        server: String,
        result: Result<Vec<Sticker>, String>,
    ) -> Vec<Effect> {
        let name = self.server_name(&server);
        let picker = &mut self.state.stickers;
        match result {
            Ok(stickers) => {
                tracing::info!(
                    server = %server,
                    count = stickers.len(),
                    "a server's stickers came"
                );
                match picker.servers.iter_mut().find(|known| known.id == server) {
                    Some(known) => {
                        known.name = name;
                        known.stickers = stickers;
                    }
                    None => picker.servers.push(ServerStickers {
                        id: server.clone(),
                        name,
                        stickers,
                    }),
                }
                // The list changed, so its pictures are fetched ahead again.
                picker.prefetched.remove(&server);
                self.prefetch_stickers()
            }
            Err(error) => {
                tracing::info!(server = %server, %error, "a server's stickers failed");
                picker.error = Some(error);
                // The next open asks again rather than never.
                picker.asked_servers.remove(&server);
                vec![]
            }
        }
    }

    /// Takes the server's stickers as a change gave them, when the picker
    /// lists that server.
    pub(super) fn follow_picker_stickers(
        &mut self,
        server: &str,
        stickers: &[crate::chat::ServerSticker],
    ) {
        #[cfg(feature = "discord")]
        {
            let Some(guild) = server.strip_prefix("discord:") else {
                return;
            };
            let stickers: Vec<Sticker> = stickers
                .iter()
                .filter(|sticker| sticker.available)
                .map(|sticker| Sticker {
                    picture: yapfast_discord::ready::sticker_at(
                        &sticker.id,
                        sticker.format.into(),
                        yapfast_discord::stickers::TILE_SIZE,
                    ),
                    id: sticker.id.clone(),
                    name: sticker.form.name.clone(),
                    format: sticker.format.into(),
                    usable: true,
                })
                .collect();
            let name = self.server_name(guild);
            let picker = &mut self.state.stickers;
            match picker.servers.iter_mut().find(|known| known.id == guild) {
                Some(known) => known.stickers = stickers,
                None => picker.servers.push(ServerStickers {
                    id: guild.to_owned(),
                    name,
                    stickers,
                }),
            }
        }
        #[cfg(not(feature = "discord"))]
        let _ = (server, stickers);
    }

    /// Fetches the pictures of every sticker the picker offers into the disk
    /// cache, ahead of it showing them, once a collection: nothing is decoded
    /// or held, and a tile then reads its picture from the disk.
    fn prefetch_stickers(&mut self) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        let mut wanted = vec![];
        for pack in &picker.packs {
            if picker.prefetched.insert(pack.id.clone()) {
                wanted.extend(pictures(pack));
            }
        }
        for server in &picker.servers {
            if picker.prefetched.insert(server.id.clone()) {
                wanted.extend(
                    server
                        .stickers
                        .iter()
                        .map(|sticker| sticker.picture.clone()),
                );
            }
        }
        for sticker in &picker.favorites {
            if picker.prefetched.insert(format!("favorite:{}", sticker.id)) {
                wanted.push(sticker.picture.clone());
            }
        }
        // What the disk already holds needs no download: the check is a stat,
        // and keeps a cache that lost a picture from staying short one.
        #[cfg(feature = "discord")]
        wanted.retain(|url| !yapfast_discord::cache::picture_is_kept(url));
        (!wanted.is_empty())
            .then_some(Effect::Backend(Command::Discord(
                DiscordCommand::PrefetchPictures(wanted),
            )))
            .into_iter()
            .collect()
    }

    /// The pictures of the picker's tiles in sight: those are asked for and
    /// the rest let go, so a tile scrolled away or a closed picker costs no
    /// memory.
    pub(super) fn stickers_shown(&mut self, shown: Vec<String>) -> Vec<Effect> {
        let picker = &mut self.state.stickers;
        if !picker.open {
            return vec![];
        }
        picker.pictures.retain(|url, _| shown.contains(url));
        let mut effects = vec![];
        for url in shown {
            if url.is_empty() || picker.pictures.contains_key(&url) {
                continue;
            }
            picker.pictures.insert(url.clone(), StickerPicture::Loading);
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::StickerPicture { url },
            )));
        }
        effects
    }

    /// A sticker's picture came, or failed to.
    pub(super) fn sticker_picture_came(&mut self, url: String, picture: Option<Arc<Picture>>) {
        let picker = &mut self.state.stickers;
        let Some(shown) = picker.pictures.get_mut(&url) else {
            return;
        };
        *shown = match picture {
            Some(picture) => StickerPicture::Shown(picture),
            None => StickerPicture::Failed,
        };
    }

    /// A sticker clicked: sends it as a message of its own in the open room,
    /// and the picker closes. One this account may not use goes nothing, as
    /// the picker shows it locked.
    pub(super) fn pick_sticker(&mut self, id: String) -> Vec<Effect> {
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        if !self.state.can_pick_stickers() {
            return vec![];
        }
        // Frequently Used counts it, by id; the server's list or a pack names
        // the one picked.
        let picked = self
            .state
            .stickers
            .collections(self.discord_nitro())
            .into_iter()
            .flat_map(|collection| collection.stickers)
            .find(|sticker| sticker.id == id);
        let Some(sticker) = picked.filter(|sticker| sticker.usable) else {
            return vec![];
        };
        let uses = &mut self.state.stickers.uses;
        match uses.iter_mut().find(|used| used.sticker.id == sticker.id) {
            Some(used) => used.uses = used.uses.saturating_add(1),
            None => uses.push(StickerUse {
                sticker: sticker.clone(),
                uses: 1,
            }),
        }
        let mut effects = self.close_stickers();
        effects.push(Effect::Backend(Command::Discord(
            DiscordCommand::SendSticker { room, id },
        )));
        effects
    }

    /// The search field changed.
    pub(super) fn search_stickers(&mut self, query: String) -> Vec<Effect> {
        self.state.stickers.query = query;
        vec![]
    }

    /// A sticker's star: keeps it among the favorites, or lets it go.
    pub(super) fn favorite_sticker(&mut self, id: String) -> Vec<Effect> {
        if let Some(index) = self
            .state
            .stickers
            .favorites
            .iter()
            .position(|sticker| sticker.id == id)
        {
            self.state.stickers.favorites.remove(index);
            return vec![];
        }
        let nitro = self.discord_nitro();
        let Some(sticker) = self
            .state
            .stickers
            .collections(nitro)
            .into_iter()
            .flat_map(|collection| collection.stickers)
            .find(|sticker| sticker.id == id)
        else {
            return vec![];
        };
        self.state.stickers.favorites.push(sticker);
        vec![]
    }

    /// A sticker was sent, or why it couldn't be.
    pub(super) fn sticker_sent(&mut self, room: String, result: Result<(), String>) {
        if self.state.selected.as_ref() != Some(&room) {
            return;
        }
        if let Err(error) = result {
            self.state.notice = Some(error);
        }
    }

    /// The stickers this device sent, the most used first, for an adapter to
    /// keep across runs.
    pub fn sticker_uses(&self) -> &[StickerUse] {
        &self.state.stickers.uses
    }

    /// Restores what [`Self::sticker_uses`] kept, keeping any higher count.
    pub fn remember_sticker_uses(&mut self, uses: Vec<StickerUse>) {
        for used in uses {
            match self
                .state
                .stickers
                .uses
                .iter_mut()
                .find(|known| known.sticker.id == used.sticker.id)
            {
                Some(known) => known.uses = known.uses.max(used.uses),
                None => self.state.stickers.uses.push(used),
            }
        }
    }

    /// The stickers this device keeps, for an adapter to keep across runs.
    pub fn sticker_favorites(&self) -> &[Sticker] {
        &self.state.stickers.favorites
    }

    /// Restores what [`Self::sticker_favorites`] kept.
    pub fn remember_sticker_favorites(&mut self, favorites: Vec<Sticker>) {
        let kept = &mut self.state.stickers.favorites;
        for sticker in favorites {
            if !kept.iter().any(|known| known.id == sticker.id) {
                kept.push(sticker);
            }
        }
    }
}

/// The server the room `id` names, if it is in one: a DM's room names `@me`
/// where a server's names its id, and a DM has no server stickers.
pub(super) fn room_server(id: &str) -> Option<String> {
    let (guild, _) = discord::parse_room(id)?;
    guild
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| guild.to_owned())
}

/// The pictures of a collection's stickers, at the size the picker's tiles
/// show them.
fn pictures(collection: &StickerCollection) -> impl Iterator<Item = String> + '_ {
    collection
        .stickers
        .iter()
        .map(|sticker| sticker.picture.clone())
}

#[cfg(test)]
mod tests {
    use super::room_server;

    #[test]
    fn a_dm_names_no_server_for_stickers() {
        // A server's room names the server before its channel; a DM's names
        // `@me`, which is no server to ask for stickers.
        assert_eq!(
            room_server("discord:1400000000000000100:1400000000000000102"),
            Some("1400000000000000100".into())
        );
        assert_eq!(room_server("discord:@me:1400000000000000120"), None);
        assert_eq!(room_server("matrix:!room:example.org"), None);
    }
}
