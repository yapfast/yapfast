//! Profile cards and roles: their rows, presence and custom status lines,
//! and what a profile offers about friendship.

use super::*;

/// A bio with each custom emoji as its glyph where the emoji's picture has
/// arrived already, and as a blank of an emoji's width where it has not, so
/// the bio doesn't show `:name:` and then jump when the picture comes.
fn bio_with_emoji(bio: &str, emoji: &[yapfast_core::chat::CustomEmoji]) -> String {
    let mut out = crate::custom_emoji::text(bio, emoji);
    for each in emoji {
        if crate::custom_emoji::glyph(&each.id).is_none() {
            out = out.replace(&each.text, "\u{2003}");
        }
    }
    out
}

/// A running time as the activity panel words it: "for 12 min".
#[cfg(feature = "discord")]
fn running_for(started_ms: u64, now_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(started_ms) / 60_000;
    match minutes {
        0 => tr("just started").to_owned(),
        1..=59 => fill(tr("for {} min"), &[&minutes]),
        60..=1439 => fill(tr("for {} hr"), &[&(minutes / 60)]),
        _ => fill(tr("for {} d"), &[&(minutes / 1440)]),
    }
}

/// How long ago something finished as the activity panel words it, in
/// Discord's steps: "3h ago", "2d ago".
#[cfg(feature = "discord")]
pub(super) fn finished_ago(at_ms: u64, now_ms: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let seconds = now_ms.saturating_sub(at_ms) / 1000;
    match seconds {
        0..MINUTE => fill(tr("{}s ago"), &[&seconds]),
        MINUTE..HOUR => fill(tr("{}m ago"), &[&(seconds / MINUTE)]),
        _ if seconds < 12 * HOUR => fill(tr("{}h ago"), &[&(seconds / HOUR)]),
        _ if seconds < 9 * DAY => fill(tr("{}d ago"), &[&((seconds / DAY).max(1))]),
        _ if seconds < 28 * DAY => fill(tr("{}w ago"), &[&(seconds / (7 * DAY))]),
        _ => fill(tr("{}mo ago"), &[&((seconds / (30 * DAY)).max(1))]),
    }
}

/// What the channel's Activity panel shows: Discord's feed, in its order, of
/// those `state.activity_people` says it may show. Between two channels of a
/// server the last answer shows until this one's comes.
#[cfg(feature = "discord")]
pub(super) fn activity_panel(
    state: &yapfast_core::application::AppState,
    room: &str,
    members: &[yapfast_core::chat::ChatMember],
    now_ms: u64,
) -> Vec<MemberActivityRow> {
    use yapfast_core::{
        chat::{ActivityKind, FeedEntry, Presence},
        discord::parse_room,
        yapfast_discord::activity_feed::Kind,
    };
    let Some((asked, people)) = &state.activity_people else {
        return Vec::new();
    };
    let server = |room| parse_room(room).map(|(server, _)| server);
    if server(asked) != server(room) {
        return Vec::new();
    }
    // What their presence says of it, which is more than the feed does.
    let doing = |member: &yapfast_core::chat::ChatMember, entry: &FeedEntry| {
        member
            .activities
            .iter()
            .find(|activity| match entry.kind {
                Kind::Game | Kind::Launched => {
                    activity.kind == ActivityKind::Playing && activity.name == entry.title
                }
                Kind::Listened => activity.kind == ActivityKind::Listening,
                Kind::Watched => activity.kind == ActivityKind::Watching,
                Kind::TopGame | Kind::TopArtist => false,
            })
            .cloned()
    };
    state
        .activity_feed
        .iter()
        .filter_map(|entry| {
            let person = people.iter().find(|person| person.id == entry.author)?;
            let live = entry.live_at(now_ms);
            let member = live
                .then(|| members.iter().find(|member| *member.id == *entry.author))
                .flatten();
            // Discord's client leaves out what someone offline "is" doing.
            if member.is_some_and(|member| member.presence == Some(Presence::Offline)) {
                return None;
            }
            let activity = member.and_then(|member| doing(member, entry));
            let since = match (entry.hours, entry.tracks) {
                (Some(hours), _) if entry.kind == Kind::TopGame => {
                    fill(tr("Most played: {}h"), &[&hours])
                }
                (_, Some(tracks)) if entry.kind == Kind::TopArtist => {
                    fill(tr("{} tracks this week"), &[&tracks])
                }
                _ if live => running_for(
                    activity
                        .as_ref()
                        .and_then(|activity| activity.start)
                        .unwrap_or(entry.at_ms),
                    now_ms,
                ),
                _ => finished_ago(entry.at_ms, now_ms),
            };
            let picture = activity
                .as_ref()
                .and_then(|activity| activity.large_image.as_deref())
                .or(entry.picture.as_deref())
                .unwrap_or_default();
            Some(MemberActivityRow {
                id: (*person.id).into(),
                name: (*person.name).into(),
                initial: initial(&person.name),
                tint: tint(&person.name),
                picture_url: (*person.avatar).into(),
                icon: match entry.kind {
                    Kind::Game | Kind::Launched => 1,
                    Kind::Listened => 2,
                    Kind::Watched => 3,
                    Kind::TopGame | Kind::TopArtist => 5,
                },
                live,
                game: (*entry.title).into(),
                since: since.into(),
                thumb_url: yapfast_core::yapfast_discord::activity::small_picture(picture)
                    .as_ref()
                    .into(),
            })
        })
        .collect()
}

