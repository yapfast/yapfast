//! The stage's layout, with no window in it: which tiles there are and
//! in what order, where each goes at a size, which the corner shows, the
//! pixels a tile's frames are made at, and the words under a tile.

use iced::{Point, Rectangle, Size};
use yapfast_core::voice::{self, Participant, SharePhase};

/// Between tiles, and round the stage's edge; none full screen.
pub(super) const GAP: f32 = 12.0;

pub(super) const INSET: f32 = 24.0;

/// Tiles' shape, and the widest a tile of the grid gets.
pub(super) const ASPECT: f32 = 0.5625;

pub(super) const WIDEST: f32 = 420.0;
/// The narrowest tile a call of more than four still fits the stage with,
/// before it scrolls instead: a name, its mute and its avatar still read.
pub(super) const FITS: f32 = 120.0;

/// The corner picture's size.
pub const CORNER: Size = Size::new(320.0, 180.0);

/// The tile shown large.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Focus {
    Person(String),
    Stream(String),
    OwnScreen,
}

/// What the corner picture shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    None,
    /// A stream watched, by its streamer's place among the participants.
    Watched(usize),
    Shared,
    Camera(usize),
}

/// What a tile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    OwnScreen,
    Stream(usize),
    Person(usize),
}

/// The call's tiles.
#[derive(Clone, Copy)]
pub struct Stage<'a> {
    pub participants: &'a [Participant],
    pub sharing: bool,
}

impl<'a> Stage<'a> {
    pub fn of(voice: &'a voice::State) -> Self {
        Self {
            participants: if voice.active() {
                &voice.participants
            } else {
                &[]
            },
            sharing: voice.sharing.is_some(),
        }
    }

    /// Those whose streams take tiles of their own: others who share and
    /// film, as their camera takes their own tile.
    pub fn streams(&self) -> impl Iterator<Item = usize> + '_ {
        self.participants
            .iter()
            .enumerate()
            .filter(|(_, person)| person.streaming && person.video.is_some() && !person.local)
            .map(|(index, _)| index)
    }

    /// The tiles before the participants'.
    pub fn before(&self) -> usize {
        usize::from(self.sharing) + self.streams().count()
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|person| person.id == id)
    }

    pub fn slots(&self) -> Vec<Slot> {
        self.sharing
            .then_some(Slot::OwnScreen)
            .into_iter()
            .chain(self.streams().map(Slot::Stream))
            .chain((0..self.participants.len()).map(Slot::Person))
            .collect()
    }

    /// Where `focus` is: a stream without a tile of its own shows in its
    /// streamer's.
    pub fn slot(&self, focus: &Focus) -> Option<usize> {
        match focus {
            Focus::OwnScreen => self.sharing.then_some(0),
            Focus::Person(id) => Some(self.before() + self.index(id)?),
            Focus::Stream(id) => {
                let person = self.index(id)?;
                match self.streams().position(|stream| stream == person) {
                    Some(stream) => Some(usize::from(self.sharing) + stream),
                    None => (self.participants[person].streaming
                        && !self.participants[person].local)
                        .then(|| self.before() + person),
                }
            }
        }
    }

    /// What the tile at `slot` shows, to show it large.
    pub fn focus_at(&self, slot: usize) -> Option<Focus> {
        let sharing = usize::from(self.sharing);
        if slot < sharing {
            return Some(Focus::OwnScreen);
        }
        let before = self.before();
        if slot < before {
            let person = self.streams().nth(slot - sharing)?;
            return Some(Focus::Stream(self.participants[person].id.clone()));
        }
        let person = self.participants.get(slot - before)?;
        let id = person.id.clone();
        Some(if person.video.is_some() || !person.streaming {
            Focus::Person(id)
        } else if person.local {
            if self.sharing {
                Focus::OwnScreen
            } else {
                Focus::Person(id)
            }
        } else {
            Focus::Stream(id)
        })
    }

    /// The corner shows what's large if it moves, else the stream watched
    /// last, this account's own, or someone else's camera, as Discord's
    /// picture in picture (`VideoSpeakerStore.getSpeaker`). `watching`
    /// names the streams watched, by their streamers' ids, in the order
    /// they were.
    pub fn pip(&self, focus: Option<&Focus>, watching: &[&str]) -> Pip {
        let watched = |id: &str| {
            watching
                .contains(&id)
                .then(|| self.index(id))
                .flatten()
                .map(Pip::Watched)
        };
        match focus {
            Some(Focus::OwnScreen) if self.sharing => return Pip::Shared,
            Some(Focus::Stream(id)) => {
                if let Some(pip) = watched(id) {
                    return pip;
                }
            }
            Some(Focus::Person(id)) => {
                if let Some(person) = self.index(id)
                    && self.participants[person].video.is_some()
                {
                    return Pip::Camera(person);
                }
            }
            _ => {}
        }
        if let Some(pip) = watching.iter().rev().find_map(|id| watched(id)) {
            pip
        } else if self.sharing {
            Pip::Shared
        } else {
            self.participants
                .iter()
                .position(|person| !person.local && person.video.is_some())
                .map_or(Pip::None, Pip::Camera)
        }
    }

    /// The tile the corner shows, which a click on it shows large.
    pub fn pip_slot(&self, pip: Pip) -> Option<usize> {
        match pip {
            Pip::None => None,
            Pip::Watched(person) => {
                self.slot(&Focus::Stream(self.participants.get(person)?.id.clone()))
            }
            Pip::Shared => self.slot(&Focus::OwnScreen),
            Pip::Camera(person) => Some(self.before() + person),
        }
    }
}

