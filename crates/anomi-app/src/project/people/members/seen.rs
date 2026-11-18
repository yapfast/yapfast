//! The pictures of the member list's rows: those a row shows, the moving
//! ones its collectibles play under the pointer, and asking for those of
//! the rows in view.

use yapfast_core::application::AppState;
use yapfast_core::chat::ChatMember;

use super::{Shown, lines};
use crate::pictures::{self, Requests};

/// The pictures a member's row shows: avatar, decoration, nameplate,
/// role icon, tag badge and status emoji.
pub(super) fn addresses<'a>(
    state: &AppState,
    member: &'a ChatMember,
) -> impl Iterator<Item = &'a str> + 'a {
    let cosmetics = state.cosmetics;
    let tag = cosmetics
        .tag(member.tag.as_ref())
        .and_then(|tag| tag.badge.as_deref());
    let plate = member
        .nameplate
        .as_ref()
        .filter(|_| !state.hide_nameplates)
        .map(|plate| &*plate.picture);
    [
        member.avatar.as_deref(),
        cosmetics.decoration(member.decoration.as_deref()),
        member.role_icon.as_deref(),
        tag,
        plate,
    ]
    .into_iter()
    .flatten()
}

/// The moving pictures of a member's decoration and nameplate, which play
/// under the pointer; none while the profile switches hold them still.
pub(super) fn moving(state: &AppState, member: &ChatMember) -> Vec<String> {
    let plate = member
        .nameplate
        .as_ref()
        .filter(|_| !state.hide_nameplates)
        .map(|plate| &*plate.picture);
    [
        state.cosmetics.decoration(member.decoration.as_deref()),
        plate,
    ]
    .into_iter()
    .flatten()
    .map(pictures::animated_url)
    .filter(|moving| !moving.is_empty())
    .collect()
}

/// The rows in view changed: their pictures are asked for.
pub(crate) fn seen(
    state: &AppState,
    shown: &mut Shown,
    first: usize,
    last: usize,
    requests: &mut Requests,
) {
    shown.view = (first, last);
    seen_now(state, shown, requests);
}

/// Asks for the pictures of the rows in view.
pub(super) fn seen_now(state: &AppState, shown: &Shown, requests: &mut Requests) {
    let Some(source) = &shown.source else {
        return;
    };
    let (first, last) = shown.view;
    let last = last
        .max(first + 20)
        .min(shown.places.len().saturating_sub(1));
    let in_view = shown.places.get(first..=last).unwrap_or_default();
    let mut emoji = Vec::new();
    for at in in_view.iter().flatten() {
        let member = &source[*at];
        requests.want(state, addresses(state, member));
        emoji.extend(lines::line_emoji(member.activity.as_deref()));
        requests.want(state, member.group_icon.as_deref());
    }
    requests.want(state, emoji.iter().map(String::as_str));
}
