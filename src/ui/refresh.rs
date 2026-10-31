//! What [`Adapter::refresh`] shows: the application's state, area by area.

use super::*;

/// What changed since the last refresh, read before it changes anything.
pub(super) struct Changes {
    reset: bool,
    selection: bool,
    messages: bool,
    /// The newest message was in view.
    at_end: bool,
}

impl Changes {
    fn read(adapter: &Adapter, window: &YapfastWindow) -> Self {
        let state = &adapter.app.state;
        Self {
            reset: adapter.reset_version != state.reset_version,
            selection: adapter.selection_version != state.selection_version,
            messages: !Arc::ptr_eq(&adapter.messages, &state.messages),
            at_end: window.invoke_timeline_at_end(),
        }
    }
}

/// What a backend event can change on screen.
#[derive(Clone, Copy, Default)]
pub(super) struct Reach {
    composer: bool,
    voice: bool,
    /// Pictures arrived, which can show anywhere.
    pictures: bool,
    /// The video playing changed.
    video: bool,
    pub(super) everything: bool,
}

impl Reach {
    pub(super) const EVERYTHING: Self = Self {
        composer: true,
        voice: true,
        pictures: true,
        video: true,
        everything: true,
    };

    pub(super) fn of(event: &Event) -> Self {
        match event {
            // Who's typing shows only above the composer.
            Event::Typing { .. } => Self {
                composer: true,
                ..Self::default()
            },
            // The call, with who's speaking, which changes several times a
            // second, shows only in the call's own views.
            Event::Voice(_) => Self {
                voice: true,
                ..Self::default()
            },
            Event::Picture { .. } => Self {
                pictures: true,
                ..Self::default()
            },
            Event::Video(_) => Self {
                video: true,
                ..Self::default()
            },
            _ => Self::EVERYTHING,
        }
    }

    pub(super) fn and(self, other: Self) -> Self {
        Self {
            composer: self.composer || other.composer,
            voice: self.voice || other.voice,
            pictures: self.pictures || other.pictures,
            video: self.video || other.video,
            everything: self.everything || other.everything,
        }
    }
}

/// How long a picture that arrived waits for others to show with it.
const PICTURE_BATCH: std::time::Duration = std::time::Duration::from_millis(100);

impl Adapter {
    /// Browse Channels' rows whose channel's name has its search in it, each
    /// under its category's heading.
    fn browse_rows(&self, state: &AppState, window: &YapfastWindow) -> Vec<BrowseRow> {
        use yapfast_core::application::BrowsedKind;
        let Some(browse) = &state.browse else {
            return Vec::new();
        };
        let query = window.get_browse_filter().trim().to_lowercase();
        let row = |channel: &yapfast_core::application::BrowsedChannel| BrowseRow {
            id: channel.id.as_str().into(),
            name: channel.name.as_str().into(),
            topic: channel.topic.as_str().into(),
            icon: match channel.kind {
                BrowsedKind::Voice => self.icons.voice.clone(),
                BrowsedKind::Forum => self.icons.forum.clone(),
                BrowsedKind::Category | BrowsedKind::Text | BrowsedKind::Announcement => {
                    self.icons.text.clone()
                }
            },
            category: channel.kind == BrowsedKind::Category,
            voice: channel.kind == BrowsedKind::Voice,
            forum: channel.kind == BrowsedKind::Forum,
            followed: channel.followed,
            by_category: channel.by_category,
        };
        let mut rows = Vec::new();
        let mut heading = None;
        for channel in &browse.rows {
            if channel.kind == BrowsedKind::Category {
                heading = Some(channel);
                continue;
            }
            if !channel.name.to_lowercase().contains(&query) {
                continue;
            }
            if let Some(category) = heading.take() {
                rows.push(row(category));
            }
            rows.push(row(channel));
        }
        rows
    }
}

/// What the sidebar was built from, kept to tell whether it changed. The
/// rooms and servers go by their count of changes and the other lists by a
/// hash: copies of them took megabytes on a large account.
#[derive(PartialEq)]
pub(super) struct SidebarShown {
    lists: u64,
    navigation: Navigation,
    selected: Option<String>,
    dm_screen: Option<DmScreen>,
    filter: SharedString,
    friend_filter: SharedString,
    browse_filter: SharedString,
    picture_changes: u64,
    category_changes: u64,
    list_changes: u64,
    /// Whether Discord took changes, which the header's invite button needs.
    discord_online: bool,
}

impl SidebarShown {
    fn of(state: &AppState, window: &YapfastWindow) -> Self {
        Self {
            lists: Self::lists(state),
            navigation: state.navigation.clone(),
            selected: state.selected.clone(),
            dm_screen: state.dm_screen,
            filter: window.get_filter(),
            friend_filter: window.get_friend_filter(),
            browse_filter: window.get_browse_filter(),
            picture_changes: state.picture_changes,
            category_changes: state.category_changes,
            list_changes: state.list_changes,
            discord_online: state.discord_online,
        }
    }

    /// The smaller lists the sidebar shows; the rooms and servers count
    /// their changes.
    fn lists(state: &AppState) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        (
            &state.friends,
            &state.friend_requests,
            &state.discord_blocked,
            &state.pinned,
            &state.browse,
        )
            .hash(&mut hasher);
        hasher.finish()
    }

    /// Whether the sidebar would show the same.
    fn shows(&self, state: &AppState, window: &YapfastWindow) -> bool {
        self.navigation == state.navigation
            && self.selected == state.selected
            && self.dm_screen == state.dm_screen
            && self.filter == window.get_filter()
            && self.friend_filter == window.get_friend_filter()
            && self.browse_filter == window.get_browse_filter()
            && self.picture_changes == state.picture_changes
            && self.category_changes == state.category_changes
            && self.list_changes == state.list_changes
            && self.discord_online == state.discord_online
            && self.lists == Self::lists(state)
    }
}

impl Icons {
    /// A conversation's icon, as the sidebar shows it.
    fn room(&self, room: &yapfast_core::chat::ChatRoom) -> slint::Image {
        if room.locked {
            self.lock.clone()
        } else if room.voice {
            self.voice.clone()
        } else if room.forum {
            self.forum.clone()
        } else if room.direct {
            self.dm.clone()
        } else if room.parent.is_some() {
            self.thread.clone()
        } else {
            self.text.clone()
        }
    }
}

/// What the quick switcher listed last, so its places are found again only
/// once that changes.
pub(super) struct SwitcherShown {
    query: String,
    list_changes: u64,
    places_used: u64,
    selected: Option<String>,
}

impl SwitcherShown {
    fn of(state: &AppState) -> Self {
        Self {
            query: state.switcher.clone().unwrap_or_default(),
            list_changes: state.list_changes,
            places_used: state.places_used.changes(),
            selected: state.selected.clone(),
        }
    }

    /// Whether the switcher would list the same.
    fn shows(&self, state: &AppState) -> bool {
        state.switcher.as_ref() == Some(&self.query)
            && self.list_changes == state.list_changes
            && self.places_used == state.places_used.changes()
            && self.selected == state.selected
    }
}

impl Adapter {
    /// Shows what a burst of backend events can have changed.
    pub(super) fn show(&mut self, window: &YapfastWindow, reach: Reach) {
        if reach.everything {
            self.refresh(window);
            return;
        }
        // Pictures arrive one at a time, hundreds when the emoji picker opens,
        // so those that come close together show in one refresh.
        if reach.pictures && !self.pictures_due.running() {
            let (adapter, weak) = (self.weak.clone(), window.as_weak());
            self.pictures_due
                .start(slint::TimerMode::SingleShot, PICTURE_BATCH, move || {
                    if let (Some(adapter), Some(window)) = (adapter.upgrade(), weak.upgrade()) {
                        adapter.borrow_mut().refresh(&window);
                    }
                });
        }
        if reach.composer {
            self.show_composer(window);
            self.show_aside(window);
        }
        if reach.voice {
            self.show_voice(window);
        }
        if reach.video {
            self.show_video(window);
        }
    }

    pub(super) fn refresh(&mut self, window: &YapfastWindow) {
        refresh_avatar_sources(&self.app.state);
        window
            .global::<PictureStore>()
            .set_revision(self.app.state.picture_changes as i32);
        self.show_viewer(window);
        self.show_profile(window);
        let changes = Changes::read(self, window);
        if changes.reset || self.navigation != self.app.state.navigation {
            window.set_filter(SharedString::default());
        }
        if changes.reset {
            window.set_password(SharedString::default());
            window.set_recovery(SharedString::default());
            window.set_settings_open(false);
        }
        self.show_workspace(window);
        self.show_sidebar(window);
        self.show_account(window);
        // A server's search stays, with its words, in the server's channels.
        if changes.selection && self.app.state.search_shown().is_none() {
            window.set_search_text(SharedString::default());
        }
        self.show_header(window);
        self.show_dm_tools(window);
        self.show_threads(window);
        self.show_forum(window);
        self.show_new_post(window);
        self.show_new_thread(window);
        self.show_members(window);
        self.show_invite(window);
        self.show_join(window);
        self.show_channel_dialog(window);
        self.show_leave(window);
        self.show_move_sync(window);
        self.show_profile_editor(window);
        self.show_server_settings(window);
        self.show_channel_settings(window);
        self.show_mfa(window);
        self.show_dialogs(window);
        self.show_reactions(window);
        self.show_gifs(window);
        self.show_composer(window);
        self.show_messages(window, &changes);
        self.show_aside(window);
        self.show_voice(window);
        self.show_video(window);
        let state = &self.app.state;
        if changes.selection {
            self.focus_when_ready = state.shows_text() && !state.room_ready;
        }
        if changes.selection && state.shows_text() {
            window.invoke_focus_composer();
        } else if self.focus_when_ready && state.room_ready {
            self.focus_when_ready = false;
            window.invoke_focus_composer();
        } else if changes.reset && !state.preview && state.account.is_none() {
            window.invoke_focus_sign_in();
        }
        self.navigation = state.navigation.clone();
        self.selection_version = state.selection_version;
        self.reset_version = state.reset_version;
        self.messages = state.messages.clone();
        self.forwarding = state.forwarding.is_some();
        self.show_tray();
        self.show_title(window);
        window.set_tray_available(self.tray.as_ref().is_some_and(crate::tray::Tray::available));
        if self.app.reaction_uses() != &self.layout.reactions {
            self.layout.reactions = self.app.reaction_uses().clone();
            self.layout.write(&self.root);
        }
        if changes.selection && !state.preview && state.account.is_some() {
            let mut layout = self.layout.clone();
            layout.channels.extend(self.app.last_channels());
            if layout != self.layout {
                layout.write(&self.root);
                self.layout = layout;
            }
        }
    }

    /// Whether Yapfast is signed in or previewing, what the account is doing, and
    /// linking Discord.
    fn show_workspace(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let workspace = state.preview || state.account.is_some();
        window.set_workspace(workspace);
        window.set_preview(state.preview);
        window.set_busy(state.busy);
        window.set_trusted(state.trusted);
        window.set_forward_allowed(
            state.preview
                || state.trusted
                || state
                    .selected
                    .as_deref()
                    .is_some_and(|room| Network::of(room) == Network::Discord),
        );
        window.set_online(state.online);
        // Only the sign-in screen offers it, and finding out reads the OS
        // credential store or the disk.
        if !workspace {
            window.set_restore_available(yapfast_core::chat::can_restore(&self.root));
        }
        window.set_dms_selected(state.navigation == Navigation::DirectMessages);
        window.set_dms_unread(
            state
                .rooms
                .iter()
                .any(|room| room.direct && room.unread > 0),
        );
        // As in Discord, the DM home counts DM mentions and the friend
        // requests waiting for an answer.
        let dm_mentions: u64 = state
            .rooms
            .iter()
            .filter(|room| room.direct)
            .map(|room| room.mentions)
            .sum();
        let incoming = state
            .friend_requests
            .iter()
            .filter(|request| request.incoming && !request.spam)
            .count() as u64;
        window.set_dms_badge(count_badge(dm_mentions + incoming));
        window.set_discord_linked(
            state.discord_linked || state.servers.iter().any(|server| server.discord),
        );
        window.set_discord_build(cfg!(feature = "discord"));
        window.set_matrix_build(cfg!(feature = "matrix"));
        // Without Matrix, the account's dot is its Discord status.
        window.set_account_presence(
            if cfg!(feature = "matrix") || state.preview {
                ""
            } else if !state.discord_online {
                "offline"
            } else {
                match state.discord_presence.as_deref() {
                    Some("idle") => "idle",
                    Some("dnd") => "dnd",
                    Some("invisible") => "offline",
                    _ => "online",
                }
            }
            .into(),
        );
        #[cfg(feature = "discord")]
        {
            use yapfast_core::application::DiscordLink;
            let (phase, name, note) = match &state.discord_link {
                None => (0, "", ""),
                Some(DiscordLink::Starting) => (1, "", ""),
                Some(DiscordLink::Code(_)) => (2, "", ""),
                Some(DiscordLink::Scanned(name)) => (3, name.as_str(), ""),
                Some(DiscordLink::Captcha(_)) => (5, "", ""),
                Some(DiscordLink::Failed(error)) => (4, "", error.as_str()),
                Some(DiscordLink::Checking) => (6, "", ""),
            };
            window.set_link_phase(phase);
            window.set_link_name(name.into());
            window.set_link_note(note.into());
            if let Some(DiscordLink::Code(address)) = &state.discord_link
                && self
                    .link_code
                    .as_ref()
                    .is_none_or(|(shown, _)| shown != address)
            {
                self.link_code = qr_code(address).map(|image| (address.clone(), image));
                window.set_link_code(
                    self.link_code
                        .as_ref()
                        .map(|(_, image)| image.clone())
                        .unwrap_or_default(),
                );
            }
        }
        window.set_show_hidden_channels(state.show_hidden_channels);
        window.set_sync_discord_reads(state.sync_discord_reads);
    }