/// Where each tile goes on a stage of `size`, by place, with `focused`
/// large and the others in rows beneath it, and how tall the tiles are
/// together. Full screen, or with `hide_members` while a tile is large,
/// shows the large one alone and sets every other to [`Size::ZERO`].
pub fn tiles(
    count: usize,
    focused: Option<usize>,
    size: Size,
    full: bool,
    hide_members: bool,
) -> (Vec<Rectangle>, f32) {
    if count == 0 {
        return (Vec::new(), size.height);
    }
    let (gap, inset) = if full { (0.0, 0.0) } else { (GAP, INSET) };
    let (width, height) = (size.width, size.height);
    let span = width - 2.0 * inset;
    let focusing = focused.is_some_and(|focused| focused < count);
    let alone = focusing && (full || hide_members);
    // A few people take the columns, and the size, that make their tiles
    // largest within both the width and the height. Side by side wins
    // unless stacking makes tiles a tenth larger. More people take tiles at
    // least 240 px wide, and scroll.
    let fitted = |columns: usize| {
        let rows = count.div_ceil(columns) as f32;
        let columns = columns as f32;
        WIDEST
            .min((span - (columns - 1.0) * gap) / columns)
            .min((height - 2.0 * inset - (rows - 1.0) * gap) / rows / ASPECT)
    };
    let fewest = ((span + gap) / (240.0 + gap))
        .floor()
        .min(count as f32)
        .max(1.0) as usize;
    let mut best_fit = 1;
    while best_fit < count.min(4) && fitted(best_fit + 1) * 1.1 > fitted(best_fit) {
        best_fit += 1;
    }
    // More people fit the stage too, in the columns that make their tiles
    // largest, while those tiles stay at least FITS wide: a row cut off at
    // the stage's foot hides its names under the controls. Past that, they
    // take tiles 240 px wide and scroll.
    let fit_all = (count > 4)
        .then(|| {
            (1..=count)
                .max_by(|a, b| fitted(*a).total_cmp(&fitted(*b)))
                .filter(|&columns| fitted(columns) >= FITS)
        })
        .flatten();
    let others = count - 1;
    let strip_width = if others == 0 {
        0.0
    } else {
        ((span - (others as f32 - 1.0) * gap) / others as f32).clamp(120.0, 192.0)
    };
    let strip_columns = ((span + gap) / (strip_width + gap))
        .floor()
        .min(others as f32)
        .max(1.0) as usize;
    let strip_rows = if alone || others == 0 {
        0
    } else {
        others.div_ceil(strip_columns)
    };
    let strip_height =
        strip_rows as f32 * strip_width * ASPECT + (strip_rows as f32 - 1.0).max(0.0) * gap;
    let under_focus = if strip_rows > 0 {
        gap + strip_height
    } else {
        0.0
    };
    let focus_width = if full {
        width
    } else {
        span.min((height - 2.0 * inset - under_focus).max(160.0) / ASPECT)
    };
    let focus_height = if full { height } else { focus_width * ASPECT };
    let focus_top = if full {
        0.0
    } else {
        inset.max((height - focus_height - under_focus) / 2.0)
    };
    let columns = if focusing {
        strip_columns
    } else if count <= 4 {
        best_fit
    } else {
        fit_all.unwrap_or(fewest)
    };
    let tile_width = if focusing {
        strip_width
    } else if count <= 4 {
        fitted(columns).max(160.0)
    } else if fit_all.is_some() {
        fitted(columns)
    } else {
        WIDEST.min((span - (columns as f32 - 1.0) * gap) / columns as f32)
    };
    let tile_height = tile_width * ASPECT;
    let placed = if focusing { others } else { count };
    let rows = placed.div_ceil(columns.max(1));
    let grid_height = rows as f32 * tile_height + (rows as f32 - 1.0).max(0.0) * gap;
    let grid_width = columns as f32 * tile_width + (columns as f32 - 1.0).max(0.0) * gap;
    let first_row = if focusing {
        focus_top + focus_height + gap
    } else {
        inset.max((height - grid_height) / 2.0)
    };
    let content = if focusing {
        height.max(focus_top + focus_height + under_focus + inset)
    } else {
        height.max(grid_height + 2.0 * inset)
    };
    // On whole pixels: a frame made at a tile's size is then copied in one
    // to one, and the tile's round corners take the renderer's fast path.
    let whole = |x: f32, y: f32, width: f32, height: f32| {
        let (left, top) = (x.round(), y.round());
        Rectangle::new(
            Point::new(left, top),
            Size::new((x + width).round() - left, (y + height).round() - top),
        )
    };
    let rects = (0..count)
        .map(|slot| {
            if focusing && Some(slot) == focused {
                return whole(
                    (width - focus_width) / 2.0,
                    focus_top,
                    focus_width,
                    focus_height,
                );
            }
            // Full screen, or with the strip hidden, shows the large one alone.
            if alone {
                return Rectangle::new(Point::new(0.0, height), Size::ZERO);
            }
            let place = if focusing && focused.is_some_and(|focused| slot > focused) {
                slot - 1
            } else {
                slot
            };
            let x =
                (width - grid_width) / 2.0 + (place % columns.max(1)) as f32 * (tile_width + gap);
            let y = first_row + (place / columns.max(1)) as f32 * (tile_height + gap);
            whole(x, y, tile_width, tile_height)
        })
        .collect();
    (rects, content)
}

