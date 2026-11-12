//! Profile cards, the profile beside a DM, and role cards.

use super::*;
use crate::chat::{ProfileWidget, Wishlist};

impl Application {
    /// Shows someone in the open conversation, or else a Discord friend.
    pub(super) fn show_profile(&mut self, user: String, large: bool) -> Vec<Effect> {
        let (Some(room), Some(member)) = (self.state.selected_room(), self.state.person(&user))
        else {
            let effects = self.show_friend_profile(user.clone(), large);
            if self
                .state
                .profile
                .as_ref()
                .is_some_and(|view| view.profile.id == user)
            {
                return effects;
            }
            return self.show_linked_discord_profile(user, large);
        };
        let member = &member;
        let discord = room.network == Network::Discord;
        let role = match member.role {
            Role::Admin => Some("Admin"),
            Role::Moderator => Some("Moderator"),
            Role::Member => None,
        };
        let profile = Profile {
            id: member.id.clone(),
            name: member.name.clone(),
            // A Discord username comes with the profile.
            handle: if discord {
                String::new()
            } else {
                member.id.clone()
            },
            avatar: member.avatar.clone(),
            decoration: member.decoration.clone(),
            style: member.style.clone(),
            presence: member.presence,
            activity: member.activity.clone(),
            activities: member.activities.clone(),
            // Discord's roles come with the profile too.
            roles: role
                .filter(|_| !discord)
                .map(|name| ProfileRole {
                    name: name.into(),
                    color: None,
                })
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let room = room.id.clone();
        self.open_profile(room, profile, large)
    }

    /// Opens a Discord profile named only by its user id, as behind a
    /// `/users/{id}` link. The id stands in as the name until the fetch
    /// fills the card; the open room when it is Discord's is the context
    /// for a server profile, and a DM makes Discord answer with their
    /// main profile when there is no such room.
    fn show_linked_discord_profile(&mut self, user: String, large: bool) -> Vec<Effect> {
        if !self.state.discord_online
            || user.is_empty()
            || !user.bytes().all(|byte| byte.is_ascii_digit())
        {
            return vec![];
        }
        let room = self
            .state
            .selected_room()
            .filter(|room| room.network == Network::Discord)
            .map(|room| room.id.clone())
            .or_else(|| {
                self.state
                    .rooms
                    .iter()
                    .find(|room| room.network == Network::Discord && room.direct)
                    .map(|room| room.id.clone())
            })
            .or_else(|| {
                self.state
                    .rooms
                    .iter()
                    .find(|room| room.network == Network::Discord)
                    .map(|room| room.id.clone())
            });
        let Some(room) = room else {
            return vec![];
        };
        self.open_profile(
            room,
            Profile {
                id: user.clone(),
                name: user,
                ..Default::default()
            },
            large,
        )
    }

    pub(super) fn show_dm_profile(&mut self, id: String) -> Vec<Effect> {
        let Some((room, person)) = self
            .state
            .rooms
            .iter()
            .filter(|room| room.id == id)
            .find_map(|room| Some((room, room.person.clone()?)))
        else {
            return vec![];
        };
        let profile = dm_profile(room, person);
        self.open_profile(id, profile, true)
    }

    pub(super) fn show_friend_profile(&mut self, user: String, large: bool) -> Vec<Effect> {
        let listed = self
            .state
            .profile
            .iter()
            .chain(&self.state.panel_profile)
            .flat_map(|view| &view.profile.friends)
            .find(|friend| friend.id == user);
        let friend = self.state.friends.iter().find(|friend| friend.id == user);
        let Some(name) = friend
            .map(|friend| &friend.name)
            .or(listed.map(|listed| &listed.name))
        else {
            return vec![];
        };
        let profile = Profile {
            id: user.clone(),
            name: name.clone(),
            avatar: friend
                .and_then(|friend| friend.avatar.clone())
                .or_else(|| listed.and_then(|listed| listed.avatar.clone())),
            presence: friend.and_then(|friend| friend.presence),
            activity: friend.and_then(|friend| friend.activity.clone()),
            activities: friend
                .map(|friend| friend.activities.clone())
                .unwrap_or_default(),
            ..Default::default()
        };
        // Their DM's profile is their own; elsewhere, the open
        // server's, as Discord shows someone clicked there.
        let room = self
            .state
            .rooms
            .iter()
            .find(|room| room.direct && room.person.as_ref() == Some(&user))
            .map(|room| room.id.clone())
            .or_else(|| self.state.selected.clone());
        let Some(room) = room else {
            return vec![];
        };
        self.open_profile(room, profile, large)
    }

    pub(super) fn show_role(&mut self, role: String) -> Vec<Effect> {
        let Some(room) = self.state.selected.clone() else {
            return vec![];
        };
        // Named as the message shows it, until the backend answers.
        let name = self
            .state
            .messages
            .iter()
            .flat_map(|message| &message.mentions)
            .find(|mention| mention.target == Target::Role(role.clone()))
            .map(|mention| mention.text.trim_start_matches('@').to_owned())
            .unwrap_or_default();
        self.state.role = Some(RoleView {
            id: role.clone(),
            name,
            color: None,
            members: None,
        });
        if self.state.preview || Network::of(&room) != Network::Discord {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::RoleMembers { room, role },
        ))]
    }

    /// Opens a profile card with what Yapfast knows, and fetches the rest of a
    /// Discord profile.
    pub(super) fn open_profile(
        &mut self,
        room: String,
        profile: Profile,
        large: bool,
    ) -> Vec<Effect> {
        let fetch = Network::of(&room) == Network::Discord && !self.state.preview;
        let user = profile.id.clone();
        self.state.profile = Some(ProfileView {
            profile,
            large,
            loading: fetch,
            kept: false,
            note: None,
            ..Default::default()
        });
        if fetch {
            vec![Effect::Backend(Command::Discord(DiscordCommand::Profile {
                room,
                user,
                effect: self.state.cosmetics.effects,
                frame: self.state.cosmetics.frames,
            }))]
        } else {
            vec![]
        }
    }

    /// Fills in a profile on screen with the rest Discord sent, or says it
    /// didn't, unless it shows the one kept from before.
    pub(super) fn show_fetched_profile(
        &mut self,
        user: String,
        result: Result<Profile, String>,
    ) -> Vec<Effect> {
        if let Ok(profile) = &result {
            self.keep_own_profile(&user, profile);
        }
        for view in self.loading_profiles(&user) {
            view.loading = false;
            match &result {
                Ok(profile) => fill_profile(view, profile),
                Err(_) if view.kept => {}
                Err(_) => {
                    view.note = Some("Discord didn't share the rest of this profile.".into());
                }
            }
            view.kept = false;
        }
        self.ask_open_tabs()
    }

    /// Fills in a profile on screen with the one kept from before, until
    /// Discord's comes.
    pub(super) fn show_kept_profile(&mut self, user: String, profile: Profile) -> Vec<Effect> {
        self.keep_own_profile(&user, &profile);
        for view in self.loading_profiles(&user) {
            fill_profile(view, &profile);
            view.kept = true;
        }
        self.ask_open_tabs()
    }

    /// Asks for the account's own profile, which its card draws from: the
    /// main one, outside any server.
    pub(super) fn load_own_profile(&mut self) -> Vec<Effect> {
        let Some(user) = self
            .state
            .discord_user
            .clone()
            .filter(|_| !self.state.preview)
        else {
            return vec![];
        };
        let room = crate::discord::room_id(crate::discord::DM_GUILD, "0");
        vec![Effect::Backend(Command::Discord(DiscordCommand::Profile {
            room,
            user,
            effect: false,
            frame: false,
        }))]
    }

    /// Keeps a profile that's the account's own for its card: the main one.
    /// A server's says when they joined it, and its banner, bio and theme
    /// win over the main profile's, so a card opened in a server is left out.
    fn keep_own_profile(&mut self, user: &str, profile: &Profile) {
        if self.state.discord_user.as_deref() == Some(user) && profile.joined.is_none() {
            self.state.account_profile = Some(Arc::new(profile.clone()));
        }
    }

    /// A tab of the full profile opened: it asks Discord once for what it
    /// shows, and only for the full profile, never for a card.
    pub(super) fn open_profile_tab(&mut self, tab: ProfileTab) -> Vec<Effect> {
        let Some(view) = self.state.profile.as_mut().filter(|view| view.large) else {
            return vec![];
        };
        match tab {
            ProfileTab::Board => view.board_open = true,
            ProfileTab::Activity => view.activity_open = true,
            ProfileTab::Wishlist => view.wishlist_open = true,
        }
        self.ask_open_tabs()
    }

    /// What the full profile's open tabs still want from Discord. A wishlist
    /// that failed is asked for again when the tab or the profile comes back.
    fn ask_open_tabs(&mut self) -> Vec<Effect> {
        let preview = self.state.preview;
        let Some(view) = self.state.profile.as_mut().filter(|view| view.large) else {
            return vec![];
        };
        let mut effects = vec![];
        let profile = &mut view.profile;
        // Matrix IDs start with `@`; only Discord has these.
        if preview || profile.id.starts_with('@') {
            return effects;
        }
        if view.activity_open && !profile.recent_asked && !view.loading {
            profile.recent_asked = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ProfileRecent {
                    user: profile.id.clone(),
                },
            )));
        }
        if view.board_open && !profile.board_asked && names_games(&profile.widgets) {
            profile.board_asked = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ProfileBoard {
                    user: profile.id.clone(),
                    widgets: profile.widgets.clone(),
                },
            )));
        }
        if view.wishlist_open && matches!(profile.wishlist, Wishlist::Unasked | Wishlist::Failed) {
            match &profile.wishlist_place {
                Some(place) => {
                    let id = place.id.clone();
                    profile.wishlist = Wishlist::Loading;
                    effects.push(Effect::Backend(Command::Discord(
                        DiscordCommand::Wishlist { id },
                    )));
                }
                // The profile on its way may yet say where it is.
                None if view.loading => {}
                None => profile.wishlist = Wishlist::Loaded(Vec::new()),
            }
        }
        effects
    }

    /// A wishlist came, for the full profile if it's the one it asked for.
    pub(super) fn profile_wishlist_loaded(&mut self, id: &str, wishlist: &Wishlist) {
        if let Some(view) = self.state.profile.as_mut()
            && view
                .profile
                .wishlist_place
                .as_ref()
                .is_some_and(|place| place.id == id)
            && view.profile.wishlist == Wishlist::Loading
        {
            view.profile.wishlist = wishlist.clone();
        }
    }

    pub(super) fn profile_recent_came(&mut self, user: &str, recent: Vec<chat::RecentActivity>) {
        if let Some(view) = self
            .state
            .profile
            .as_mut()
            .filter(|view| view.profile.id == user)
        {
            view.profile.recent = recent;
            view.profile.recent_came = true;
        }
    }

    pub(super) fn profile_board_named(&mut self, user: &str, widgets: Vec<ProfileWidget>) {
        let editor = self
            .state
            .profile_editor
            .as_mut()
            .and_then(|editor| editor.profile.as_mut())
            .map(|profile| &mut profile.fallback);
        let view = self.state.profile.as_mut().map(|view| &mut view.profile);
        for profile in [view, editor]
            .into_iter()
            .flatten()
            .filter(|profile| profile.id == user && profile.board_asked)
        {
            profile.widgets.clone_from(&widgets);
            profile.board_named = true;
        }
    }

    /// Gives the profiles on screen of `user` the levels of their badges, which
    /// come after the rest.
    pub(super) fn show_badge_levels(&mut self, user: &str, badges: Vec<crate::chat::ProfileBadge>) {
        for view in [&mut self.state.profile, &mut self.state.panel_profile]
            .into_iter()
            .filter_map(Option::as_mut)
            .filter(|view| view.profile.id == user && !view.loading)
        {
            view.profile.badges.clone_from(&badges);
        }
    }

    /// Puts the frame the shop described on the profiles of `user` on screen.
    pub(super) fn show_profile_frame(
        &mut self,
        user: &str,
        frame: &Arc<[crate::chat::ProfileFrameLayer]>,
    ) {
        for view in [&mut self.state.profile, &mut self.state.panel_profile]
            .into_iter()
            .filter_map(Option::as_mut)
            .filter(|view| view.profile.id == user && !view.loading)
        {
            view.profile.frame = frame.clone();
        }
    }

    /// Saves this account's note on a Discord user, which the profiles on
    /// screen show at once; Discord keeps no empty notes.
    pub(super) fn save_note(&mut self, user: String, note: String) -> Vec<Effect> {
        if self.state.discord_user.is_none() || user.starts_with('@') {
            return vec![];
        }
        let note = note.trim().to_owned();
        for view in [&mut self.state.profile, &mut self.state.panel_profile]
            .into_iter()
            .flatten()
            .filter(|view| view.profile.id == user)
        {
            view.profile.note.clone_from(&note);
        }
        vec![Effect::Backend(Command::Discord(DiscordCommand::SetNote {
            user,
            note,
        }))]
    }

    /// The profiles on screen of `user` waiting for Discord's.
    fn loading_profiles(&mut self, user: &str) -> impl Iterator<Item = &mut ProfileView> {
        [&mut self.state.profile, &mut self.state.panel_profile]
            .into_iter()
            .filter_map(Option::as_mut)
            .filter(move |view| view.profile.id == user && view.loading)
    }
}

