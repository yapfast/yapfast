// Copyright 2026 Element Creations Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE in the repository root for full details.

//! Call tiles: the roster as a UI draws it.
//!
//! One [`CallTile`] per renderable stream — a member sharing their screen is
//! two tiles — ranked so that a UI rendering the list in order is correct.
//! Derived from [`Participant`]s and pure: no engine state, no clock. Both
//! speaking sets are inputs — the raw one a tile reports, the damped one the
//! order is ranked on; hysteresis lives in the engine.

use std::cmp::Reverse;
use std::collections::HashSet;

use matrix_rtc_transport::{ConnectionState, MediaStreamKind};

use crate::participant::{Participant, StreamState, UnableToDecrypt};

/// What a tile is: a person, or a screen they are sharing.
///
/// Not a [`MediaStreamKind`]. A person tile draws the member's camera and
/// carries their microphone state; a share tile draws the screen. Which
/// stream a tile draws is [`TileKind::video_stream`], so a consumer never has
/// to guess it — and a microphone can never be mistaken for a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileKind {
    Person,
    ScreenShare,
}

impl TileKind {
    /// The stream this kind of tile draws.
    pub fn video_stream(self) -> MediaStreamKind {
        match self {
            TileKind::Person => MediaStreamKind::Camera,
            TileKind::ScreenShare => MediaStreamKind::ScreenShare,
        }
    }
}

/// Identity of one tile: the pair `(member_id, kind)`.
///
/// Stable for as long as the tile is in the call — neither component changes
/// when the member mutes, raises a hand, or starts or stops a share.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileId {
    pub member_id: String,
    pub kind: TileKind,
}

/// One renderable stream of one membership, with what a UI needs to place and
/// decorate it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallTile {
    pub member_id: String,
    pub kind: TileKind,
    pub user_id: String,
    pub device_id: Option<String>,
    /// Sorts above everything else. A screen share; a pin, once pinning exists.
    pub hero: bool,
    /// This tile's own stream is present and unmuted.
    pub has_video: bool,
    /// The member's microphone is absent or muted — what a mute icon means.
    /// Named for its subject because a tile is itself a stream that can be
    /// muted; that state is `has_video`.
    pub microphone_muted: bool,
    /// Speaking now, as the transport reports it: what a speaking ring means.
    /// Not damped — the hysteresis applies to where the tile ranks (R10), not
    /// to this flag (R11), so a tile can be speaking and not move.
    pub speaking: bool,
    pub hand_raised_at_ms: Option<u64>,
    /// See [`Participant::hand_raised_position`].
    pub hand_raised_position: Option<u32>,
    pub joined_at_ms: Option<u64>,
    /// See [`Participant::connection`]. Neither it nor `unable_to_decrypt`
    /// moves `has_video` or the order: a host draws the avatar over a remote
    /// tile that is not `Connected` or is `unable_to_decrypt` (contract C6).
    pub connection: ConnectionState,
    /// See [`Participant::unable_to_decrypt`].
    pub unable_to_decrypt: Option<UnableToDecrypt>,
}

impl CallTile {
    pub fn id(&self) -> TileId {
        TileId {
            member_id: self.member_id.clone(),
            kind: self.kind,
        }
    }
}

/// The derived tile set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tiles {
    /// Everyone else, in rank order.
    pub remote: Vec<CallTile>,
    /// Our own camera tile, beside the list and never in it. `None` until our
    /// own membership is on the roster.
    pub own: Option<CallTile>,
}

/// A tile's place in the order: enough to place it — identity and hero —
/// and to draw it as an avatar with a name — `user_id` — and nothing about
/// what the member is doing. One per tile, always, so a tile outside the
/// detail window is still drawable (contract C1, C2, C12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRef {
    pub id: TileId,
    pub user_id: String,
    pub hero: bool,
}

/// What consumers are given: the complete order, and full records for the
/// declared window only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TileRoster {
    /// Every remote tile, in rank order. Never truncated: the complement and
    /// the total are computed from it.
    pub order: Vec<TileRef>,
    /// Full records for the tiles inside the window, in `order`'s order. Join
    /// to `order` by [`TileId`], never by index — it is shorter.
    pub detail: Vec<CallTile>,
}