    /// The rail, the channel or DM list, and the friends and requests screens,
    /// built again only when what they show changed.
    pub(super) fn show_sidebar(&mut self, window: &YapfastWindow) {
        if self
            .sidebar
            .as_ref()
            .is_some_and(|shown| shown.shows(&self.app.state, window))
        {
            return;
        }
        self.sidebar = Some(SidebarShown::of(&self.app.state, window));
        let state = &self.app.state;
        let query = window.get_filter().to_lowercase();
        window.set_sidebar_title(match &state.navigation {
            Navigation::DirectMessages => "Direct messages".into(),
            Navigation::Server(id) => state
                .servers
                .iter()
                .find(|server| &server.id == id)
                .map_or("Server".into(), |server| server.name.clone().into()),
        });
        // A Discord server's name opens its menu, as Discord's does.
        window.set_header_server(match &state.navigation {
            Navigation::Server(id)
                if !state.preview
                    && state
                        .servers
                        .iter()
                        .any(|server| server.discord && &server.id == id) =>
            {
                id.as_str().into()
            }
            _ => SharedString::default(),
        });
        window.set_header_invite(match &state.navigation {
            Navigation::Server(id) => self.app.header_choices(id).contains(&MenuChoice::Invite),
            Navigation::DirectMessages => false,
        });
        window.set_filter_placeholder(
            if state.navigation == Navigation::DirectMessages {
                "Find a DM"
            } else {
                "Find a channel"
            }
            .into(),
        );
        let rooms_by_id: HashMap<&str, &yapfast_core::chat::ChatRoom> = state
            .rooms
            .iter()
            .map(|room| (room.id.as_str(), &**room))
            .collect();
        self.rail_call = call_room(state).map(str::to_owned);
        let rail = rail(state, &rooms_by_id, &self.layout.open_folders);
        update_model_by_id(&self.models.servers, rail, |row| row.id.clone());
        let mut listed_text = false;
        let mut listed_voice = false;
        let mut category = None;
        let mut listed: Vec<_> = state
            .scoped_rooms()
            .filter(|room| room.name.to_lowercase().contains(&query))
            .collect();
        let direct = state.navigation == Navigation::DirectMessages;
        // Friend requests, message requests and spam have their own screen.
        let waiting = |name: &str| {
            state
                .scoped_rooms()
                .filter(|room| room.category == name)
                .collect::<Vec<_>>()
        };
        let (message_requests, spam) = (waiting(discord::REQUESTS), waiting(discord::SPAM));
        let requests = request_rows(
            state,
            &message_requests,
            &spam,
            self.layout.twelve_hour_clock,
        );
        window.set_requests_badge(count_badge(requests.incoming.len() as u64));
        update_model(&self.models.requests_incoming, requests.incoming);
        update_model(&self.models.requests_sent, requests.sent);
        update_model(&self.models.requests_spam, requests.spam);
        // The friends list, those around first; filtered by the screen's search.
        let friend_query = window.get_friend_filter().to_lowercase();
        let in_voice = voice_places(state, &rooms_by_id);
        let friend_rows: Vec<_> = state
            .friends
            .iter()
            .filter(|friend| friend.name.to_lowercase().contains(friend_query.trim()))
            .map(|friend| {
                let (activity, activity_emoji) = activity_line(friend.activity.as_deref());
                let voice = in_voice.get(friend.id.as_str());
                FriendRow {
                    id: friend.id.as_str().into(),
                    name: friend.name.as_str().into(),
                    initial: initial(&friend.name),
                    tint: tint(&friend.id),
                    picture: slint::Image::default(),
                    picture_url: friend.avatar.as_deref().unwrap_or_default().into(),
                    presence: presence_name(Some(friend.presence.unwrap_or(Presence::Offline)))
                        .into(),
                    activity,
                    activity_emoji,
                    voice_room: voice.map_or("", |place| place.room).into(),
                    voice_channel: voice.map_or("", |place| place.channel).into(),
                    voice_server: voice.map_or("", |place| place.server).into(),
                }
            })
            .collect();
        let around: Vec<_> = friend_rows
            .iter()
            .filter(|friend| friend.presence != "offline")
            .cloned()
            .collect();
        update_model(&self.models.friends_online, around);
        update_model(&self.models.friends, friend_rows);
        window.set_friend_count(i32::try_from(state.friends.len()).unwrap_or(i32::MAX));
        // Those blocked, for Settings.
        update_model(
            &self.models.blocked,
            state
                .discord_blocked
                .iter()
                .map(|blocked| BlockedRow {
                    id: blocked.id.as_str().into(),
                    name: blocked.name.as_str().into(),
                    username: blocked.username.as_deref().unwrap_or_default().into(),
                    avatar_url: blocked.avatar.as_deref().unwrap_or_default().into(),
                    initial: initial(&blocked.name),
                })
                .collect(),
        );
        window.set_friends_open(direct && state.dm_screen == Some(DmScreen::Friends));
        // Browse Channels, at the top of a Community server's channel list;
        // its rows only while it shows, filtered by its search.
        window.set_browse_offered(
            state.discord_online
                && match &state.navigation {
                    Navigation::Server(id) => state
                        .servers
                        .iter()
                        .any(|server| &server.id == id && server.community),
                    Navigation::DirectMessages => false,
                },
        );
        window.set_browse_open(state.browse.is_some());
        update_model(&self.models.browse_rows, self.browse_rows(state, window));
        let requests_open = direct && state.dm_screen == Some(DmScreen::Requests);
        window.set_requests_open(requests_open);
        window.set_requests_current(
            direct
                && (requests_open
                    || state.selected.as_ref().is_some_and(|selected| {
                        message_requests
                            .iter()
                            .chain(&spam)
                            .any(|room| &room.id == selected)
                    })),
        );
        if direct {
            listed.retain(|room| {
                room.category != discord::REQUESTS && room.category != discord::SPAM
            });
            // Pinned DMs come first, in the order they were pinned.
            listed.sort_by_key(|room| {
                state
                    .pinned
                    .iter()
                    .position(|id| *id == room.id)
                    .unwrap_or(usize::MAX)
            });
        }
        // A Discord server's pinned channels come first, each with its
        // threads, under a heading of their own; as in Discord, they leave
        // their categories rather than show twice.
        let pinned = match &state.navigation {
            Navigation::Server(id) => state
                .servers
                .iter()
                .find(|server| &server.id == id)
                .map_or(&[][..], |server| server.pinned.as_slice()),
            Navigation::DirectMessages => &[],
        };
        let pinned_at = |room: &yapfast_core::chat::ChatRoom| {
            pinned
                .iter()
                .position(|id| *id == room.id || room.parent.as_ref() == Some(id))
        };
        let mut in_pinned = 0;
        if !pinned.is_empty() {
            listed.sort_by_key(|room| pinned_at(room).unwrap_or(usize::MAX));
            in_pinned = listed
                .iter()
                .take_while(|room| pinned_at(room).is_some())
                .count();
        }
        let mut rows: Vec<_> = listed
            .into_iter()
            .enumerate()
            .map(|(index, room)| {
                let mut folds_by = None;
                let heading = if direct {
                    String::new()
                } else if index < in_pinned {
                    if index == 0 { "PINNED CHANNELS" } else { "" }.into()
                } else if room.capabilities.categories {
                    // Discord channels sit under their categories, as in
                    // Discord, which may share a name.
                    let this = (room.category_id.as_deref(), room.category.as_str());
                    if category.replace(this) != Some(this) {
                        folds_by = room.category_id.as_deref();
                        room.category.to_uppercase()
                    } else {
                        String::new()
                    }
                } else if room.voice && !listed_voice {
                    listed_voice = true;
                    "VOICE CHANNELS".into()
                } else if !room.voice && !listed_text {
                    listed_text = true;
                    "TEXT CHANNELS".into()
                } else {
                    String::new()
                };
                let (detail, detail_emoji) = dm_detail(room);
                let voice = room
                    .person
                    .as_deref()
                    .filter(|_| room.network == Network::Discord)
                    .and_then(|person| in_voice.get(person));
                RoomRow {
                    id: room.id.clone().into(),
                    name: room.name.clone().into(),
                    category: folds_by.unwrap_or(&heading).into(),
                    heading: heading.into(),
                    icon: self.icons.room(room),
                    selected: Some(&room.id) == state.selected.as_ref(),
                    unread: room.unread > 0,
                    badge: mention_badge(room),
                    voice: room.voice,
                    chat: room.voice_chat(),
                    direct: room.direct,
                    initial: initial(&room.name),
                    tint: tint(&room.id),
                    picture: slint::Image::default(),
                    picture_url: room.avatar.as_deref().unwrap_or_default().into(),
                    discord: room.direct && room.network == Network::Discord,
                    locked: room.locked || room.join_locked,
                    person: room.person.is_some(),
                    pinned: if room.direct {
                        state.pinned.contains(&room.id)
                    } else {
                        index < in_pinned
                    },
                    muted: room.muted,
                    people: if room.people.is_empty() {
                        ModelRc::default()
                    } else {
                        let people: Vec<_> = room
                            .people
                            .iter()
                            .map(|person| PersonRow {
                                id: person.id.clone().into(),
                                name: person.name.clone().into(),
                                initial: initial(&person.name),
                                tint: tint(&person.id),
                                picture: cached_picture(
                                    &mut self.pictures,
                                    state,
                                    person.avatar.as_deref(),
                                ),
                                muted: person.muted,
                                deafened: person.deafened,
                                server_muted: person.server_muted,
                                server_deafened: person.server_deafened,
                                live: person.streaming,
                                video: person.video,
                                local: state.discord_user.as_ref() == Some(&person.id),
                                elsewhere: person.elsewhere,
                                person: person.id.clone().into(),
                                connected: true,
                                ..Default::default()
                            })
                            .collect();
                        ModelRc::new(VecModel::from(people))
                    },
                    call: room.direct && !room.people.is_empty(),
                    // Discord sends the presence of friends and of those
                    // sharing a server; anyone else in a one-to-one DM shows
                    // offline, as in Discord's DM list.
                    presence: if room.person.is_some() && room.network == Network::Discord {
                        presence_name(room.presence.or(Some(Presence::Offline)))
                    } else {
                        presence_name(room.presence)
                    }
                    .into(),
                    detail,
                    detail_emoji,
                    voice_room: voice.map_or("", |place| place.room).into(),
                    voice_channel: voice.map_or("", |place| place.channel).into(),
                    voice_server: voice.map_or("", |place| place.server).into(),
                    thread: room.parent.is_some(),
                    ..Default::default()
                }
            })
            .collect();
        // A channel's threads follow it, so a thread's line runs on to the
        // next row when that's a thread too.
        for index in 1..rows.len() {
            if rows[index].thread && rows[index - 1].thread {
                rows[index - 1].thread_continues = true;
            }
        }
        // Categories fold as their server last left them, except while
        // filtering; one without channels shows as its heading alone. The
        // pinned channels above them don't fold.
        let rows = match &state.navigation {
            Navigation::Server(server) if query.is_empty() => {
                let mut rows = rows;
                let rest = rows.split_off(in_pinned);
                let rest = with_empty_categories(rest, &self.app.categories(server));
                rows.extend(match self.layout.collapsed.get(server) {
                    Some(collapsed) => fold_collapsed(
                        rest,
                        collapsed,
                        state.voice.room.as_deref().filter(|_| state.voice.active()),
                    ),
                    None => rest,
                });
                rows
            }
            _ => rows,
        };
        window.set_sidebar_empty(if !rows.is_empty() { "" }
            else if state.scoped_rooms().next().is_some() { "No matching conversations." }
            else if state.navigation == Navigation::DirectMessages { "No direct messages yet. Existing DMs appear here after syncing." }
            else if state.servers.iter().any(|server| server.discord && state.navigation == Navigation::Server(server.id.clone())) { "No channels in this server that you can see." }
            else { "No joined channels in this server. Join a channel in another Matrix client to see it here." }.into());
        // A Discord server's channels and categories drag to move, by their
        // places in the list: each category, and each channel that shows
        // outside a collapsed one, as Discord's client lists them to drop
        // on. Pinned channels and threads stay where they are.
        let mut rows = rows;
        let mut places = Vec::new();
        if let Navigation::Server(server) = &state.navigation
            && query.is_empty()
            && state
                .servers
                .iter()
                .any(|known| known.discord && &known.id == server)
        {
            for row in rows.iter_mut().skip(in_pinned) {
                if !row.heading.is_empty() {
                    places.push(row.category.to_string());
                    row.heading_place = places.len() as i32;
                }
                if !row.hidden && !row.thread && !row.collapsed {
                    places.push(row.id.to_string());
                    row.place = places.len() as i32;
                }
            }
        }
        // A voice channel's people come as a new list each time; the one on
        // screen stays while they're the same, so its row counts as unchanged.
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(index, mut row)| {
                if let Some(shown) = self.models.rooms.row_data(index)
                    && shown.id == row.id
                {
                    row.people = same_list(&shown.people, row.people);
                }
                row
            })
            .collect();
        update_model(&self.models.rooms, rows);
        self.places = places;
        // What's being dragged keeps going, at its new place.
        if self.arranging.is_some() {
            self.show_arranging(window);
        }
    }

    /// The channel or category being dragged in a Discord server's channel
    /// list, by its place, and the places it may drop on; it stops if it
    /// left the list.
    pub(super) fn show_arranging(&mut self, window: &YapfastWindow) {
        let dragged = self
            .arranging
            .as_ref()
            .and_then(|room| self.places.iter().position(|place| place == room))
            .filter(|_| {
                self.arranging
                    .as_deref()
                    .is_some_and(|room| self.app.draggable(room))
            });
        let Some(dragged) = dragged else {
            self.arranging = None;
            window.set_room_arranging(0);
            return;
        };
        window.set_room_arranging(dragged as i32 + 1);
        window.set_room_droppable(ModelRc::new(VecModel::from(
            self.app.droppable(&self.places, dragged),
        )));
    }

    /// The account at the bottom of the sidebar, and the notice.
    fn show_account(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let account = state.account.as_deref().unwrap_or("Preview");
        let account_name = state
            .account_name
            .as_deref()
            .unwrap_or_else(|| display_part(account));
        window.set_account(account.into());
        window.set_account_name(account_name.into());
        window.set_account_initial(initial(account_name));
        window.set_account_tint(tint(account_name));
        window.set_account_picture(cached_picture(
            &mut self.pictures,
            state,
            state.account_avatar.as_deref(),
        ));
        let (custom_status, status_emoji) = state
            .custom_status
            .as_ref()
            .filter(|_| state.online && !state.preview)
            .map(status_line)
            .unzip();
        window.set_account_status_emoji(status_emoji.flatten().unwrap_or_default().into());
        window.set_account_status(
            if state.preview {
                "Local only"
            } else if let Some(status) = &custom_status {
                status
            } else if let Some(status) = &state.account_status {
                status
            } else if state.online {
                "Online"
            } else {
                "Offline, sync paused"
            }
            .into(),
        );
        window.set_has_custom_status(state.custom_status.is_some());
        window.set_show_game(state.discord_show_game);
        let playing = state.discord_playing.as_ref();
        window.set_game_activity_name(playing.map_or("", |playing| &playing.name).into());
        window.set_discord_status(state.discord_presence.as_deref().unwrap_or("online").into());
        window.set_storage(state.storage.unwrap_or_default().into());
        window.set_notice(state.notice.as_deref().unwrap_or_default().into());
    }

    /// Starting a thread: what the open channel offers, and the "New
    /// Thread" form beside it.
    fn show_new_thread(&mut self, window: &YapfastWindow) {
        let app = &self.app;
        let state = &app.state;
        window.set_can_start_threads(app.can_start_threads());
        window.set_can_create_polls(app.can_create_polls());
        window.set_threads_can_create(state.thread_list().is_some_and(|list| {
            let (public, private) = app.thread_starts(&list.room);
            public || private
        }));
        let form = state.new_thread();
        window.set_new_thread_open(form.is_some());
        let Some(form) = form else {
            return;
        };
        // The fields are the window's while it's open; Yapfast writes them only
        // when they differ, so the caret stays.
        if window.get_new_thread_name() != form.name.as_str() {
            window.set_new_thread_name(form.name.as_str().into());
        }
        if window.get_new_thread_body() != form.body.as_str() {
            window.set_new_thread_body(form.body.as_str().into());
        }
        window.set_new_thread_placeholder(
            match form.default_name.as_str() {
                "" => "New Thread",
                name => name,
            }
            .into(),
        );
        let starter = form.message.as_deref().and_then(|item| state.message(item));
        window.set_new_thread_from_message(starter.is_some());
        if let Some(starter) = starter {
            window.set_new_thread_starter(super::messages::found_row(
                starter,
                self.layout.twelve_hour_clock,
            ));
        }
        window.set_new_thread_can_private(form.can_private);
        window.set_new_thread_only_private(form.only_private);
        window.set_new_thread_private(form.private);
        window.set_new_thread_starting(form.starting);
        window.set_new_thread_error(form.error.as_deref().unwrap_or_default().into());
        window.set_new_thread_leaving(form.leaving);
    }

    /// The open channel's list of its threads, built again only once what
    /// it shows changes, or a minute passes.
    /// Who reacted: the tooltip over the reaction the pointer stays on, and
    /// the Reactions window.
    fn show_reactions(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let emoji_of = |item: &str, key: &str| -> String {
            state.message(item).map_or_else(
                || key.to_owned(),
                |message| custom_emoji::text(key, &message.emoji),
            )
        };
        match state.reaction_tip() {
            Some((tip, reaction)) => {
                let names = tip.names.as_deref().unwrap_or_default();
                window.set_reaction_tip_text(
                    messages::reaction_tip(&emoji::shortcode(&tip.key), names, reaction.count)
                        .into(),
                );
                window.set_reaction_tip_emoji(emoji_of(&tip.item, &tip.key).into());
            }
            None => {
                window.set_reaction_tip_text(SharedString::default());
                window.set_reaction_tip_emoji(SharedString::default());
            }
        }
        let shown = state.reactions.as_ref();
        window.set_reactions_open(shown.is_some());
        let Some(shown) = shown else {
            if self.models.reaction_pills.row_count() > 0
                || self.models.reactor_rows.row_count() > 0
            {
                self.models.reaction_pills.set_vec(Vec::new());
                self.models.reactor_rows.set_vec(Vec::new());
            }
            return;
        };
        window.set_reactions_key(shown.key.as_str().into());
        window.set_reactions_voters(shown.poll);
        window.set_reactors_loading(shown.loading);
        window.set_reactors_more(state.more_reactors());
        window.set_reactors_error(shown.error.clone().unwrap_or_default().into());
        let pills = if shown.poll {
            state
                .window_answers()
                .iter()
                .map(|answer| ReactionPill {
                    key: answer.id.to_string().into(),
                    label: answer
                        .emoji
                        .as_deref()
                        .map(|key| emoji_of(&shown.item, key))
                        .unwrap_or_default()
                        .into(),
                    name: answer.text.as_str().into(),
                    count: i32::try_from(answer.votes).unwrap_or(i32::MAX),
                })
                .collect()
        } else {
            state
                .window_reactions()
                .into_iter()
                .map(|reaction| ReactionPill {
                    key: reaction.key.as_str().into(),
                    label: emoji_of(&shown.item, &reaction.key).into(),
                    name: emoji::shortcode(&reaction.key).into(),
                    count: i32::try_from(reaction.count).unwrap_or(i32::MAX),
                })
                .collect()
        };
        update_model(&self.models.reaction_pills, pills);
        let rows = shown
            .people
            .iter()
            .map(|person| ReactorRow {
                id: person.id.as_str().into(),
                name: person.name.as_str().into(),
                username: person.username.as_str().into(),
                avatar: person.avatar.as_str().into(),
                initial: initial(&person.name),
                tint: tint(&person.name),
                removable: !shown.poll && (person.me || shown.can_remove),
            })
            .collect();
        update_model(&self.models.reactor_rows, rows);
    }

    fn show_threads(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        window.set_can_list_threads(state.threads_channel().is_some());
        let list = state.thread_list();
        if list.is_some() && !window.get_threads_open() {
            window.set_threads_query(SharedString::default());
        }
        window.set_threads_open(list.is_some());
        let Some(list) = list else {
            if self.threads_shown.take().is_some() {
                self.models.thread_rows.set_vec(Vec::new());
            }
            return;
        };
        let channel = &list.room;
        window.set_threads_searchable(
            self.app
                .may(channel, yapfast_core::chat::Permission::ReadHistory),
        );
        window.set_threads_more(list.more && list.search.is_none());
        window.set_threads_loading(list.loading);
        let sections = self.app.thread_sections().unwrap_or_default();
        let found = list.search.as_ref().map(|search| &search.found);
        // While a search shows, its results stand in for the sections.
        let (note, empty) = match found {
            Some(None) => ("Searching...".to_owned(), false),
            Some(Some(Err(error))) => (error.clone(), false),
            Some(Some(Ok(found))) => (String::new(), found.is_empty()),
            None if sections.is_empty() && list.loading => ("Loading threads...".into(), false),
            None if sections.is_empty() => {
                (list.error.clone().unwrap_or_default(), list.error.is_none())
            }
            None => (String::new(), false),
        };
        window.set_threads_note(note.into());
        window.set_threads_empty(empty);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |now| now.as_millis() as u64);
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        (&sections, found, now / 60_000).hash(&mut hasher);
        let shown = hasher.finish();
        if self.threads_shown == Some(shown) {
            return;
        }
        self.threads_shown = Some(shown);
        let mut rows = Vec::new();
        match found {
            Some(Some(Ok(found))) => {
                rows.extend(found.iter().map(|thread| thread_row(thread, now)));
            }
            _ => {
                let headed = [
                    (&sections.joined, "joined thread", "joined threads"),
                    (
                        &sections.other,
                        "other active thread",
                        "other active threads",
                    ),
                ];
                for (threads, one, many) in headed {
                    let heading = match threads.len() {
                        1 => format!("1 {one}"),
                        count => format!("{count} {many}"),
                    };
                    push_section(&mut rows, threads, heading.to_uppercase(), now);
                }
                push_section(&mut rows, &sections.older, "OLDER THREADS".into(), now);
            }
        }
        let headings = rows.iter().filter(|row| !row.heading.is_empty()).count();
        window.set_threads_headings(headings as i32);
        self.models.thread_rows.set_vec(rows);
    }

    /// The open Discord forum's posts, in place of messages.
    fn show_forum(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let forum = state.forum();
        window.set_room_forum(state.selected_room().is_some_and(|room| room.forum));
        let Some(forum) = forum else {
            if self.forum_shown.take().is_some() {
                self.models.post_rows.set_vec(Vec::new());
                window.set_posts_query(SharedString::default());
            }
            return;
        };
        let room = &forum.list.room;
        window.set_posts_searchable(
            self.app
                .may(room, yapfast_core::chat::Permission::ReadHistory),
        );
        let posts = self.app.forum_posts().unwrap_or_default();
        let info = self.app.forum_info(room).unwrap_or_default();
        let cards = posts.active.len() + posts.older.len();
        window.set_posts_more(forum.wants_more(cards));
        window.set_posts_loading(forum.loading());
        window.set_posts_searching(posts.found.is_some());
        let (note, empty) = match &posts.found {
            Some(None) => ("Searching...".to_owned(), false),
            Some(Some(Err(error))) => (error.clone(), false),
            Some(Some(Ok(found))) => (String::new(), found.is_empty()),
            None if cards == 0 && forum.list.loading => ("Loading posts...".into(), false),
            None if cards == 0 => (
                forum.list.error.clone().unwrap_or_default(),
                forum.list.error.is_none(),
            ),
            None => (String::new(), false),
        };
        window.set_posts_note(note.into());
        window.set_posts_empty(empty);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |now| now.as_millis() as u64);
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        (&posts, room, now / 60_000).hash(&mut hasher);
        let shown = hasher.finish();
        if self.forum_shown == Some(shown) {
            return;
        }
        self.forum_shown = Some(shown);
        let row = |post| post_row(post, &info, now);
        let rows: Vec<PostRow> = match &posts.found {
            Some(Some(Ok(found))) => found.iter().map(row).collect(),
            _ => {
                let mut rows: Vec<PostRow> = posts.active.iter().map(row).collect();
                let older = posts
                    .older
                    .iter()
                    .map(row)
                    .enumerate()
                    .map(|(index, mut row)| {
                        if index == 0 {
                            row.heading = "OLDER POSTS".into();
                        }
                        row
                    });
                rows.extend(older);
                rows
            }
        };
        self.models.post_rows.set_vec(rows);
    }

    /// The form for a post in the forum on screen.
    fn show_new_post(&mut self, window: &YapfastWindow) {
        let app = &self.app;
        let state = &app.state;
        window.set_can_post(app.can_post());
        let post = state.new_post();
        window.set_post_composing(post.is_some());
        let Some(post) = post else {
            if self.post_tags_shown.take().is_some() {
                self.models.post_chosen_tags.set_vec(Vec::new());
                self.models.post_tag_choices.set_vec(Vec::new());
            }
            return;
        };
        // The fields are the window's while it's open; Yapfast writes them only
        // when they differ, so the caret stays.
        if window.get_post_title() != post.title.as_str() {
            window.set_post_title(post.title.as_str().into());
        }
        if window.get_post_body() != post.body.as_str() {
            window.set_post_body(post.body.as_str().into());
        }
        let info = app.forum_info(&post.room).unwrap_or_default();
        window.set_post_tag_required(info.require_tag);
        window.set_post_more_tags(post.tags.len() < yapfast_core::application::POST_TAGS);
        let topic = state.topic.as_ref();
        window.set_post_has_guidelines(topic.is_some_and(|topic| !topic.body.trim().is_empty()));
        if !matches!((topic, &self.guidelines_shown), (Some(topic), Some(shown)) if Arc::ptr_eq(topic, shown))
        {
            self.guidelines_shown = topic.cloned();
            window.set_post_guidelines(topic.map_or_else(slint::StyledText::default, |topic| {
                super::messages::topic_text(topic, false)
            }));
        }
        window.set_posting(post.posting);
        window.set_post_error(post.error.as_deref().unwrap_or_default().into());
        window.set_post_leaving(post.leaving);
        let tags = app.post_tags(&post.room);
        let shown = (tags.clone(), post.tags.clone());
        if self.post_tags_shown.as_ref() == Some(&shown) {
            return;
        }
        let choice = |tag: &yapfast_core::chat::ForumTag| PostTagChoice {
            id: tag.id.as_str().into(),
            tag: post_tag(&tag.name, tag.emoji.as_ref()),
            chosen: post.tags.contains(&tag.id),
        };
        let chosen: Vec<PostTagChoice> = post
            .tags
            .iter()
            .filter_map(|id| tags.iter().find(|tag| &tag.id == id))
            .map(choice)
            .collect();
        let choices: Vec<PostTagChoice> = tags.iter().map(choice).collect();
        self.models.post_chosen_tags.set_vec(chosen);
        self.models.post_tag_choices.set_vec(choices);
        self.post_tags_shown = Some(shown);
    }

    /// The open conversation's header.
    fn show_header(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let room = state.selected_room();
        window.set_room_selected(room.is_some());
        window.set_room_name(room.map_or("", |room| room.name.as_str()).into());
        window.set_room_direct(room.is_some_and(|room| room.direct));
        window.set_room_presence(presence_name(room.and_then(|room| room.presence)).into());
        let (activity, activity_emoji) =
            activity_line(room.and_then(|room| room.activity.as_deref()));
        window.set_room_activity(activity);
        window.set_room_activity_emoji(activity_emoji);
        window.set_profile_available(state.one_to_one().is_some());
        window.set_room_request(room.is_some_and(|room| room.request));
        window.set_request_title(
            match room {
                Some(room) if room.category == discord::SPAM => {
                    format!("Discord thinks {} may be spam", room.name)
                }
                Some(room) => format!("{} wants to message you", room.name),
                None => String::new(),
            }
            .into(),
        );
        window.set_room_initial(room.map_or(SharedString::default(), |room| initial(&room.name)));
        window.set_room_tint(room.map_or(slint::Color::default(), |room| tint(&room.id)));
        window.set_room_picture(cached_picture(
            &mut self.pictures,
            state,
            room.and_then(|room| room.avatar.as_deref()),
        ));
        let parent = room
            .and_then(|room| room.parent.as_deref())
            .and_then(|parent| state.rooms.iter().find(|listed| listed.id == parent));
        window.set_room_parent(parent.map_or("", |parent| parent.name.as_str()).into());
        window.set_room_parent_id(parent.map_or("", |parent| parent.id.as_str()).into());
        window.set_room_parent_forum(parent.is_some_and(|parent| parent.forum));
        window.set_room_icon(if room.is_some_and(|room| room.parent.is_some()) {
            self.icons.thread.clone()
        } else if room.is_some_and(|room| room.voice) {
            self.icons.voice.clone()
        } else if room.is_some_and(|room| room.forum) {
            self.icons.forum.clone()
        } else if room.is_some_and(|room| room.direct) {
            self.icons.dm.clone()
        } else {
            self.icons.text.clone()
        });
        // Discord counts a channel's members only once its list comes.
        window.set_members(
            room.map_or(SharedString::default(), |room| match room.members {
                // A forum has no member list.
                _ if room.forum => SharedString::default(),
                0 => "Members".into(),
                1 => "1 member".into(),
                count => format!("{count} members").into(),
            }),
        );
        window.set_members_available(state.lists_members());
        window.set_room_id(state.selected.as_deref().unwrap_or_default().into());
        window.set_room_muted(room.is_some_and(|room| room.muted));
        window.set_can_notify(state.selected.as_deref().is_some_and(|id| {
            state
                .menu_choices(id)
                .contains(&yapfast_core::application::MenuChoice::ChannelNotifications)
        }));
        // A forum's topic is its post guidelines, which the form shows.
        let topic = state
            .topic
            .as_ref()
            .filter(|_| !room.is_some_and(|room| room.forum));
        window.set_has_topic(topic.is_some());
        let same = match (topic, &self.topic_shown) {
            (Some(topic), Some(shown)) => Arc::ptr_eq(topic, shown),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.topic_shown = topic.cloned();
            let styled = |one_line| {
                topic.map_or_else(slint::StyledText::default, |topic| {
                    topic_text(topic, one_line)
                })
            };
            window.set_room_topic(styled(true));
            window.set_room_topic_full(styled(false));
        }
    }

    /// What the open DM's header opens: its pinned messages, what its
    /// search found, and the friends picker.
    fn show_dm_tools(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let twelve_hour = self.layout.twelve_hour_clock;
        window.set_can_pin(
            state
                .conversation_that(|can| can.pins)
                .is_some_and(|room| !room.forum),
        );
        window.set_can_search(state.conversation_that(|can| can.search).is_some());
        window.set_can_add_people(
            state.discord_online && state.dm_that(|can| can.group_dms).is_some(),
        );
        let selected = state.selected.as_deref();
        let pins = state
            .pins
            .as_ref()
            .filter(|pins| Some(pins.room.as_str()) == selected);
        window.set_pins_open(pins.is_some());
        window.set_pins_note(
            match pins.map(|pins| &pins.messages) {
                Some(None) => "Loading pinned messages...",
                Some(Some(Err(error))) => error.as_str(),
                Some(Some(Ok(pinned))) if pinned.is_empty() => {
                    if state.selected_room().is_some_and(|room| room.direct) {
                        "This conversation doesn't have any pinned messages yet."
                    } else {
                        "This channel doesn't have any pinned messages yet."
                    }
                }
                _ => "",
            }
            .into(),
        );
        let pinned = match pins.and_then(|pins| pins.messages.as_ref()) {
            Some(Ok(pinned)) => pinned.as_slice(),
            _ => &[],
        };
        show_found(&self.models.pins, pinned, twelve_hour);

        let search = state.search_shown();
        let server = search.is_some_and(|search| discord::parse_server(&search.place).is_some());
        // The box names where it looks: the DM, or the channel's server.
        let place = state.search_place();
        let looks_in = place
            .as_deref()
            .and_then(|place| state.servers.iter().find(|server| server.id == place))
            .map(|server| server.name.as_str())
            .or(state.selected_room().map(|room| room.name.as_str()))
            .unwrap_or_default();
        window.set_search_placeholder(format!("Search {looks_in}").into());
        window.set_search_open(search.is_some());
        window.set_search_loading(search.is_some_and(|search| search.loading));
        window.set_search_more(search.is_some_and(Search::more));
        window.set_search_heading(
            match search {
                Some(search) if search.results.is_empty() && search.loading => {
                    "Searching...".into()
                }
                Some(search) if search.results.is_empty() && search.error.is_some() => {
                    "Search".into()
                }
                Some(Search { total: 1, .. }) => "1 Result".into(),
                Some(search) => format!("{} Results", search.total),
                None => String::new(),
            }
            .into(),
        );
        window.set_search_note(
            match search {
                Some(Search {
                    error: Some(error), ..
                }) => error.as_str(),
                Some(search) if !search.loading && search.results.is_empty() => {
                    if server {
                        "Nothing in this server matches. Try other words."
                    } else {
                        "Nothing in this conversation matches. Try other words."
                    }
                }
                _ => "",
            }
            .into(),
        );
        show_hits(
            &self.models.search_results,
            state,
            search.map_or(&[][..], |search| &search.results),
            server,
            twelve_hour,
        );

        let picker = state
            .add_people
            .as_ref()
            .filter(|picker| Some(picker.room.as_str()) == selected);
        if picker.is_some() && !window.get_add_people_open() {
            window.set_add_people_query(SharedString::default());
        }
        window.set_add_people_open(picker.is_some());
        window.set_add_people_one_to_one(state.one_to_one().is_some());
        window.set_add_people_room(state.group_room() as i32);
        window.set_add_people_chosen(picker.map_or(0, |picker| picker.chosen.len() as i32));
        window.set_add_people_adding(picker.is_some_and(|picker| picker.adding));
        window.set_add_people_note(
            picker
                .and_then(|picker| picker.error.as_deref())
                .unwrap_or_default()
                .into(),
        );
        // Friends' presence changes often, and a refresh comes with each
        // change; the rows are built again only when what they show changed.
        let friends = picker.map(|_| state.addable_friends());
        let shown = friends.as_ref().zip(picker).map(|(friends, picker)| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::hash::DefaultHasher::new();
            (friends, &picker.chosen).hash(&mut hasher);
            hasher.finish()
        });
        if shown == self.picker_shown {
            return;
        }
        self.picker_shown = shown;
        let people = match friends.zip(picker) {
            Some((friends, picker)) => friends
                .into_iter()
                .map(|friend| PickRow {
                    id: friend.id.clone().into(),
                    name: friend.name.clone().into(),
                    initial: initial(&friend.name),
                    tint: tint(&friend.name),
                    picture_url: friend.avatar.clone().unwrap_or_default().into(),
                    presence: presence_name(friend.presence).into(),
                    chosen: picker.chosen.contains(&friend.id),
                })
                .collect(),
            None => vec![],
        };
        update_model(&self.models.add_people, people);
    }

    /// The member list, the reactions offered on messages, and a role's card.
    fn show_members(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let now = yapfast_core::clock::now_ms();
        let members = match &state.members {
            Some(Ok(members)) => members.as_slice(),
            _ => &[],
        };
        window.set_roster_note(
            match &state.members {
                None => "Loading members...",
                Some(Err(error)) => *error,
                Some(Ok(members)) if members.is_empty() => "No one else has joined yet.",
                Some(Ok(_)) => "",
            }
            .into(),
        );
        update_model(
            &self.models.roster,
            members
                .iter()
                .enumerate()
                .map(|(index, member)| {
                    let (activity, activity_emoji) = activity_line(member.activity.as_deref());
                    MemberRow {
                        id: member.id.clone().into(),
                        name: member.name.clone().into(),
                        initial: initial(&member.name),
                        tint: tint(&member.name),
                        picture: slint::Image::default(),
                        picture_url: member.avatar.as_deref().unwrap_or_default().into(),
                        timed_out: member.timed_out_until.is_some_and(|until| until > now),
                        presence: presence_name(member.presence).into(),
                        color: role_color(member.color),
                        has_color: member.color.is_some(),
                        // One line under the name, as Discord shows it.
                        activity,
                        activity_emoji,
                        // Discord names its groups; Matrix members group by role.
                        heading: if let Some(group) = &member.group {
                            if index == 0 || members[index - 1].group.as_ref() != Some(group) {
                                group.to_uppercase().into()
                            } else {
                                SharedString::default()
                            }
                        } else if index == 0 || members[index - 1].role != member.role {
                            let count = members
                                .iter()
                                .filter(|other| other.role == member.role)
                                .count();
                            let role = match member.role {
                                Role::Admin => "ADMINS",
                                Role::Moderator => "MODERATORS",
                                Role::Member => "MEMBERS",
                            };
                            format!("{role} — {count}").into()
                        } else {
                            SharedString::default()
                        },
                    }
                })
                .collect(),
        );
        update_model(
            &self.models.bar_reactions,
            self.app
                .frequent_reactions(3)
                .iter()
                .map(|key| emoji::cell(key))
                .collect(),
        );
        update_model(
            &self.models.menu_reactions,
            self.app
                .frequent_reactions(4)
                .iter()
                .map(|key| emoji::cell(key))
                .collect(),
        );
        let role = state.role.as_ref();
        window.set_role_open(role.is_some());
        if let Some(role) = role {
            window.set_role_name(role.name.clone().into());
            window.set_role_color(role_color(role.color));
            window.set_role_has_color(role.color.is_some());
            window.set_role_note(
                match &role.members {
                    None => "Loading...",
                    Some(members) if members.is_empty() => {
                        "Yapfast hasn't seen anyone with this role yet."
                    }
                    Some(_) => "",
                }
                .into(),
            );
        }
        let role_members: Vec<MemberRow> = role
            .and_then(|role| role.members.as_ref())
            .into_iter()
            .flatten()
            .map(|member| MemberRow {
                id: member.id.clone().into(),
                name: member.name.clone().into(),
                initial: initial(&member.name),
                tint: tint(&member.name),
                picture: slint::Image::default(),
                picture_url: member.avatar.as_deref().unwrap_or_default().into(),
                color: role_color(member.color),
                has_color: member.color.is_some(),
                ..Default::default()
            })
            .collect();
        update_model(&self.models.role_members, role_members);
    }

    /// A Discord server's or channel's invite dialog.
    fn show_invite(&self, window: &YapfastWindow) {
        let invite = self.app.state.invite.as_ref();
        // A dialog opening starts from the link it makes first.
        if let Some(invite) = invite
            && !window.get_invite_open()
        {
            let index = |list: &[u32], value| {
                list.iter()
                    .position(|known| *known == value)
                    .map_or(0, |index| index as i32)
            };
            window.set_invite_age(index(&INVITE_AGES, invite.max_age));
            window.set_invite_uses(index(&INVITE_USES, invite.max_uses));
        }
        window.set_invite_open(invite.is_some());
        let Some(invite) = invite else {
            return;
        };
        let lasts = match invite.max_age {
            0 => "never expires".to_owned(),
            1800 => "expires in 30 minutes".to_owned(),
            3600 => "expires in an hour".to_owned(),
            86_400 => "expires in a day".to_owned(),
            age if age % 86_400 == 0 => format!("expires in {} days", age / 86_400),
            age => format!("expires in {} hours", age / 3600),
        };
        let works = match invite.max_uses {
            0 => String::new(),
            1 => " and works once".to_owned(),
            uses => format!(" and works {uses} times"),
        };
        let place = if invite.voice {
            invite.channel.clone()
        } else {
            format!("#{}", invite.channel)
        };
        window.set_invite_prompt(InvitePrompt {
            title: format!("Invite friends to {}", invite.server).into(),
            detail: format!("They'll land in {place}.").into(),
            link: invite.link.as_deref().unwrap_or_default().into(),
            note: if invite.link.is_some() {
                format!("This link {lasts}{works}.").into()
            } else {
                SharedString::default()
            },
            making: invite.making,
            error: invite.error.as_deref().unwrap_or_default().into(),
        });
    }

    /// Join a Server, or the dialog a clicked invite link opens.
    fn show_join(&self, window: &YapfastWindow) {
        let state = &self.app.state;
        window.set_can_join_server(state.discord_online || state.preview);
        let join = state.join.as_ref();
        // A dialog opening starts with an empty field.
        if join.is_some() && !window.get_join_open() {
            window.set_join_link(SharedString::default());
        }
        window.set_join_open(join.is_some());
        let Some(join) = join else {
            return;
        };
        let preview = join.preview.as_ref();
        let detail = preview.map(|preview| match (preview.members, preview.online) {
            (Some(members), Some(online)) if members >= 5 || online > 0 => {
                format!("{online} online, {members} members")
            }
            _ if preview.voice => preview.channel.clone(),
            _ => format!("#{}", preview.channel),
        });
        let confirm = if join.form.is_some() {
            "Open in browser"
        } else if join.member {
            "Open Server"
        } else if preview.is_some() {
            "Join Server"
        } else if join.from_link {
            ""
        } else {
            "Find Invite"
        };
        let status = if join.form.is_some() {
            "You joined. This server asks new members to fill in a form before they can talk, which Yapfast can't show yet. Finish it in Discord's web app."
        } else if join.captcha.is_some() {
            "Discord asks for a captcha first. Solve it in your browser."
        } else if join.joining {
            "Joining..."
        } else if join.resolving {
            "Looking up the invite..."
        } else if join.member {
            "You're already in this server."
        } else {
            ""
        };
        window.set_join_prompt(JoinPrompt {
            title: if join.from_link {
                "You've been invited to join a server"
            } else {
                "Join a Server"
            }
            .into(),
            field: !join.from_link,
            found: preview.is_some(),
            name: preview
                .map(|preview| preview.name.as_str())
                .unwrap_or_default()
                .into(),
            picture_url: preview
                .and_then(|preview| preview.icon.as_deref())
                .unwrap_or_default()
                .into(),
            initial: preview
                .map(|preview| initial(&preview.name))
                .unwrap_or_default(),
            detail: detail.unwrap_or_default().into(),
            confirm: confirm.into(),
            busy: join.resolving || join.joining,
            status: status.into(),
            captcha: join.captcha.is_some(),
            form: join.form.is_some(),
            error: join.error.as_deref().unwrap_or_default().into(),
        });
    }

    /// The dialog that makes, copies or deletes a Discord channel or
    /// category.
    fn show_channel_dialog(&self, window: &YapfastWindow) {
        let dialog = self.app.state.channel_dialog.as_ref();
        // Its fields start from the channel's, as it opens.
        if let Some(dialog) = dialog
            && !window.get_channel_open()
        {
            let settings = dialog.settings.clone().unwrap_or_default();
            window.set_channel_name(settings.name.into());
            window.set_channel_kind(i32::from(dialog.kind == ChannelKind::Voice));
        }
        window.set_channel_open(dialog.is_some());
        let Some(dialog) = dialog else {
            return;
        };
        let category = dialog.kind == ChannelKind::Category;
        let thing = if category { "category" } else { "channel" };
        let named = |name: &str| {
            if dialog.kind == ChannelKind::Text {
                format!("#{name}")
            } else {
                name.to_owned()
            }
        };
        let (title, detail, confirm) = match &dialog.ask {
            ChannelAsk::Create { .. } => (
                format!("Create {thing}"),
                if dialog.name.is_empty() {
                    format!("In {}", dialog.server_name)
                } else {
                    format!("In {}", dialog.name)
                },
                format!("Create {thing}"),
            ),
            ChannelAsk::Duplicate(_) => (
                "Duplicate channel".to_owned(),
                format!(
                    "A copy of {}, with its settings and permissions, in the same category.",
                    named(&dialog.name)
                ),
                "Create channel".to_owned(),
            ),
            ChannelAsk::Delete(_) => (
                format!("Delete {thing}"),
                format!(
                    "Are you sure you want to delete {}? This cannot be undone.",
                    named(&dialog.name)
                ),
                format!("Delete {thing}"),
            ),
        };
        let deleting = matches!(dialog.ask, ChannelAsk::Delete(_));
        window.set_channel_prompt(ChannelPrompt {
            title: title.into(),
            detail: detail.into(),
            confirm: confirm.into(),
            danger: deleting,
            kinds: matches!(dialog.ask, ChannelAsk::Create { .. }) && !category,
            named: !deleting,
            field: if category {
                "CATEGORY NAME"
            } else {
                "CHANNEL NAME"
            }
            .into(),
            placeholder: if category {
                "New Category"
            } else {
                "new-channel"
            }
            .into(),
            saving: dialog.saving,
            error: dialog.error.as_deref().unwrap_or_default().into(),
            ..Default::default()
        });
    }

    /// The editor of this account's Discord profile, its main one or a
    /// server's, as its profile card shows it.
    pub(super) fn show_profile_editor(&mut self, window: &YapfastWindow) {
        use yapfast_core::application::{BIO_LIMIT, NAME_LIMIT, PRONOUNS_LIMIT, ProfileText};
        let editor = self.app.state.profile_editor.as_ref();
        // Its texts follow the editor's: what Discord has once that's here,
        // and again after Reset. Typing changes both alike.
        if let Some(editor) = editor {
            macro_rules! follow {
                ($text:expr, $get:ident, $set:ident) => {
                    if window.$get() != $text.as_str() {
                        window.$set($text.as_str().into());
                    }
                };
            }
            follow!(
                editor.name,
                get_profile_editor_name,
                set_profile_editor_name
            );
            follow!(
                editor.pronouns,
                get_profile_editor_pronouns,
                set_profile_editor_pronouns
            );
            follow!(editor.bio, get_profile_editor_bio, set_profile_editor_bio);
            follow!(
                editor.colour,
                get_profile_editor_colour,
                set_profile_editor_colour
            );
        }
        let Some(editor) = editor else {
            // Its pictures go with it.
            if window.get_profile_editor_open() {
                window.set_profile_editor_open(false);
                window.set_profile_editor_prompt(ProfileEditorPrompt::default());
            }
            return;
        };
        window.set_profile_editor_open(true);
        let main = editor.main();
        let fallback = editor.profile.as_ref().map(|profile| &profile.fallback);
        let fallback_text = |text: Option<&String>| text.cloned().unwrap_or_default();
        let (avatar, new_avatar) = editor.picture(ProfilePicture::Avatar);
        let (banner, new_banner) = editor.picture(ProfilePicture::Banner);
        let written = |text: &str, fallback: Option<&String>| {
            let text = text.trim();
            if text.is_empty() {
                fallback_text(fallback)
            } else {
                text.to_owned()
            }
        };
        let colour = editor.accent();
        let accent = if editor.may_write(ProfileText::Colour) {
            colour.unwrap_or(editor.profile.as_ref().and_then(|profile| profile.accent))
        } else {
            editor.profile.as_ref().and_then(|profile| profile.accent)
        };
        // The bio as written is Markdown, where the fallback's was rendered.
        let view = ProfileView {
            profile: yapfast_core::chat::Profile {
                id: fallback
                    .map(|fallback| fallback.id.clone())
                    .unwrap_or_default(),
                name: written(&editor.name, fallback.map(|fallback| &fallback.name)),
                handle: fallback_text(fallback.map(|fallback| &fallback.handle)),
                avatar: avatar
                    .map(str::to_owned)
                    .or_else(|| fallback.and_then(|fallback| fallback.avatar.clone())),
                banner: banner
                    .map(str::to_owned)
                    .or_else(|| fallback.and_then(|fallback| fallback.banner.clone())),
                accent: accent.or_else(|| fallback.and_then(|fallback| fallback.accent)),
                pronouns: written(
                    &editor.pronouns,
                    fallback.map(|fallback| &fallback.pronouns),
                ),
                bio: written(&editor.bio, fallback.map(|fallback| &fallback.bio)),
                created: fallback.and_then(|fallback| fallback.created.clone()),
                joined: fallback.and_then(|fallback| fallback.joined.clone()),
                ..Default::default()
            },
            large: false,
            loading: false,
            kept: false,
            note: None,
        };
        let mut preview = profile_data(&mut self.pictures, &self.app.state, &view);
        // Someone's own card offers them nothing.
        preview.friend = 0;
        if let Some(picture) = new_avatar {
            preview.picture = slint_image(picture);
        }
        if let Some(picture) = new_banner {
            preview.banner = slint_image(picture);
        }
        let reading = match editor.reading.as_ref().map(|(kind, _)| *kind) {
            Some(ProfilePicture::Avatar) => "avatar",
            Some(ProfilePicture::Banner) => "banner",
            None => "",
        };
        let too_long = if editor.name.chars().count() > NAME_LIMIT {
            if main {
                format!("A display name can be up to {NAME_LIMIT} characters.")
            } else {
                format!("A nickname can be up to {NAME_LIMIT} characters.")
            }
        } else if editor.pronouns.chars().count() > PRONOUNS_LIMIT {
            format!("Pronouns can be up to {PRONOUNS_LIMIT} characters.")
        } else {
            String::new()
        };
        let left = BIO_LIMIT as i64 - editor.bio.chars().count() as i64;
        let handle = fallback_text(fallback.map(|fallback| &fallback.handle));
        let (title, detail, bio_placeholder) = if main {
            (
                "Edit profile",
                "Your profile".to_owned(),
                "Tell everyone a bit about yourself",
            )
        } else {
            (
                "Edit server profile",
                format!("Your profile in {}", editor.server_name),
                "Tell this server a bit about yourself",
            )
        };
        window.set_profile_editor_prompt(ProfileEditorPrompt {
            title: title.into(),
            detail: detail.into(),
            main,
            name_placeholder: handle.as_str().into(),
            pronouns_fallback: fallback
                .map(|fallback| fallback.pronouns.as_str())
                .unwrap_or_default()
                .into(),
            handle: handle.into(),
            bio_placeholder: bio_placeholder.into(),
            loading: editor.loading(),
            saving: editor.saving,
            error: editor.error.as_deref().unwrap_or_default().into(),
            name_enabled: editor.may_write(ProfileText::Name),
            avatar_enabled: editor.may_picture(ProfilePicture::Avatar),
            banner_enabled: editor.may_picture(ProfilePicture::Banner),
            bio_enabled: editor.may_write(ProfileText::Bio),
            offer: !main && !editor.nitro(),
            upgrade: editor.upgrade(),
            banner_shown: !main || editor.nitro(),
            colour_shown: editor.may_write(ProfileText::Colour),
            colour: colour
                .ok()
                .flatten()
                .map(|accent| {
                    slint::Color::from_rgb_u8(
                        (accent >> 16) as u8,
                        (accent >> 8) as u8,
                        accent as u8,
                    )
                })
                .unwrap_or_default(),
            has_colour: matches!(colour, Ok(Some(_))),
            colour_wrong: colour.is_err(),
            avatar_reset: editor.resettable(ProfilePicture::Avatar),
            banner_reset: editor.resettable(ProfilePicture::Banner),
            reset_word: if main { "Remove" } else { "Reset" }.into(),
            reading: reading.into(),
            bio_left: i32::try_from(left).unwrap_or(i32::MIN),
            too_long: too_long.into(),
            changed: editor.change().is_some(),
            preview,
        });
    }

    /// Asks before leaving a Discord server or group DM, as Discord's
    /// dialogs do.
    fn show_leave(&self, window: &YapfastWindow) {
        let leave = self.app.state.leave.as_ref();
        window.set_leave_open(leave.is_some());
        let Some(leave) = leave else {
            return;
        };
        let what = if leave.group { "group" } else { "server" };
        window.set_leave_prompt(ChannelPrompt {
            title: format!("Leave '{}'", leave.name).into(),
            detail: format!(
                "Are you sure you want to leave {}? You won't be able to rejoin this {what} unless you are re-invited.",
                leave.name
            )
            .into(),
            confirm: format!("Leave {what}").into(),
            option: if leave.group {
                "Leave without notifying other members".into()
            } else {
                Default::default()
            },
            ticked: leave.silent,
            danger: true,
            saving: leave.leaving,
            error: leave.error.as_deref().unwrap_or_default().into(),
            ..Default::default()
        });
    }

    /// Asks whether a channel dragged into a category takes its
    /// permissions, as Discord's client does.
    fn show_move_sync(&self, window: &YapfastWindow) {
        let asked = self.app.state.move_sync.as_ref();
        window.set_move_sync_open(asked.is_some());
        let Some(asked) = asked else {
            return;
        };
        window.set_move_sync_prompt(ChannelPrompt {
            title: "Sync permissions?".into(),
            detail: format!(
                "Would you like to sync {} permissions with {}?",
                asked.channel, asked.category
            )
            .into(),
            confirm: "Sync Permissions".into(),
            alternative: "Keep Current Permissions".into(),
            ..Default::default()
        });
    }

    /// The dialog from a DM's menu about its other person.
    fn show_person_dialog(&self, window: &YapfastWindow) {
        let dialog = self.app.state.person_dialog.as_ref();
        // The field starts from the note or nickname there is, once it's here.
        if let Some(text) = dialog.and_then(|dialog| dialog.text.as_deref())
            && (!window.get_person_open() || window.get_person_prompt().loading)
        {
            window.set_person_text(text.into());
        }
        window.set_person_open(dialog.is_some());
        let Some(dialog) = dialog else {
            return;
        };
        let name = &dialog.name;
        let (title, text, field, confirm, danger) = match dialog.ask {
            PersonAsk::Note => (
                format!("Note on {name}"),
                "Only you see your note, in Discord too.".into(),
                "Note",
                "Save",
                false,
            ),
            PersonAsk::Nickname => (
                format!("Nickname for {name}"),
                "Only you see the nickname you give a friend. Leave it empty to remove it.".into(),
                "Nickname",
                "Save",
                false,
            ),
            PersonAsk::Unfriend => (
                format!("Remove {name} as a friend?"),
                format!("You'll need to send {name} a friend request to be friends again."),
                "",
                "Remove friend",
                true,
            ),
            PersonAsk::Block => (
                format!("Block {name}?"),
                format!(
                    "Blocking {name} also removes them as a friend. Their messages show collapsed, and they can't message you."
                ),
                "",
                "Block",
                true,
            ),
            PersonAsk::ServerNickname if self.app.state.relation(&dialog.user) == Relation::Me => (
                "Change your nickname".into(),
                "Everyone in this server sees it. Leave it empty to go by your name.".into(),
                "Nickname",
                "Save",
                false,
            ),
            PersonAsk::ServerNickname => (
                format!("Change {name}'s nickname"),
                "Everyone in this server sees it. Leave it empty to reset it to their name.".into(),
                "Nickname",
                "Save",
                false,
            ),
            PersonAsk::Timeout => (
                format!("Time out {name}"),
                format!(
                    "Until it ends, {name} can read the server but can't send messages, react, or join voice. The reason goes in the server's audit log."
                ),
                "Reason (optional)",
                "Time out",
                true,
            ),
            PersonAsk::Kick => (
                format!("Kick {name}?"),
                format!(
                    "{name} can join again with a new invite. The reason goes in the server's audit log."
                ),
                "Reason (optional)",
                "Kick",
                true,
            ),
            PersonAsk::Ban => (
                format!("Ban {name}?"),
                format!(
                    "{name} can't join again until unbanned. The reason goes in the server's audit log."
                ),
                "Reason (optional)",
                "Ban",
                true,
            ),
        };
        let (heading, options) = match dialog.ask {
            PersonAsk::Timeout => ("DURATION", TIMEOUT_TIMES.map(span).to_vec()),
            PersonAsk::Ban => (
                "DELETE THEIR MESSAGES FROM",
                // As Discord's: hours up to a day, then days.
                BAN_HISTORY
                    .map(|seconds| match seconds / 3600 {
                        0 => "Don't delete any".to_owned(),
                        1 => "The last hour".to_owned(),
                        hours if hours <= 24 => format!("The last {hours} hours"),
                        hours => format!("The last {} days", hours / 24),
                    })
                    .to_vec(),
            ),
            _ => ("", vec![]),
        };
        // The same dialog keeps the rows it shows.
        let shown = window.get_person_prompt();
        let options = if shown.title == title.as_str() && shown.options.row_count() == options.len()
        {
            shown.options
        } else {
            let options: Vec<SharedString> = options.into_iter().map(Into::into).collect();
            ModelRc::new(VecModel::from(options))
        };
        window.set_person_prompt(PersonPrompt {
            title: title.into(),
            text: text.into(),
            options_heading: heading.into(),
            options,
            chosen: i32::try_from(dialog.choice).unwrap_or_default(),
            field: field.into(),
            confirm: confirm.into(),
            danger,
            loading: dialog.text.is_none(),
            saving: dialog.saving,
            error: dialog.error.as_deref().unwrap_or_default().into(),
        });
    }

    /// The dialogs for adding a friend, the custom status, a new DM and
    /// forwarding, and the quick switcher.
    fn show_dialogs(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let add_friend = state.add_friend.as_ref();
        if add_friend.is_some() && !window.get_add_friend_open() {
            window.set_add_friend_name(SharedString::default());
        }
        window.set_add_friend_open(add_friend.is_some());
        window.set_add_friend_sending(add_friend.is_some_and(|dialog| dialog.sending));
        let sent = add_friend.is_some_and(|dialog| matches!(dialog.outcome, Some(Ok(_))));
        // A request that went clears the name, ready for the next.
        if sent && !window.get_add_friend_sent() {
            window.set_add_friend_name(SharedString::default());
        }
        window.set_add_friend_sent(sent);
        window.set_add_friend_note(match add_friend {
            Some(AddFriend { sending: true, .. }) => "Sending...".into(),
            Some(AddFriend {
                outcome: Some(Ok(name)),
                ..
            }) => format!("Sent {name} a friend request.").into(),
            Some(AddFriend {
                outcome: Some(Err(error)),
                ..
            }) => error.as_str().into(),
            _ => SharedString::default(),
        });
        self.show_person_dialog(window);
        self.show_poll_draft(window);
        let editor = state.status_editor.as_ref();
        // Opening the dialog starts from the status there is.
        if editor.is_some() && !window.get_status_open() {
            let current = state.custom_status.as_ref();
            window.set_status_text(current.map_or("", |status| status.text.as_str()).into());
            window.set_status_emoji(
                current
                    .and_then(status_key)
                    .map_or_else(EmojiCell::default, |key| emoji::cell(&key)),
            );
            window.set_status_clear_after(0);
        }
        window.set_status_open(editor.is_some());
        window.set_status_saving(editor.is_some_and(|editor| editor.saving));
        window.set_status_error(
            editor
                .and_then(|editor| editor.error.as_deref())
                .unwrap_or_default()
                .into(),
        );
        let new_dm = state.new_dm.as_ref();
        if new_dm.is_some() && !window.get_new_dm_open() {
            window.set_new_dm_query(SharedString::default());
        }
        window.set_new_dm_open(new_dm.is_some());
        window.set_new_dm_starting(new_dm.is_some_and(|new_dm| new_dm.starting));
        let people: Vec<ContactRow> = new_dm
            .into_iter()
            .flat_map(|new_dm| new_dm.people())
            .map(|person| ContactRow {
                name: person.name.clone().into(),
                detail: person.detail.clone().into(),
                initial: initial(&person.name),
                tint: tint(&person.name),
                picture: cached_picture(&mut self.pictures, state, person.avatar.as_deref()),
                discord: person.discord,
            })
            .collect();
        window.set_new_dm_note(match new_dm {
            Some(NewDm { error: Some(error), .. }) => error.as_str().into(),
            Some(NewDm { starting: true, .. }) => "Starting the conversation...".into(),
            Some(new_dm) if people.is_empty() && new_dm.query.trim().is_empty() => {
                if cfg!(feature = "matrix") {
                    "Type a name to find people you know on Discord, or a full Matrix id such as @name:example.org."
                } else {
                    "Type a name to find people you know on Discord."
                }
                .into()
            }
            Some(_) if people.is_empty() => if cfg!(feature = "matrix") {
                "No one found. For Matrix, type a full id such as @name:example.org."
            } else {
                "No one found."
            }
            .into(),
            _ => SharedString::default(),
        });
        update_model(&self.models.new_dm_people, people);
        let forwarding = state.forwarding.as_ref();
        window.set_forward_open(forwarding.is_some());
        // Choosing a place clears the search.
        let query = forwarding.map_or("", |forwarding| forwarding.query.as_str());
        if window.get_forward_query() != query {
            window.set_forward_query(query.into());
        }
        if forwarding.is_some() != self.forwarding {
            window.set_forward_note(
                forwarding
                    .map(|forwarding| forwarding.note.as_str())
                    .unwrap_or_default()
                    .into(),
            );
        }
        window.set_forward_preview(
            forwarding
                .map(|forwarding| forwarding.quote.as_str())
                .unwrap_or_default()
                .into(),
        );
        window.set_forward_limit(yapfast_core::application::FORWARD_LIMIT as i32);
        window
            .set_forward_chosen(forwarding.map_or(0, |forwarding| forwarding.chosen.len()) as i32);
        window.set_forward_error(
            forwarding
                .and_then(|forwarding| forwarding.error.as_deref())
                .unwrap_or_default()
                .into(),
        );
        let discord =
            forwarding.is_some_and(|forwarding| Network::of(&forwarding.room) == Network::Discord);
        window.set_forward_empty(
            if discord {
                "We searched far and wide. Unfortunately, no results were found."
            } else if !query.is_empty() {
                "No conversations match."
            } else {
                "There's nowhere to forward to yet. Yapfast forwards Matrix messages only into encrypted conversations."
            }
            .into(),
        );
        // As Discord's client warns, the message added doesn't go where
        // slowmode would hold it.
        let slowed: Vec<String> = forwarding
            .into_iter()
            .flat_map(|forwarding| &forwarding.chosen)
            .filter_map(|id| state.rooms.iter().find(|room| room.id == *id))
            .filter(|room| room.slowed)
            .map(|room| state.destination(room))
            .collect();
        window.set_forward_slowmode(
            match slowed.as_slice() {
                [] => String::new(),
                [one] => {
                    format!("The message you added will not be sent to slowmode channel: {one}.")
                }
                many => format!(
                    "The message you added will not be sent to slowmode channels: {}.",
                    many.join(", ")
                ),
            }
            .into(),
        );
        update_model(
            &self.models.forward_targets,
            forwarding.map_or_else(Vec::new, |forwarding| {
                state
                    .forward_places()
                    .into_iter()
                    .map(|room| PlaceRow {
                        chosen: forwarding.chosen.contains(&room.id),
                        ..self.place_row(room, true)
                    })
                    .collect()
            }),
        );
        window.set_switcher_open(state.switcher.is_some());
        if self
            .switcher
            .as_ref()
            .is_some_and(|shown| shown.shows(state))
        {
            return;
        }
        self.switcher = state.switcher.as_ref().map(|_| SwitcherShown::of(state));
        if let Some(query) = &state.switcher
            && window.get_switcher_query() != query.as_str()
        {
            window.set_switcher_query(query.as_str().into());
        }
        update_model(
            &self.models.switcher_places,
            state
                .switcher_places()
                .into_iter()
                .map(|place| match place {
                    Place::Room(room) => PlaceRow {
                        unread: room.unread > 0 && !room.muted,
                        badge: mention_badge(room),
                        ..self.place_row(room, false)
                    },
                    Place::Server(server) => PlaceRow {
                        id: server.id.clone().into(),
                        name: server.name.clone().into(),
                        place: "Server".into(),
                        avatar: true,
                        initial: initial(&server.name),
                        tint: tint(&server.id),
                        picture_url: server.icon.clone().unwrap_or_default().into(),
                        ..PlaceRow::default()
                    },
                })
                .collect(),
        );
    }

    /// A conversation as Forward To and the quick switcher list it: a DM by
    /// its picture, and a channel by its server's icon in Forward To, as
    /// Discord's dialog shows it, or else by its own.
    fn place_row(&self, room: &yapfast_core::chat::ChatRoom, server_icon: bool) -> PlaceRow {
        let state = &self.app.state;
        let server = state
            .servers
            .iter()
            .find(|server| !room.direct && server.channels.contains(&room.id));
        let icon = server.filter(|_| server_icon);
        PlaceRow {
            id: room.id.clone().into(),
            name: room.name.clone().into(),
            place: match server {
                _ if room.direct => "Direct message".into(),
                Some(server) => server.name.clone().into(),
                None => "Channel".into(),
            },
            icon: self.icons.room(room),
            avatar: room.direct || icon.is_some(),
            initial: initial(icon.map_or(&room.name, |server| &server.name)),
            tint: tint(icon.map_or(&room.id, |server| &server.id)),
            picture_url: icon
                .map_or(room.avatar.as_deref(), |server| server.icon.as_deref())
                .unwrap_or_default()
                .into(),
            ..PlaceRow::default()
        }
    }

    /// The composer: what it may do, its files, its reply or edit, mentions and
    /// its hint.
    /// The thread beside the open channel: its rows, built again only for
    /// messages that changed, and its composer.
    pub(super) fn show_aside(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let Some(aside) = &state.aside else {
            if window.get_aside_open() {
                window.set_aside_open(false);
                self.aside_messages = Arc::new(Vec::new());
                self.aside_rows.clear();
                self.models.aside_messages.clear();
                self.models.aside_staged.clear();
            }
            return;
        };
        let room = &aside.room;
        window.set_aside_can_attach(state.aside_can_attach());
        window.set_aside_can_mark_spoilers(state.aside_can_mark_spoilers());
        let rows = staged_rows(
            &mut self.thumbnails,
            &self.runtime,
            &self.weak,
            state.aside_staged(),
        );
        update_model(&self.models.aside_staged, rows);
        let name = state.aside_room().map_or("", |listed| listed.name.as_str());
        window.set_aside_open(true);
        if window.get_aside_room() != room.as_str() {
            window.set_aside_room(room.as_str().into());
        }
        window.set_aside_name(name.into());
        window.set_aside_ready(aside.ready);
        window.set_aside_loading(aside.loading_history);
        window.set_aside_start(aside.history_start);
        window.set_aside_failed(aside.history_failed);
        window.set_aside_composer_enabled(state.can_write_in(room));
        window.set_aside_can_send(state.can_send_in(room));
        let draft = state.aside_draft();
        if window.get_aside_draft() != draft {
            window.set_aside_draft(draft.into());
        }
        window.set_aside_reply_ping(matches!(
            state.aside_compose(),
            Some(Compose::Reply { ping: true, .. })
        ));
        window.set_aside_compose_label(match state.aside_compose() {
            Some(Compose::Reply { author, body, .. }) => {
                format!("Replying to {author}: {body}").into()
            }
            Some(Compose::Edit { .. }) => "Editing your message".into(),
            None => SharedString::default(),
        });
        if Arc::ptr_eq(&self.aside_messages, &aside.messages) && !self.aside_restyle {
            return;
        }
        let marks = Marks {
            first_new: yapfast_core::chat::first_new(&aside.messages, aside.seen.as_deref()),
            first_unread: None,
            revealed: &self.revealed,
            restyle: std::mem::take(&mut self.aside_restyle),
        };
        let layout = &self.layout;
        show_timeline(
            &self.models.aside_messages,
            &mut self.aside_rows,
            &self.aside_messages,
            &aside.messages,
            &marks,
            |message| layout.restyle(message, message_row(message, marks.revealed)),
        );
        self.aside_messages = aside.messages.clone();
    }

    /// Colours the people and roles the draft mentions, as its message will
    /// show them. Who it may mention is found again only when the draft or
    /// what they come from changed.
    fn mark_mentions(&mut self) {
        let state = &self.app.state;
        let draft = state.draft();
        let sources = MentionSources {
            messages: Arc::as_ptr(&state.messages) as usize,
            members: state.members.iter().flatten().map(Vec::len).sum(),
            found: state.found_members.len(),
            groups: state.mention_groups.len(),
            color: theme::mention_color(),
        };
        if self
            .mentions_marked
            .as_ref()
            .is_some_and(|marked| marked.draft == draft && marked.sources == sources)
        {
            return;
        }
        let marks = if draft.contains('@') {
            yapfast_core::mention::marks(draft, &state.mention_candidates())
        } else {
            Vec::new()
        };
        let marked = MentionsMarked {
            draft: draft.to_owned(),
            sources,
        };
        i_slint_core::yapfast_marks::mark(
            marked.draft.as_str().into(),
            marks.into_iter().map(|(range, color)| {
                let rgb = color.unwrap_or(sources.color) & 0xff_ffff;
                (range, slint::Color::from_argb_encoded(0xff00_0000 | rgb))
            }),
        );
        self.mentions_marked = Some(marked);
    }

    pub(super) fn show_composer(&mut self, window: &YapfastWindow) {
        self.mark_mentions();
        let state = &self.app.state;
        let room = state.selected_room();
        window.set_voice_room(room.is_some_and(|room| room.voice));
        window.set_voice_chat(state.voice_chat);
        window.set_has_voice_chat(room.is_some_and(|room| room.voice_chat()));
        window.set_room_encrypted(room.is_some_and(|room| room.encrypted));
        window.set_call_encrypted(room.is_some_and(|room| room.call_encrypted()));
        window.set_room_ready(state.room_ready);
        window.set_draft(state.draft().into());
        let discord_room = room.is_some_and(|room| room.network == Network::Discord);
        // An unverified Matrix device can type; the hint says why it can't send.
        window.set_composer_enabled(
            state.room_ready
                && !state.busy
                && state.shows_text()
                && room.is_some_and(|room| {
                    if discord_room {
                        state.writable(room)
                    } else {
                        state.preview || room.encrypted
                    }
                }),
        );
        window.set_can_send(state.can_send());
        window.set_can_attach(state.can_attach());
        window.set_can_mark_spoilers(state.can_mark_spoilers());
        let staged = state.staged();
        // The thread beside keeps its files' pictures too.
        let aside_staged = state.aside_staged();
        self.thumbnails.retain(|path, _| {
            staged
                .iter()
                .chain(aside_staged)
                .any(|file| &file.path == path)
        });
        let rows = staged_rows(&mut self.thumbnails, &self.runtime, &self.weak, staged);
        update_model(&self.models.staged, rows);
        window.set_confirm_delete(state.confirm_delete.clone().unwrap_or_default().into());
        window
            .global::<Polls>()
            .set_confirm_end(state.confirm_end_poll.clone().unwrap_or_default().into());
        window.set_reply_ping(matches!(
            state.compose(),
            Some(Compose::Reply { ping: true, .. })
        ));
        window.set_compose_label(match state.compose() {
            Some(Compose::Reply { author, body, .. }) => {
                format!("Replying to {author}: {body}").into()
            }
            Some(Compose::Edit { .. }) => "Editing your message".into(),
            None => SharedString::default(),
        });
        window.set_sending(state.sending.is_some());
        window.set_typing(state.typing_line().into());
        let suggestions = state.mention_suggestions();
        let mentions: Vec<MentionRow> = suggestions
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                // People come first, then roles and groups, each under a heading.
                let starts_section = index == 0 || suggestions[index - 1].group != candidate.group;
                MentionRow {
                    heading: match (starts_section, candidate.group) {
                        (false, _) => "",
                        (true, false) => "MEMBERS",
                        (true, true) => "ROLES",
                    }
                    .into(),
                    initial: initial(&candidate.name),
                    tint: tint(&candidate.name),
                    picture: cached_picture(&mut self.pictures, state, candidate.avatar.as_deref()),
                    color: role_color(candidate.color),
                    has_color: candidate.color.is_some(),
                    group: candidate.group,
                    detail: match candidate.mention.as_str() {
                        "@everyone" => "Everyone in this channel".into(),
                        "@here" => "Everyone online here".into(),
                        _ => candidate.username.clone().unwrap_or_default().into(),
                    },
                    name: candidate.name.as_str().into(),
                }
            })
            .collect();
        if window.get_mention_index() >= mentions.len() as i32 {
            window.set_mention_index(0);
        }
        update_model(&self.models.mentions, mentions);
        window.set_loading_history(state.loading_history);
        window.set_history_start(state.history_start);
        window.set_history_failed(state.history_failed);
        window.set_detached(state.detached);
        window.set_loading_later(state.loading_later);
        window.set_later_failed(state.later_failed);
        // Without Matrix everything is Discord, so the encryption and key hints
        // say nothing new; what stops sending still shows.
        let discord_only = !cfg!(feature = "matrix");
        let timed_out = room
            .and_then(|room| room.timed_out_until)
            .filter(|_| discord_room)
            .map(|until| {
                format!(
                    "You're timed out in this server until {}.",
                    super::menus::until(until / 1000, self.layout.twelve_hour_clock)
                )
            });
        window.set_composer_hint(
            if room.is_none() {
                "Choose a DM or server channel to write a message."
            } else if state.preview {
                "Enter to send locally, Shift+Enter for a new line."
            } else if room.is_some_and(|room| room.request) {
                if discord_only {
                    ""
                } else {
                    "Discord · not end-to-end encrypted."
                }
            } else if let Some(timed_out) = &timed_out {
                timed_out
            } else if discord_room && room.is_some_and(|room| room.read_only || room.locked) {
                "You can't send messages in this Discord channel."
            } else if discord_room && !state.discord_online {
                "Discord is disconnected. Yapfast is reconnecting."
            } else if discord_only {
                ""
            } else if discord_room {
                "Discord · not end-to-end encrypted. Enter to send, Shift+Enter for a new line."
            } else if !state.trusted {
                "Sending is locked until this device is verified."
            } else if room.is_some_and(|room| room.encrypted) {
                "End-to-end encrypted. Enter to send, Shift+Enter for a new line."
            } else {
                "Unencrypted room. Reading only in this build."
            }
            .into(),
        );
        // A sent message, or another room's draft, ends the `:name` word.
        if let Some((at, name)) = &self.emoji_word
            && self.app.state.draft().get(*at..at + 1 + name.len())
                != Some(format!(":{name}").as_str())
        {
            self.emoji_word = None;
            self.show_emoji_suggestions(window);
        }
    }

    /// Emoji for the `:name` being typed, or none.
    pub(super) fn show_emoji_suggestions(&mut self, window: &YapfastWindow) {
        let (query, cells) = match &self.emoji_word {
            Some((_, name)) => (
                name.as_str(),
                emoji::suggestions(name, &self.app.usable_emoji(), EMOJI_SUGGESTIONS),
            ),
            None => ("", Vec::new()),
        };
        window.set_emoji_suggestion_query(query.into());
        if window.get_emoji_suggestion_index() >= cells.len() as i32 {
            window.set_emoji_suggestion_index(0);
        }
        update_model(&self.models.emoji_suggestions, cells);
    }

    /// The messages, built again only where they changed, and where the list
    /// scrolls.
    fn show_messages(&mut self, window: &YapfastWindow, changes: &Changes) {
        let &Changes {
            reset,
            selection: selection_changed,
            messages: messages_changed,
            at_end,
        } = changes;
        let state = &self.app.state;
        // Avatars that arrived since show on rows built before them.
        let pictures_arrived = self.pictures_seen != state.picture_changes;
        self.pictures_seen = state.picture_changes;
        // Those that went off screen go from the window's copies too.
        if pictures_arrived {
            self.pictures
                .retain(|url, _| state.pictures.contains_key(url));
        }
        // Custom emoji in messages whose pictures arrived get glyphs, and the
        // rows that showed them by name are built again. Those shown on their
        // own, in the picker, suggestions and reaction buttons, show pictures.
        if pictures_arrived || messages_changed {
            let state = &self.app.state;
            let in_messages = || state.messages.iter().flat_map(|message| &message.emoji);
            if custom_emoji::add(in_messages().filter_map(|emoji| arrived(state, emoji))) {
                self.restyle = true;
            }
            let usable = if pictures_arrived {
                self.app.usable_emoji()
            } else {
                Vec::new()
            };
            let state = &self.app.state;
            let added = custom_emoji::add_images(
                usable
                    .iter()
                    .flat_map(|set| &set.emoji)
                    .chain(state.messages.iter().flat_map(|message| &message.emoji))
                    .filter_map(|emoji| arrived(state, emoji)),
            );
            if added {
                if self.emoji_word.is_some() {
                    self.show_emoji_suggestions(window);
                }
                if !window.get_react_item().is_empty() {
                    emoji::fill_pictures(&self.models.emoji_rows, &self.models.emoji_sections);
                }
            }
        }
        let state = &self.app.state;
        // A poll's time left counts down by the minute, and at midnight
        // today's times become yesterday's.
        let minute = yapfast_core::clock::now_ms() / 60_000;
        if self.clock_minute != minute {
            let date = yapfast_core::clock::date;
            let new_day =
                self.clock_minute != 0 && date(self.clock_minute * 60) != date(minute * 60);
            self.clock_minute = minute;
            self.restyle |= polls::open_poll(&state.messages, minute * 60) | new_day;
        }
        let restyle = std::mem::take(&mut self.restyle);
        // The thread beside builds its rows again with the timeline's.
        self.aside_restyle |= restyle;
        if messages_changed || selection_changed || reset || pictures_arrived || restyle {
            if selection_changed || reset {
                window.set_react_item(SharedString::default());
                window.set_active_item(SharedString::default());
                window.set_jump_item(SharedString::default());
            }
            let marks = Marks {
                first_new: yapfast_core::chat::first_new(&state.messages, state.seen.as_deref()),
                first_unread: state.first_unread.as_deref(),
                revealed: &self.revealed,
                restyle,
            };
            let layout = &self.layout;
            show_timeline(
                &self.models.messages,
                &mut self.rows,
                &self.messages,
                &state.messages,
                &marks,
                |message| layout.restyle(message, message_row(message, marks.revealed)),
            );
            if selection_changed || reset {
                self.latest_due = false;
            } else if messages_changed && !state.detached && std::mem::take(&mut self.latest_due) {
                window.invoke_scroll_to_latest();
            }
            // A sent echo can get its event id without changing the row count
            // or scroll indexes. Report the new item ids after layout settles.
            if messages_changed || selection_changed || reset {
                window.invoke_media_viewport();
            }
            // A jump to a message places the list itself, even one made after
            // this refresh; see `show_message`.
            let weak = window.as_weak();
            let jumped = move || {
                weak.upgrade()
                    .filter(|window| window.get_jump_item().is_empty())
            };
            if selection_changed || at_end {
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Some(window) = jumped() {
                        window.invoke_scroll_to_latest();
                    }
                });
            }
        }
        let unread = state.first_unread.as_ref().and_then(|item| {
            let index = state
                .messages
                .iter()
                .position(|message| &message.item == item)?;
            Some((index, item))
        });
        window.set_caught_up(state.caught_up);
        if messages_changed || selection_changed || reset {
            window.set_unread_index(
                unread.map_or(-1, |(index, _)| i32::try_from(index).unwrap_or(i32::MAX)),
            );
            window.set_unread_label(
                unread
                    .map(|(index, _)| unread_label(&state.messages[index..], state.detached))
                    .unwrap_or_default()
                    .into(),
            );
        }
        if state.jump != self.jumped {
            self.jumped = state.jump.clone();
            if let Some((item, _)) = &state.jump
                && let Some(index) = state
                    .messages
                    .iter()
                    .position(|message| &message.item == item)
            {
                show_message(window, index, item, true);
            }
        }
        if state.unread_jump != self.unread_jumped {
            self.unread_jumped = state.unread_jump;
            if let Some((index, item)) = unread {
                show_message(window, index, item, false);
            }
        }
    }

    /// Which of the call's tiles shows large, the streams with tiles of their
    /// own, and what the corner shows while the call is off screen.
    pub(super) fn show_stage(&mut self, window: &YapfastWindow) {
        let voice = &self.app.state.voice;
        let watching = voice
            .watching
            .as_ref()
            .map(|watching| watching.user.as_str());
        if watching != self.watched.as_deref() {
            // A stream starts large, and leaves when watching it stops.
            if let Some(user) = watching {
                self.call_focus = Some(Focus::Stream(user.to_owned()));
            } else if matches!(&self.call_focus, Some(Focus::Stream(user)) if self.watched.as_ref() == Some(user))
            {
                self.call_focus = None;
            }
            self.watched = watching.map(str::to_owned);
        }
        let stage = Stage {
            participants: if voice.active() {
                &voice.participants
            } else {
                &[]
            },
            sharing: voice.sharing.is_some(),
        };
        let focused = self.call_focus.as_ref().and_then(|focus| stage.slot(focus));
        if focused.is_none() {
            self.call_focus = None;
        }
        let slot = |slot: Option<usize>| slot.map_or(-1, |slot| slot as i32);
        window.set_call_focus(slot(focused));
        update_model(
            &self.models.stream_tiles,
            stage.streams().map(|person| person as i32).collect(),
        );
        window.set_watch_user(watching.unwrap_or_default().into());
        let pip = stage.pip(self.call_focus.as_ref(), watching);
        window.set_pip_kind(match pip {
            Pip::None => 0,
            Pip::Watched => 1,
            Pip::Shared => 2,
            Pip::Camera(_) => 3,
        });
        window.set_pip_person(match pip {
            Pip::Camera(person) => person as i32,
            _ => -1,
        });
        window.set_pip_slot(slot(stage.pip_slot(pip, watching)));
    }

    /// The call: the voice dock, sharing and watching a screen, a DM's call,
    /// ringing, and the devices.
    fn show_voice(&mut self, window: &YapfastWindow) {
        self.ring();
        // The server of a call shows it on the rail, so the rail follows a
        // call that starts, ends or moves; other voice events leave it be.
        let state = &self.app.state;
        if self.rail_call.as_deref() != call_room(state) {
            self.rail_call = call_room(state).map(str::to_owned);
            let rooms_by_id = state
                .rooms
                .iter()
                .map(|room| (room.id.as_str(), &**room))
                .collect();
            let rail = rail(state, &rooms_by_id, &self.layout.open_folders);
            update_model_by_id(&self.models.servers, rail, |row| row.id.clone());
        }
        let timed = matches!(state.voice.phase, Phase::Connected | Phase::Reconnecting)
            .then(|| state.voice.room.clone())
            .flatten();
        self.time_call(window, timed.as_deref());
        let state = &self.app.state;
        let room = state.selected_room();
        let voice = &state.voice;
        let voice_here =
            room.is_some_and(|room| voice.room.as_ref() == Some(&room.id)) && voice.active();
        let account = state.account.as_deref().unwrap_or("Preview");
        window.set_voice_active(voice.active());
        // The account panel's picture rings as the account's own row in the call does.
        let local_speaking = voice.active()
            && voice
                .participants
                .iter()
                .any(|person| person.local && person.speaking && !person.muted && person.connected);
        window.set_account_speaking(local_speaking);
        // Speaking changes reach only this part of the window, not refresh().
        self.show_tray();
        window.set_voice_visible(voice.active() || voice.error.is_some());
        window.set_voice_ready(matches!(
            voice.phase,
            Phase::Connected | Phase::Reconnecting
        ));
        window.set_voice_leaving(voice.phase == Phase::Leaving);
        window.set_voice_here(voice_here);
        window.set_can_share(
            !state.preview
                && voice.phase == Phase::Connected
                && voice
                    .room
                    .as_deref()
                    .is_some_and(|room| Network::of(room) == Network::Discord),
        );
        window.set_sharing(voice.sharing.is_some());
        window.set_share_resolution(match voice.quality.resolution {
            voice::Resolution::P720 => 0,
            voice::Resolution::P1080 => 1,
            voice::Resolution::Source => 2,
        });
        window.set_share_fps(voice.quality.fps as i32);
        window.set_share_high_quality(voice.quality.high_quality);
        window.set_share_live(
            voice
                .sharing
                .as_ref()
                .is_some_and(|sharing| sharing.phase == voice::SharePhase::Live),
        );
        window.set_share_status(
            match voice.sharing.as_ref().map(|sharing| sharing.phase) {
                Some(voice::SharePhase::Choosing) => "Choose what to share",
                Some(voice::SharePhase::Starting) => "Starting your stream...",
                Some(voice::SharePhase::Live) => "Live",
                None => "",
            }
            .into(),
        );
        if voice.sharing.is_none() {
            window.set_share_frame(slint::Image::default());
        }
        window.set_watching(voice.watching.is_some());
        match &voice.watching {
            Some(watching) => {
                window.set_watch_title(format!("{}'s screen", watching.name).into());
                window.set_watch_status(
                    if watching.frames.is_none() {
                        format!("Connecting to {}'s stream...", watching.name)
                    } else {
                        format!("Waiting for {}'s picture...", watching.name)
                    }
                    .into(),
                );
            }
            None => window.set_watch_frame(slint::Image::default()),
        }
        window.set_voice_channel(
            voice
                .room
                .as_deref()
                .filter(|_| voice.active())
                .unwrap_or_default()
                .into(),
        );
        window.set_voice_tone(match voice.phase {
            Phase::Connected => 1,
            Phase::Disconnected => 2,
            _ => 0,
        });
        window.set_voice_detail(
            if state.preview {
                format!("{} · Sample", voice.name)
            } else if voice.end_to_end {
                format!("{} · End-to-end encrypted", voice.name)
            } else {
                format!("{} · Secure connection", voice.name)
            }
            .into(),
        );
        let link = window.global::<CallLink>();
        link.set_measured(voice.active() && voice.link.is_some());
        link.set_quality(match voice.link_quality {
            voice::Quality::Unknown => 0,
            voice::Quality::Fine => 1,
            voice::Quality::Average => 2,
            voice::Quality::Bad => 3,
        });
        link.set_end_to_end(voice.end_to_end);
        window.set_can_join(room.is_some_and(|room| state.can_join_voice(&room.id)));
        window.set_muted(voice.muted);
        window.set_deafened(voice.deafened);
        window.set_voice_status(
            match voice.phase {
                Phase::Disconnected => "Voice disconnected",
                Phase::Joining => "Joining voice...",
                Phase::Connected if state.preview => "Preview voice",
                Phase::Connected => "Voice connected",
                Phase::Reconnecting => "Reconnecting voice...",
                Phase::Leaving => "Leaving voice...",
            }
            .into(),
        );
        window.set_voice_name(voice.name.clone().into());
        window.set_voice_microphone_status(
            if voice.deafened {
                "Deafened and muted"
            } else if voice.muted {
                "Microphone muted"
            } else {
                "Microphone live"
            }
            .into(),
        );
        let present = room.map_or(&[][..], |room| room.people.as_slice());
        // A Discord DM's call, shown above its messages.
        let dm_call = room.filter(|room| room.direct && room.capabilities.dm_calls);
        let dm_person = dm_call.and_then(|room| room.person.as_deref());
        window.set_dm_person(dm_person.unwrap_or_default().into());
        window.set_dm_friend(dm_person.map_or(0, |person| match state.relation(person) {
            // The header offers no "sent", which the profile card shows.
            Relation::Outgoing => 0,
            relation => friend_offer(relation),
        }));
        window.set_can_call(dm_call.is_some_and(|room| !room.request) && !voice_here);
        window.set_call_going(!present.is_empty());
        window.set_call_shown(
            dm_call.is_some_and(|room| voice_here || !room.people.is_empty() || room.incoming),
        );
        window.set_call_room(dm_call.map_or_else(Default::default, |room| room.id.as_str().into()));
        let incoming = dm_call.is_some_and(|room| room.incoming) && !voice_here;
        window.set_call_incoming(incoming);
        window.set_call_note(
            match (incoming, present.is_empty()) {
                (true, true) => "Incoming call.".into(),
                (true, false) => format!("{} calling you.", who_is_here(present)),
                (false, _) => format!("{} in the call.", who_is_here(present)),
            }
            .into(),
        );
        let call_people: Vec<_> = present
            .iter()
            .map(|person| PersonRow {
                id: person.id.as_str().into(),
                name: person.name.as_str().into(),
                initial: initial(&person.name),
                tint: tint(&person.id),
                picture: cached_picture(&mut self.pictures, state, person.avatar.as_deref()),
                connected: true,
                ..Default::default()
            })
            .collect();
        update_model(&self.models.call_people, call_people);
        show_incoming(window, state, &mut self.pictures);
        window.set_voice_description(
            if room.is_some_and(|room| room.locked) {
                "Your roles on this Discord server don't let you join this channel.".into()
            } else if let Some(room) = dm_call.filter(|_| voice_here) {
                let ringing: Vec<&str> = room
                    .ringing
                    .iter()
                    .map(|person| person.name.as_str())
                    .collect();
                if !ringing.is_empty() {
                    format!("Calling {}...", ringing.join(", "))
                } else if voice.participants.len() == 1 {
                    "Nobody else is in the call yet.".into()
                } else {
                    "The call stays connected while you browse other conversations.".into()
                }
            } else if voice_here && voice.participants.len() == 1 {
                "You're here alone. Stay as long as you like; others can join later.".into()
            } else if voice_here {
                "Voice stays connected while you browse other conversations.".into()
            } else {
                let join = if joins_muted(window) {
                    "Join muted and hang out."
                } else {
                    "Join and hang out."
                };
                if present.is_empty() {
                    format!("{join} The channel stays here when empty.")
                } else {
                    format!("{} here. {join}", who_is_here(present))
                }
            }
            .into(),
        );
        window.set_voice_error(voice.error.as_deref().unwrap_or_default().into());
        window.set_voice_warning(voice.warning.as_deref().unwrap_or_default().into());
        update_model(
            &self.models.participants,
            if voice.active() {
                voice
                    .participants
                    .iter()
                    .map(|person| PersonRow {
                        id: person.id.clone().into(),
                        local: person.local,
                        name: display_part(&person.user).into(),
                        initial: initial(display_part(&person.user)),
                        tint: tint(display_part(if person.local {
                            account
                        } else {
                            &person.user
                        })),
                        speaking: person.speaking && !person.muted && person.connected,
                        connected: person.connected,
                        status: if let Some(problem) = &person.problem {
                            problem.as_str()
                        } else if !person.connected {
                            "Connection lost"
                        } else if person.deafened {
                            "Deafened"
                        } else if person.muted {
                            "Muted"
                        } else if person.speaking {
                            "Speaking"
                        } else {
                            "Listening"
                        }
                        .into(),
                        muted: person.muted,
                        deafened: person.deafened,
                        live: person.streaming,
                        video: person.video.is_some(),
                        elsewhere: false,
                        server_muted: person.server_muted,
                        server_deafened: person.server_deafened,
                        person: person.person.clone().into(),
                        picture: cached_picture(
                            &mut self.pictures,
                            state,
                            person.avatar.as_deref(),
                        ),
                    })
                    .collect()
            } else {
                vec![]
            },
        );
        // Viewers in the call's order, as their rows show them.
        let viewers = |ids: &[String], streamer: Option<&str>| -> Vec<PersonRow> {
            self.models
                .participants
                .iter()
                .filter(|row| {
                    Some(row.id.as_str()) != streamer && ids.iter().any(|id| row.id == id.as_str())
                })
                .collect()
        };
        update_model(
            &self.models.share_viewers,
            voice
                .sharing
                .as_ref()
                .map(|sharing| viewers(&sharing.viewers, None))
                .unwrap_or_default(),
        );
        update_model(
            &self.models.watch_viewers,
            voice
                .watching
                .as_ref()
                .map(|watching| viewers(&watching.viewers, Some(&watching.user)))
                .unwrap_or_default(),
        );
        let names = |model: &VecModel<PersonRow>| -> SharedString {
            if model.row_count() == 0 {
                return SharedString::new();
            }
            let names: Vec<String> = model.iter().map(|row| row.name.to_string()).collect();
            format!("Watching: {}", names.join(", ")).into()
        };
        window.set_share_viewer_names(names(&self.models.share_viewers));
        window.set_watch_viewer_names(names(&self.models.watch_viewers));
        // A picture per participant, kept while their camera stays on and
        // they stay in that place.
        let frames = &self.models.participant_frames;
        let count = if voice.active() {
            voice.participants.len()
        } else {
            0
        };
        for (index, person) in voice.participants.iter().take(count).enumerate() {
            let moved = self.frame_ids.get(index) != Some(&person.id);
            if (person.video.is_none() || moved) && index < frames.row_count() {
                frames.set_row_data(index, slint::Image::default());
            }
        }
        self.frame_ids = voice
            .participants
            .iter()
            .take(count)
            .map(|person| person.id.clone())
            .collect();
        while frames.row_count() > count {
            frames.remove(frames.row_count() - 1);
        }
        while frames.row_count() < count {
            frames.push(slint::Image::default());
        }
        self.show_stage(window);
        self.show_soundboard(window);
        self.show_soundboard_settings(window);
        let state = &self.app.state;
        let voice = &state.voice;
        window.set_can_film(voice.can_film);
        window.set_camera_on(voice.camera_phase != voice::CameraPhase::Off);
        window.set_camera_starting(voice.camera_phase == voice::CameraPhase::Starting);
        window.set_call_video(
            voice
                .participants
                .iter()
                .any(|person| person.video.is_some()),
        );
        let prompt = state.switch_voice.as_ref().map(|id| {
            let room = state.rooms.iter().find(|room| &room.id == id);
            let target = room.map_or("the other channel", |room| room.name.as_str());
            // Whose stream it joins to watch, if it's for one.
            let watch = state
                .pending_watch
                .as_ref()
                .filter(|(watched, _)| watched == id)
                .map(|(_, user)| {
                    room.and_then(|room| room.people.iter().find(|person| &person.id == user))
                        .map_or("someone", |person| person.name.as_str())
                });
            match watch {
                // Only the prompt shows how it joins; a fading one keeps it.
                Some(name) if !voice.active() => {
                    window.set_switch_joins(true);
                    format!("Join {target} to watch {name}'s stream?")
                }
                Some(name) => {
                    window.set_switch_joins(false);
                    format!(
                        "You're already in {}. Leave it and join {target} to watch {name}'s stream?{}",
                        voice.name,
                        joining_muted(window)
                    )
                }
                None => {
                    window.set_switch_joins(false);
                    format!(
                        "You're already in {}. Leave it and join {target}?{}",
                        voice.name,
                        joining_muted(window)
                    )
                }
            }
        });
        window.set_switch_prompt(prompt.unwrap_or_default().into());
        let names = &mut self.models.device_names;
        let (rows, index) = device_rows(&voice.microphones, voice.microphone.as_deref(), names);
        update_model(&self.models.microphones, rows);
        window.set_microphone_index(index);
        let (rows, index) = device_rows(&voice.speakers, voice.speaker.as_deref(), names);
        update_model(&self.models.speakers, rows);
        window.set_speaker_index(index);
        let (rows, index) = device_rows(&voice.cameras, voice.camera.as_deref(), names);
        update_model(&self.models.cameras, rows);
        window.set_camera_index(index);
    }

    /// The call's soundboard, its rows made again only when its sounds
    /// change or pictures arrive, and why it can't play, as Discord says.
    fn show_soundboard(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let voice = &state.voice;
        let groups = voice.soundboard.as_ref().filter(|_| voice.active());
        window.set_can_soundboard(groups.is_some());
        let server_muted = voice
            .participants
            .iter()
            .any(|person| person.local && person.server_muted);
        window.set_soundboard_note(
            if voice.deafened {
                "Soundboard disabled when deafened"
            } else if server_muted {
                "Soundboard disabled when server muted"
            } else {
                ""
            }
            .into(),
        );
        let Some(groups) = groups else {
            self.soundboard_shown = None;
            if self.models.soundboard_rows.row_count() > 0 {
                self.models.soundboard_rows.set_vec(vec![]);
            }
            return;
        };
        let shown = self.soundboard_shown.as_ref();
        if shown.is_some_and(|(kept, revision)| {
            Arc::ptr_eq(kept, groups) && *revision == state.picture_changes
        }) {
            return;
        }
        self.soundboard_shown = Some((groups.clone(), state.picture_changes));
        let rows = sound_rows(groups, &mut self.pictures, state);
        self.models.soundboard_rows.set_vec(rows);
    }

    /// Settings' soundboard volume and entrance sounds, while Voice & video
    /// shows them.
    fn show_soundboard_settings(&mut self, window: &YapfastWindow) {
        if !window.get_settings_open() || window.get_settings_section() != 1 {
            return;
        }
        let state = &self.app.state;
        window.set_discord_nitro(self.app.discord_nitro());
        if self.soundboard_volume_shown != Some(state.soundboard_volume) {
            self.soundboard_volume_shown = Some(state.soundboard_volume);
            let slider = yapfast_core::volume::to_slider(state.soundboard_volume / 100.0);
            window.set_soundboard_volume(slider.round() as i32);
        }
        // Discord's servers by their own ids, which the app's carry after
        // a prefix.
        let discord_servers = || {
            state.servers.iter().filter_map(|server| {
                Some((yapfast_core::discord::parse_server(&server.id)?, server))
            })
        };
        let same = self.entrance_servers.len() == discord_servers().count() + 1
            && discord_servers()
                .zip(self.entrance_servers.iter().skip(1))
                .all(|((guild, _), kept)| kept.as_deref() == Some(guild));
        if !same {
            let servers: Vec<Option<String>> = std::iter::once(None)
                .chain(discord_servers().map(|(guild, _)| Some(guild.to_owned())))
                .collect();
            let names: Vec<SharedString> = std::iter::once("All servers".into())
                .chain(discord_servers().map(|(_, server)| server.name.as_str().into()))
                .collect();
            self.models.entrance_servers.set_vec(names);
            let chosen = usize::try_from(window.get_entrance_server())
                .ok()
                .and_then(|index| self.entrance_servers.get(index).cloned())
                .flatten();
            let index = servers
                .iter()
                .position(|server| *server == chosen)
                .unwrap_or(0);
            window.set_entrance_server(index as i32);
            self.entrance_servers = servers;
        }
        let server = usize::try_from(window.get_entrance_server())
            .ok()
            .and_then(|index| self.entrance_servers.get(index).cloned())
            .flatten();
        let catalog = state.sound_catalog.as_ref();
        let sound = state
            .join_sounds
            .iter()
            .find(|join| join.server == server)
            .map(|join| {
                let named = catalog
                    .iter()
                    .flat_map(|groups| groups.iter())
                    .flat_map(|group| &group.sounds)
                    .find(|sound| sound.id == join.sound && sound.guild == join.guild);
                match (named, catalog) {
                    (Some(sound), _) => sound.name.as_str(),
                    (None, None) => "Loading...",
                    (None, Some(_)) => "A sound that's gone",
                }
            });
        window.set_entrance_sound(sound.unwrap_or_default().into());
        let Some(groups) = catalog else { return };
        let shown = self.entrance_shown.as_ref();
        if shown.is_some_and(|(kept, revision)| {
            Arc::ptr_eq(kept, groups) && *revision == state.picture_changes
        }) {
            return;
        }
        self.entrance_shown = Some((groups.clone(), state.picture_changes));
        let rows = sound_rows(groups, &mut self.pictures, state);
        self.models.entrance_rows.set_vec(rows);
    }

    /// The open profile card, and the profile beside a one-to-one DM, with
    /// what's known so far.
    fn show_profile(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        window.set_profile_open(state.profile.is_some());
        window.set_panel_profile_shown(state.panel_profile.is_some());
        if let Some(view) = &state.panel_profile {
            let mut data = profile_data(&mut self.pictures, state, view);
            // The DM list keeps up with whether they're around; the profile
            // was as it was when it opened.
            if let Some(room) = state.selected_room() {
                data.presence = presence_name(room.presence.or(view.profile.presence)).into();
                if let Some(activity) = &room.activity {
                    let (emoji, activity) = if names_an_activity(activity, &room.activities) {
                        (None, "")
                    } else {
                        yapfast_core::discord::status_parts(activity)
                    };
                    data.activity = activity.into();
                    data.activity_emoji = emoji.unwrap_or_default().into();
                }
                data.activities = activity_rows(&room.activities);
            }
            let shown = window.get_panel_profile();
            if let Some(data) = changed_profile(&shown, data) {
                window.set_panel_profile(data);
            }
        }
        let Some(view) = &state.profile else {
            window.set_profile_member(SharedString::default());
            self.tick_activities(window);
            return;
        };
        window.set_profile_large(view.large);
        window.set_profile_member(if view.large {
            SharedString::default()
        } else {
            view.profile.id.clone().into()
        });
        let data = profile_data(&mut self.pictures, state, view);
        if let Some(data) = changed_profile(&window.get_profile(), data) {
            window.set_profile(data);
        }
        self.tick_activities(window);
    }

    /// The image viewer: the full image once it arrives, the preview until then.
    /// The video playing, which its cell shows through [`VideoPlayer`]; its
    /// pictures come with each stream tick.
    fn show_video(&mut self, window: &YapfastWindow) {
        let player = window.global::<VideoPlayer>();
        let video = self.app.state.video.as_ref();
        // Stream ticks bring its pictures, until a quiet moment while it
        // isn't playing.
        window.set_video_active(video.is_some_and(|video| video.frames.is_some()));
        self.video_quiet.set(0);
        let same = video.is_some_and(|video| {
            player.get_item() == video.item.as_str() && player.get_index() == video.index as i32
        });
        if !same {
            player.set_has_frame(false);
            player.set_frame(slint::Image::default());
            player.set_progress(0.0);
            player.set_time(SharedString::default());
            player.set_expanded(false);
        }
        let Some(video) = video else {
            player.set_item(SharedString::default());
            player.set_index(-1);
            player.set_error(SharedString::default());
            return;
        };
        player.set_item(video.item.clone().into());
        player.set_index(video.index as i32);
        player.set_loading(video.loading);
        player.set_playing(video.playing);
        player.set_ended(video.ended);
        player.set_sound(video.sound);
        player.set_muted(video.muted);
        player.set_volume(video.volume);
        player.set_error(video.error.clone().unwrap_or_default().into());
        let attachment = self
            .app
            .state
            .message(&video.item)
            .and_then(|message| message.attachments.get(video.index));
        player.set_name(attachment.map_or_else(SharedString::default, |attachment| {
            attachment.name.as_str().into()
        }));
        let (width, height) = video
            .size
            .or_else(|| attachment.and_then(|attachment| attachment.dimensions))
            .unwrap_or_default();
        player.set_video_width(i32::try_from(width).unwrap_or(0));
        player.set_video_height(i32::try_from(height).unwrap_or(0));
        player.set_duration(
            video
                .duration_ms
                .and_then(|duration| i32::try_from(duration).ok())
                .unwrap_or(0),
        );
        if video.ended {
            player.set_progress(1.0);
        }
        if !same || video.ended {
            let at = if video.ended {
                video.duration_ms.unwrap_or(0)
            } else {
                0
            };
            player.set_time(video_time(at, video.duration_ms).into());
        }
    }

    fn show_viewer(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let viewing = state.viewer.as_ref().and_then(|viewer| {
            let message = state.message(&viewer.item)?;
            Some((viewer, message, message.attachments.get(viewer.index)?))
        });
        window.set_viewer_open(viewing.is_some());
        let Some((viewer, message, attachment)) = viewing else {
            // The window would keep the picture it showed, up to 9 MiB, until
            // the next one.
            if self.viewer_image.take().is_some() {
                window.set_viewer_image(slint::Image::default());
                window.set_viewer_frames(ModelRc::default());
                window.set_viewer_delays(ModelRc::default());
            }
            self.viewer_size = None;
            return;
        };
        let loaded = viewer.pictures.get(&viewer.index);
        let full = match loaded {
            Some(Some(Ok(picture))) => Some(picture),
            _ => None,
        };
        window.set_viewer_note(
            match loaded {
                Some(Some(Err(error))) => error,
                Some(None) => "Loading the full image...",
                _ => "",
            }
            .into(),
        );
        // The full image's size from the start where it's known, else the
        // preview's. Either way the image keeps the size it opened at, so the
        // full image only sharpens it on arrival.
        let (width, height) = match &self.viewer_size {
            Some((item, index, width, height))
                if *item == viewer.item && *index == viewer.index =>
            {
                (*width, *height)
            }
            _ => {
                let size = match (full, attachment.dimensions, &attachment.preview) {
                    (Some(full), _, _) => Some((full.width, full.height)),
                    (None, Some((width, height)), _) => Some(fit(width, height)),
                    (None, None, Some(preview)) => Some((preview.width, preview.height)),
                    _ => None,
                };
                self.viewer_size =
                    size.map(|(width, height)| (viewer.item.clone(), viewer.index, width, height));
                size.unwrap_or((1, 1))
            }
        };
        window.set_viewer_width(width as i32);
        window.set_viewer_height(height as i32);
        let shown = full.or(attachment.preview.as_ref());
        let image = match (shown, &self.viewer_image) {
            (Some(shown), Some((cached, image))) if Arc::ptr_eq(shown, cached) => image.clone(),
            (Some(shown), _) => {
                let image = picture(shown);
                self.viewer_image = Some((shown.clone(), image.clone()));
                image
            }
            (None, _) => slint::Image::default(),
        };
        window.set_viewer_image(image);
        let (frames, delays) = shown.map(animation).unwrap_or_default();
        window.set_viewer_frames(frames);
        window.set_viewer_delays(delays);
        window.set_viewer_name(attachment.name.clone().into());
        window.set_viewer_item(viewer.item.clone().into());
        window.set_viewer_index(viewer.index as i32);
        let images: Vec<usize> = message
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, attachment)| attachment.image)
            .map(|(index, _)| index)
            .collect();
        let place = images.iter().position(|index| *index == viewer.index);
        window.set_viewer_position(match place {
            Some(place) if images.len() > 1 => format!("{} / {}", place + 1, images.len()).into(),
            _ => SharedString::default(),
        });
    }
}

