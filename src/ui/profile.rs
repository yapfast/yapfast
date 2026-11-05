//! Profile cards and roles: their rows, presence and custom status lines,
//! and what a profile offers about friendship.

use super::*;

/// Only mentions get a number; plain unread activity is a dot, because sync
/// does not deliver every message of a room that isn't open.
/// "Ann is", "Ann and Bob are", or "Ann, Bob and 3 others are".
/// What a profile or DM offers for a Discord friendship, as in
/// ProfileData.friend.
pub(super) fn friend_offer(relation: Relation) -> i32 {
    match relation {
        Relation::Stranger => 1,
        Relation::Outgoing => 2,
        Relation::Incoming => 3,
        Relation::Me | Relation::Friend => 0,
    }
}

/// The custom status as the account panel shows it: its words on one line,
/// after its emoji when that's Unicode, and the address of its custom emoji's
/// picture.
pub(super) fn status_line(status: &yapfast_core::chat::CustomStatus) -> (String, Option<String>) {
    let text = one_line(&status.text);
    match (&status.emoji, &status.emoji_id) {
        (_, Some(id)) => (text, yapfast_core::discord::emoji_picture(id)),
        (Some(emoji), None) if text.is_empty() => (emoji.clone(), None),
        (Some(emoji), None) => (format!("{emoji} {text}"), None),
        (None, None) => (text, None),
    }
}

/// A custom status or what someone is doing, as a row shows it: its words on
/// one line, and the address of its custom emoji's picture.
pub(super) fn activity_line(activity: Option<&str>) -> (SharedString, SharedString) {
    let (emoji, words) = activity.map_or((None, ""), yapfast_core::discord::status_parts);
    (one_line(words).into(), emoji.unwrap_or_default().into())
}

/// When ActivityClock's seconds start: 2020 began, in Unix seconds, so
/// Slint's 32-bit integers hold them for decades.
const ACTIVITY_EPOCH: u64 = 1_577_836_800;

/// Now on ActivityClock.
fn activity_now() -> i32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i32::try_from(now.saturating_sub(ACTIVITY_EPOCH)).unwrap_or(i32::MAX)
}

/// Someone's activities as their profile's cards show them.
pub(super) fn activity_rows(activities: &yapfast_core::chat::Activities) -> ModelRc<ActivityRow> {
    if activities.is_empty() {
        return ModelRc::default();
    }
    #[cfg(feature = "discord")]
    let rows: Vec<ActivityRow> = activities.iter().map(activity_row).collect();
    #[cfg(not(feature = "discord"))]
    let rows: Vec<ActivityRow> = Vec::new();
    ModelRc::new(VecModel::from(rows))
}

#[cfg(feature = "discord")]
fn activity_row(activity: &yapfast_core::chat::Activity) -> ActivityRow {
    let seconds = |ms: Option<u64>| {
        ms.map_or(0, |ms| {
            i32::try_from((ms / 1000).saturating_sub(ACTIVITY_EPOCH)).unwrap_or(0)
        })
    };
    let text = |text: &Option<Box<str>>| SharedString::from(text.as_deref().unwrap_or_default());
    // A Hang status's emoji goes before its words, as a status's does.
    let name = match &activity.emoji {
        Some(emoji) => format!("{emoji} {}", activity.name),
        None => activity.name.to_string(),
    };
    let (name_emoji, name) = yapfast_core::discord::status_parts(&name);
    let state = match (&activity.state, activity.party) {
        (Some(state), Some((size, most))) => format!("{state} ({size} of {most})"),
        (None, Some((size, most))) => format!("{size} of {most}"),
        (state, None) => state.as_deref().unwrap_or_default().to_owned(),
    };
    // Spotify's card is titled by the song, then by whom and on which album.
    let (name, details, state) = if activity.spotify() {
        let after = |word: &str, text: &Option<Box<str>>| {
            text.as_deref()
                .map_or(String::new(), |text| format!("{word} {text}"))
        };
        (
            activity.details.as_deref().unwrap_or(name).to_owned(),
            after("by", &activity.state),
            // A podcast has no artist, and Discord names only the episode.
            if activity.state.is_some() {
                after("on", &activity.large_text)
            } else {
                String::new()
            },
        )
    } else {
        (
            name.to_owned(),
            activity.details.as_deref().unwrap_or_default().to_owned(),
            state,
        )
    };
    ActivityRow {
        heading: activity.heading().into(),
        name: name.into(),
        name_emoji: name_emoji.unwrap_or_default().into(),
        details: details.into(),
        state: state.into(),
        large_url: text(&activity.large_image),
        large_text: text(&activity.large_text),
        small_url: text(&activity.small_image),
        small_text: text(&activity.small_text),
        start: seconds(activity.start),
        end: seconds(activity.end),
        url: text(&activity.url),
        link: text(&activity.link),
    }
}