/// The pixels a tile of `bounds` shows at in a window of `scale` pixels
/// to the unit, while any of it is in the stage's `height` scrolled
/// `scrolled` down.
pub fn shown_at(bounds: Rectangle, scrolled: f32, height: f32, scale: f32) -> Option<(u32, u32)> {
    let pixels = |side: f32| ((side * scale).round() as u32).max(2);
    let top = bounds.y - scrolled;
    (bounds.width > 0.0 && top < height && top + bounds.height > 0.0)
        .then(|| (pixels(bounds.width), pixels(bounds.height)))
}

/// How someone shows in their tile: what they're doing, or what's wrong.
pub fn status(person: &Participant) -> &str {
    if let Some(problem) = &person.problem {
        problem
    } else if !person.connected {
        "Connection lost"
    } else if person.deafened {
        "Deafened"
    } else if person.speaking_blocked {
        "Speaking restricted"
    } else if person.muted {
        "Muted"
    } else if person.speaking {
        "Speaking"
    } else {
        "Listening"
    }
}

/// Speaking, as the ring and the lists show it: unmuted and connected.
pub fn speaking(person: &Participant) -> bool {
    person.speaking && !person.muted && person.connected
}

/// What this account's sharing is doing, in words.
pub fn share_status(voice: &voice::State) -> &'static str {
    match voice.sharing.as_ref().map(|sharing| sharing.phase) {
        Some(SharePhase::Choosing) => "Choose what to share",
        Some(SharePhase::Starting) => "Starting your stream...",
        Some(SharePhase::Live) => "Live",
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tiles_sit_side_by_side_and_one_large_puts_the_other_under_it() {
        let stage = Size::new(1500.0, 800.0);
        let (grid, _) = tiles(2, None, stage, false, false);
        assert_eq!(grid[0].y, grid[1].y);
        assert!(grid[0].x + grid[0].width <= grid[1].x);
        let (focused, _) = tiles(2, Some(1), stage, false, false);
        assert!(focused[1].width > focused[0].width);
        assert!(focused[0].y > focused[1].y + focused[1].height);
        let (full, _) = tiles(2, Some(1), stage, true, false);
        assert_eq!(full[1], Rectangle::new(Point::ORIGIN, stage));
        assert_eq!(shown_at(full[0], 0.0, stage.height, 1.0), None, "alone");
    }

    #[test]
    fn hiding_members_collapses_the_strip_and_expands_the_focused_tile() {
        let stage = Size::new(1500.0, 800.0);
        let (shown, _) = tiles(3, Some(0), stage, false, false);
        let (hidden, content) = tiles(3, Some(0), stage, false, true);
        assert!(hidden[0].height > shown[0].height);
        assert!(hidden[0].width > shown[0].width);
        assert_eq!(hidden[0].height, stage.height - 2.0 * INSET);
        assert_eq!(hidden[1].size(), Size::ZERO);
        assert_eq!(hidden[2].size(), Size::ZERO);
        assert_eq!(shown_at(hidden[1], 0.0, stage.height, 1.0), None);
        assert_eq!(shown_at(hidden[2], 0.0, stage.height, 1.0), None);
        assert!(shown_at(hidden[0], 0.0, stage.height, 1.0).is_some());
        assert_eq!(content, stage.height);
        // Without a focused tile, hiding members leaves the grid alone.
        assert_eq!(
            tiles(3, None, stage, false, true),
            tiles(3, None, stage, false, false)
        );
    }

    #[test]
    fn up_to_nine_fit_the_stage_with_room_for_every_name() {
        // A voice channel's stage in a 1120 by 760 window with the preview's
        // line over it, and the ones beside: nobody is cut off at its foot.
        for stage in [
            Size::new(784.0, 572.0),
            Size::new(1104.0, 712.0),
            Size::new(564.0, 451.0),
            Size::new(524.0, 452.0),
        ] {
            for count in [2, 4, 6, 9] {
                let (grid, content) = tiles(count, None, stage, false, false);
                assert!(content <= stage.height + 0.5, "{count} in {stage:?} scroll");
                for tile in &grid {
                    assert!(
                        tile.y + tile.height <= stage.height,
                        "{count} in {stage:?}: a tile runs past the foot"
                    );
                    assert!(tile.width >= FITS - 1.0, "{count} in {stage:?}: too small");
                }
            }
        }
    }

    #[test]
    fn many_tiles_scroll_and_only_those_in_sight_are_made() {
        let stage = Size::new(900.0, 500.0);
        let (grid, content) = tiles(60, None, stage, false, false);
        assert!(content > stage.height, "taller than the stage");
        let last = *grid.last().unwrap();
        assert_eq!(shown_at(last, 0.0, stage.height, 1.0), None, "below");
        assert!(shown_at(last, content - stage.height, stage.height, 1.0).is_some());
        assert_eq!(
            shown_at(grid[0], content - stage.height, stage.height, 1.0),
            None,
            "scrolled away"
        );
    }

    #[test]
    fn a_tile_below_the_stage_has_no_frames_made() {
        let below = Rectangle::new(Point::new(0.0, 600.0), Size::new(300.0, 168.0));
        assert_eq!(shown_at(below, 0.0, 598.0, 1.0), None);
        let half = Rectangle::new(Point::new(0.0, 500.0), Size::new(300.4, 168.6));
        assert_eq!(shown_at(half, 0.0, 598.0, 1.0), Some((300, 169)));
        // In the window's pixels: twice as many on a display at 200%.
        assert_eq!(shown_at(half, 0.0, 598.0, 2.0), Some((601, 337)));
    }

    fn person(id: &str, local: bool, streaming: bool, filming: bool) -> Participant {
        Participant {
            id: id.into(),
            user: id.into(),
            person: id.into(),
            local,
            muted: false,
            deafened: false,
            server_muted: false,
            speaking_blocked: false,
            server_deafened: false,
            speaking: false,
            connected: true,
            problem: None,
            streaming,
            video: filming.then(yapfast_core::picture::Feed::new),
            avatar: None,
        }
    }

    #[test]
    fn a_stream_takes_its_own_tile_only_beside_a_camera() {
        let people = [
            person("me", true, true, true),
            person("ana", false, true, false),
            person("bo", false, true, true),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: true,
        };
        assert_eq!(stage.streams().collect::<Vec<_>>(), [2]);
        assert_eq!(stage.before(), 2);
        assert_eq!(
            stage.slots(),
            [
                Slot::OwnScreen,
                Slot::Stream(2),
                Slot::Person(0),
                Slot::Person(1),
                Slot::Person(2),
                Slot::Person(3)
            ]
        );
        assert_eq!(stage.slot(&Focus::OwnScreen), Some(0));
        assert_eq!(stage.slot(&Focus::Stream("bo".into())), Some(1));
        assert_eq!(
            stage.slot(&Focus::Stream("ana".into())),
            Some(3),
            "in her tile"
        );
        assert_eq!(stage.slot(&Focus::Stream("cy".into())), None);
        assert_eq!(stage.slot(&Focus::Person("cy".into())), Some(5));
        for slot in 0..6 {
            let focus = stage.focus_at(slot).unwrap();
            assert_eq!(stage.slot(&focus), Some(slot), "{focus:?}");
        }
        assert_eq!(stage.focus_at(6), None);
    }

    #[test]
    fn the_corner_shows_what_is_large_or_else_a_stream_or_a_camera() {
        let people = [
            person("me", true, false, true),
            person("ana", false, true, false),
            person("cy", false, false, true),
        ];
        let stage = Stage {
            participants: &people,
            sharing: false,
        };
        let cy = Focus::Person("cy".into());
        assert_eq!(stage.pip(Some(&cy), &["ana"]), Pip::Camera(2));
        assert_eq!(stage.pip(None, &["ana"]), Pip::Watched(1));
        assert_eq!(stage.pip_slot(Pip::Watched(1)), Some(1));
        assert_eq!(
            stage.pip(None, &["cy", "ana"]),
            Pip::Watched(1),
            "the one watched last"
        );
        assert_eq!(
            stage.pip(None, &[]),
            Pip::Camera(2),
            "not this account's own"
        );
        let sharing = Stage {
            sharing: true,
            ..stage
        };
        assert_eq!(sharing.pip(Some(&Focus::OwnScreen), &["ana"]), Pip::Shared);
        assert_eq!(sharing.pip_slot(Pip::Shared), Some(0));
    }
}