/// The DMs with unread messages, the latest first, as Discord lists them on
/// its rail above the servers. Message requests and spam stay on their screen.
pub(super) fn unread_dm_rows(state: &AppState) -> Vec<ServerRow> {
    state
        .rooms
        .iter()
        .filter(|room| {
            room.direct
                && !room.request
                && room.category != discord::SPAM
                && (room.unread > 0 || room.mentions > 0)
        })
        .map(|room| ServerRow {
            id: room.id.as_str().into(),
            name: room.name.as_str().into(),
            initial: initial(&room.name),
            picture_url: room.avatar.as_deref().unwrap_or_default().into(),
            unread: true,
            // Discord counts each of a DM's messages; Matrix counts its
            // unread messages apart from its mentions.
            badge: count_badge(room.mentions.max(room.unread)),
            discord: room.network == Network::Discord,
            dm: true,
            ..Default::default()
        })
        .collect()
}

/// The whole rail: unread DMs, a line, then the servers and folders. Unread
/// DMs come and go above the servers, so the model keeps rows by id rather
/// than all moving down one.
fn rail(
    state: &AppState,
    rooms_by_id: &HashMap<&str, &yapfast_core::chat::ChatRoom>,
    open_folders: &std::collections::BTreeSet<String>,
) -> Vec<ServerRow> {
    let mut rail = unread_dm_rows(state);
    rail.push(ServerRow {
        divider: true,
        ..Default::default()
    });
    rail.extend(rail_rows(state, rooms_by_id, open_folders));
    rail
}