/// Our own tile and whether we are sharing our screen, beside the roster
/// rather than in it. Changes when we act, not when the call moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalState {
    pub tile: CallTile,
    /// A screen-share publication of ours is up **and unmuted**. Publication
    /// state, not intent: it goes false however the share ended.
    pub is_screen_sharing: bool,
}

/// Which tiles get full records: a rank range, plus identities drawn out of
/// rank order — a tile shown full-screen, a picture-in-picture source.
///
/// The default is everything, so a consumer that never declares a window
/// sees full records for every tile. Windowing is opt-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailWindow {
    pub offset: u32,
    pub len: u32,
    pub also: HashSet<TileId>,
}

impl Default for DetailWindow {
    fn default() -> Self {
        Self {
            offset: 0,
            len: u32::MAX,
            also: HashSet::new(),
        }
    }
}

/// Derives ranked tiles from the roster.
///
/// `speaking` holds the `member_id`s speaking now, and is what each tile's
/// [`CallTile::speaking`] reports. `ranked_speaking` holds the ones counted as
/// speaking for the order, after whatever damping the caller applies (R10).
/// Passing one set for both ranks on the raw signal.
///
/// With `ranking_threshold` or fewer remote tiles, the order ignores what
/// members are doing — hand, speaking, video — so a small call never
/// shuffles; 0 always ranks.
pub fn derive_tiles(
    roster: &[Participant],
    speaking: &HashSet<String>,
    ranked_speaking: &HashSet<String>,
    ranking_threshold: u32,
) -> Tiles {
    let mut remote = Vec::with_capacity(roster.len());
    let mut own = None;
    for participant in roster {
        let camera = tile(participant, TileKind::Person, speaking);
        if participant.is_local {
            // R7: beside the list, not in it. R13: our own share is never a
            // tile — we do not subscribe to our own outgoing stream.
            own = Some(camera);
            continue;
        }
        remote.push(camera);
        if stream(participant, MediaStreamKind::ScreenShare).is_some() {
            remote.push(tile(participant, TileKind::ScreenShare, speaking));
        }
    }
    let by_activity = remote.len() > ranking_threshold as usize;
    remote.sort_by_cached_key(|t| rank_key(t, by_activity, ranked_speaking.contains(&t.member_id)));
    Tiles { remote, own }
}

/// Applies a window to the ranked remote tiles.
///
/// `detail` is a subsequence of `order`: a tile is included when its rank
/// falls in `[offset, offset + len)` or its id is in `also`, and appears once,
/// at its rank position. Ranks past the end and ids not in the call are
/// ignored rather than errors — a tile named in `also` may have just left.
pub fn window(ranked: &[CallTile], w: &DetailWindow) -> TileRoster {
    let start = w.offset as usize;
    // Saturating: on a 32-bit target `offset + u32::MAX` overflows `usize`.
    let end = start.saturating_add(w.len as usize);
    let mut order = Vec::with_capacity(ranked.len());
    let mut detail = Vec::new();
    for (rank, tile) in ranked.iter().enumerate() {
        let id = tile.id();
        let windowed = (start..end).contains(&rank) || w.also.contains(&id);
        order.push(TileRef {
            id,
            user_id: tile.user_id.clone(),
            hero: tile.hero,
        });
        if windowed {
            detail.push(tile.clone());
        }
    }
    TileRoster { order, detail }
}

fn tile(p: &Participant, kind: TileKind, speaking: &HashSet<String>) -> CallTile {
    CallTile {
        member_id: p.member_id.clone(),
        kind,
        user_id: p.user_id.clone(),
        device_id: p.device_id.clone(),
        // R3: a share is a hero. R12 falls out of derivation: the own tile is
        // only ever built for `Camera`.
        hero: kind == TileKind::ScreenShare,
        has_video: stream(p, kind.video_stream()).is_some_and(|s| !s.muted),
        microphone_muted: stream(p, MediaStreamKind::Microphone).is_none_or(|s| s.muted),
        speaking: speaking.contains(&p.member_id),
        hand_raised_at_ms: p.hand_raised_at_ms,
        hand_raised_position: p.hand_raised_position,
        joined_at_ms: p.joined_at_ms,
        connection: p.connection,
        unable_to_decrypt: p.unable_to_decrypt,
    }
}

