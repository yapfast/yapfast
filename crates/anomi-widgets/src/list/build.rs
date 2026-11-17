//! Building the rows a layout places: each row near the view is built (or
//! taken from this widget's last layout, if it lays out again before the
//! next view), given the state its key had, and measured at the list's
//! width. The Slint side: the repeater's instances in
//! `vendor/i-slint-core/model/repeater.rs`, made and dropped as rows come
//! into view and leave it.

use std::collections::HashMap;

use iced::advanced::layout::Limits;
use iced::advanced::widget::{Tree, Widget as _};
use iced::{Size, Vector};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

use super::heights::Heights;
use super::place::{Placed, Rows};

/// Where the rows come from.
pub enum Source<'a, Message> {
    /// Built when they come near the view.
    Lazy {
        count: usize,
        key: Box<dyn Fn(usize) -> u64 + 'a>,
        row: Box<dyn Fn(usize) -> Element<'a, Message> + 'a>,
    },
    /// Built already: all of them are laid out.
    Eager {
        keys: Vec<u64>,
        rows: Vec<Option<Element<'a, Message>>>,
    },
}

impl<'a, Message: 'a> Source<'a, Message> {
    pub fn count(&self) -> usize {
        match self {
            Source::Lazy { count, .. } => *count,
            Source::Eager { keys, .. } => keys.len(),
        }
    }

    pub fn key(&self, index: usize) -> u64 {
        match self {
            Source::Lazy { key, .. } => key(index),
            Source::Eager { keys, .. } => keys[index],
        }
    }

    pub fn is_eager(&self) -> bool {
        matches!(self, Source::Eager { .. })
    }

    /// Row `index`, built.
    pub fn build(&mut self, index: usize) -> Element<'a, Message> {
        match self {
            Source::Lazy { row, .. } => row(index),
            Source::Eager { rows, .. } => rows
                .get_mut(index)
                .and_then(Option::take)
                .unwrap_or_else(|| iced::widget::space().boxed()),
        }
    }
}

/// A row built in this layout.
struct Made<'a, Message> {
    key: u64,
    row: Element<'a, Message>,
    tree: Tree,
}

/// The placement's rows: builds each one asked for.
pub struct Builder<'s, 'a, Message> {
    source: &'s mut Source<'a, Message>,
    renderer: &'s Renderer,
    limits: Limits,
    /// This widget's rows from its last layout, by index from `kept_first`.
    kept: Vec<Option<Element<'a, Message>>>,
    kept_first: usize,
    /// The state of the rows built at the last layout, by key.
    trees: HashMap<u64, Tree>,
    made: HashMap<usize, Made<'a, Message>>,
    heights: &'s mut Heights,
}

impl<'s, 'a, Message: 'a> Builder<'s, 'a, Message> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: &'s mut Source<'a, Message>,
        renderer: &'s Renderer,
        width: f32,
        view: f32,
        kept: Vec<Element<'a, Message>>,
        kept_first: usize,
        trees: impl Iterator<Item = (u64, Tree)>,
        heights: &'s mut Heights,
    ) -> Self {
        Self {
            source,
            renderer,
            limits: row_limits(width, view),
            kept: kept.into_iter().map(Some).collect(),
            kept_first,
            trees: trees.collect(),
            made: HashMap::new(),
            heights,
        }
    }

    /// Hands over the rows placed, in order, each tree moved to its place:
    /// the rows, their trees and their keys. The rest are dropped.
    pub fn install(
        mut self,
        placed: &Placed,
        left: f32,
    ) -> (Vec<Element<'a, Message>>, Vec<Tree>, Vec<u64>) {
        let length = placed.tops.len();
        let (mut rows, mut trees, mut keys) = (
            Vec::with_capacity(length),
            Vec::with_capacity(length),
            Vec::with_capacity(length),
        );
        for (at, top) in placed.tops.iter().enumerate() {
            let index = placed.first + at;
            if !self.made.contains_key(&index) {
                let _ = self.height(index);
            }
            let Some(Made { key, row, mut tree }) = self.made.remove(&index) else {
                continue;
            };
            tree.translation = Vector::new(left, *top);
            rows.push(row);
            trees.push(tree);
            keys.push(key);
        }
        (rows, trees, keys)
    }
}

impl<'a, Message: 'a> Rows for Builder<'_, 'a, Message> {
    fn count(&self) -> usize {
        self.source.count()
    }

    fn height(&mut self, index: usize) -> f32 {
        if let Some(made) = self.made.get(&index) {
            return made.tree.size.height;
        }
        let key = self.source.key(index);
        let kept = index
            .checked_sub(self.kept_first)
            .and_then(|at| self.kept.get_mut(at))
            .and_then(Option::take);
        let mut row = kept.unwrap_or_else(|| self.source.build(index));
        let mut tree = self.trees.remove(&key).unwrap_or_else(|| Tree::new(&row));
        tree.diff(&mut row);
        row.layout(&mut tree, self.renderer, &self.limits);
        let height = tree.size.height;
        self.heights.set(key, height);
        self.made.insert(index, Made { key, row, tree });
        height
    }
}

/// A row is as wide as the list less its padding, and as tall as it needs:
/// a row that fills its height shrinks to what it holds.
pub fn row_limits(width: f32, view: f32) -> Limits {
    Limits::with_flags(
        Size::new(width, 0.0),
        Size::new(width, view.max(0.0)),
        Size::new(false, true),
        Size::new(false, true),
    )
}