/// The rail's rows: each server, and each folder with its servers after it
/// while it's open.
pub(super) fn rail_rows(
    state: &AppState,
    rooms_by_id: &HashMap<&str, &yapfast_core::chat::ChatRoom>,
    open_folders: &std::collections::BTreeSet<String>,
) -> Vec<ServerRow> {
    // A server's channels: whether any is unread, and its mentions. One its
    // channel list leaves out marks it only by mentions, as in Discord.
    let marks = |server: &yapfast_core::chat::ChatServer| {
        let rooms = || {
            server
                .channels
                .iter()
                .filter_map(|channel| rooms_by_id.get(channel.as_str()))
                .filter(|room| !room.direct)
        };
        (
            rooms().any(|room| {
                room.mentions > 0 || (room.unread > 0 && !server.muted && !room.unlisted)
            }),
            rooms().map(|room| room.mentions).sum::<u64>(),
        )
    };
    let selected = |server: &yapfast_core::chat::ChatServer| {
        state.navigation == Navigation::Server(server.id.clone())
    };
    let call = call_room(state);
    let in_call = |server: &yapfast_core::chat::ChatServer| {
        call.is_some_and(|room| server.channels.iter().any(|channel| channel == room))
    };
    let mut rows = Vec::with_capacity(state.servers.len());
    let mut servers = state.servers.iter().peekable();
    while let Some(server) = servers.next() {
        let Some(folder) = &server.folder else {
            let (unread, mentions) = marks(server);
            rows.push(ServerRow {
                call: in_call(server),
                ..server_row(server, selected(server), unread, mentions)
            });
            continue;
        };
        let mut inside = vec![server];
        while let Some(next) = servers.next_if(|next| next.folder.as_ref() == Some(folder)) {
            inside.push(next);
        }
        let open = open_folders.contains(&folder.id);
        let tint = folder.color.map_or(FOLDER_COLOR, |rgb| {
            let [_, red, green, blue] = rgb.to_be_bytes();
            slint::Color::from_rgb_u8(red, green, blue)
        });
        let (unread, mentions) = inside
            .iter()
            .map(|server| marks(server))
            .fold((false, 0), |(unread, mentions), (one, more)| {
                (unread || one, mentions + more)
            });
        let name = folder.name.clone().unwrap_or_else(|| {
            let mut names: Vec<_> = inside
                .iter()
                .take(3)
                .map(|server| server.name.as_str())
                .collect();
            if inside.len() > 3 {
                names.push("…");
            }
            names.join(", ")
        });
        rows.push(ServerRow {
            id: folder.id.as_str().into(),
            name: name.into(),
            selected: inside.iter().any(|server| selected(server)),
            unread,
            badge: count_badge(mentions),
            call: inside.iter().any(|server| in_call(server)),
            discord: true,
            folder: true,
            open,
            tint,
            preview: ModelRc::new(VecModel::from(
                inside
                    .iter()
                    .take(4)
                    .map(|server| FolderServer {
                        initial: initials(&server.name),
                        picture_url: server.icon.as_deref().unwrap_or_default().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            ..Default::default()
        });
        if open {
            let last = inside.len() - 1;
            for (index, server) in inside.into_iter().enumerate() {
                let (unread, mentions) = marks(server);
                rows.push(ServerRow {
                    nested: true,
                    last: index == last,
                    tint,
                    call: in_call(server),
                    ..server_row(server, selected(server), unread, mentions)
                });
            }
        }
    }
    rows
}

/// The voice channel of the call going on, if there is one.
/// A server voice channel someone is in, as the friends list names it.
pub(super) struct VoicePlace<'a> {
    pub room: &'a str,
    pub channel: &'a str,
    pub server: &'a str,
}

/// Who is in a voice channel of a server this account is in, by user id.
/// DM calls stay out: Discord's friends list shows only servers' channels.
pub(super) fn voice_places<'a>(
    state: &'a AppState,
    rooms_by_id: &HashMap<&str, &'a yapfast_core::chat::ChatRoom>,
) -> HashMap<&'a str, VoicePlace<'a>> {
    let mut places = HashMap::new();
    for server in &state.servers {
        for channel in &server.channels {
            let Some(room) = rooms_by_id.get(channel.as_str()).filter(|room| room.voice) else {
                continue;
            };
            for person in &room.people {
                places.insert(
                    person.id.as_str(),
                    VoicePlace {
                        room: &room.id,
                        channel: &room.name,
                        server: &server.name,
                    },
                );
            }
        }
    }
    places
}

fn call_room(state: &AppState) -> Option<&str> {
    state
        .voice
        .active()
        .then_some(state.voice.room.as_deref())
        .flatten()
}

/// Discord's colour for a folder that wasn't given one.
const FOLDER_COLOR: slint::Color = slint::Color::from_rgb_u8(0x58, 0x65, 0xf2);

fn server_row(
    server: &yapfast_core::chat::ChatServer,
    selected: bool,
    unread: bool,
    mentions: u64,
) -> ServerRow {
    ServerRow {
        id: server.id.clone().into(),
        name: server.name.clone().into(),
        initial: initials(&server.name),
        picture: slint::Image::default(),
        picture_url: server.icon.as_deref().unwrap_or_default().into(),
        selected,
        unread,
        discord: server.discord,
        unavailable: server.unavailable,
        badge: count_badge(mentions),
        ..Default::default()
    }
}

/// Rows under a collapsed category: only the selected channel, unread ones and
/// voice channels with people in them stay, as in Discord. A category with
/// none of those keeps its heading alone, to open it again.
pub(super) fn fold_collapsed(
    rows: Vec<RoomRow>,
    collapsed: &std::collections::BTreeSet<String>,
    voice_room: Option<&str>,
) -> Vec<RoomRow> {
    let mut group = SharedString::default();
    let mut folded = Vec::with_capacity(rows.len());
    for mut row in rows {
        if !row.heading.is_empty() {
            group = row.category.clone();
        }
        if group.is_empty() || !collapsed.contains(group.as_str()) {
            folded.push(row);
            continue;
        }
        row.collapsed = true;
        if row.selected
            || row.unread
            || row.people.row_count() > 0
            || voice_room == Some(row.id.as_str())
        {
            folded.push(row);
        } else if !row.heading.is_empty() {
            // Its id still names the category's first channel, for the
            // heading's menu.
            folded.push(RoomRow {
                id: row.id,
                heading: row.heading,
                category: row.category,
                collapsed: true,
                hidden: true,
                ..Default::default()
            });
        }
    }
    folded
}

/// Rows with a heading alone for each of a server's `categories` that holds
/// no channels, where Discord's order puts it among those that do.
pub(super) fn with_empty_categories(
    rows: Vec<RoomRow>,
    categories: &[ListedCategory],
) -> Vec<RoomRow> {
    if !categories.iter().any(|category| category.empty) {
        return rows;
    }
    // Its id names the category, for the heading's menu.
    let heading = |category: &ListedCategory| RoomRow {
        id: category.id.as_str().into(),
        heading: category.name.to_uppercase().into(),
        category: category.id.as_str().into(),
        hidden: true,
        ..Default::default()
    };
    let mut next = 0;
    let mut listed = Vec::with_capacity(rows.len() + categories.len());
    for row in rows {
        if !row.heading.is_empty()
            && let Some(at) = categories[next..]
                .iter()
                .position(|known| known.id == row.category.as_str())
        {
            listed.extend(
                categories[next..next + at]
                    .iter()
                    .filter(|category| category.empty)
                    .map(heading),
            );
            next += at + 1;
        }
        listed.push(row);
    }
    listed.extend(
        categories[next..]
            .iter()
            .filter(|category| category.empty)
            .map(heading),
    );
    listed
}

/// A custom emoji and its picture, once that has arrived.
/// Whether the next call joins with the microphone off: unmuting on joining
/// is turned off, or deafening on joining is on.
fn joins_muted(window: &YapfastWindow) -> bool {
    !window.get_unmute_on_join() || window.get_deafen_on_join()
}

/// What a prompt to switch calls adds when the next one joins muted.
fn joining_muted(window: &YapfastWindow) -> &'static str {
    if joins_muted(window) {
        " You'll join muted."
    } else {
        ""
    }
}

/// A composer's waiting files, each with its picture once decoded; those not
/// decoded yet start decoding.
fn staged_rows(
    thumbnails: &mut HashMap<PathBuf, Option<pictures::Thumbnail>>,
    runtime: &BackgroundRuntime,
    weak: &std::rc::Weak<RefCell<Adapter>>,
    staged: &[yapfast_core::chat::Staged],
) -> Vec<StagedFile> {
    let mut rows = Vec::with_capacity(staged.len());
    for file in staged {
        let picture = match thumbnails.get(&file.path) {
            Some(thumbnail) => thumbnail.as_ref().map(|thumbnail| {
                if file.spoiler {
                    thumbnail.covered.clone()
                } else {
                    thumbnail.picture.clone()
                }
            }),
            None => {
                if file.image {
                    thumbnails.insert(file.path.clone(), None);
                    load_thumbnail(runtime, weak.clone(), file.path.clone());
                }
                None
            }
        };
        rows.push(StagedFile {
            name: file.name.as_str().into(),
            size: file_size(file.size).into(),
            has_picture: picture.is_some(),
            picture: picture.unwrap_or_default(),
            spoiler: file.spoiler,
        });
    }
    rows
}

fn arrived<'a>(
    state: &'a AppState,
    emoji: &'a yapfast_core::chat::CustomEmoji,
) -> Option<(&'a str, &'a yapfast_core::chat::Picture)> {
    Some((
        emoji.id.as_str(),
        state.pictures.get(&emoji.picture)?.as_ref(),
    ))
}