/// A Discord board as its tab shows it.
fn widget_rows(widgets: &[yapfast_core::chat::ProfileWidget]) -> ModelRc<ProfileWidgetRow> {
    let rows: Vec<ProfileWidgetRow> = widgets
        .iter()
        .map(|widget| ProfileWidgetRow {
            title: widget.title.as_str().into(),
            elsewhere: !widget.lists_games,
            games: ModelRc::new(VecModel::from(
                widget
                    .games
                    .iter()
                    .map(|game| ProfileGameRow {
                        name: game.name.as_str().into(),
                        initial: initial(&game.name),
                        icon: game.icon.as_deref().unwrap_or_default().into(),
                        comment: game.comment.as_deref().unwrap_or_default().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// A Discord wishlist as its tab shows it: its items, and a line about it. An
/// empty list has no line, as the tab says that itself.
fn wishlist_rows(
    profile: &yapfast_core::chat::Profile,
) -> (ModelRc<ProfileWishlistRow>, &'static str) {
    use yapfast_core::chat::Wishlist;
    let private = profile
        .wishlist_place
        .as_ref()
        .is_some_and(|place| place.private);
    match &profile.wishlist {
        Wishlist::Unasked | Wishlist::Loading => (ModelRc::default(), "Loading the wishlist…"),
        Wishlist::Hidden => (ModelRc::default(), "This wishlist is private."),
        Wishlist::Failed => (ModelRc::default(), "Couldn't load the wishlist."),
        Wishlist::Loaded(items) => {
            let rows: Vec<ProfileWishlistRow> = items
                .iter()
                .map(|item| ProfileWishlistRow {
                    name: item.name.as_str().into(),
                    initial: initial(&item.name),
                    url: item.picture.as_deref().unwrap_or_default().into(),
                })
                .collect();
            let note = if private && !rows.is_empty() {
                "Your wishlist is private: only you can see it."
            } else {
                ""
            };
            (ModelRc::new(VecModel::from(rows)), note)
        }
    }
}

/// Board rows, each holding a list of its own, compared as `same_list` does.
pub(super) fn same_widget_rows(
    old: &ModelRc<ProfileWidgetRow>,
    new: ModelRc<ProfileWidgetRow>,
) -> ModelRc<ProfileWidgetRow> {
    let rows: Vec<ProfileWidgetRow> = new
        .iter()
        .enumerate()
        .map(|(index, row)| match old.row_data(index) {
            Some(shown) => ProfileWidgetRow {
                games: same_list(&shown.games, row.games.clone()),
                ..row
            },
            None => row,
        })
        .collect();
    same_list(old, ModelRc::new(VecModel::from(rows)))
}

/// Whether a line under a name only says what one of the activities does.
pub(super) fn names_an_activity(line: &str, activities: &yapfast_core::chat::Activities) -> bool {
    #[cfg(feature = "discord")]
    return activities
        .iter()
        .any(|activity| activity.line().as_deref() == Some(line));
    #[cfg(not(feature = "discord"))]
    {
        let _ = (line, activities);
        false
    }
}

/// ActivityClock's answer to what shows an activity in a window that comes
/// and goes, which keeps its own time: the account's menu.
pub(super) fn bind_activity_clock(window: &YapfastWindow) {
    let weak = window.as_weak();
    window.global::<ActivityClock>().on_due(move || {
        if let Some(window) = weak.upgrade() {
            window.global::<ActivityClock>().set_now(activity_now());
        }
    });
}

impl Adapter {
    /// Keeps ActivityClock current, once a second, while a profile on
    /// screen times an activity, and leaves it be otherwise.
    pub(super) fn tick_activities(&mut self, window: &YapfastWindow) {
        let timed = |data: ProfileData| {
            data.activities
                .iter()
                .any(|row| row.start > 0 || row.end > 0)
        };
        let shown = (self.app.state.profile.is_some() && timed(window.get_profile()))
            || (self.app.state.panel_profile.is_some() && timed(window.get_panel_profile()));
        if !shown {
            self.activity_clock.stop();
            return;
        }
        if self.activity_clock.running() {
            return;
        }
        window.global::<ActivityClock>().set_now(activity_now());
        let window = window.as_weak();
        self.activity_clock.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(1),
            move || {
                if let Some(window) = window.upgrade() {
                    window.global::<ActivityClock>().set_now(activity_now());
                }
            },
        );
    }
}

/// The picker's key for the custom status's emoji: the emoji, or `name:id`.
pub(super) fn status_key(status: &yapfast_core::chat::CustomStatus) -> Option<String> {
    match (&status.emoji, &status.emoji_id) {
        (Some(name), Some(id)) => Some(format!("{name}:{id}")),
        (Some(emoji), None) => Some(emoji.clone()),
        _ => None,
    }
}

/// Role rows, each holding a list of its own, compared as `same_list` does.
pub(super) fn same_role_rows(
    old: &ModelRc<ProfileRoleRow>,
    new: ModelRc<ProfileRoleRow>,
) -> ModelRc<ProfileRoleRow> {
    let rows: Vec<ProfileRoleRow> = new
        .iter()
        .enumerate()
        .map(|(index, row)| match old.row_data(index) {
            Some(shown) => ProfileRoleRow {
                roles: same_list(&shown.roles, row.roles),
            },
            None => row,
        })
        .collect();
    same_list(old, ModelRc::new(VecModel::from(rows)))
}

/// The font Discord numbers `font`, as its client draws a name in it: the
/// family, the weight it loads, and the letter spacing it adds, in ems. Numbers
/// its client no longer draws, and fonts Yapfast doesn't have, are the UI's own.
fn name_face(style: Option<&yapfast_core::chat::NameStyle>) -> (&'static str, i32, f32) {
    match style.map(|style| style.font) {
        Some(3) => ("Cherry Bomb One", 400, 0.04),
        Some(4) => ("Chicle", 400, 0.0),
        Some(6) => ("MuseoModerno", 500, 0.01),
        Some(8) => ("Pixelify Sans", 400, 0.02),
        Some(10) => ("Sinistre", 400, 0.01),
        Some(12) => ("Zilla Slab", 600, 0.03),
        Some(13) => ("Playpen Sans", 700, 0.0),
        // Orbitron, renamed as its license asks of a modified copy.
        Some(14) => ("Yapfast Orbit", 700, 0.03),
        Some(15) => ("New Rocker", 400, 0.0),
        Some(16) => ("Kalam", 700, 0.0),
        _ => ("", 0, 0.0),
    }
}

/// The family of a styled name's font; empty for the UI's own.
pub(super) fn name_font(style: Option<&yapfast_core::chat::NameStyle>) -> SharedString {
    name_face(style).0.into()
}

/// The weight of a styled name's font; none for the UI's own.
pub(super) fn name_weight(style: Option<&yapfast_core::chat::NameStyle>) -> i32 {
    name_face(style).1
}

/// The letter spacing of a styled name's font, in ems.
pub(super) fn name_spacing(style: Option<&yapfast_core::chat::NameStyle>) -> f32 {
    name_face(style).2
}

/// The colours a styled name picked; Discord's single black means none.
fn name_colours(style: Option<&yapfast_core::chat::NameStyle>) -> &[u32] {
    match style.map_or(&[][..], |style| style.colors.as_slice()) {
        [0] => &[],
        colours => colours,
    }
}

/// How a styled name moves: Discord's number, or none for a plain one.
pub(super) fn name_effect(style: Option<&yapfast_core::chat::NameStyle>) -> i32 {
    style.map_or(0, |style| style.effect as i32)
}

/// Whether a styled name picked colours of its own.
pub(super) fn has_name_paint(style: Option<&yapfast_core::chat::NameStyle>) -> bool {
    !name_colours(style).is_empty()
}

/// A styled name's colours as the name draws them: a key for its gummy
/// letters, and five stops spread evenly over what it picked.
pub(super) fn name_paint(style: Option<&yapfast_core::chat::NameStyle>) -> NamePreset {
    let colours = name_colours(style);
    let channel = |colour: u32, shift: u32| ((colour >> shift) & 0xff) as f32;
    let stop = |at: f32| {
        if colours.is_empty() {
            return slint::Color::from_rgb_u8(255, 255, 255);
        }
        let place = at * (colours.len() - 1) as f32;
        let (low, part) = (place.floor() as usize, place.fract());
        let (from, to) = (colours[low], colours[(low + 1).min(colours.len() - 1)]);
        let mix = |shift| {
            (channel(from, shift) + (channel(to, shift) - channel(from, shift)) * part) as u8
        };
        slint::Color::from_rgb_u8(mix(16), mix(8), mix(0))
    };
    NamePreset {
        key: colours
            .iter()
            .map(|colour| format!("{colour:06x}"))
            .collect::<Vec<_>>()
            .join(",")
            .into(),
        c1: stop(0.0),
        c2: stop(0.25),
        c3: stop(0.5),
        c4: stop(0.75),
        c5: stop(1.0),
    }
}

/// The profile to show in place of `shown`, keeping its lists where they're
/// the same; none when nothing changed.
pub(super) fn changed_profile(shown: &ProfileData, mut data: ProfileData) -> Option<ProfileData> {
    data.servers = same_list(&shown.servers, data.servers);
    data.friends = same_list(&shown.friends, data.friends);
    data.frame = same_list(&shown.frame, data.frame);
    data.recent = same_list(&shown.recent, data.recent);
    data.activities = same_list(&shown.activities, data.activities);
    data.widgets = same_widget_rows(&shown.widgets, data.widgets);
    data.wishlist = same_list(&shown.wishlist, data.wishlist);
    data.badges = same_list(&shown.badges, data.badges);
    data.effect = same_list(&shown.effect, data.effect);
    data.picture_delays = same_list(&shown.picture_delays, data.picture_delays);
    data.decoration_delays = same_list(&shown.decoration_delays, data.decoration_delays);
    data.banner_delays = same_list(&shown.banner_delays, data.banner_delays);
    data.role_rows = same_role_rows(&shown.role_rows, data.role_rows);
    data.role_rows_large = same_role_rows(&shown.role_rows_large, data.role_rows_large);
    // Comparing an animation's frames would ask for each in turn, sending its
    // decoder off from the one showing. A different animation has a different
    // still, which is compared, so the number of frames is all it takes.
    let without_frames = |profile: &ProfileData| ProfileData {
        picture_frames: ModelRc::default(),
        decoration_frames: ModelRc::default(),
        banner_frames: ModelRc::default(),
        ..profile.clone()
    };
    let same_frames = shown.picture_frames.row_count() == data.picture_frames.row_count()
        && shown.decoration_frames.row_count() == data.decoration_frames.row_count()
        && shown.banner_frames.row_count() == data.banner_frames.row_count();
    (!same_frames || without_frames(shown) != without_frames(&data)).then_some(data)
}

/// A profile as a card, the full profile or the DM panel shows it.
pub(super) fn profile_data(
    pictures: &mut HashMap<String, slint::Image>,
    state: &AppState,
    view: &ProfileView,
) -> ProfileData {
    let profile = &view.profile;
    // Nothing fetched or listed says what this account does; the session does.
    let activities = if state.account_id.as_deref() == Some(profile.id.as_str()) {
        &state.discord_activities
    } else {
        &profile.activities
    };
    let handle = [profile.handle.as_str(), profile.pronouns.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    // The profile keeps the status's own line breaks. A line that only
    // names an activity goes, as its card says more.
    let own_status = state
        .custom_status
        .as_ref()
        .filter(|_| state.account_id.as_deref() == Some(profile.id.as_str()) && state.online)
        .map(status_line);
    let (activity_emoji, activity) = match &own_status {
        Some((words, emoji)) => (emoji.clone(), words.as_str()),
        None => profile
            .activity
            .as_deref()
            .filter(|line| !names_an_activity(line, activities))
            .map_or((None, ""), yapfast_core::discord::status_parts),
    };
    // What the profile switches let show, and where its pictures come from.
    let cosmetics = state.cosmetics;
    let avatar = cosmetics.avatar(profile);
    let banner = cosmetics.banner(profile);
    let decoration = cosmetics.decoration(profile);
    let effect = cosmetics.effect(profile);
    let frame = cosmetics.frame(profile);
    let tag = cosmetics.tag(profile);
    let name_style = cosmetics.name_style(profile);
    let moving = |url: Option<&str>| {
        url.and_then(|url| state.pictures.get(url))
            .map(pictures::animation)
            .unwrap_or_default()
    };
    let (banner_frames, banner_delays) = moving(banner.as_deref());
    let (picture_frames, picture_delays) = moving(avatar.as_deref());
    let (decoration_frames, decoration_delays) = moving(decoration.as_deref());
    // A moving avatar's still is usually here already, from the chat, to show
    // until the moving one comes.
    let mut picture = cached_picture(pictures, state, avatar.as_deref());
    if picture.size().width == 0
        && let Some(still) = avatar.as_deref().map(|url| url.replace(".gif?", ".png?"))
    {
        picture = cached_picture(pictures, state, Some(&still));
    }
    let (wishlist, wishlist_note) = wishlist_rows(profile);
    let own = state.account_id.as_deref() == Some(profile.id.as_str());
    let friend = state.friends.iter().find(|friend| friend.id == profile.id);
    // Matrix IDs start with `@`; the preview's test members look like them.
    let discord = !profile.id.starts_with('@') || state.preview;
    let mutuals_known = !view.loading || view.kept;
    // A call they're in, in a conversation or channel this account sees.
    let call = state
        .rooms
        .iter()
        .find(|room| room.people.iter().any(|person| person.id == profile.id));
    // A single black is what Discord gives a name left uncoloured.
    let name_colors: &[u32] = name_style.map_or(&[], |style| style.colors.as_slice());
    let name_colors = if name_colors == [0] {
        &[][..]
    } else {
        name_colors
    };
    ProfileData {
        id: profile.id.clone().into(),
        name: profile.name.clone().into(),
        initial: initial(&profile.name),
        tint: tint(&profile.name),
        picture,
        picture_frames,
        picture_delays,
        decoration: cached_picture(pictures, state, decoration.as_deref()),
        decoration_frames,
        decoration_delays,
        handle: handle.into(),
        name_font: name_font(name_style),
        name_weight: name_weight(name_style),
        name_spacing: name_spacing(name_style),
        has_name_colors: !name_colors.is_empty(),
        name_effect: name_effect(name_style),
        name_paint: name_paint(name_style),
        effect: ModelRc::new(VecModel::from(
            effect
                .iter()
                .map(|layer| ProfileEffectRow {
                    url: layer.src.clone().into(),
                    start: layer.start as i32,
                    duration: layer.duration as i32,
                    looping: layer.looping,
                    loop_delay: layer.loop_delay as i32,
                    x: layer.x,
                    y: layer.y,
                    width: layer.width as i32,
                    height: layer.height as i32,
                })
                .collect::<Vec<_>>(),
        )),
        // It plays once every layer is here, so none comes in late.
        effect_ready: !effect.is_empty()
            && effect
                .iter()
                .all(|layer| state.pictures.contains_key(&layer.src)),
        tag_text: tag.map(|tag| tag.text.as_str()).unwrap_or_default().into(),
        tag_icon: tag
            .and_then(|tag| tag.badge.as_deref())
            .unwrap_or_default()
            .into(),
        banner: cached_picture(pictures, state, banner.as_deref()),
        banner_frames,
        banner_delays,
        accent: role_color(profile.accent),
        has_accent: profile.accent.is_some(),
        theme_top: role_color(profile.theme.map(|[top, _]| top)),
        theme_bottom: role_color(profile.theme.map(|[_, bottom]| bottom)),
        has_theme: profile.theme.is_some(),
        theme_light: profile.theme_is_light(),
        badges: ModelRc::new(VecModel::from(
            profile
                .badges
                .iter()
                .map(|badge| ProfileBadgeRow {
                    label: badge.description.clone().into(),
                    icon: badge.icon.clone().into(),
                    link: badge.link.clone().unwrap_or_default().into(),
                    directory: badge
                        .directory
                        .and_then(|id| i32::try_from(id).ok())
                        .unwrap_or(-1),
                })
                .collect::<Vec<_>>(),
        )),
        presence: presence_name(profile.presence).into(),
        activity: activity.into(),
        activity_emoji: activity_emoji.unwrap_or_default().into(),
        activities: activity_rows(activities),
        widgets: widget_rows(&profile.widgets),
        board_note: if profile.board_asked && !profile.board_named {
            "Loading the board…".into()
        } else {
            Default::default()
        },
        wishlist,
        wishlist_note: wishlist_note.into(),
        bio: markdown(&profile.bio),
        has_bio: !profile.bio.trim().is_empty(),
        role_rows: role_rows(&profile.roles, PROFILE_CARD_WIDTH),
        role_rows_large: role_rows(&profile.roles, PROFILE_LARGE_WIDTH),
        created: profile.created.clone().unwrap_or_default().into(),
        joined: profile.joined.clone().unwrap_or_default().into(),
        servers: ModelRc::new(VecModel::from(
            profile
                .servers
                .iter()
                .map(|server| ProfileServerRow {
                    id: server.id.as_str().into(),
                    name: server.name.as_str().into(),
                    initial: initial(&server.name),
                    picture: cached_picture(pictures, state, server.icon.as_deref()),
                    banner: cached_picture(pictures, state, server.banner.as_deref()),
                    members: server
                        .members
                        .map(|count| count_of(count, "member", "members"))
                        .unwrap_or_default()
                        .into(),
                    channels: count_of(server.channels as u64, "channel", "channels").into(),
                    nick: server.nick.as_deref().unwrap_or_default().into(),
                })
                .collect::<Vec<_>>(),
        )),
        friends: ModelRc::new(VecModel::from(
            profile
                .friends
                .iter()
                .map(|friend| {
                    let known = state.friends.iter().find(|known| known.id == friend.id);
                    ProfileFriendRow {
                        id: friend.id.as_str().into(),
                        name: friend.name.as_str().into(),
                        initial: initial(&friend.name),
                        tint: tint(&friend.id),
                        picture: cached_picture(pictures, state, friend.avatar.as_deref()),
                        picture_url: friend.avatar.as_deref().unwrap_or_default().into(),
                        presence: presence_name(Some(
                            known
                                .and_then(|known| known.presence)
                                .unwrap_or(Presence::Offline),
                        ))
                        .into(),
                    }
                })
                .collect::<Vec<_>>(),
        )),
        note: if view.loading && !view.kept {
            "Loading the profile...".into()
        } else {
            view.note.clone().unwrap_or_default().into()
        },
        // Matrix IDs start with `@`; Discord user ids are numbers.
        friend: if profile.id.starts_with('@') {
            0
        } else {
            friend_offer(state.relation(&profile.id))
        },
        // Discord's card names them as the profile does.
        message_hint: if state
            .person_choices(&profile.id)
            .contains(&MenuChoice::Message)
        {
            format!("Message @{}", profile.name).into()
        } else {
            Default::default()
        },
        own,
        badges_window: discord && !profile.bot,
        friends_since: friend
            .and_then(|friend| friend.since.as_deref())
            .or(profile.friends_since.as_deref())
            .unwrap_or_default()
            .into(),
        // The client's list of tabs: the Board for one's own or with widgets,
        // and the Wishlist for one's own or a public one from someone a friend
        // or in a shared server. Its mutual tabs show for everyone else.
        board_tab: discord && (own || !profile.widgets.is_empty()),
        wishlist_tab: discord
            && (own
                || (profile
                    .wishlist_place
                    .as_ref()
                    .is_some_and(|place| !place.private)
                    && (friend.is_some() || !profile.servers.is_empty()))),
        mutual_tabs: discord && !own && !profile.bot,
        friends_label: mutual_label(mutuals_known, profile.friends.len(), "Friend").into(),
        servers_label: mutual_label(mutuals_known, profile.servers.len(), "Server").into(),
        mutuals_known,
        is_friend: friend.is_some(),
        theme_window: profile
            .theme
            .map_or_else(slint::Color::default, |[top, bottom]| {
                theme_window(top, bottom, profile.theme_is_light())
            }),
        theme_button: profile
            .theme
            .map_or_else(slint::Color::default, |[top, _]| theme_button(top)),
        frame: ModelRc::new(VecModel::from(
            frame
                .iter()
                .flat_map(|frame| &frame.layers)
                .map(|layer| ProfileFrameRow {
                    url: layer.src.as_str().into(),
                    kind: layer.kind.as_str().into(),
                    anchor: layer.anchor.as_str().into(),
                    front: layer.front,
                })
                .collect::<Vec<_>>(),
        )),
        frame_width: frame.map_or(0, |frame| frame.inner_width as i32),
        frame_top: frame.map_or(0, |frame| frame.overflow_top as i32),
        frame_bottom: frame.map_or(0, |frame| frame.overflow_bottom as i32),
        frame_side: frame.map_or(0, |frame| frame.overflow_horizontal as i32),
        recent: ModelRc::new(VecModel::from(
            profile
                .recent
                .iter()
                .map(|entry| ProfileRecentRow {
                    title: entry.title.as_str().into(),
                    subtitle: entry.subtitle.as_str().into(),
                    url: entry.picture.as_deref().unwrap_or_default().into(),
                    initial: initial(&entry.title),
                    music: entry.music,
                    when: entry.at.map(time_ago).unwrap_or_default().into(),
                })
                .collect::<Vec<_>>(),
        )),
        call_room: call.map_or("", |room| room.id.as_str()).into(),
        call_name: call.map_or("", |room| room.name.as_str()).into(),
        call_heading: call
            .map_or("", |room| {
                if room.voice {
                    "In a voice channel"
                } else {
                    "In a call"
                }
            })
            .into(),
        call_can_join: call.is_some_and(|room| state.can_join_voice(&room.id)),
    }
}

/// How long ago a time in Unix milliseconds was, as the Activity tab says it.
fn time_ago(at: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let minutes = now.saturating_sub(at) / 60_000;
    match minutes {
        0 => "Just now".to_owned(),
        1..60 => format!("{minutes}m ago"),
        60..1440 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
    }
}

/// A colour's CIE Lab, the space Discord's client works these out in, from RGB.
fn to_lab(rgb: u32) -> [f32; 3] {
    let linear = |shift: u32| {
        let channel = ((rgb >> shift) & 0xff) as f32 / 255.;
        if channel > 0.04045 {
            ((channel + 0.055) / 1.055).powf(2.4)
        } else {
            channel / 12.92
        }
    };
    let (r, g, b) = (linear(16), linear(8), linear(0));
    let f = |t: f32| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16. / 116.
        }
    };
    let x = f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047);
    let y = f(0.2126 * r + 0.7152 * g + 0.0722 * b);
    let z = f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883);
    [116. * y - 16., 500. * (x - y), 200. * (y - z)]
}

/// Lab back to RGB, clamped to what can show.
fn from_lab([l, a, b]: [f32; 3]) -> u32 {
    let y = (l + 16.) / 116.;
    let g = |t: f32| {
        if t.powi(3) > 0.008856 {
            t.powi(3)
        } else {
            (t - 16. / 116.) / 7.787
        }
    };
    let (x, z) = (g(y + a / 500.) * 0.95047, g(y - b / 200.) * 1.08883);
    let y = g(y);
    let channel = |value: f32| {
        let value = value.clamp(0., 1.);
        let value = if value > 0.0031308 {
            1.055 * value.powf(1. / 2.4) - 0.055
        } else {
            12.92 * value
        };
        (value * 255.).round() as u32
    };
    channel(3.2406 * x - 1.5372 * y - 0.4986 * z) << 16
        | channel(-0.9689 * x + 1.8758 * y + 0.0415 * z) << 8
        | channel(0.0557 * x - 0.204 * y + 1.057 * z)
}

fn luminance(rgb: u32) -> f32 {
    let linear = |shift: u32| {
        let channel = ((rgb >> shift) & 0xff) as f32 / 255.;
        if channel > 0.03928 {
            ((channel + 0.055) / 1.055).powf(2.4)
        } else {
            channel / 12.92
        }
    };
    0.2126 * linear(16) + 0.7152 * linear(8) + 0.0722 * linear(0)
}

/// A themed window's background, as the client's
/// `--profile-gradient-modal-background-color`: its two colours half and half in
/// Lab, with the lightness held to 10% or less, or 80% or more on a light theme.
fn theme_window(top: u32, bottom: u32, light: bool) -> slint::Color {
    let (top, bottom) = (to_lab(top), to_lab(bottom));
    let mix = from_lab([
        (top[0] + bottom[0]) / 2.,
        (top[1] + bottom[1]) / 2.,
        (top[2] + bottom[2]) / 2.,
    ]);
    let channel = |shift: u32| ((mix >> shift) & 0xff) as f32 / 255.;
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (high + low) / 2.;
    let held = if light {
        lightness.clamp(0.8, 1.)
    } else {
        lightness.clamp(0., 0.1)
    };
    let spread = high - low;
    let saturation = if spread == 0. {
        0.
    } else {
        spread / (1. - (2. * lightness - 1.).abs()).max(f32::EPSILON)
    };
    let hue = if spread == 0. {
        0.
    } else if high == r {
        ((g - b) / spread).rem_euclid(6.)
    } else if high == g {
        (b - r) / spread + 2.
    } else {
        (r - g) / spread + 4.
    };
    let reach = (1. - (2. * held - 1.).abs()) * saturation;
    let second = reach * (1. - (hue.rem_euclid(2.) - 1.).abs());
    let (r, g, b) = match hue as u32 {
        0 => (reach, second, 0.),
        1 => (second, reach, 0.),
        2 => (0., reach, second),
        3 => (0., second, reach),
        4 => (second, 0., reach),
        _ => (reach, 0., second),
    };
    let base = held - reach / 2.;
    let byte = |value: f32| ((value + base).clamp(0., 1.) * 255.).round() as u8;
    slint::Color::from_rgb_u8(byte(r), byte(g), byte(b))
}

/// A themed profile's main button, as the client's `--profile-gradient-button-color`:
/// its first colour darkened or brightened till white on it contrasts 7 to 10.
fn theme_button(top: u32) -> slint::Color {
    const WHITE: f32 = 1.05;
    let contrast = |rgb: u32| WHITE / (luminance(rgb) + 0.05);
    let mut lab = to_lab(top);
    let mut colour = top;
    for _ in 0..100 {
        let now = contrast(colour);
        if (7. ..=10.).contains(&now) {
            break;
        }
        // Lab's step for darken and brighten in the client's colour library.
        lab[0] += if now < 7. { -18. } else { 18. };
        colour = from_lab(lab);
    }
    slint::Color::from_rgb_u8((colour >> 16) as u8, (colour >> 8) as u8, colour as u8)
}

/// A mutual tab's words, as the client's: none till it knows, then how many.
fn mutual_label(known: bool, count: usize, what: &str) -> String {
    match (known, count) {
        (false, _) => format!("Mutual {what}s"),
        (true, 0) => format!("No Mutual {what}s"),
        (true, 1) => format!("1 Mutual {what}"),
        (true, count) => format!("{count} Mutual {what}s"),
    }
}

/// Someone's badge, as their window shows it: at the level they've reached,
/// with what the client takes from this account's own badge, `mine`: the words
/// about unlocking it, and whether to offer its own.
pub(super) fn person_badge_row(
    badge: &yapfast_core::chat::DirectoryBadge,
    mine: Option<&yapfast_core::chat::DirectoryBadge>,
) -> BadgeRow {
    /// The client's badge numbers it treats specially.
    const NITRO: u64 = 1;
    const BOOST: u64 = 12;
    const QUESTS: u64 = 14;
    const APRIL_FOOLS: u64 = 15;
    const ORBS: u64 = 16;
    const GIFTING: u64 = 17;
    const STAFF: u64 = 22;
    let nitro = badge.id == NITRO;
    let (eyebrow, name) = match (&badge.level, nitro) {
        (Some(level), true) => (String::new(), format!("Nitro {level}")),
        (None, true) => (String::new(), "Nitro".to_owned()),
        (Some(level), false) => (badge.name.clone(), level.clone()),
        (None, false) => (String::new(), badge.name.clone()),
    };
    let mine_owned = mine.is_some_and(|mine| mine.owned);
    // Those from before badges could be earned, which the client's map knows:
    // the numbers below Gifting.
    let legacy = badge.id < GIFTING && !badge.earnable;
    let date = if badge.id == STAFF {
        None
    } else if badge.id == APRIL_FOOLS {
        Some("April 2026".to_owned())
    } else if legacy && badge.unlocked.is_none() {
        // The client picks one of three at random when it has no date.
        let lines = ["Very, very early", "iykyk", "Lost to time"];
        Some(lines[badge.id as usize % lines.len()].to_owned())
    } else {
        badge.unlocked.clone()
    };
    // What its button does in the client, as far as a page of Discord's goes:
    // it never gives or buys anything from here. Boosting has only settings.
    let (action, link) = match badge.id {
        STAFF => ("Work With Us", "https://discord.com/careers"),
        NITRO if mine_owned => ("Nitro Home", "https://discord.com/store"),
        NITRO => ("Get Nitro", "https://discord.com/store"),
        BOOST => ("", ""),
        ORBS => ("Orbs Shop", "https://discord.com/shop?tab=orbs"),
        QUESTS => ("Quests Home", "https://discord.com/quest-home"),
        GIFTING => ("Send Gift", "https://discord.com/shop"),
        _ => ("", ""),
    };
    let maxed =
        mine.is_some_and(|mine| mine.owned && !mine.levels.is_empty() && mine.next_name.is_none());
    BadgeRow {
        id: badge.id as i32,
        name: name.to_uppercase().into(),
        owned: true,
        description: if maxed {
            "You've maxed out this badge. Sit back, relax, and enjoy the view from the top.".into()
        } else {
            mine.map_or(badge.description.as_str(), |mine| mine.description.as_str())
                .into()
        },
        info: badge.info.as_str().into(),
        rarity: match badge.level_rarity {
            1 => "Common",
            2 => "Rare",
            3 => "Epic",
            5 => "Mythic",
            _ => "",
        }
        .into(),
        icon: badge.icon.as_str().into(),
        art: badge.level_art.as_str().into(),
        unlocked: date.unwrap_or_default().into(),
        has_next: false,
        next_name: Default::default(),
        next_icon: Default::default(),
        current_icon: Default::default(),
        progress: 0.0,
        helper: Default::default(),
        levels: ModelRc::new(VecModel::from(
            badge
                .levels
                .iter()
                .map(|level| BadgeLevelRow {
                    name: level.name.as_str().into(),
                    icon: level.icon.as_str().into(),
                    owned: level.owned,
                    milestone: level.milestone.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )),
        eyebrow: eyebrow.into(),
        date_label: if badge.first_level {
            "Date unlocked"
        } else {
            "Level unlocked"
        }
        .into(),
        beta: matches!(badge.id, 18..=21),
        legacy,
        gone: !badge.earnable && !mine_owned && badge.id != STAFF,
        action: action.into(),
        action_link: link.into(),
        action_quiet: mine_owned,
        view_yours: mine_owned,
    }
}

/// A name as the badge window's headings shorten it.
pub(super) fn badge_window_name(name: &str) -> String {
    if name.chars().count() > 19 {
        format!("{}...", name.chars().take(19).collect::<String>())
    } else {
        name.to_owned()
    }
}

pub(super) fn presence_name(presence: Option<Presence>) -> &'static str {
    match presence {
        Some(Presence::Online) => "online",
        Some(Presence::Mobile) => "mobile",
        Some(Presence::Idle) => "idle",
        Some(Presence::DoNotDisturb) => "dnd",
        Some(Presence::Offline) => "offline",
        None => "",
    }
}

/// Roles as chips, a row at a time. Slint has no wrapping layout, so chips
/// are measured roughly: Atkinson Hyperlegible Next Medium at 13px averages
/// under 7px a letter, as Satoshi Medium does.
pub(super) fn role_rows(roles: &[ProfileRole], width: f32) -> ModelRc<ProfileRoleRow> {
    const GAP: f32 = 4.;
    // Padding, the colour dot and the space after it.
    const CHROME: f32 = 8. + 10. + 6. + 8.;
    let mut rows: Vec<Vec<yapfast_ui::ProfileRole>> = Vec::new();
    let mut used = width;
    for role in roles {
        let chip = CHROME + 7. * role.name.chars().count() as f32;
        if used + GAP + chip > width || rows.is_empty() {
            rows.push(Vec::new());
            used = chip;
        } else {
            used += GAP + chip;
        }
        if let Some(row) = rows.last_mut() {
            row.push(yapfast_ui::ProfileRole {
                name: role.name.clone().into(),
                color: role_color(role.color),
                has_color: role.color.is_some(),
            });
        }
    }
    ModelRc::new(VecModel::from(
        rows.into_iter()
            .map(|roles| ProfileRoleRow {
                roles: ModelRc::new(VecModel::from(roles)),
            })
            .collect::<Vec<_>>(),
    ))
}

/// A Discord role's RGB colour for Slint.
pub(super) fn role_color(rgb: Option<u32>) -> slint::Color {
    rgb.map_or_else(slint::Color::default, |rgb| {
        slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    })
}
