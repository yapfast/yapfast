//! A Discord server member's menu, as Discord's client offers it: what it
//! shows depends on this account's permissions in the server and on its
//! highest role against theirs. Each check reads the server the latest
//! snapshot shares, and the member's roles and timeout from the service's
//! names, only when a menu opens or a choice is made.

use super::*;
use crate::chat::{MemberChange, ServerRole};

#[cfg(feature = "discord")]
use crate::chat::Permission;

/// How long [`MenuChoice::Timeout`] times someone out, in seconds, as
/// Discord's client offers it: 60 seconds, 5 or 10 minutes, an hour, a day
/// or a week. Discord allows up to 28 days.
pub const TIMEOUT_TIMES: [u32; 6] = [60, 5 * 60, 10 * 60, 3600, 24 * 3600, 7 * 24 * 3600];

/// How far back [`MenuChoice::Ban`] deletes the member's messages, in
/// seconds, as Discord's client offers it: none, the last hour, 6, 12 or 24
/// hours, 3 days or 7 days.
pub const BAN_HISTORY: [u32; 7] = [
    0,
    3600,
    6 * 3600,
    12 * 3600,
    24 * 3600,
    3 * 24 * 3600,
    7 * 24 * 3600,
];

/// A role in a member's roles submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleChoice {
    pub role: ServerRole,
    /// The member holds it.
    pub held: bool,
    /// This account may give it or take it away; the rest show greyed.
    pub grantable: bool,
}

/// The change a member's dialog makes, with its choice and what was written.
pub(super) fn change(ask: PersonAsk, choice: usize, text: String) -> MemberChange {
    let pick = |times: &[u32]| times[choice.min(times.len() - 1)];
    match ask {
        PersonAsk::Timeout => MemberChange::Timeout {
            until_ms: Some(crate::clock::now_ms() + u64::from(pick(&TIMEOUT_TIMES)) * 1000),
            reason: text,
        },
        PersonAsk::Kick => MemberChange::Kick { reason: text },
        PersonAsk::Ban => MemberChange::Ban {
            delete_seconds: pick(&BAN_HISTORY),
            reason: text,
        },
        _ => MemberChange::Nickname(text),
    }
}

impl Application {
    /// What someone's menu offers: in a Discord server's channel, their
    /// menu as a member of it; elsewhere [`AppState::person_choices`]. Each
    /// invites them to a server, as Discord's do.
    pub fn person_choices(&self, user: &str) -> Vec<MenuChoice> {
        #[cfg(feature = "discord")]
        let mut choices = match self.member_place(user) {
            Some(place) => self.member_choices(place, user),
            None => self.state.person_choices(user),
        };
        #[cfg(not(feature = "discord"))]
        let mut choices = self.state.person_choices(user);
        self.offer_invite(user, &mut choices);
        choices
    }

    /// Someone's menu in a voice channel, from a right click on them in its
    /// call or under it in the channel list: their menu as a member of its
    /// server, with what the call offers, as Discord's has it. Nothing in a
    /// DM's or a Matrix room's call, where the right click sets only their
    /// volume.
    pub fn voice_person_choices(&self, room: &str, user: &str) -> Vec<MenuChoice> {
        #[cfg(feature = "discord")]
        if self.is_member_place(room, user) {
            let mut choices = self.member_choices(room, user);
            self.offer_invite(user, &mut choices);
            return choices;
        }
        let _ = (room, user);
        vec![]
    }

    /// Where someone's menu is a Discord server member's: the server whose
    /// Members is open in its settings, else the open channel of a known
    /// server; for a Discord user.
    #[cfg(feature = "discord")]
    pub fn member_place(&self, user: &str) -> Option<&str> {
        if let Some(settings) = self.state.server_settings.as_ref() {
            let place = settings.server.as_str();
            let members = matches!(settings.page, Some(SettingsPage::Members(_)));
            return (members && self.is_member_place(place, user)).then_some(place);
        }
        let place = self.state.selected.as_deref()?;
        (self.state.discord_server_open() && self.is_member_place(place, user)).then_some(place)
    }