/// Shows found messages, building rows only for those not shown already:
/// a search's next page adds to the end of the results.
fn show_found(model: &VecModel<FoundRow>, messages: &[ChatMessage], twelve_hour: bool) {
    let kept = model
        .iter()
        .zip(messages)
        .take_while(|(row, message)| row.item == message.item.as_str())
        .count();
    while model.row_count() > kept {
        model.remove(model.row_count() - 1);
    }
    for message in &messages[kept..] {
        model.push(found_row(message, twelve_hour));
    }
}

/// A length of time in seconds as a moderation dialog offers it, such as
/// "60 seconds", "6 hours" or "1 week".
pub(super) fn span(seconds: u32) -> String {
    let (count, unit) = match seconds {
        seconds if seconds % (7 * 86400) == 0 => (seconds / (7 * 86400), "week"),
        seconds if seconds % 86400 == 0 => (seconds / 86400, "day"),
        seconds if seconds % 3600 == 0 => (seconds / 3600, "hour"),
        seconds if seconds % 60 == 0 && seconds > 60 => (seconds / 60, "minute"),
        seconds => (seconds, "second"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural}")
}

/// Search results, keeping the rows of those still first and building the
/// rest. A server's name the channel each is in, above the first of a run
/// in one channel.
fn show_hits(
    model: &VecModel<FoundRow>,
    state: &AppState,
    hits: &[yapfast_core::chat::SearchHit],
    server: bool,
    twelve_hour: bool,
) {
    let kept = model
        .iter()
        .zip(hits)
        .take_while(|(row, hit)| row.item == hit.message.item.as_str())
        .count();
    while model.row_count() > kept {
        model.remove(model.row_count() - 1);
    }
    for (index, hit) in hits.iter().enumerate().skip(kept) {
        let starts = index == 0 || hits[index - 1].room != hit.room;
        let heading = state
            .rooms
            .iter()
            .find(|room| room.id == hit.room)
            .filter(|_| server && starts)
            .map(|room| room.name.as_str())
            .unwrap_or_default();
        model.push(FoundRow {
            room: hit.room.as_str().into(),
            heading: heading.into(),
            ..found_row(&hit.message, twelve_hour)
        });
    }
}

/// What the bar that jumps to the "New" divider says: how many messages
/// others sent from `unread` on, and since when, with the date unless it
/// was today. `more` came after them, past a window of older messages.
pub(super) fn unread_label(unread: &[Arc<ChatMessage>], more: bool) -> String {
    let count = unread.iter().filter(|message| !message.own).count();
    let messages = if count == 1 && !more {
        "message"
    } else {
        "messages"
    };
    let count = if more {
        format!("{count}+")
    } else {
        count.to_string()
    };
    let Some(first) = unread.first().filter(|first| first.time.contains(':')) else {
        return format!("{count} new {messages}");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |now| now.as_secs());
    let on = day(first.timestamp);
    if on.is_empty() || on == day(now) {
        format!("{count} new {messages} since {}", first.time)
    } else {
        format!("{count} new {messages} since {} on {on}", first.time)
    }
}

/// How far into a video, and how long it is when known, as "0:12 / 1:30".
pub(super) fn video_time(at_ms: u64, duration_ms: Option<u64>) -> String {
    let clock = |ms: u64| {
        let seconds = ms / 1000;
        match seconds / 3600 {
            0 => format!("{}:{:02}", seconds / 60, seconds % 60),
            hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
        }
    };
    match duration_ms {
        Some(duration) => format!("{} / {}", clock(at_ms.min(duration)), clock(duration)),
        None => clock(at_ms),
    }
}

/// A section of a list of threads: its heading over the first, if it has any.
fn push_section(
    rows: &mut Vec<ThreadRow>,
    threads: &[yapfast_core::chat::ListedThread],
    heading: String,
    now: u64,
) {
    for (index, thread) in threads.iter().enumerate() {
        let mut row = thread_row(thread, now);
        if index == 0 {
            row.heading = heading.clone().into();
        }
        rows.push(row);
    }
}

/// A thread as its list shows it: an active one with its newest message,
/// an archived one with who started it, as Discord's client does.
fn thread_row(thread: &yapfast_core::chat::ListedThread, now: u64) -> ThreadRow {
    let ago = ago(thread.active_ms, now);
    let (person, lead, text, time) = match (&thread.recent, &thread.owner, thread.archived) {
        (Some(recent), _, false) => (Some(&recent.author), "", recent.text.as_str(), ago),
        (None, _, false) => (None, "", "No recent messages", ago),
        (_, Some(owner), true) => (Some(owner), "Started by ", "", format!("Last active {ago}")),
        (_, None, true) => (None, "", "", format!("Last active {ago}")),
    };
    ThreadRow {
        room: thread.room.as_str().into(),
        heading: SharedString::default(),
        name: thread.name.as_str().into(),
        private: thread.private,
        archived: thread.archived,
        person: person.map_or("", |person| person.name.as_str()).into(),
        initial: person.map_or(SharedString::default(), |person| initial(&person.name)),
        tint: person.map_or(slint::Color::default(), |person| tint(&person.name)),
        picture_url: person.map_or("", |person| person.avatar.as_str()).into(),
        color: role_color(person.and_then(|person| person.color)),
        has_color: person.is_some_and(|person| person.color.is_some()),
        lead: lead.into(),
        text: text.into(),
        time: time.into(),
        members: match thread.members {
            0 => SharedString::default(),
            count => count.to_string().into(),
        },
        member_pictures: ModelRc::new(VecModel::from(
            thread
                .member_avatars
                .iter()
                .map(|avatar| SharedString::from(avatar.as_str()))
                .collect::<Vec<_>>(),
        )),
    }
}

/// A forum's tag, or an emoji, as a card or the form for a post shows it.
fn post_tag(name: &str, emoji: Option<&yapfast_core::chat::ShownEmoji>) -> PostTag {
    PostTag {
        name: name.into(),
        emoji: emoji
            .filter(|emoji| emoji.picture.is_none())
            .map_or(SharedString::default(), |emoji| emoji.text.as_str().into()),
        emoji_url: emoji
            .and_then(|emoji| emoji.picture.as_deref())
            .unwrap_or_default()
            .into(),
    }
}

/// A forum post as its card shows it, as Discord's client does: its first
/// three tags, its first message's author and text, its top reaction or the
/// forum's own, its replies, and when it was last active, or made when the
/// forum sorts by that.
fn post_row(
    post: &yapfast_core::application::ForumPost,
    info: &yapfast_core::application::ForumInfo,
    now: u64,
) -> PostRow {
    const TAGS: usize = 3;
    let thread = &post.thread;
    let tag = post_tag;
    let tags: Vec<PostTag> = post
        .tags
        .iter()
        .take(TAGS)
        .map(|shown| tag(&shown.name, shown.emoji.as_ref()))
        .collect();
    let starter = post.starter.as_ref();
    let first = starter.and_then(Option::as_ref);
    let (person, text) = match starter {
        Some(Some(first)) => (Some(&first.author), first.text.as_str()),
        Some(None) => (None, "Original message was deleted"),
        None => (None, ""),
    };
    let reaction = match first.and_then(|first| first.reaction.as_ref()) {
        Some((emoji, count)) => Some((tag("", Some(emoji)), count.to_string())),
        // A post without reactions offers the forum's own.
        None if first.is_some() => info
            .reaction
            .as_ref()
            .map(|emoji| (tag("", Some(emoji)), String::new())),
        None => None,
    };
    let at = if info.by_creation {
        thread.created_ms
    } else {
        thread.active_ms
    };
    PostRow {
        room: thread.room.as_str().into(),
        heading: SharedString::default(),
        name: thread.name.as_str().into(),
        pinned: thread.pinned,
        new: post.new,
        unread: thread.unread,
        tags: ModelRc::new(VecModel::from(tags)),
        more_tags: match post.tags.len().saturating_sub(TAGS) {
            0 => SharedString::default(),
            more => format!("+{more}").into(),
        },
        person: person.map_or("", |person| person.name.as_str()).into(),
        color: role_color(person.and_then(|person| person.color)),
        has_color: person.is_some_and(|person| person.color.is_some()),
        text: text.into(),
        has_reaction: reaction.is_some(),
        reaction_count: reaction
            .as_ref()
            .map_or(SharedString::default(), |(_, count)| count.as_str().into()),
        reaction: reaction.map(|(emoji, _)| emoji).unwrap_or_default(),
        replies: thread.replies.to_string().into(),
        time: ago(at, now).into(),
        picture_url: first
            .and_then(|first| first.picture.as_deref())
            .unwrap_or_default()
            .into(),
        spoiler: first.is_some_and(|first| first.spoiler),
    }
}

/// How long ago something was, as Discord's list of threads says it: "5s
/// ago" up to "30d ago", then ">30d ago" and ">1y ago".
pub(super) fn ago(at_ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        seconds if seconds < 60 => format!("{seconds}s ago"),
        seconds if seconds < 3600 => format!("{}m ago", seconds / 60),
        seconds if seconds < 86400 => format!("{}h ago", seconds / 3600),
        seconds if seconds <= 30 * 86400 => format!("{}d ago", seconds / 86400),
        seconds if seconds < 365 * 86400 => ">30d ago".into(),
        _ => ">1y ago".into(),
    }
}