fn stream(p: &Participant, kind: MediaStreamKind) -> Option<&StreamState> {
    p.streams.iter().find(|s| s.kind == kind)
}

/// Sort key, lowest first, so R8's order comes out highest first: hero, raised
/// hand (earliest first), speaking, video, join time (earliest first), then a
/// tie-break that is arbitrary but identical on every client.
///
/// Speaking is `ranked_speaking` — the damped signal — not the tile's own
/// flag, which is raw. Without `by_activity`, hand, speaking and video drop
/// out and the order is hero, then join time.
///
/// `member_id` is the tie-break rather than anything local, because two
/// clients seeing the same call must produce the same order. It is 16 random
/// bytes as hex — meaningless, and the same everywhere.
#[allow(clippy::type_complexity)]
fn rank_key(
    t: &CallTile,
    by_activity: bool,
    ranked_speaking: bool,
) -> (
    Reverse<bool>,
    (bool, u64),
    Reverse<bool>,
    Reverse<bool>,
    (bool, u64),
    String,
    u8,
) {
    (
        Reverse(t.hero),
        none_last(t.hand_raised_at_ms.filter(|_| by_activity)),
        Reverse(by_activity && ranked_speaking),
        Reverse(by_activity && t.has_video),
        none_last(t.joined_at_ms),
        t.member_id.clone(),
        match t.kind {
            TileKind::ScreenShare => 1,
            _ => 0,
        },
    )
}