    /// Someone as the open Members of a server's settings lists them.
    #[cfg(feature = "discord")]
    pub(super) fn listed_member(
        &self,
        place: &str,
        user: &str,
    ) -> Option<&crate::chat::ListedMember> {
        let settings = self.state.server_settings.as_ref()?;
        let Some(SettingsPage::Members(page)) = settings.page.as_ref() else {
            return None;
        };
        let (server, _, _) = self.discord_place(place)?;
        (discord::server_id(&server.id) == settings.server)
            .then(|| page.members.iter().find(|member| member.person.id == user))?
    }

    /// Whether someone's menu in `place` is a Discord server member's.
    #[cfg(feature = "discord")]
    fn is_member_place(&self, place: &str, user: &str) -> bool {
        let state = &self.state;
        !state.preview
            && state.discord_user.is_some()
            && !user.starts_with('@')
            && self.discord_place(place).is_some()
    }

    /// Someone in the voice channel `place`, as its list shows them.
    pub fn voice_person(&self, place: &str, user: &str) -> Option<&crate::chat::VoicePerson> {
        self.state
            .rooms
            .iter()
            .find(|room| room.id == place && room.voice)?
            .people
            .iter()
            .find(|person| person.id == user)
    }

    /// The voice channels someone in the voice channel `place` may be moved
    /// to, by room id and name, in the channel list's order, as Discord's
    /// Move to offers them: for this account, those it may join, which it
    /// joins; for someone else, with Move Members here, those it may move
    /// them to, which it or they may join.
    pub fn move_targets(&self, place: &str, user: &str) -> Vec<(String, String)> {
        #[cfg(feature = "discord")]
        if let Some((server, Some(current), me)) = self.discord_place(place)
            && self.voice_person(place, user).is_some()
            && (user == me || self.may(place, Permission::MoveMembers))
        {
            use yapfast_discord::ready::permission::CONNECT;
            let roles = self.member_roles(place, user);
            return server
                .voice_channels
                .iter()
                .filter(|channel| channel.id != current)
                .filter_map(|channel| {
                    let room = discord::room_id(&server.id, &channel.id);
                    let may = |permission| self.may(&room, permission);
                    let allowed = if user == me {
                        may(Permission::Connect)
                    } else {
                        may(Permission::MoveMembers)
                            && (may(Permission::Connect)
                                || server.channel_permissions(channel, user, &roles) & CONNECT != 0)
                    };
                    allowed.then(|| (room, channel.name.clone()))
                })
                .collect();
        }
        let _ = (place, user);
        vec![]
    }