/// The requests screen's rows: those waiting for this account's answer,
/// the friend requests it sent, and spam.
pub(super) struct RequestRows {
    pub(super) incoming: Vec<RequestRow>,
    pub(super) sent: Vec<RequestRow>,
    pub(super) spam: Vec<RequestRow>,
}

/// Friend requests and message requests as the requests screen lists them,
/// someone's friend request and message request on one row.
pub(super) fn request_rows(
    state: &AppState,
    message_requests: &[&yapfast_core::chat::ChatRoom],
    spam: &[&yapfast_core::chat::ChatRoom],
    twelve_hour: bool,
) -> RequestRows {
    let mut rows = RequestRows {
        incoming: Vec::new(),
        sent: Vec::new(),
        spam: Vec::new(),
    };
    let rooms = || message_requests.iter().chain(spam);
    let with_room = |row: &mut RequestRow, room: &yapfast_core::chat::ChatRoom| {
        row.room = room.id.as_str().into();
        row.unread = room.unread > 0;
        if let Some(preview) = &room.preview {
            row.message = preview.text.as_str().into();
            let seconds = preview.at / 1000;
            let today = yapfast_core::clock::date(yapfast_core::clock::now_ms() / 1000);
            row.time = if yapfast_core::clock::date(seconds) == today {
                yapfast_core::clock::time_on(seconds, twelve_hour)
            } else {
                yapfast_core::clock::day(seconds)
            }
            .into();
        }
    };
    for request in &state.friend_requests {
        let mut row = RequestRow {
            id: request.id.as_str().into(),
            name: request.name.as_str().into(),
            initial: initial(&request.name),
            tint: tint(&request.id),
            picture_url: request.avatar.as_deref().unwrap_or_default().into(),
            friend: if request.incoming { 1 } else { 2 },
            ..Default::default()
        };
        let room = rooms().find(|room| room.person.as_deref() == Some(request.id.as_str()));
        if let Some(room) = room {
            with_room(&mut row, room);
        }
        let spammed = request.spam || room.is_some_and(|room| room.category == discord::SPAM);
        match () {
            () if spammed => rows.spam.push(row),
            // A message waits for an answer, whoever asked to be friends.
            () if request.incoming || room.is_some() => rows.incoming.push(row),
            () => rows.sent.push(row),
        }
    }
    let befriending = |room: &&&yapfast_core::chat::ChatRoom| {
        !state
            .friend_requests
            .iter()
            .any(|request| room.person.as_deref() == Some(request.id.as_str()))
    };
    for (list, rooms) in [
        (&mut rows.incoming, message_requests),
        (&mut rows.spam, spam),
    ] {
        for room in rooms.iter().filter(befriending) {
            let mut row = RequestRow {
                id: room.person.as_deref().unwrap_or(&room.id).into(),
                name: room.name.as_str().into(),
                initial: initial(&room.name),
                tint: tint(&room.id),
                picture_url: room.avatar.as_deref().unwrap_or_default().into(),
                ..Default::default()
            };
            with_room(&mut row, room);
            list.push(row);
        }
    }
    rows
}