/// The icon, bullet and replacing words of the line under a name for what
/// someone is doing; see `yapfast_discord::activity::subline`.
#[cfg(feature = "discord")]
pub(super) fn subline(
    activities: &yapfast_core::chat::Activities,
    line: Option<&str>,
) -> (i32, bool, Option<String>) {
    let subline = yapfast_core::yapfast_discord::activity::subline(activities, line);
    (i32::from(subline.icon), subline.bullet, subline.name)
}

#[cfg(not(feature = "discord"))]
pub(super) fn subline(
    _activities: &yapfast_core::chat::Activities,
    _line: Option<&str>,
) -> (i32, bool, Option<String>) {
    (0, false, None)
}

#[cfg(not(feature = "discord"))]
pub(super) fn activity_panel(
    _state: &yapfast_core::application::AppState,
    _room: &str,
    _members: &[yapfast_core::chat::ChatMember],
    _now_ms: u64,
) -> Vec<MemberActivityRow> {
    Vec::new()
}

/// "1 mutual friend" or "5 mutual servers"; this account's own profile shares
/// its friends and servers with nobody, so there it says "1 friend" or
/// "5 servers".
fn mutual_count(count: usize, friends: bool, shared: bool) -> String {
    match (friends, shared) {
        (true, true) => trn("{n} mutual friend", "{n} mutual friends", count),
        (true, false) => trn("{n} friend", "{n} friends", count),
        (false, true) => trn("{n} mutual server", "{n} mutual servers", count),
        (false, false) => trn("{n} server", "{n} servers", count),
    }
}

