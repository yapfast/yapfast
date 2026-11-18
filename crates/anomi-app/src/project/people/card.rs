//! A profile as its card draws it: the core's `ProfileView` put into the
//! card's words, colours and pictures (`profile_data`,
//! `src/ui/profile.rs:588`). The popout, the panel beside a DM and the full
//! profile's side all make theirs here.

use yapfast_core::application::{AppState, Application, MenuChoice, ProfileView, Relation};
use yapfast_core::chat::{Presence, Profile as CoreProfile};
use yapfast_core::discord;
use yapfast_lang::tr;

use anomi_vm::kind::{Effect, EffectLayer, FrameAnchor, FrameKind, FrameLayer, Status};
use anomi_vm::people::profile::{
    Card, Connection, Friendship, Joined, Mutual, Role, Theme, connection_icon,
};
use anomi_vm::text::{self, CustomEmoji, Source};

use super::lines;
use crate::pictures;
use crate::project::account::profile::{accent, badges};
use crate::project::rows;

/// Someone's card from what the core shows of them.
pub fn card(core: &Application, view: &ProfileView) -> Card {
    let state = &core.state;
    let profile = &view.profile;
    let cosmetics = state.cosmetics;
    // Matrix IDs start with `@`; Discord user ids are numbers.
    let discord = !profile.id.is_empty() && !profile.id.starts_with('@');
    let relation = state.relation(&profile.id);
    let own = discord && relation == Relation::Me;
    let menu = core.person_choices(&profile.id);
    let messages = menu.contains(&MenuChoice::Message);
    Card {
        id: profile.id.clone(),
        name: profile.name.clone(),
        avatar: rows::avatar(
            state,
            &profile.name,
            &profile.name,
            profile.avatar.as_deref(),
        ),
        decoration: pictures::find(state, cosmetics.decoration(profile.decoration.as_deref())),
        name_style: lines::name_style(state, profile.style.as_deref(), false),
        presence: rows::presence(profile.presence),
        handle: profile.handle.clone(),
        pronouns: profile.pronouns.clone(),
        tag: lines::tag(state, profile.tag.as_ref()),
        banner: rows::expected(state, profile.banner.as_deref()),
        accent: accent(state, profile).map(rows::rgb),
        theme: profile
            .theme
            .map(|[primary, secondary]| Theme::new([rows::rgb(primary), rows::rgb(secondary)])),
        effect: effect(state, profile),
        frame: frame(state, profile),
        thought: Some(thought(state, profile, own)),
        // One's own shows what one's sessions say, as Discord's client
        // does: no presence comes for oneself.
        activities: lines::cards(
            state,
            if profile.activities.is_empty() && own {
                &state.discord_activities
            } else {
                &profile.activities
            },
        ),
        now: lines::now(),
        badges: badges(state, &profile.badges),
        bio: (!profile.bio.trim().is_empty()).then(|| bio(state, profile)),
        roles: profile
            .roles
            .iter()
            .map(|role| Role {
                name: role.name.clone(),
                color: role.color.map(rows::rgb),
            })
            .collect(),
        created: profile.created.clone().unwrap_or_default(),
        joined: profile.joined.as_ref().map(|date| Joined {
            date: date.clone(),
            server: profile.server_name.clone(),
            initial: rows::initial(&profile.server_name),
            icon: rows::expected(state, profile.server_icon.as_deref()),
        }),
        friends_since: if own {
            String::new()
        } else {
            friends_since(state, &profile.id)
        },
        mutual: Mutual {
            faces: profile
                .friends
                .iter()
                .take(3)
                .map(|friend| {
                    rows::avatar(state, &friend.name, &friend.id, friend.avatar.as_deref())
                })
                .collect(),
            friends: profile.friends.len(),
            servers: profile.servers.len(),
        },
        connections: profile
            .connections
            .iter()
            .map(|connection| Connection {
                kind: connection.kind.clone(),
                icon: connection_icon(&connection.kind),
                name: connection.name.clone(),
                url: connection.url.clone().unwrap_or_default(),
            })
            .collect(),
        note: (discord && !own).then(|| profile.note.clone()),
        remark: if view.loading && !view.kept {
            tr!("Loading the profile...").to_owned()
        } else {
            view.note.clone().unwrap_or_default()
        },
        friend: if !discord {
            Friendship::None
        } else {
            match relation {
                Relation::Friend => Friendship::Friends,
                Relation::Stranger if state.is_blocked(&profile.id) => Friendship::None,
                Relation::Stranger => Friendship::Offer,
                Relation::Outgoing => Friendship::Sent,
                Relation::Incoming => Friendship::Theirs,
                Relation::Me | Relation::Bot => Friendship::None,
            }
        },
        // In their DM too, whose menu has no Message, as Discord's full
        // profile offers it there.
        can_message: messages
            || state
                .one_to_one()
                .is_some_and(|room| room.person.as_deref() == Some(profile.id.as_str())),
        message_hint: if messages {
            tr!("Message @{}", profile.name)
        } else {
            String::new()
        },
        own,
        bot: relation == Relation::Bot,
        ..Card::default()
    }
}