/// What the composer's mentions were coloured for: its draft, and what those
/// it may mention come from.
pub(super) struct MentionsMarked {
    draft: String,
    sources: MentionSources,
}

#[derive(Clone, Copy, PartialEq)]
struct MentionSources {
    /// The timeline's address, as its authors may be mentioned.
    messages: usize,
    members: usize,
    found: usize,
    groups: usize,
    color: u32,
}

/// A soundboard's groups as its rows show them: each group's heading, then
/// its sounds three to a row.
fn sound_rows(
    groups: &[voice::SoundGroup],
    pictures: &mut HashMap<String, slint::Image>,
    state: &AppState,
) -> Vec<SoundRow> {
    let mut rows = Vec::new();
    for group in groups {
        rows.push(SoundRow {
            heading: group.name.to_uppercase().into(),
            sounds: ModelRc::default(),
        });
        for chunk in group.sounds.chunks(3) {
            let cells: Vec<SoundCell> = chunk
                .iter()
                .map(|sound| SoundCell {
                    id: sound.id.as_str().into(),
                    name: sound.name.as_str().into(),
                    emoji: EmojiCell {
                        // A custom emoji waits as a quiet square until its
                        // picture comes.
                        glyph: if sound.picture.is_some() {
                            sound.name.as_str().into()
                        } else {
                            sound.emoji.as_str().into()
                        },
                        name: sound.name.as_str().into(),
                        key: SharedString::new(),
                        picture: cached_picture(pictures, state, sound.picture.as_deref()),
                    },
                    favorite: sound.favorite,
                })
                .collect();
            rows.push(SoundRow {
                heading: SharedString::new(),
                sounds: ModelRc::new(VecModel::from(cells)),
            });
        }
    }
    rows
}