/// Only mentions get a number; plain unread activity is a dot, because sync
/// does not deliver every message of a room that isn't open.
/// "Ann is", "Ann and Bob are", or "Ann, Bob and 3 others are".
/// What a profile or DM offers for a Discord friendship, as in
/// ProfileData.friend; no request to someone blocked, which Discord refuses.
pub(super) fn friend_offer(state: &AppState, user: &str) -> i32 {
    match state.relation(user) {
        Relation::Stranger if state.is_blocked(user) => 0,
        Relation::Stranger => 1,
        Relation::Outgoing => 2,
        Relation::Incoming => 3,
        Relation::Me | Relation::Friend | Relation::Bot => 0,
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
        (Some(state), Some((size, most))) => fill(tr("{} ({} of {})"), &[state, &size, &most]),
        (None, Some((size, most))) => fill(tr("{} of {}"), &[&size, &most]),
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
            after("on", &activity.large_text),
        )
    } else {
        (
            name.to_owned(),
            activity.details.as_deref().unwrap_or_default().to_owned(),
            state,
        )
    };
    ActivityRow {
        heading: activity.heading().to_uppercase().into(),
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

/// The profile to show in place of `shown`, keeping its lists where they're
/// the same; none when nothing changed.
pub(super) fn changed_profile(shown: &ProfileData, mut data: ProfileData) -> Option<ProfileData> {
    data.servers = same_list(&shown.servers, data.servers);
    data.friends = same_list(&shown.friends, data.friends);
    data.activities = same_list(&shown.activities, data.activities);
    data.effect_layers = same_list(&shown.effect_layers, data.effect_layers);
    data.badges = same_list(&shown.badges, data.badges);
    data.connections = same_list(&shown.connections, data.connections);
    data.recent = same_list(&shown.recent, data.recent);
    data.widgets = same_widget_rows(&shown.widgets, data.widgets);
    data.wishlist = same_list(&shown.wishlist, data.wishlist);
    data.role_rows = same_role_rows(&shown.role_rows, data.role_rows);
    data.role_rows_large = same_role_rows(&shown.role_rows_large, data.role_rows_large);
    (*shown != data).then_some(data)
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
        Wishlist::Unasked | Wishlist::Loading => {
            (ModelRc::default(), tr("Loading the wishlist..."))
        }
        Wishlist::Hidden => (ModelRc::default(), tr("This wishlist is private.")),
        Wishlist::Failed => (ModelRc::default(), tr("Couldn't load the wishlist.")),
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
                tr("Your wishlist is private: only you can see it.")
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

/// How long ago a time in Unix milliseconds was, as the Activity tab says it.
fn time_ago(at: u64) -> String {
    let minutes = yapfast_core::clock::now_ms().saturating_sub(at) / 60_000;
    match minutes {
        0 => tr("Just now").to_owned(),
        1..60 => fill(tr("{}m ago"), &[&minutes]),
        60..1440 => fill(tr("{}h ago"), &[&(minutes / 60)]),
        _ => fill(tr("{}d ago"), &[&(minutes / 1440)]),
    }
}

/// A profile effect's layers as the card plays them, in the order they lie.
fn effect_layers(effect: &yapfast_core::chat::ProfileEffect) -> Vec<EffectLayer> {
    let millis = |time: u32| i32::try_from(time).unwrap_or(i32::MAX);
    effect
        .layers
        .iter()
        .map(|layer| EffectLayer {
            url: layer.src.as_str().into(),
            x: layer.x,
            y: layer.y,
            start: millis(layer.start),
            duration: millis(layer.duration),
            looped: layer.looped,
            loop_delay: millis(layer.loop_delay),
        })
        .collect()
}

/// A profile as a card, the full profile or the DM panel shows it.
pub(super) fn profile_data(
    pictures: &mut HashMap<String, slint::Image>,
    state: &AppState,
    view: &ProfileView,
) -> ProfileData {
    let profile = &view.profile;
    // Emoji in the bio whose pictures are here become glyphs now, so the
    // bio below can show them.
    crate::custom_emoji::add(profile.bio_emoji.iter().filter_map(|emoji| {
        Some((
            emoji.id.as_str(),
            state.pictures.get(&emoji.picture)?.as_ref(),
        ))
    }));
    // The profile keeps the status's own line breaks. A line that only
    // names an activity goes, as its card says more.
    let (activity_emoji, activity) = profile
        .activity
        .as_deref()
        .filter(|line| !names_an_activity(line, &profile.activities))
        .map_or((None, ""), yapfast_core::discord::status_parts);
    let cosmetics = state.cosmetics;
    let effect = cosmetics.effect(profile.effect.as_deref()).and_then(|sku| {
        state
            .profile_effects
            .iter()
            .find(|effect| effect.sku == sku)
    });
    let avatar = cached_picture(pictures, state, profile.avatar.as_deref());
    let banner = cached_picture(pictures, state, profile.banner.as_deref());
    // Matrix IDs start with `@`; Discord user ids are numbers.
    let discord = !profile.id.is_empty() && !profile.id.starts_with('@');
    // This account's own profile lists all its friends and servers, not the
    // ones it shares with anyone, so its card names them without "mutual".
    let own = discord && state.relation(&profile.id) == Relation::Me;
    let (wishlist, wishlist_note) = wishlist_rows(profile);
    // With no banner, accent colour or theme, Discord tints the banner area
    // with the avatar's dominant colour.
    let accent = profile.accent.or_else(|| {
        (profile.banner.is_none() && profile.theme.is_none())
            .then(|| avatar_color(profile.avatar.as_deref(), &avatar))
            .flatten()
    });
    ProfileData {
        effect_layers: ModelRc::new(VecModel::from(
            effect.map_or_else(Vec::new, |effect| effect_layers(effect)),
        )),
        effect_still: effect
            .and_then(|effect| effect.still.as_deref())
            .unwrap_or_default()
            .into(),
        effect_end: effect.map_or(0, |effect| {
            effect
                .layers
                .iter()
                .map(|layer| layer.start + layer.duration)
                .max()
                .map_or(0, |end| i32::try_from(end).unwrap_or(i32::MAX))
        }),
        effect_loops: effect.is_some_and(|effect| effect.layers.iter().any(|layer| layer.looped)),
        theme_primary: role_color(profile.theme.map(|theme| theme[0])),
        theme_secondary: role_color(profile.theme.map(|theme| theme[1])),
        has_theme: profile.theme.is_some(),
        id: profile.id.clone().into(),
        name: profile.name.clone().into(),
        initial: initial(&profile.name),
        tint: tint(&profile.name),
        picture: avatar.clone(),
        decoration_url: cosmetics
            .decoration(profile.decoration.as_deref())
            .unwrap_or_default()
            .into(),
        name_style: super::name_style::row(
            cosmetics.style(profile.style.as_deref()),
            &profile.name,
        ),
        handle: profile.handle.as_str().into(),
        pronouns: profile.pronouns.as_str().into(),
        tag: tag_text(cosmetics.tag(profile.tag.as_ref())),
        tag_badge: tag_badge(cosmetics.tag(profile.tag.as_ref())),
        banner: banner.clone(),
        accent: role_color(accent),
        has_accent: accent.is_some(),
        presence: presence_name(profile.presence).into(),
        activity: activity.into(),
        activity_emoji: activity_emoji.unwrap_or_default().into(),
        activities: activity_rows(&profile.activities),
        badges: ModelRc::new(VecModel::from(
            profile
                .badges
                .iter()
                .map(|badge| ProfileBadgeRow {
                    icon: badge.icon.as_str().into(),
                    description: badge.description.as_str().into(),
                    link: badge.link.as_deref().unwrap_or_default().into(),
                    kind: badge
                        .level
                        .as_ref()
                        .and_then(|l| l.kind.as_deref())
                        .unwrap_or_default()
                        .into(),
                    title: badge
                        .level
                        .as_ref()
                        .map(|l| l.title.as_str())
                        .unwrap_or_default()
                        .into(),
                    progress: badge
                        .level
                        .as_ref()
                        .map(|l| l.progress.as_str())
                        .unwrap_or_default()
                        .into(),
                    picture: badge
                        .level
                        .as_ref()
                        .map(|l| l.picture.as_str())
                        .unwrap_or_default()
                        .into(),
                })
                .collect::<Vec<_>>(),
        )),
        bio: markdown(&bio_with_emoji(&profile.bio, &profile.bio_emoji)),
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
            tr("Loading the profile...").into()
        } else {
            view.note.clone().unwrap_or_default().into()
        },
        // Matrix IDs start with `@`; Discord user ids are numbers.
        friend: if profile.id.starts_with('@') {
            0
        } else {
            match state.relation(&profile.id) {
                Relation::Friend => 4,
                _ => friend_offer(state, &profile.id),
            }
        },
        connections: ModelRc::new(VecModel::from(
            profile
                .connections
                .iter()
                .map(|connection| ProfileConnectionRow {
                    kind: connection.kind.as_str().into(),
                    name: connection.name.as_str().into(),
                    url: connection.url.as_deref().unwrap_or_default().into(),
                    since: connection.since.as_deref().unwrap_or_default().into(),
                    verified: connection.verified,
                })
                .collect::<Vec<_>>(),
        )),
        widgets: widget_rows(&profile.widgets),
        board_note: if profile.board_asked && !profile.board_named {
            tr("Loading the board...").into()
        } else {
            Default::default()
        },
        wishlist,
        wishlist_note: wishlist_note.into(),
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
        // The client's list of tabs: the Board for one's own or with widgets,
        // and the Wishlist for one's own or a public one from a friend or
        // someone in a shared server.
        board_tab: discord && (own || !profile.widgets.is_empty()),
        wishlist_tab: discord
            && (own
                || (profile
                    .wishlist_place
                    .as_ref()
                    .is_some_and(|place| !place.private)
                    && (state.relation(&profile.id) == Relation::Friend
                        || !profile.servers.is_empty()))),
        own,
        mutual_friends_text: mutual_count(profile.friends.len(), true, !own).into(),
        mutual_servers_text: mutual_count(profile.servers.len(), false, !own).into(),
        note_text: profile.note.as_str().into(),
        note_editable: !profile.id.starts_with('@') && !own,
        can_message: state
            .person_choices(&profile.id)
            .contains(&MenuChoice::Message),
        // Discord's card names them as the profile does.
        message_hint: if state
            .person_choices(&profile.id)
            .contains(&MenuChoice::Message)
        {
            fill(tr("Message @{}"), &[&profile.name]).into()
        } else {
            Default::default()
        },
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

/// How many avatars' colours are kept before they're worked out afresh.
const AVATAR_COLORS: usize = 64;

thread_local! {
    /// [`dominant_color`] of the avatars profiles have shown, by address:
    /// `profile_data` runs on every refresh with a profile open.
    static AVATAR_COLOR: RefCell<HashMap<String, u32>> = RefCell::default();
}

/// [`dominant_color`] of the avatar at `url`, worked out once it has arrived.
pub(super) fn avatar_color(url: Option<&str>, avatar: &slint::Image) -> Option<u32> {
    let url = url?;
    AVATAR_COLOR.with_borrow_mut(|colors| {
        if let Some(color) = colors.get(url) {
            return Some(*color);
        }
        let color = dominant_color(avatar)?;
        if colors.len() >= AVATAR_COLORS {
            colors.clear();
        }
        colors.insert(url.to_owned(), color);
        Some(color)
    })
}

/// The colour that stands out most in a picture, as 0xRRGGBB: the average of
/// its opaque pixels, each counted by how saturated it is, so a vivid avatar
/// on a grey background gives its vivid colour. A grey picture gives its
/// plain average. None while the picture is empty or not a pixel buffer.
pub(super) fn dominant_color(picture: &slint::Image) -> Option<u32> {
    let pixels = picture.to_rgba8()?;
    // About 1000 samples are enough for an average.
    let step = (pixels.as_slice().len() / 1024).max(1);
    let (mut total, mut sum, mut count) = (0.0f64, [0.0f64; 3], 0.0f64);
    for pixel in pixels.as_slice().iter().step_by(step) {
        if pixel.a < 128 {
            continue;
        }
        let channels = [f64::from(pixel.r), f64::from(pixel.g), f64::from(pixel.b)];
        let high = channels.iter().copied().fold(0.0, f64::max);
        let low = channels.iter().copied().fold(255.0, f64::min);
        // Saturation, with a floor so that grey pictures still average.
        let weight = (high - low) / 255.0 + 0.02;
        for (at, channel) in channels.iter().enumerate() {
            sum[at] += channel * weight;
        }
        total += weight;
        count += 1.0;
    }
    if count == 0.0 {
        return None;
    }
    let mut color = 0u32;
    for sum in sum {
        color = (color << 8) | (sum / total).round().clamp(0.0, 255.0) as u32;
    }
    Some(color)
}

/// A server tag's letters for Slint, or nothing for one who wears none.
pub(super) fn tag_text(tag: Option<&yapfast_core::chat::ServerTag>) -> slint::SharedString {
    tag.map_or_else(Default::default, |tag| (&*tag.text).into())
}

/// The address of a server tag's badge for Slint, or nothing without one.
pub(super) fn tag_badge(tag: Option<&yapfast_core::chat::ServerTag>) -> slint::SharedString {
    tag.and_then(|tag| tag.badge.as_deref())
        .unwrap_or_default()
        .into()
}

/// A Discord role's RGB colour for Slint.
pub(super) fn role_color(rgb: Option<u32>) -> slint::Color {
    rgb.map_or_else(slint::Color::default, |rgb| {
        slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    })
}