/// `Option` orders `None` first; a missing hand or join time belongs last.
fn none_last(v: Option<u64>) -> (bool, u64) {
    (v.is_none(), v.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str) -> Participant {
        Participant {
            member_id: id.into(),
            user_id: format!("@{id}:example.org"),
            device_id: None,
            is_local: false,
            connection: ConnectionState::Connected,
            unable_to_decrypt: None,
            streams: vec![],
            hand_raised_at_ms: None,
            hand_raised_position: None,
            joined_at_ms: None,
        }
    }

    fn publishing(mut p: Participant, kind: MediaStreamKind, muted: bool) -> Participant {
        p.streams.push(StreamState { kind, muted });
        p
    }

    fn order(tiles: &[CallTile]) -> Vec<(&str, TileKind)> {
        tiles
            .iter()
            .map(|t| (t.member_id.as_str(), t.kind))
            .collect()
    }

    fn silent() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn r1_a_sharer_is_two_tiles_and_a_non_sharer_one() {
        let roster = [
            publishing(member("a"), MediaStreamKind::ScreenShare, false),
            member("b"),
        ];
        let tiles = derive_tiles(&roster, &silent(), &silent(), 0);
        assert_eq!(
            order(&tiles.remote),
            [
                ("a", TileKind::ScreenShare),
                ("a", TileKind::Person),
                ("b", TileKind::Person),
            ]
        );
    }

    #[test]
    fn r13_our_own_share_makes_no_tile() {
        let mut me = publishing(member("me"), MediaStreamKind::ScreenShare, false);
        me.is_local = true;
        let tiles = derive_tiles(&[me], &silent(), &silent(), 0);
        assert!(tiles.remote.is_empty());
        assert_eq!(tiles.own.as_ref().map(|t| t.kind), Some(TileKind::Person));
    }

    #[test]
    fn r7_r12_own_tile_is_beside_the_list_and_never_hero() {
        let mut me = member("me");
        me.is_local = true;
        let tiles = derive_tiles(&[member("a"), me], &silent(), &silent(), 0);
        assert_eq!(order(&tiles.remote), [("a", TileKind::Person)]);
        let own = tiles.own.expect("own tile");
        assert_eq!(own.member_id, "me");
        assert!(!own.hero);
    }

    #[test]
    fn r3_a_hero_outranks_a_raised_hand() {
        let mut hand = member("hand");
        hand.hand_raised_at_ms = Some(1);
        let sharer = publishing(member("share"), MediaStreamKind::ScreenShare, false);
        let tiles = derive_tiles(&[hand, sharer], &silent(), &silent(), 0);
        assert_eq!(
            tiles.remote[0].id(),
            TileId {
                member_id: "share".into(),
                kind: TileKind::ScreenShare
            }
        );
        assert!(tiles.remote[0].hero);
    }

    #[test]
    fn r4_the_earlier_hand_ranks_first() {
        let mut late = member("late");
        late.hand_raised_at_ms = Some(200);
        let mut early = member("early");
        early.hand_raised_at_ms = Some(100);
        let tiles = derive_tiles(&[late, early], &silent(), &silent(), 0);
        assert_eq!(
            order(&tiles.remote),
            [("early", TileKind::Person), ("late", TileKind::Person)]
        );
    }

    /// R10 damps the order, R11 keeps a tile's own state immediate: a member
    /// who has just started talking reports speaking before they are ranked
    /// for it, and keeps ranking for it through the cooldown after they stop.
    #[test]
    fn r10_the_flag_is_raw_and_only_the_rank_is_damped() {
        let raw: HashSet<String> = ["b".to_string()].into();
        let tiles = derive_tiles(&[member("a"), member("b")], &raw, &silent(), 0);
        assert_eq!(
            order(&tiles.remote),
            [("a", TileKind::Person), ("b", TileKind::Person)],
            "not yet ranked for it"
        );
        assert!(tiles.remote[1].speaking, "but already reported speaking");

        let ranked: HashSet<String> = ["b".to_string()].into();
        let tiles = derive_tiles(&[member("a"), member("b")], &silent(), &ranked, 0);
        assert_eq!(tiles.remote[0].member_id, "b", "still ranked for it");
        assert!(!tiles.remote[0].speaking, "but no longer reported speaking");
    }

    #[test]
    fn r5_speaking_outranks_silent() {
        let speaking: HashSet<String> = ["b".to_string()].into();
        let tiles = derive_tiles(&[member("a"), member("b")], &speaking, &speaking, 0);
        assert_eq!(
            order(&tiles.remote),
            [("b", TileKind::Person), ("a", TileKind::Person)]
        );
        assert!(tiles.remote[0].speaking);
    }

    #[test]
    fn r6_video_outranks_no_video() {
        let roster = [
            member("a"),
            publishing(member("b"), MediaStreamKind::Camera, false),
        ];
        let tiles = derive_tiles(&roster, &silent(), &silent(), 0);
        assert_eq!(
            order(&tiles.remote),
            [("b", TileKind::Person), ("a", TileKind::Person)]
        );
        assert!(tiles.remote[0].has_video);
        assert!(!tiles.remote[1].has_video);
    }

    #[test]
    fn at_or_below_the_threshold_activity_does_not_move_a_tile() {
        let mut early = member("early");
        early.joined_at_ms = Some(1000);
        let mut late = publishing(member("late"), MediaStreamKind::Camera, false);
        late.joined_at_ms = Some(2000);
        late.hand_raised_at_ms = Some(1);
        let speaking: HashSet<String> = ["late".to_string()].into();
        let tiles = derive_tiles(&[late, early], &speaking, &speaking, 2);
        assert_eq!(
            order(&tiles.remote),
            [("early", TileKind::Person), ("late", TileKind::Person)]
        );
        // The flags still report what the member is doing.
        assert!(tiles.remote[1].speaking);
    }

    #[test]
    fn at_or_below_the_threshold_a_hero_still_leads() {
        let mut early = member("early");
        early.joined_at_ms = Some(1000);
        let mut sharer = publishing(member("sharer"), MediaStreamKind::ScreenShare, false);
        sharer.joined_at_ms = Some(2000);
        let tiles = derive_tiles(&[early, sharer], &silent(), &silent(), 3);
        assert_eq!(
            order(&tiles.remote),
            [
                ("sharer", TileKind::ScreenShare),
                ("early", TileKind::Person),
                ("sharer", TileKind::Person),
            ]
        );
    }

    #[test]
    fn above_the_threshold_the_speaker_ranks_first() {
        let mut early = member("early");
        early.joined_at_ms = Some(1000);
        let mut late = member("late");
        late.joined_at_ms = Some(2000);
        let speaking: HashSet<String> = ["late".to_string()].into();
        // Spotlight: one tile shown, so two tiles are ranked.
        let tiles = derive_tiles(&[early, late], &speaking, &speaking, 1);
        assert_eq!(
            order(&tiles.remote),
            [("late", TileKind::Person), ("early", TileKind::Person)]
        );
    }

    #[test]
    fn r8_full_order_then_join_time_then_member_id() {
        let sharer = publishing(member("share"), MediaStreamKind::ScreenShare, false);
        let mut hand = member("hand");
        hand.hand_raised_at_ms = Some(1);
        let video = publishing(member("video"), MediaStreamKind::Camera, false);
        let mut joined_late = member("z-late");
        joined_late.joined_at_ms = Some(2000);
        let mut joined_early = member("z-early");
        joined_early.joined_at_ms = Some(1000);
        let no_time_b = member("b");
        let no_time_a = member("a");
        let speaking: HashSet<String> = ["talk".to_string()].into();

        let roster = [
            joined_late,
            no_time_b,
            video,
            member("talk"),
            hand,
            no_time_a,
            joined_early,
            sharer,
        ];
        let tiles = derive_tiles(&roster, &speaking, &speaking, 0);
        assert_eq!(
            order(&tiles.remote),
            [
                ("share", TileKind::ScreenShare),
                ("hand", TileKind::Person),
                ("talk", TileKind::Person),
                ("video", TileKind::Person),
                ("z-early", TileKind::Person),
                ("z-late", TileKind::Person),
                // Plain tiles with no join time, by member_id — including the
                // sharer's camera tile, which is not a hero (C4) and publishes
                // no camera here.
                ("a", TileKind::Person),
                ("b", TileKind::Person),
                ("share", TileKind::Person),
            ]
        );
    }

    #[test]
    fn c2_order_is_total_and_deterministic_without_join_times() {
        let roster = [member("c"), member("a"), member("b")];
        let first = derive_tiles(&roster, &silent(), &silent(), 0);
        let again = derive_tiles(&roster, &silent(), &silent(), 0);
        assert_eq!(first, again);
        assert_eq!(
            order(&first.remote),
            [
                ("a", TileKind::Person),
                ("b", TileKind::Person),
                ("c", TileKind::Person)
            ]
        );
    }

    #[test]
    fn r16_camera_identity_survives_a_share_starting_and_stopping() {
        let before = derive_tiles(&[member("a")], &silent(), &silent(), 0);
        let during = derive_tiles(
            &[publishing(member("a"), MediaStreamKind::ScreenShare, false)],
            &silent(),
            &silent(),
            0,
        );
        let after = derive_tiles(&[member("a")], &silent(), &silent(), 0);
        let camera = |t: &Tiles| {
            t.remote
                .iter()
                .find(|x| x.kind == TileKind::Person)
                .map(CallTile::id)
        };
        assert_eq!(camera(&before), camera(&during));
        assert_eq!(camera(&during), camera(&after));
    }

    #[test]
    fn a_muted_remote_share_is_a_hero_without_video() {
        let tiles = derive_tiles(
            &[publishing(member("a"), MediaStreamKind::ScreenShare, true)],
            &silent(),
            &silent(),
            0,
        );
        let share = &tiles.remote[0];
        assert_eq!(share.kind, TileKind::ScreenShare);
        assert!(share.hero);
        assert!(!share.has_video);
    }

    #[test]
    fn microphone_muted_covers_absent_and_muted() {
        let unmuted_mic = publishing(member("a"), MediaStreamKind::Microphone, false);
        let muted_mic = publishing(member("b"), MediaStreamKind::Microphone, true);
        let no_mic = member("c");
        let tiles = derive_tiles(&[unmuted_mic, muted_mic, no_mic], &silent(), &silent(), 0);
        let muted: Vec<(&str, bool)> = tiles
            .remote
            .iter()
            .map(|t| (t.member_id.as_str(), t.microphone_muted))
            .collect();
        assert_eq!(muted, [("a", false), ("b", true), ("c", true)]);
    }

    // ---- window ------------------------------------------------------------

    fn ranked(n: usize) -> Vec<CallTile> {
        let roster: Vec<Participant> = (0..n).map(|i| member(&format!("m{i:03}"))).collect();
        derive_tiles(&roster, &silent(), &silent(), 0).remote
    }

    fn ids(tiles: &[CallTile]) -> Vec<TileId> {
        tiles.iter().map(CallTile::id).collect()
    }

    #[test]
    fn default_window_is_everything() {
        let tiles = ranked(5);
        let roster = window(&tiles, &DetailWindow::default());
        assert_eq!(roster.order.len(), 5);
        assert_eq!(ids(&roster.detail), ids(&tiles));
    }

    #[test]
    fn range_detail_is_a_subsequence_of_a_complete_order() {
        let tiles = ranked(5);
        let w = DetailWindow {
            offset: 1,
            len: 2,
            also: HashSet::new(),
        };
        let roster = window(&tiles, &w);
        assert_eq!(roster.order.len(), 5, "order is never truncated");
        assert_eq!(ids(&roster.detail), ids(&tiles[1..3]));
    }

    /// A tile outside the window has no record, but its reference must still
    /// draw as an avatar with a name — so it carries the user id (C12).
    #[test]
    fn a_reference_outside_the_window_still_names_its_user() {
        let tiles = ranked(5);
        let w = DetailWindow {
            offset: 0,
            len: 1,
            also: HashSet::new(),
        };
        let roster = window(&tiles, &w);
        assert_eq!(roster.detail.len(), 1);
        let outside = &roster.order[4];
        assert_eq!(outside.id, tiles[4].id());
        assert_eq!(outside.user_id, tiles[4].user_id);
    }

    #[test]
    fn also_appears_at_its_rank_position_not_appended() {
        let tiles = ranked(5);
        let w = DetailWindow {
            offset: 0,
            len: 1,
            also: [tiles[3].id()].into(),
        };
        let roster = window(&tiles, &w);
        assert_eq!(ids(&roster.detail), vec![tiles[0].id(), tiles[3].id()]);
    }

    #[test]
    fn also_inside_the_range_is_not_duplicated() {
        let tiles = ranked(5);
        let w = DetailWindow {
            offset: 0,
            len: 3,
            also: [tiles[1].id()].into(),
        };
        assert_eq!(window(&tiles, &w).detail.len(), 3);
    }

    #[test]
    fn range_past_the_end_clamps() {
        let tiles = ranked(3);
        let tail = DetailWindow {
            offset: 2,
            len: 10,
            also: HashSet::new(),
        };
        assert_eq!(ids(&window(&tiles, &tail).detail), vec![tiles[2].id()]);
        let beyond = DetailWindow {
            offset: 5,
            len: 10,
            also: HashSet::new(),
        };
        assert!(window(&tiles, &beyond).detail.is_empty());
        assert_eq!(window(&tiles, &beyond).order.len(), 3);
    }

    #[test]
    fn unknown_also_id_is_ignored() {
        let tiles = ranked(2);
        let w = DetailWindow {
            offset: 0,
            len: 0,
            also: [TileId {
                member_id: "left-already".into(),
                kind: TileKind::Person,
            }]
            .into(),
        };
        assert!(window(&tiles, &w).detail.is_empty());
    }

    #[test]
    fn builds_only_what_is_windowed() {
        let tiles = ranked(200);
        let w = DetailWindow {
            offset: 0,
            len: 20,
            also: HashSet::new(),
        };
        let roster = window(&tiles, &w);
        assert_eq!(roster.order.len(), 200);
        assert_eq!(roster.detail.len(), 20);
    }
}