    /// A server member's menu, in the order of Discord's: who they are and
    /// how to reach them, in a voice channel they're in its mute, their
    /// nickname and how this account stands with them, their roles and
    /// where to move them, moderation in red, then copying their ID. This
    /// account's own leaves out what it can't do to itself.
    #[cfg(feature = "discord")]
    fn member_choices(&self, place: &str, user: &str) -> Vec<MenuChoice> {
        let state = &self.state;
        let me = state.relation(user) == Relation::Me;
        let mut choices = vec![MenuChoice::Profile];
        if !me {
            // Not from a server's settings, which cover the channel.
            if state.server_settings.is_none()
                && state
                    .selected_room()
                    .is_some_and(|room| state.writable(room))
            {
                choices.push(MenuChoice::Mention);
            }
            choices.push(MenuChoice::Message);
            if self.can_call(user) {
                choices.push(MenuChoice::Call);
            }
            choices.push(MenuChoice::AddNote);
            match state.friends.iter().find(|friend| friend.id == user) {
                Some(friend) if friend.nickname.is_some() => choices.push(MenuChoice::EditNickname),
                Some(_) => choices.push(MenuChoice::AddNickname),
                None => {}
            }
        }
        // In a voice channel they're in: muting them or their soundboard
        // for this account, or its own mute and deafen in the call it's in
        // there.
        let in_voice = self.voice_person(place, user).is_some();
        if in_voice && !me {
            choices.extend([MenuChoice::LocalMute, MenuChoice::SoundboardMute]);
        } else if in_voice && state.voice.active() && state.voice.room.as_deref() == Some(place) {
            choices.extend([MenuChoice::SelfMute, MenuChoice::SelfDeafen]);
        }
        // One's own, as Discord's menu has it, opens the per-server profile
        // editor, of which the nickname is a part.
        if me {
            if state.discord_online {
                choices.push(MenuChoice::EditServerProfile);
            }
        } else if self.may_rename(place, user) {
            choices.push(MenuChoice::ServerNickname);
        }
        if !me {
            choices.extend(state.relationship_choices(user));
        }
        if self.may(place, Permission::ManageRoles) && !self.server_roles(place).is_empty() {
            choices.push(MenuChoice::Roles);
        }
        if in_voice && !self.move_targets(place, user).is_empty() {
            choices.push(MenuChoice::MoveTo);
        }
        if in_voice {
            if self.may(place, Permission::MuteMembers) {
                choices.push(MenuChoice::ServerMute);
            }
            if self.may(place, Permission::DeafenMembers) {
                choices.push(MenuChoice::ServerDeafen);
            }
            if !me && self.may(place, Permission::MoveMembers) {
                choices.push(MenuChoice::Disconnect);
            }
        }
        if self.may_moderate(place, user, Permission::Moderate) && !self.is_admin(place, user) {
            choices.push(if self.timed_out(place, user) {
                MenuChoice::RemoveTimeout
            } else {
                MenuChoice::Timeout
            });
        }
        if self.may_moderate(place, user, Permission::Kick) {
            choices.push(MenuChoice::Kick);
        }
        if self.may_moderate(place, user, Permission::Ban) {
            choices.push(MenuChoice::Ban);
        }
        // Only from Members in the server's settings, as in Discord's.
        if self.may_transfer_to(place, user) {
            choices.push(MenuChoice::TransferOwnership);
        }
        choices.push(MenuChoice::CopyId);
        choices
    }

    /// Whether a member's call can start from their menu: their DM's, or a
    /// new DM's with a friend.
    #[cfg(feature = "discord")]
    fn can_call(&self, user: &str) -> bool {
        let state = &self.state;
        match state.dm_with(user) {
            Some(dm) => {
                state.can_join_voice(&dm.id)
                    && !(state.voice.active() && state.voice.room.as_ref() == Some(&dm.id))
            }
            None => state.relation(user) == Relation::Friend && state.discord_online && !state.busy,
        }
    }

    /// A member's roles in the Discord server a place names, shared with
    /// what learned them: the service's names, else the member list's row.
    #[cfg(feature = "discord")]
    pub fn member_roles(&self, place: &str, user: &str) -> Arc<[String]> {
        let Some((server, _, me)) = self.discord_place(place) else {
            return Arc::default();
        };
        let known = self
            .discord
            .as_ref()
            .and_then(|snapshot| snapshot.names.read().shared_roles(&server.id, user));
        known
            .or_else(|| (user == me).then(|| server.marks.roles.as_slice().into()))
            .or_else(|| self.state.person(user).map(|person| person.roles))
            .or_else(|| {
                self.listed_member(place, user)
                    .map(|member| member.roles.as_slice().into())
            })
            .unwrap_or_default()
    }

    /// Whether a member of the Discord server a place names is timed out
    /// now, as far as Yapfast knows.
    #[cfg(feature = "discord")]
    fn timed_out(&self, place: &str, user: &str) -> bool {
        let Some((server, _, _)) = self.discord_place(place) else {
            return false;
        };
        let until = self
            .discord
            .as_ref()
            .and_then(|snapshot| snapshot.names.read().timed_out_until(&server.id, user))
            .or_else(|| {
                self.state
                    .person(user)
                    .and_then(|person| person.timed_out_until)
            })
            .or_else(|| {
                self.listed_member(place, user)
                    .map(|member| member.timed_out_until)
            });
        until.is_some_and(|until| until > crate::clock::now_ms())
    }