/// The status in the bubble: one's own custom status as this account set
/// it, whatever the member list says; anyone else's line, unless it only
/// names an activity, whose card says more. It keeps its own line breaks.
fn thought(state: &AppState, profile: &CoreProfile, own: bool) -> Status {
    let (emoji, words) = if own {
        match &state.custom_status {
            Some(status) => {
                let text = status.text.trim();
                match (&status.emoji, &status.emoji_id) {
                    (_, Some(id)) => (discord::emoji_picture(id), text.to_owned()),
                    (Some(emoji), None) if text.is_empty() => (None, emoji.clone()),
                    (Some(emoji), None) => (None, format!("{emoji} {text}")),
                    (None, None) => (None, text.to_owned()),
                }
            }
            None => (None, String::new()),
        }
    } else {
        let (emoji, words) = profile
            .activity
            .as_deref()
            .filter(|line| !lines::names_an_activity(line, &profile.activities))
            .map_or((None, ""), discord::status_parts);
        (emoji, words.to_owned())
    };
    Status {
        text: words,
        emoji: rows::expected(state, emoji.as_deref()),
        ..Status::default()
    }
}

/// The bio, its custom emoji as their pictures once they came.
fn bio(state: &AppState, profile: &CoreProfile) -> anomi_vm::text::Text {
    let emoji: Vec<CustomEmoji> = profile
        .bio_emoji
        .iter()
        .map(|emoji| CustomEmoji {
            text: emoji.text.clone(),
            id: emoji.id.clone(),
            still: pictures::find(state, Some(&emoji.picture)),
        })
        .collect();
    text::markdown(Source {
        emoji: &emoji,
        ..Source::new(&profile.bio)
    })
}

/// When they became friends, from the friend list.
fn friends_since(state: &AppState, user: &str) -> String {
    state
        .friends
        .iter()
        .find(|friend| friend.id == user)
        .and_then(|friend| friend.since)
        .map(|ms| yapfast_core::clock::profile_day(ms / 1000))
        .unwrap_or_default()
}

/// The profile effect they equip, while effects play. Its pictures are
/// looked up without being asked for: the card asks for them as it is
/// about to play it (`EffectWanted`).
fn effect(state: &AppState, profile: &CoreProfile) -> Option<Effect> {
    let effect = effect_of(state, profile)?;
    let find = |address: &str| state.pictures.get(address).and_then(pictures::shown);
    Some(Effect {
        still: effect.still.as_deref().and_then(find),
        layers: effect
            .layers
            .iter()
            .map(|layer| EffectLayer {
                picture: find(&layer.src),
                x: layer.x as f32,
                y: layer.y as f32,
                start: layer.start,
                duration: layer.duration,
                looped: layer.looped,
                loop_delay: layer.loop_delay,
            })
            .collect(),
    })
}

/// The core's effect a profile equips, once it's described.
pub fn effect_of<'a>(
    state: &'a AppState,
    profile: &CoreProfile,
) -> Option<&'a yapfast_core::chat::ProfileEffect> {
    let sku = state.cosmetics.effect(profile.effect.as_deref())?;
    state
        .profile_effects
        .iter()
        .find(|effect| effect.sku == sku)
        .map(AsRef::as_ref)
}

/// The frame's layers, the back ones first, while frames show.
fn frame(state: &AppState, profile: &CoreProfile) -> Vec<FrameLayer> {
    let Some(frame) = state.cosmetics.frame(Some(&profile.frame)) else {
        return Vec::new();
    };
    frame
        .iter()
        .filter_map(|layer| {
            Some(FrameLayer {
                picture: pictures::find(state, Some(&layer.src)),
                kind: FrameKind::from_name(&layer.kind)?,
                anchor: FrameAnchor::from_name(&layer.anchor),
                front: layer.front,
            })
        })
        .collect()
}

/// The panel beside a DM: the DM list keeps up with whether they're around
/// and what they do; the profile was as it was when it opened.
pub fn follow_room(state: &AppState, card: &mut Card, view: &ProfileView) {
    let Some(room) = state.selected_room() else {
        return;
    };
    card.presence = rows::presence(room.presence.or(view.profile.presence));
    if let Some(activity) = &room.activity {
        let (emoji, words) = if lines::names_an_activity(activity, &room.activities) {
            (None, "")
        } else {
            discord::status_parts(activity)
        };
        card.thought = Some(Status {
            text: words.to_owned(),
            emoji: rows::expected(state, emoji.as_deref()),
            ..Status::default()
        });
    }
    card.activities = lines::cards(state, &room.activities);
}

/// Offline, for a presence the profile doesn't say.
pub fn presence_or_offline(presence: Option<Presence>) -> anomi_vm::kind::Presence {
    rows::presence(Some(presence.unwrap_or(Presence::Offline)))
}
