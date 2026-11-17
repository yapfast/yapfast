//! The grid as a widget of its own: `ParticipantGrid` lays its tiles out
//! from its own size (`ui/voice.slint:910`-`:982`), which only a layout
//! knows, and scrolls them as a `Flickable` when they're taller than it.
//! Its tiles are V03's `tiles::view`, each kept by its tile's key, so a
//! tile keeps its state (its pointer, its rings) and its glide as others
//! come and go before it. Only the tiles that show are built: full screen,
//! or with the others hidden, the large one alone.
//!
//! The Widget itself is in `widget`; this holds what it's made of and
//! what it keeps between views.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::widget::Tree;
use iced::{Rectangle, Size, mouse};

use anomi_theme::prelude::*;
use anomi_vm::voice::stage::{Placement, Stage};
use anomi_vm::voice::{self, tiles::Key};

use super::glide::Glide;
use super::layout::Arrangement;
use crate::tiles;

/// The participant grid: the tiles that show, each with its slot and key.
pub struct Grid<'a> {
    pub(super) arrangement: Arrangement,
    pub(super) slots: Vec<usize>,
    pub(super) keys: Vec<u64>,
    pub(super) tiles: Vec<Element<'a, voice::Intent>>,
    pub(super) motion: Motion,
}

/// The grid of `stage`'s tiles.
pub fn grid(stage: &Stage, motion: Motion) -> Grid<'_> {
    let arrangement = Arrangement::of(stage);
    let shown: Vec<_> = stage
        .tiles
        .iter()
        .enumerate()
        .filter(|(slot, _)| arrangement.shown(*slot))
        .collect();
    let mut grid = Grid {
        arrangement,
        slots: Vec::with_capacity(shown.len()),
        keys: Vec::with_capacity(shown.len()),
        tiles: Vec::with_capacity(shown.len()),
        motion,
    };
    for (slot, tile) in shown {
        grid.slots.push(slot);
        grid.keys.push(hashed(&tile.key));
        grid.tiles
            .push(tiles::view(tile, motion).map(voice::Intent::from).boxed());
    }
    grid
}

/// A tile's key as the grid keeps it.
fn hashed(key: &Key) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// What the grid keeps between views.
#[derive(Debug, Default)]
pub(super) struct State {
    /// The keys of the tiles in `Tree::children`, and their glides, in the
    /// same order.
    pub keys: Vec<u64>,
    pub glides: Vec<Glide>,
    /// The room at the last layout, and the arrangement laid out in it.
    pub size: Option<Size>,
    pub arrangement: Arrangement,
    /// How far down the tiles are scrolled, and how tall they are.
    pub scroll: f32,
    pub content: f32,
    /// The latest frame's time, and the one the last layout placed the
    /// tiles at.
    pub now: Option<Instant>,
    pub laid: Option<Instant>,
    /// Where the tiles are, and what the app last heard of it.
    pub placement: Placement,
    pub reported: Option<Placement>,
    /// The left button went down beside every tile.
    pub beside: bool,
}

impl State {
    /// Moves each kept tile's tree and glide to where its key now is, and
    /// makes new ones for tiles new to the grid.
    pub fn rekey(&mut self, children: &mut Vec<Tree>, grid: &Grid<'_>) {
        if self.keys == grid.keys {
            return;
        }
        let mut kept: Vec<Option<(Tree, Glide)>> = std::mem::take(children)
            .into_iter()
            .zip(std::mem::take(&mut self.glides))
            .map(Some)
            .collect();
        for (tile, key) in grid.tiles.iter().zip(&grid.keys) {
            let found = self
                .keys
                .iter()
                .position(|old| old == key)
                .and_then(|at| kept.get_mut(at)?.take());
            let (tree, glide) = found.unwrap_or_else(|| (Tree::new(tile), Glide::default()));
            children.push(tree);
            self.glides.push(glide);
        }
        self.keys.clone_from(&grid.keys);
    }

    /// How far the tiles may scroll in a room `height` tall.
    pub fn most(&self, height: f32) -> f32 {
        (self.content - height).max(0.0)
    }
}

/// The pointer, for what lies inside `clip`: none outside it, as the
/// tiles scrolled under its edges are.
pub(super) fn inside(cursor: mouse::Cursor, clip: Rectangle) -> mouse::Cursor {
    if cursor.is_over(clip) {
        cursor
    } else {
        mouse::Cursor::Unavailable
    }
}