    /// Whether a member is an administrator, or the owner, whom Discord
    /// doesn't let anyone time out.
    #[cfg(feature = "discord")]
    fn is_admin(&self, place: &str, user: &str) -> bool {
        use yapfast_discord::ready::permission::ADMINISTRATOR;
        let roles = self.member_roles(place, user);
        self.discord_place(place).is_some_and(|(server, _, _)| {
            server.server_permissions(user, &roles) & ADMINISTRATOR != 0
        })
    }

    /// Whether this account may act on another member as `permission`
    /// allows: it holds it and outranks them, which leaves out itself and
    /// the owner.
    #[cfg(feature = "discord")]
    fn may_moderate(&self, place: &str, user: &str, permission: Permission) -> bool {
        self.may(place, permission) && self.outranks(place, user, &self.member_roles(place, user))
    }

    /// Whether this account may change a member's nickname: its own with
    /// Change Nickname, another's with Manage Nicknames over them.
    #[cfg(feature = "discord")]
    fn may_rename(&self, place: &str, user: &str) -> bool {
        if self.state.relation(user) == Relation::Me {
            self.may(place, Permission::ChangeNickname)
                || self.may(place, Permission::ManageNicknames)
        } else {
            self.may_moderate(place, user, Permission::ManageNicknames)
        }
    }

    /// The server's roles for a member's roles submenu, highest first: each
    /// ticked when they hold it, greyed when this account may not give or
    /// take it. Built when the menu opens.
    pub fn role_choices(&self, place: &str, user: &str) -> Vec<RoleChoice> {
        #[cfg(feature = "discord")]
        {
            let held = self.member_roles(place, user);
            self.server_roles(place)
                .into_iter()
                .map(|role| RoleChoice {
                    held: held.contains(&role.id),
                    grantable: self.may_grant(place, &role.id),
                    role,
                })
                .collect()
        }
        #[cfg(not(feature = "discord"))]
        {
            let _ = (place, user);
            vec![]
        }
    }

    /// Puts `@Name ` in the open channel's draft, after what's there.
    pub(super) fn mention_person(&mut self, user: String) -> Vec<Effect> {
        let state = &self.state;
        let writable = state
            .selected_room()
            .is_some_and(|room| state.writable(room));
        let person = state.person(&user);
        let (true, Some(person), false) = (writable, person, state.relation(&user) == Relation::Me)
        else {
            return vec![];
        };
        let mut draft = state.draft().to_owned();
        if !draft.is_empty() && !draft.ends_with(char::is_whitespace) {
            draft.push(' ');
        }
        draft.push('@');
        draft.push_str(&person.name);
        draft.push(' ');
        self.dispatch(Action::SetDraft(draft))
    }

    /// Opens the DM with someone and starts its call, or starts a DM with a
    /// friend first, whose call starts once it opens.
    pub(super) fn call_person(&mut self, user: String) -> Vec<Effect> {
        if let Some(dm) = self.state.dm_with(&user) {
            let dm = dm.id.clone();
            if !self.state.can_join_voice(&dm) {
                return vec![];
            }
            let mut effects = self.dispatch(Action::Open(dm.clone()));
            effects.extend(self.dispatch(Action::RequestJoinVoice(dm)));
            return effects;
        }
        let friend = self.state.relation(&user) == Relation::Friend;
        if !friend || !self.state.discord_online || self.state.busy {
            return vec![];
        }
        self.starting_dm = true;
        self.pending_call = true;
        vec![Effect::Backend(Command::StartDm {
            id: user,
            discord: true,
        })]
    }

