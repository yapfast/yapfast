//! A row of a menu a screen lays out itself with its submenu beside the
//! menu's card, as the device menus at the account card's carets have
//! Discord's "Input Device" and "Input Profile". While `open`, the submenu
//! shows as an overlay, its first row level with the row, [`GAP`] past the
//! card's right edge, or its left one when the window has no room on the
//! right; moved up to stay inside the window and scrolling when taller, as
//! [`super::ContextMenu`]'s submenus are, and fading in over `QUICK`.
//!
//! The pointer lights its rows and a click on one sends that row's intent;
//! the keys are the menu's, which say the row they light (`lit`). It takes
//! the pointer over itself, so the menu under it sees none there and a
//! press on it closes nothing.

mod sub;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme, View};
use anomi_vm::menu::Row;

use super::panel::{NARROWEST, PADDING, Panel};
use super::place::MARGIN;
use super::row;

/// Between the card and the submenu, as `ContextMenu`'s.
const GAP: f32 = 4.0;

pub struct Beside<'a, M> {
    content: Element<'a, M>,
    rows: &'a [Row<M>],
    open: bool,
    reach: f32,
    lit: Option<usize>,
    motion: Motion,
}

/// `content`, a row, with `rows` beside the card while `open`.
pub fn beside<'a, M>(
    content: impl View<M> + 'a,
    rows: &'a [Row<M>],
    open: bool,
    motion: Motion,
) -> Beside<'a, M> {
    Beside {
        content: content.boxed(),
        rows,
        open,
        reach: 0.0,
        lit: None,
        motion,
    }
}

impl<M> Beside<'_, M> {
    /// How far the card's edges are from the row's (its padding), so the
    /// submenu clears the card and not only the row.
    pub fn reach(mut self, reach: f32) -> Self {
        self.reach = reach;
        self
    }

    /// The submenu's row the keys light.
    pub fn lit(mut self, row: Option<usize>) -> Self {
        self.lit = row;
        self
    }
}

/// What the submenu keeps between views: its card laid out (again only
/// when its rows change) and placed, the row under the pointer and the one
/// a press began on, its fading in.
pub(crate) struct State {
    pub panel: Panel,
    measured: Option<u64>,
    /// It showed at the last overlay: a new showing fades in.
    shown: bool,
    /// The keys' row it last scrolled to.
    revealed: Option<usize>,
    pub window: Size,
    pub pointer: Option<usize>,
    pub pressed: Option<usize>,
    pub fade: Animation<bool>,
    pub now: Instant,
}

impl State {
    /// Measures the rows if they changed and places the card beside `row`,
    /// whose card reaches `reach` past it, within `window`.
    fn place<M>(&mut self, rows: &[Row<M>], row: Rectangle, reach: f32, window: Size) {
        let key = key(rows);
        if self.measured != Some(key) {
            self.measured = Some(key);
            self.panel = Panel::new(rows, NARROWEST).keep(&self.panel);
        }
        self.panel.fit(window.height - 2.0 * MARGIN);
        let size = self.panel.bounds.size();
        let right = row.x + row.width + reach + GAP;
        let x = if right + size.width <= window.width - MARGIN {
            right
        } else {
            (row.x - reach - GAP - size.width).max(MARGIN)
        };
        let y = (row.y - PADDING)
            .min(window.height - MARGIN - size.height)
            .max(MARGIN);
        self.panel.bounds = Rectangle::new(Point::new(x, y), size);
        self.window = window;
    }
}

/// What laying the submenu out depends on, hashed.
fn key<M>(rows: &[Row<M>]) -> u64 {
    let mut hasher = DefaultHasher::new();
    rows.len().hash(&mut hasher);
    for each in rows {
        row::hash(each, &mut hasher);
    }
    hasher.finish()
}

impl<M> widget::Meta for Beside<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Beside<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            panel: Panel::default(),
            measured: None,
            shown: false,
            revealed: None,
            window: Size::ZERO,
            pointer: None,
            pressed: None,
            fade: Animation::new(false),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let row = layout.bounds() + translation;
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let (child, child_tree) = layout.iter_mut(children).next().unwrap();
        let mut overlays =
            self.content
                .overlay(child_tree, child, renderer, viewport, translation, window);
        if !self.open || self.rows.is_empty() {
            state.shown = false;
            state.pointer = None;
            state.revealed = None;
            return overlays;
        }
        state.place(self.rows, row, self.reach, window);
        if !state.shown {
            state.shown = true;
            state.fade = if self.motion.still() {
                Animation::new(true)
            } else {
                Animation::new(false)
                    .duration(QUICK)
                    .easing(ARRIVE)
                    .go(true, Instant::now())
            };
        }
        if self.lit != state.revealed {
            state.revealed = self.lit;
            if let Some(lit) = self.lit {
                state.panel.reveal(lit);
            }
        }
        overlays.push(sub::overlay(self.rows, state, self.lit));
        overlays
    }
}
