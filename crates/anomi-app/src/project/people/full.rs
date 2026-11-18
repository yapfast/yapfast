//! The full profile's tabs: Activity, mutual friends and servers, the
//! Board and the Wishlist (`profile_data`, `src/ui/profile.rs:588`, its
//! lists). Its left column is the card [`super::card::card`] makes.

use yapfast_core::application::{AppState, ProfileView, Relation};
use yapfast_core::chat::{ProfileWidget, Wishlist, WishlistPlace};
use yapfast_lang::{tr, trn};

use anomi_vm::people::full_profile::{Friend, FullProfile, Game, Recent, Server, Widget, Wish};
use anomi_vm::people::profile::Card;

use super::card::presence_or_offline;
use crate::project::rows;

/// The full profile of `view`, around its `card`; `chosen` is the tab the
/// person chose, which stays while the same profile shows.
pub fn full(
    state: &AppState,
    view: &ProfileView,
    card: Card,
    chosen: Option<anomi_vm::people::full_profile::Page>,
) -> FullProfile {
    let profile = &view.profile;
    let discord = !profile.id.is_empty() && !profile.id.starts_with('@');
    let relation = state.relation(&profile.id);
    let own = discord && relation == Relation::Me;
    // Discord has said who's mutual, so an empty list means none: not
    // while it loads, nor when it failed.
    let mutuals_known = view.kept || (!view.loading && view.note.is_none());
    let (wishlist, wishlist_note) =
        wishlist(state, profile.wishlist_place.as_ref(), &profile.wishlist);
    let public_wishlist = profile
        .wishlist_place
        .as_ref()
        .is_some_and(|place| !place.private);
    FullProfile {
        card,
        chosen,
        board_tab: discord && (own || !profile.widgets.is_empty()),
        wishlist_tab: discord
            && (own
                || (public_wishlist
                    && (relation == Relation::Friend || !profile.servers.is_empty()))),
        friends_tab: discord && !own && relation != Relation::Bot,
        servers_tab: discord && !own,
        friends_label: label(mutuals_known, profile.friends.len(), true),
        servers_label: label(mutuals_known, profile.servers.len(), false),
        mutuals_known,
        friends: profile
            .friends
            .iter()
            .map(|friend| Friend {
                id: friend.id.clone(),
                name: friend.name.clone(),
                avatar: rows::avatar(state, &friend.name, &friend.id, friend.avatar.as_deref()),
                presence: presence_or_offline(
                    state
                        .friends
                        .iter()
                        .find(|known| known.id == friend.id)
                        .and_then(|known| known.presence),
                ),
            })
            .collect(),
        servers: profile
            .servers
            .iter()
            .map(|server| Server {
                id: server.id.clone(),
                name: server.name.clone(),
                initial: rows::initial(&server.name),
                icon: rows::expected(state, server.icon.as_deref()),
                nick: server.nick.clone().unwrap_or_default(),
            })
            .collect(),
        // Discord's answer to what they did lately has come, or there is
        // none to wait for.
        recent_known: !discord || state.preview || profile.recent_came,
        recent: profile
            .recent
            .iter()
            .map(|entry| Recent {
                title: entry.title.clone(),
                subtitle: entry.subtitle.clone(),
                initial: rows::initial(&entry.title),
                picture: rows::expected(state, entry.picture.as_deref()),
                music: entry.music,
                when: entry.at.map(time_ago).unwrap_or_default(),
            })
            .collect(),
        widgets: widgets(state, &profile.widgets),
        board_note: if profile.board_asked && !profile.board_named {
            tr!("Loading the board...").to_owned()
        } else {
            String::new()
        },
        wishlist,
        wishlist_note: wishlist_note.to_owned(),
    }
}

/// A mutual tab's name, as the client's: none till Discord has said, then
/// how many.
fn label(known: bool, count: usize, friends: bool) -> String {
    match (known, count, friends) {
        (false, _, true) => tr!("Mutual Friends").to_owned(),
        (false, _, false) => tr!("Mutual Servers").to_owned(),
        (true, 0, true) => tr!("No Mutual Friends").to_owned(),
        (true, 0, false) => tr!("No Mutual Servers").to_owned(),
        (true, count, true) => trn!("{n} Mutual Friend", "{n} Mutual Friends", count),
        (true, count, false) => trn!("{n} Mutual Server", "{n} Mutual Servers", count),
    }
}

/// The board's widgets, a card each, as its tab shows them.
pub(crate) fn widgets(state: &AppState, widgets: &[ProfileWidget]) -> Vec<Widget> {
    widgets
        .iter()
        .map(|widget| Widget {
            title: widget.title.clone(),
            elsewhere: !widget.lists_games,
            games: widget
                .games
                .iter()
                .map(|game| Game {
                    name: game.name.clone(),
                    initial: rows::initial(&game.name),
                    icon: rows::expected(state, game.icon.as_deref()),
                    comment: game.comment.clone().unwrap_or_default(),
                })
                .collect(),
        })
        .collect()
}

/// A Discord wishlist as its tab shows it: its items, and a line about it.
pub(crate) fn wishlist(
    state: &AppState,
    place: Option<&WishlistPlace>,
    wishlist: &Wishlist,
) -> (Vec<Wish>, &'static str) {
    let private = place.is_some_and(|place| place.private);
    match wishlist {
        Wishlist::Unasked | Wishlist::Loading => (Vec::new(), tr!("Loading the wishlist...")),
        Wishlist::Hidden => (Vec::new(), tr!("This wishlist is private.")),
        Wishlist::Failed => (Vec::new(), tr!("Couldn't load the wishlist.")),
        Wishlist::Loaded(items) => {
            let wishes: Vec<Wish> = items
                .iter()
                .map(|item| Wish {
                    name: item.name.clone(),
                    initial: rows::initial(&item.name),
                    picture: rows::expected(state, item.picture.as_deref()),
                })
                .collect();
            let note = if private && !wishes.is_empty() {
                tr!("Your wishlist is private: only you can see it.")
            } else {
                ""
            };
            (wishes, note)
        }
    }
}

/// How long ago a time in Unix milliseconds was, as the Activity tab says.
fn time_ago(at: u64) -> String {
    let minutes = yapfast_core::clock::now_ms().saturating_sub(at) / 60_000;
    match minutes {
        0 => tr!("Just now").to_owned(),
        1..60 => tr!("{}m ago", minutes),
        60..1440 => tr!("{}h ago", minutes / 60),
        _ => tr!("{}d ago", minutes / 1440),
    }
}