/// Whether a board has games or applications to name.
pub(super) fn names_games(widgets: &[ProfileWidget]) -> bool {
    widgets
        .iter()
        .any(|widget| !widget.games.is_empty() || widget.application.is_some())
}

/// Puts `profile` on screen in `view`. Presence and activities come from the
/// member list; the profile doesn't carry them.
fn fill_profile(view: &mut ProfileView, profile: &Profile) {
    let note = std::mem::take(&mut view.profile.note);
    // What a tab already asked for stays, as long as it's the same list.
    let same_place = view.profile.wishlist_place == profile.wishlist_place;
    let same_board = same_board(&view.profile, profile);
    let mut filled = Profile {
        note: if profile.note.is_empty() {
            note
        } else {
            profile.note.clone()
        },
        presence: profile.presence.or(view.profile.presence),
        activity: profile.activity.clone().or(view.profile.activity.take()),
        activities: std::mem::take(&mut view.profile.activities),
        ..profile.clone()
    };
    filled.recent = std::mem::take(&mut view.profile.recent);
    filled.recent_asked = view.profile.recent_asked;
    filled.recent_came = view.profile.recent_came;
    if same_place {
        filled.wishlist = std::mem::take(&mut view.profile.wishlist);
    }
    if same_board {
        keep_board(&mut filled, &mut view.profile);
    }
    view.profile = filled;
}

