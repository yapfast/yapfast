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
                collectibles: self.state.cosmetics.collectibles(),
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
        if view.activity_open && !preview && !profile.recent_asked && !view.loading {
            profile.recent_asked = true;
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::ProfileRecent {
                    user: profile.id.clone(),
                },
            )));
        }
        if view.board_open && !preview && !profile.board_asked {
            let named = profile
                .widgets
                .iter()
                .any(|widget| !widget.games.is_empty() || widget.application.is_some());
            if named {
                profile.board_asked = true;
                effects.push(Effect::Backend(Command::Discord(
                    DiscordCommand::ProfileBoard {
                        user: profile.id.clone(),
                        widgets: profile.widgets.clone(),
                    },
                )));
            }
        }
        if view.wishlist_open
            && !preview
            && matches!(profile.wishlist, Wishlist::Unasked | Wishlist::Failed)
        {
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
        }
    }

    pub(super) fn profile_board_named(&mut self, user: &str, widgets: Vec<ProfileWidget>) {
        if let Some(view) = self
            .state
            .profile
            .as_mut()
            .filter(|view| view.profile.id == user)
        {
            view.profile.widgets = widgets;
            view.profile.board_named = true;
        }
    }

    /// Opens someone's badge window, on the badge clicked; this account's goes
    /// to its own window, as the client's does. Asks for this account's too,
    /// which says what the window offers it.
    pub(super) fn show_person_badges(&mut self, user: String, badge: Option<u64>) -> Vec<Effect> {
        if self.state.account_id.as_deref() == Some(user.as_str()) {
            self.state.badges_initial = badge;
            return self.show_own_badges();
        }
        let Some(view) = self
            .state
            .profile
            .as_ref()
            .filter(|view| view.profile.id == user)
        else {
            return vec![];
        };
        let name = view.profile.name.clone();
        let avatar = view.profile.avatar.clone();
        let preview = self.state.preview;
        self.state.person_badges = Some(PersonBadges {
            user: user.clone(),
            name,
            avatar,
            badge,
            error: preview.then(|| "Nothing to show in the preview.".to_owned()),
            badges: None,
        });
        if preview {
            return vec![];
        }
        let mut effects = vec![Effect::Backend(Command::Discord(
            DiscordCommand::PersonBadges { user },
        ))];
        if self.state.badges.is_none() {
            effects.push(Effect::Backend(Command::Discord(
                DiscordCommand::BadgeDirectory,
            )));
        }
        effects
    }

    pub(super) fn person_badges_loaded(
        &mut self,
        user: &str,
        result: Result<Vec<crate::chat::DirectoryBadge>, String>,
    ) {
        if let Some(view) = self
            .state
            .person_badges
            .as_mut()
            .filter(|view| view.user == user)
        {
            match result {
                Ok(badges) => {
                    view.badges = Some(badges);
                    view.error = None;
                }
                Err(error) => view.error = Some(error),
            }
        }
    }

    /// Opens the account's badge window, which asks Discord each time; the
    /// preview's is empty.
    pub(super) fn show_own_badges(&mut self) -> Vec<Effect> {
        self.state.badges_open = true;
        self.state.badges_error = None;
        if self.state.preview {
            self.state.badges = Some(Vec::new());
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::BadgeDirectory,
        ))]
    }

    /// Asks for the account's own profile, which its popout draws from.
    pub(super) fn load_own_profile(&mut self) -> Vec<Effect> {
        let Some(user) = self.state.account_id.clone() else {
            return vec![];
        };
        // A private place, so it's their own profile and not a server's.
        let room = crate::discord::room_id(yapfast_discord::ready::PRIVATE, "0");
        vec![Effect::Backend(Command::Discord(DiscordCommand::Profile {
            room,
            user,
            collectibles: self.state.cosmetics.collectibles(),
        }))]
    }

    fn keep_own_profile(&mut self, user: &str, profile: &Profile) {
        if self.state.account_id.as_deref() == Some(user) {
            self.state.account_profile = Some(profile.clone());
        }
    }

    /// The profiles on screen of `user` waiting for Discord's.
    fn loading_profiles(&mut self, user: &str) -> impl Iterator<Item = &mut ProfileView> {
        [&mut self.state.profile, &mut self.state.panel_profile]
            .into_iter()
            .filter_map(Option::as_mut)
            .filter(move |view| view.profile.id == user && view.loading)
    }
}

/// Puts `profile` on screen in `view`. Presence and activities come from the
/// member list; the profile doesn't carry them.
fn fill_profile(view: &mut ProfileView, profile: &Profile) {
    // What a tab already asked for stays, as long as it's the same list.
    let same_place = view.profile.wishlist_place == profile.wishlist_place;
    let game_ids = |widgets: &[ProfileWidget]| -> Vec<String> {
        widgets
            .iter()
            .flat_map(|widget| &widget.games)
            .map(|game| game.id.clone())
            .collect()
    };
    let same_board = view.profile.board_asked
        && view.profile.widgets.len() == profile.widgets.len()
        && game_ids(&view.profile.widgets) == game_ids(&profile.widgets);
    let mut filled = Profile {
        presence: profile.presence.or(view.profile.presence),
        activity: profile.activity.clone().or(view.profile.activity.take()),
        activities: std::mem::take(&mut view.profile.activities),
        ..profile.clone()
    };
    filled.recent = std::mem::take(&mut view.profile.recent);
    filled.recent_asked = view.profile.recent_asked;
    if same_place {
        filled.wishlist = std::mem::take(&mut view.profile.wishlist);
    }
    if same_board {
        filled.widgets = std::mem::take(&mut view.profile.widgets);
        filled.board_asked = view.profile.board_asked;
        filled.board_named = view.profile.board_named;
    }
    view.profile = filled;
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
        presence: room.presence,
        activity: room.activity.clone(),
        activities: room.activities.clone(),
        id: person,
        ..Default::default()
    }
}