    /// Opens a member's dialog: their nickname, or for another member, a
    /// timeout, a kick or a ban, each only when this account may.
    pub(super) fn ask_member(
        &mut self,
        place: String,
        user: String,
        ask: PersonAsk,
    ) -> Vec<Effect> {
        #[cfg(feature = "discord")]
        {
            let allowed = match ask {
                PersonAsk::ServerNickname => self.may_rename(&place, &user),
                PersonAsk::Timeout => {
                    self.may_moderate(&place, &user, Permission::Moderate)
                        && !self.is_admin(&place, &user)
                        && !self.timed_out(&place, &user)
                }
                PersonAsk::Kick => self.may_moderate(&place, &user, Permission::Kick),
                PersonAsk::Ban => self.may_moderate(&place, &user, Permission::Ban),
                _ => false,
            };
            let Some((server, _, _)) = self.discord_place(&place).filter(|_| allowed) else {
                return vec![];
            };
            if !self.state.discord_online {
                return vec![];
            }
            let guild = server.id.clone();
            let (name, nickname) = match &self.discord {
                Some(snapshot) => {
                    let names = snapshot.names.read();
                    let owned = |name: Option<&str>| name.map(str::to_owned);
                    (
                        owned(names.name(&guild, &user)),
                        owned(names.nickname(&guild, &user)),
                    )
                }
                None => (None, None),
            };
            let name = name
                .or_else(|| self.state.person(&user).map(|person| person.name))
                .unwrap_or_else(|| "this member".into());
            let text = match ask {
                PersonAsk::ServerNickname => nickname.unwrap_or_default(),
                _ => String::new(),
            };
            self.state.person_dialog = Some(PersonDialog {
                user,
                name,
                ask,
                text: Some(text),
                saving: false,
                error: None,
                server: Some(discord::server_id(&guild)),
                choice: 0,
            });
        }
        #[cfg(not(feature = "discord"))]
        let _ = (place, user, ask);
        vec![]
    }

    /// Gives a member a role or takes it away, ends their timeout, or mutes,
    /// deafens, moves or disconnects them in voice, when this account may;
    /// Discord's GUILD_MEMBER_UPDATE or VOICE_STATE_UPDATE shows it.
    pub(super) fn change_member(
        &mut self,
        place: String,
        user: String,
        change: MemberChange,
    ) -> Vec<Effect> {
        #[cfg(feature = "discord")]
        {
            let allowed = match &change {
                MemberChange::Role { role, .. } => self.may_grant(&place, role),
                MemberChange::Timeout { until_ms: None, .. } => {
                    self.may_moderate(&place, &user, Permission::Moderate)
                        && self.timed_out(&place, &user)
                }
                MemberChange::ServerMute(_) => {
                    self.voice_person(&place, &user).is_some()
                        && self.may(&place, Permission::MuteMembers)
                }
                MemberChange::ServerDeafen(_) => {
                    self.voice_person(&place, &user).is_some()
                        && self.may(&place, Permission::DeafenMembers)
                }
                MemberChange::Move(None) => {
                    self.voice_person(&place, &user).is_some()
                        && self.state.relation(&user) != Relation::Me
                        && self.may(&place, Permission::MoveMembers)
                }
                // This account moves itself by joining.
                MemberChange::Move(Some(to)) => {
                    self.state.relation(&user) != Relation::Me
                        && self
                            .move_targets(&place, &user)
                            .iter()
                            .any(|(room, _)| room == to)
                }
                _ => false,
            };
            let server = self
                .discord_place(&place)
                .filter(|_| allowed && self.state.discord_online)
                .map(|(server, _, _)| discord::server_id(&server.id));
            if let Some(server) = server {
                return vec![Effect::Backend(Command::Discord(DiscordCommand::Member {
                    server,
                    user,
                    change,
                }))];
            }
        }
        let _ = (place, user, change);
        vec![]
    }
}

#[cfg(all(feature = "discord", any(test, feature = "test-fixtures")))]
mod fixture;
#[cfg(all(feature = "discord", any(test, feature = "test-fixtures")))]
pub use fixture::{member_server, with_clubs};
#[cfg(all(test, feature = "discord"))]
mod tests;
