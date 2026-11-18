//! The member list from the core's members (`show_members` and
//! `show_roster`, `src/ui/refresh.rs:2429`, `:2507`).
//!
//! The busiest list of the client: in a large server presence and activity
//! change several times a second, and each change comes as the whole list
//! again. A member whose `ChatMember` didn't change keeps the row made
//! before (an `Arc`, found at the same place or by id), so a change builds
//! the one row it concerns and moves it between groups; the headings, a
//! few, are made again. Rows ask for their pictures only while in view, and
//! a picture coming builds again only the rows that show it. A hidden list
//! builds nothing and keeps nothing.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::{AppState, Application, Cosmetics};
use yapfast_core::chat::{ChatMember, Role};
use yapfast_lang::tr;

use anomi_vm::kind::Nameplate;
use anomi_vm::people::members::{Heading, Member, Members, Row};

use super::lines;
use crate::layout::Layout;
use crate::pictures::{self, Requests, Watch};
use crate::project::rows;

mod seen;

pub(super) use seen::seen;
use seen::{addresses, moving, seen_now};

/// What the rows were made from, and what's in view.
#[derive(Default)]
pub(crate) struct Shown {
    /// The core's list the rows were made of, and what else they read.
    source: Option<Arc<Vec<ChatMember>>>,
    look: Option<Look>,
    /// The row made for each member of `source`, in its order.
    made: Vec<Arc<Member>>,
    /// For each row of the list, the member of `source` it shows; `None`
    /// for a heading.
    places: Vec<Option<usize>>,
    /// The pictures the rows show, by [`key`] of each member's id; the
    /// headings' under [`HEADINGS`].
    pictures: Watch<u64>,
    /// The rows in view, first and last, by place in the list.
    view: (usize, usize),
    /// When the soonest timeout of a member ends, in Unix milliseconds.
    timeout_ends: Option<u64>,
    /// The member under the pointer, whose collectibles play, and the one
    /// the rows were made with.
    pub(super) pointed: Option<String>,
    pointed_built: Option<String>,
}

/// The key the headings' pictures are noted under.
const HEADINGS: u64 = 0;

/// What every row reads beyond its member.
#[derive(Clone, PartialEq)]
struct Look {
    room: Option<String>,
    cosmetics: Cosmetics,
    hide_nameplates: bool,
    owner: Option<String>,
    in_server: bool,
}

impl Look {
    fn of(core: &Application) -> Self {
        let state = &core.state;
        let room = state.selected.clone();
        Self {
            owner: room.as_deref().and_then(|room| core.server_owner(room)),
            in_server: lines::in_server(room.as_deref()),
            room,
            cosmetics: state.cosmetics,
            hide_nameplates: state.hide_nameplates,
        }
    }
}

/// A member's id as the picture watch keys their row.
fn key(id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish() | 1
}

