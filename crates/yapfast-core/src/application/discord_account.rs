//! The linked Discord account: its friends and requests, linking, the custom status, and what its snapshots list.

use super::{members, *};

impl Application {
    pub(super) fn open_dm_screen(&mut self, screen: DmScreen) -> Vec<Effect> {
        if self.state.dm_screen == Some(screen)
            || self.state.navigation != Navigation::DirectMessages
        {
            return vec![];
        }
        let open = self.state.dm_screen.is_some();
        self.state.dm_screen = Some(screen);
        if open { vec![] } else { self.deselect() }
    }

    pub(super) fn add_friend(&mut self, username: String) -> Vec<Effect> {
        let username = username.trim().trim_start_matches('@').to_lowercase();
        let Some(dialog) = self
            .state
            .add_friend
            .as_mut()
            .filter(|dialog| !dialog.sending)
        else {
            return vec![];
        };
        if username.is_empty() {
            return vec![];
        }
        dialog.sending = true;
        dialog.outcome = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::AddFriend { username },
        ))]
    }

    pub(super) fn set_custom_status(
        &mut self,
        emoji: Option<String>,
        text: String,
        clear_after: ClearAfter,
    ) -> Vec<Effect> {
        let text = text.trim().to_owned();
        let emoji = emoji.filter(|emoji| !emoji.is_empty());
        if text.is_empty() && emoji.is_none() {
            return self.dispatch(Action::ClearCustomStatus);
        }
        let (emoji, emoji_id) = match emoji {
            Some(name) if name.contains(':') => match self.status_emoji(&name) {
                Ok(found) => (
                    Some(found.text.trim_matches(':').to_owned()),
                    Some(found.id),
                ),
                Err(error) => {
                    if let Some(editor) = &mut self.state.status_editor {
                        editor.error = Some(error.into());
                    }
                    return vec![];
                }
            },
            emoji => (emoji, None),
        };
        let status = chat::CustomStatus {
            text,
            emoji,
            emoji_id,
            expires_at_ms: clear_after.at(chrono::Local::now()),
        };
        self.save_status(Some(status))
    }

    pub(super) fn answer_request(&mut self, accept: bool) -> Vec<Effect> {
        match self.state.selected_room() {
            Some(room) if room.request && self.state.discord_online => {
                vec![Effect::Backend(Command::Discord(
                    DiscordCommand::AnswerRequest {
                        room: room.id.clone(),
                        accept,
                    },
                ))]
            }
            _ => vec![],
        }
    }

    /// Keeps the profile beside a one-to-one DM to the open DM's other
    /// person, fetching the rest of a Discord profile once per person.
    pub(super) fn show_panel_profile(&mut self) -> Vec<Effect> {
        let room = self.state.one_to_one().filter(|_| self.state.profile_panel);
        let Some((room, person)) = room.and_then(|room| Some((room, room.person.clone()?))) else {
            self.state.panel_profile = None;
            return vec![];
        };
        if self
            .state
            .panel_profile
            .as_ref()
            .is_some_and(|view| view.profile.id == person)
        {
            return vec![];
        }
        let fetch = room.network == Network::Discord && !self.state.preview;
        let (room, profile) = (room.id.clone(), dm_profile(room, person.clone()));
        self.state.panel_profile = Some(ProfileView {
            profile,
            large: false,
            loading: fetch,
            kept: false,
            note: None,
        });
        if fetch {
            vec![Effect::Backend(Command::Discord(DiscordCommand::Profile {
                room,
                user: person,
            }))]
        } else {
            vec![]
        }
    }

    /// Reading the newest messages of an unread Discord channel reads it,
    /// as Discord's client does: once they're in view with the window in
    /// front, and the "New" divider is placed, so the view isn't about to
    /// jump to it. A forum reads on showing it. Matrix rooms are read in the
    /// backend.
    pub(super) fn mark_discord_read(&self) -> Vec<Effect> {
        match self.state.selected_room() {
            Some(room)
                // A window of older messages doesn't read the newest.
                if self.state.room_ready
                    && !self.state.detached
                    && (room.forum || self.state.reading && self.read_marker.is_none())
                    && room.network == Network::Discord
                    && (room.unread > 0 || room.mentions > 0) =>
            {
                vec![Effect::Backend(Command::MarkRead {
                    room: room.id.clone(),
                    sync: self.state.sync_discord_reads,
                })]
            }
            _ => vec![],
        }
    }

    pub(super) fn list_discord(&mut self) -> Vec<Effect> {
        #[allow(unused_mut)]
        let mut effects = vec![];
        #[cfg(feature = "discord")]
        if let Some(snapshot) = &self.discord {
            let rung: Vec<String> = self
                .discord_index
                .rooms()
                .filter(|room| room.incoming)
                .map(|room| room.id.clone())
                .collect();
            self.discord_index
                .update(snapshot, self.state.show_hidden_channels);
            // A call that starts ringing this account notifies once, unless
            // it's set to Do Not Disturb.
            let silent = self.state.do_not_disturb();
            for room in self.discord_index.rooms() {
                if room.incoming
                    && !silent
                    && !rung.contains(&room.id)
                    && self.state.voice.room.as_ref() != Some(&room.id)
                {
                    let caller = room.people.first();
                    effects.push(Effect::Notify(chat::Alert {
                        room: room.id.clone(),
                        title: room.name.clone(),
                        body: match caller {
                            Some(caller) if room.person.is_none() => {
                                format!("{} is calling", caller.name)
                            }
                            _ => "Incoming call".into(),
                        },
                        picture: room
                            .avatar
                            .as_ref()
                            .and_then(|avatar| self.state.pictures.get(avatar).cloned()),
                        call: true,
                        mention: false,
                    }));
                }
            }
        }
        effects.extend(self.show_lists());
        effects
    }

    /// How linking a Discord account goes, from [`Effect::LinkDiscord`].
    #[cfg(feature = "discord")]
    pub fn linking(&mut self, progress: crate::discord_link::Progress) -> Vec<Effect> {
        use crate::discord_link::Progress;
        // Linking that was stopped says nothing more.
        let Some(link) = &mut self.state.discord_link else {
            return vec![];
        };
        match progress {
            Progress::Code(address) => *link = DiscordLink::Code(address),
            Progress::Scanned(name) => *link = DiscordLink::Scanned(name),
            // The browser shows the captcha; the dialog says so and can
            // open it again.
            Progress::Captcha(page) => {
                *link = DiscordLink::Captcha(page.clone());
                return vec![Effect::OpenPage(page)];
            }
            // A code that showed expired, so a new one replaces it. Without
            // one, the service turned Yapfast away.
            Progress::Expired if *link == DiscordLink::Starting => {
                *link = DiscordLink::Failed(
                    "Discord's login service closed the connection. Try again.".into(),
                );
            }
            Progress::Expired => {
                *link = DiscordLink::Starting;
                return vec![Effect::LinkDiscord];
            }
            Progress::Failed(error) => *link = DiscordLink::Failed(error),
            Progress::Linked => {
                self.state.discord_link = None;
                self.state.discord_linked = true;
                // A running Yapfast connects the account; one without Matrix
                // signs in with it.
                return if self.state.account.is_some() {
                    vec![Effect::Backend(Command::Discord(DiscordCommand::Start))]
                } else {
                    self.start(chat::Start::Restore)
                };
            }
        }
        vec![]
    }

    /// Sends the custom status to Discord from its dialog.
    pub(super) fn save_status(&mut self, status: Option<chat::CustomStatus>) -> Vec<Effect> {
        let Some(editor) = self
            .state
            .status_editor
            .as_mut()
            .filter(|editor| !editor.saving)
        else {
            return vec![];
        };
        editor.saving = true;
        editor.error = None;
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::SetCustomStatus(status),
        ))]
    }

    /// The custom emoji for a status, by its `:name:` or its `name:id` key,
    /// which only Nitro may use.
    pub(super) fn status_emoji(&self, name: &str) -> Result<chat::CustomEmoji, &'static str> {
        #[cfg(feature = "discord")]
        if let Some(snapshot) = &self.discord {
            if !snapshot
                .account
                .as_ref()
                .is_some_and(|account| account.premium_type > 0)
            {
                return Err("A custom emoji in a status needs Discord Nitro.");
            }
            return snapshot
                .servers
                .iter()
                .flat_map(|server| server.emojis.iter())
                .find(|emoji| {
                    emoji.available
                        && (format!(":{}:", emoji.name) == name
                            || format!("{}:{}", emoji.name, emoji.id) == name)
                })
                .map(|emoji| chat::CustomEmoji {
                    text: format!(":{}:", emoji.name),
                    id: emoji.id.clone(),
                    animated: emoji.animated,
                    picture: yapfast_discord::ready::custom_emoji(
                        &emoji.id,
                        false,
                        chat::EMOJI_PICTURE_SIZE,
                    ),
                })
                .ok_or("None of your servers has that emoji.");
        }
        let _ = name;
        Err("Discord isn't linked on this device.")
    }

    pub(super) fn change_relationship(
        &mut self,
        user: String,
        change: chat::RelationshipChange,
    ) -> Vec<Effect> {
        if self.state.discord_user.is_none() || self.state.preview {
            return vec![];
        }
        vec![Effect::Backend(Command::Discord(
            DiscordCommand::Relationship { user, change },
        ))]
    }

    /// Opens a dialog about a Discord user, from their DM's menu or their
    /// menu as a server member, fetching the note it shows. A server's own
    /// dialogs open through [`Self::ask_member`].
    pub(super) fn ask_person(&mut self, user: String, ask: PersonAsk) -> Vec<Effect> {
        if self.state.discord_user.is_none()
            || self.state.preview
            || self.state.relation(&user) == Relation::Me
            || user.starts_with('@')
        {
            return vec![];
        }
        let friend = self.state.friends.iter().find(|friend| friend.id == user);
        // As their DM names them, or the open channel.
        let name = self
            .state
            .dm_with(&user)
            .map(|room| room.name.clone())
            .or_else(|| self.state.person(&user).map(|person| person.name))
            .or_else(|| friend.map(|friend| friend.name.clone()));
        let Some(name) = name else {
            return vec![];
        };
        let text = match ask {
            PersonAsk::Note => None,
            PersonAsk::Nickname => match friend {
                Some(friend) => Some(friend.nickname.clone().unwrap_or_default()),
                None => return vec![],
            },
            PersonAsk::Unfriend | PersonAsk::Block => Some(String::new()),
            PersonAsk::ServerNickname | PersonAsk::Timeout | PersonAsk::Kick | PersonAsk::Ban => {
                return vec![];
            }
        };
        self.state.person_dialog = Some(PersonDialog {
            user: user.clone(),
            name,
            ask,
            text,
            saving: false,
            error: None,
            server: None,
            choice: 0,
        });
        if ask == PersonAsk::Note {
            vec![Effect::Backend(Command::Discord(DiscordCommand::Note {
                user,
            }))]
        } else {
            vec![]
        }
    }

    /// Carries out what the person dialog asks for: saving the note or
    /// nickname written, unfriending or blocking, or a member's change with
    /// its reason.
    pub(super) fn answer_person(&mut self, text: String) -> Vec<Effect> {
        let Some(dialog) = self
            .state
            .person_dialog
            .as_mut()
            .filter(|dialog| !dialog.saving && dialog.text.is_some())
        else {
            return vec![];
        };
        let user = dialog.user.clone();
        let text = text.trim().to_owned();
        let command = match dialog.ask {
            PersonAsk::Note => DiscordCommand::SetNote { user, note: text },
            PersonAsk::Nickname => DiscordCommand::SetNickname {
                user,
                nickname: text,
            },
            PersonAsk::Unfriend | PersonAsk::Block => {
                let change = if dialog.ask == PersonAsk::Block {
                    chat::RelationshipChange::Block
                } else {
                    chat::RelationshipChange::Unfriend
                };
                self.state.person_dialog = None;
                return self.change_relationship(user, change);
            }
            PersonAsk::ServerNickname | PersonAsk::Timeout | PersonAsk::Kick | PersonAsk::Ban => {
                let Some(server) = dialog.server.clone() else {
                    return vec![];
                };
                let change = members::change(dialog.ask, dialog.choice, text);
                DiscordCommand::Member {
                    server,
                    user,
                    change,
                }
            }
        };
        dialog.saving = true;
        dialog.error = None;
        vec![Effect::Backend(Command::Discord(command))]
    }

    /// Shows what a Discord snapshot says about the account and its lists.
    #[cfg(feature = "discord")]
    pub(super) fn show_snapshot(
        &mut self,
        snapshot: Arc<yapfast_discord::service::Snapshot>,
    ) -> Vec<Effect> {
        self.state.discord_online = snapshot.status == yapfast_discord::service::Status::Online;
        // Without Matrix, the account is Discord's.
        #[cfg(not(feature = "matrix"))]
        {
            use yapfast_discord::service::Status;
            self.state.online = self.state.discord_online;
            if let Some(account) = &snapshot.account {
                self.state.account_name = Some(account.name.clone());
                self.state.account_avatar = Some(account.avatar.clone());
            }
            self.state.account_status = Some(match &snapshot.status {
                Status::Online => "Online".into(),
                Status::Connecting => "Connecting to Discord...".into(),
                Status::Offline(reason) => reason.clone(),
            });
        }
        self.state.discord_linked = true;
        self.state.discord_user = snapshot.user_id.clone();
        self.state.discord_presence =
            Some(snapshot.presence.clone()).filter(|presence| !presence.is_empty());
        self.state.discord_friends = snapshot.friends.clone();
        use yapfast_discord::service::Snapshot;
        // The tracker shares each list of people between snapshots while it
        // stays the same, so one the last snapshot shared shows already.
        let earlier = self.discord.as_deref();
        let changed = |same: fn(&Snapshot, &Snapshot) -> bool| {
            earlier.is_none_or(|earlier| !same(earlier, &snapshot))
        };
        if changed(|a, b| Arc::ptr_eq(&a.blocked_list, &b.blocked_list)) {
            self.state.discord_blocked = snapshot
                .blocked_list
                .iter()
                .map(|blocked| chat::Blocked {
                    id: blocked.user_id.clone(),
                    name: blocked.name.clone(),
                    username: blocked.username.clone(),
                    avatar: Some(blocked.avatar.clone()),
                })
                .collect();
        }
        self.state.discord_ignored = snapshot.ignored.clone();
        if changed(|a, b| Arc::ptr_eq(&a.friend_list, &b.friend_list)) {
            self.state.friends = snapshot
                .friend_list
                .iter()
                .map(|friend| chat::Friend {
                    id: friend.user_id.clone(),
                    name: friend.name.clone(),
                    nickname: friend.nickname.clone(),
                    avatar: Some(friend.avatar.clone()),
                    presence: friend.presence.map(discord::presence),
                    activity: friend.activity.clone(),
                    activities: friend.activities.clone(),
                })
                .collect();
        }
        self.state.custom_status =
            snapshot
                .custom_status
                .as_ref()
                .map(|custom| chat::CustomStatus {
                    text: custom.text.clone(),
                    emoji: custom.emoji_name.clone(),
                    emoji_id: custom.emoji_id.clone(),
                    expires_at_ms: custom.expires_at_ms,
                });
        self.state.discord_show_game = snapshot.show_game;
        self.state.soundboard_volume = snapshot
            .soundboard_volume
            .unwrap_or(yapfast_discord::soundboard::DEFAULT_VOLUME);
        if changed(|a, b| Arc::ptr_eq(&a.join_sounds, &b.join_sounds)) {
            self.state.join_sounds = snapshot
                .join_sounds
                .iter()
                .map(|join| JoinSound {
                    server: join.server.clone(),
                    sound: join.sound.clone(),
                    guild: join.guild.clone(),
                })
                .collect();
        }
        let playing = snapshot.playing.as_deref();
        // Snapshots come often; the game rarely changes.
        let shown = self.state.discord_playing.as_ref();
        if shown.map(|shown| (shown.name.as_str(), shown.started_ms))
            != playing.map(|playing| (playing.name.as_str(), playing.started_ms))
        {
            self.state.discord_playing = playing.map(|playing| chat::Playing {
                name: playing.name.clone(),
                started_ms: playing.started_ms,
            });
        }
        if changed(|a, b| Arc::ptr_eq(&a.friend_requests, &b.friend_requests)) {
            self.state.friend_requests = snapshot
                .friend_requests
                .iter()
                .map(|request| chat::FriendRequest {
                    id: request.user_id.clone(),
                    name: request.name.clone(),
                    avatar: Some(request.avatar.clone()),
                    incoming: request.incoming,
                    spam: request.spam,
                })
                .collect();
        }
        // The earlier snapshot still holds its servers' categories and roles,
        // so sharing them means nothing about them changed.
        let changed = self.discord.as_ref().is_none_or(|earlier| {
            snapshot.servers.len() != earlier.servers.len()
                || snapshot
                    .servers
                    .iter()
                    .zip(&earlier.servers)
                    .any(|(now, then)| {
                        now.id != then.id
                            || !Arc::ptr_eq(&now.categories, &then.categories)
                            || !Arc::ptr_eq(&now.roles, &then.roles)
                    })
        });
        if changed {
            self.state.category_changes = self.state.category_changes.wrapping_add(1);
        }
        self.discord = Some(snapshot);
        let mut effects = self.list_discord();
        self.refresh_browse();
        effects.extend(self.follow_server_settings());
        effects.extend(self.follow_channel_settings());
        effects.extend(self.mark_discord_read());
        effects.extend(self.mark_aside_read());
        // Posts that came to the forum on screen want their first messages.
        effects.extend(self.ask_starters());
        effects
    }

    /// Forgets the Discord account after it was unlinked.
    pub(super) fn forget_discord(&mut self) -> Vec<Effect> {
        #[cfg(feature = "discord")]
        {
            self.discord = None;
            self.discord_index = discord::Index::default();
        }
        self.state.discord_linked = false;
        self.state.discord_online = false;
        self.state.discord_user = None;
        self.state.discord_friends = Arc::default();
        self.state.discord_blocked.clear();
        self.state.discord_ignored = Arc::default();
        self.state.person_dialog = None;
        self.state.server_settings = None;
        self.state.channel_settings = None;
        self.state.browse = None;
        self.state.mfa.clear();
        self.state.friends.clear();
        self.state.friend_requests.clear();
        self.state.custom_status = None;
        self.state.discord_show_game = false;
        self.state.discord_playing = None;
        self.state.join_sounds.clear();
        self.state.sound_catalog = None;
        self.show_lists()
    }
}