/// Whether `new` has the board `old` already asked Discord to name.
pub(super) fn same_board(old: &Profile, new: &Profile) -> bool {
    let game_ids = |widgets: &[ProfileWidget]| -> Vec<String> {
        widgets
            .iter()
            .flat_map(|widget| &widget.games)
            .map(|game| game.id.clone())
            .collect()
    };
    old.board_asked
        && old.widgets.len() == new.widgets.len()
        && game_ids(&old.widgets) == game_ids(&new.widgets)
}

/// Moves the board `old` asked to name, and its names, to `new`.
pub(super) fn keep_board(new: &mut Profile, old: &mut Profile) {
    new.widgets = std::mem::take(&mut old.widgets);
    new.board_asked = old.board_asked;
    new.board_named = old.board_named;
}

/// What a one-to-one DM knows of its other person's profile.
pub(super) fn dm_profile(room: &ChatRoom, person: String) -> Profile {
    Profile {
        name: room.name.clone(),
        // A Discord username comes with the profile.
        handle: if room.network == Network::Discord {
            String::new()
        } else {
            person.clone()
        },
        avatar: room.avatar.clone(),
        decoration: room.decoration.clone(),
        style: room.style.clone(),
        presence: room.presence,
        activity: room.activity.clone(),
        activities: room.activities.clone(),
        id: person,
        ..Default::default()
    }
}