/// Shows the member list while it's `wanted` beside the conversation.
pub(super) fn show(
    core: &Application,
    layout: &Layout,
    wanted: bool,
    members: &mut Members,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    let state = &core.state;
    if !wanted {
        if shown.source.is_some() || !members.rows.is_empty() || !members.note.is_empty() {
            *members = Members::default();
            *shown = Shown::default();
        }
        return;
    }
    members.gems = layout.boost_gems.unwrap_or(true);
    let selected = state.profile.as_ref().filter(|view| !view.large);
    let selected = selected.map_or("", |view| view.profile.id.as_str());
    if members.selected != selected {
        members.selected = selected.to_owned();
    }
    let note = match &state.members {
        None => tr!("Loading members..."),
        Some(Err(error)) => error,
        Some(Ok(list)) if list.is_empty() => tr!("No one else has joined yet."),
        Some(Ok(_)) => "",
    };
    if members.note != note {
        members.note = note.to_owned();
    }
    let room = state.selected_room();
    members.group = room.is_some_and(|room| room.direct);
    let list = match &state.members {
        Some(Ok(list)) => Some(list),
        _ => None,
    };
    let look = Look::of(core);
    let mut moved: HashSet<u64> = shown.pictures.moved(state).into_iter().collect();
    if shown.pointed != shown.pointed_built {
        let both = [&shown.pointed, &shown.pointed_built];
        moved.extend(both.into_iter().flatten().map(|id| key(id)));
        if let Some(member) =
            (shown.pointed.as_deref()).and_then(|id| list?.iter().find(|member| member.id == id))
        {
            requests.want(state, moving(state, member).iter().map(String::as_str));
        }
    }
    let now = yapfast_core::clock::now_ms();
    let expired = shown.timeout_ends.is_some_and(|ends| ends <= now);
    let same = match (list, &shown.source) {
        (Some(new), Some(old)) => Arc::ptr_eq(new, old),
        (None, None) => true,
        _ => false,
    };
    if same && shown.look.as_ref() == Some(&look) && moved.is_empty() && !expired {
        return;
    }
    build(state, &look, list, &moved, expired, members, shown);
    shown.pointed_built.clone_from(&shown.pointed);
    shown.look = Some(look);
    seen_now(state, shown, requests);
}

/// Makes the rows again, keeping each unchanged member's.
fn build(
    state: &AppState,
    look: &Look,
    list: Option<&Arc<Vec<ChatMember>>>,
    moved: &HashSet<u64>,
    expired: bool,
    members: &mut Members,
    shown: &mut Shown,
) {
    let now = yapfast_core::clock::now_ms();
    let reuse = shown.look.as_ref() == Some(look);
    let old_source = shown.source.take();
    let old: &[ChatMember] = old_source.as_deref().map_or(&[], Vec::as_slice);
    let old_made = std::mem::take(&mut shown.made);
    let new: &[ChatMember] = list.map_or(&[], |list| list.as_slice());
    let mut by_id: Option<HashMap<&str, usize>> = None;
    let mut made = Vec::with_capacity(new.len());
    let mut rows = Vec::with_capacity(new.len() + 8);
    let mut places = Vec::with_capacity(new.len() + 8);
    for (index, member) in new.iter().enumerate() {
        if let Some(heading) = heading(state, new, index) {
            shown.pictures.note(
                state,
                member.group_icon.as_deref().unwrap_or_default(),
                HEADINGS,
            );
            rows.push(Row::Heading(heading));
            places.push(None);
        }
        let id = key(&member.id);
        let timed = expired && member.timed_out_until.is_some();
        let found = (reuse && !timed && !moved.contains(&id))
            .then(|| {
                if old.get(index).is_some_and(|old| same(old, member)) {
                    return Some(index);
                }
                let by_id = by_id.get_or_insert_with(|| {
                    old.iter()
                        .enumerate()
                        .map(|(at, member)| (member.id.as_str(), at))
                        .collect()
                });
                by_id
                    .get(member.id.as_str())
                    .copied()
                    .filter(|&at| same(&old[at], member))
            })
            .flatten()
            .and_then(|at| old_made.get(at).cloned());
        let pointed = shown.pointed.as_deref() == Some(member.id.as_str());
        let row = found.unwrap_or_else(|| {
            for address in addresses(state, member) {
                shown.pictures.note(state, address, id);
            }
            let moving = if pointed {
                moving(state, member)
            } else {
                Vec::new()
            };
            for address in &moving {
                shown.pictures.note(state, address, id);
            }
            if let Some(emoji) = lines::line_emoji(member.activity.as_deref()) {
                shown.pictures.note(state, &emoji, id);
            }
            Arc::new(row(state, look, member, now, pointed))
        });
        rows.push(Row::Member(row.clone()));
        places.push(Some(index));
        made.push(row);
    }
    let ids: HashSet<u64> = new.iter().map(|member| key(&member.id)).collect();
    shown
        .pictures
        .retain(|row| row == HEADINGS || ids.contains(&row));
    shown.timeout_ends = new
        .iter()
        .filter_map(|member| member.timed_out_until)
        .filter(|&until| until > now)
        .min();
    shown.made = made;
    shown.places = places;
    shown.source = list.cloned();
    members.rows = rows;
}

/// Whether a member's row shows the same: all but their group, whose
/// heading, with its count, is a row of its own. Discord's groups name
/// their size, so a presence change renames every member's group.
fn same(old: &ChatMember, new: &ChatMember) -> bool {
    old.id == new.id
        && old.name == new.name
        && old.presence == new.presence
        && old.activity == new.activity
        && old.activities == new.activities
        && old.avatar == new.avatar
        && old.decoration == new.decoration
        && old.nameplate == new.nameplate
        && old.style == new.style
        && old.color == new.color
        && old.role_icon == new.role_icon
        && old.tag == new.tag
        && old.bot == new.bot
        && old.role == new.role
        && old.timed_out_until == new.timed_out_until
        && old.boosting_since == new.boosting_since
}

/// The heading that starts a group at `index`: Discord names its groups,
/// with their size; Matrix members group by role, counted here.
fn heading(state: &AppState, members: &[ChatMember], index: usize) -> Option<Heading> {
    let member = &members[index];
    let before = index.checked_sub(1).map(|at| &members[at]);
    let text = match &member.group {
        Some(group) if before.is_none_or(|before| before.group.as_ref() != Some(group)) => {
            group.clone()
        }
        Some(_) => return None,
        None if before.is_none_or(|before| before.role != member.role) => {
            let count = members
                .iter()
                .filter(|other| other.role == member.role)
                .count();
            match member.role {
                Role::Admin => tr!("ADMINS — {}", count),
                Role::Moderator => tr!("MODERATORS — {}", count),
                Role::Member => tr!("MEMBERS — {}", count),
            }
        }
        None => return None,
    };
    Some(Heading {
        text,
        icon: rows::expected(state, member.group_icon.as_deref()),
    })
}

/// A member's row (`MemberRow`).
fn row(state: &AppState, look: &Look, member: &ChatMember, now: u64, pointed: bool) -> Member {
    let cosmetics = look.cosmetics;
    // Under the pointer, the moving picture once it came, else the still.
    let shown = |still: Option<&str>| {
        let moving = still
            .filter(|_| pointed)
            .map(pictures::animated_url)
            .filter(|moving| !moving.is_empty());
        moving
            .and_then(|moving| pictures::find(state, Some(&moving)))
            .or_else(|| pictures::find(state, still))
    };
    let (status, bullet) = lines::subline(state, &member.activities, member.activity.as_deref());
    let plate = member
        .nameplate
        .as_ref()
        .filter(|_| !look.hide_nameplates)
        .map(|plate| Nameplate {
            picture: shown(Some(&plate.picture)),
            palette: plate
                .colors
                .map(|[dark, light]| [rows::rgb(dark), rows::rgb(light)]),
        })
        .unwrap_or_default();
    Member {
        id: member.id.clone(),
        name: member.name.clone(),
        avatar: rows::avatar(state, &member.name, &member.name, member.avatar.as_deref()),
        decoration: shown(cosmetics.decoration(member.decoration.as_deref())),
        presence: rows::presence(member.presence),
        color: member.color.map(rows::rgb),
        name_style: lines::name_style(state, member.style.as_deref(), look.in_server),
        plate,
        status,
        bullet,
        tag: lines::tag(state, member.tag.as_ref()),
        role_icon: rows::expected(state, member.role_icon.as_deref()),
        bot: member.bot,
        owner: look.owner.as_deref() == Some(member.id.as_str()),
        boosting: member
            .boosting_since
            .map(|since| {
                tr!(
                    "Server boosting since {}",
                    yapfast_core::clock::day(since / 1000)
                )
            })
            .unwrap_or_default(),
        timed_out: member.timed_out_until.is_some_and(|until| until > now),
    }
}
