//! The rows as they are now: where each is drawn, the pointer they see, the
//! dragged row's copy and its drop line, and the report of those in view.

use iced::advanced::Layout;
use iced::advanced::layout::Limits;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget as _};
use iced::{Point, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

use super::List;
use super::anchor;
use super::build::row_limits;
use super::drag::{self, Gesture};
use super::ghost;
use super::place::Frame;
use super::report::{MARGIN, Viewport};
use super::state::State;
use crate::scrollbar::Bar;

/// How far past the view rows are built, as a share of its height, and at
/// least.
const BUILT_PAST: f32 = 0.25;
const BUILT_PAST_LEAST: f32 = 64.0;

/// A row's layout moved up by what the list scrolled since it was laid out.
pub fn moved(layout: Layout, shift: f32) -> Layout {
    layout.move_to(layout.position() - Vector::new(0.0, shift))
}

impl<'a, Message: 'a> List<'a, Message> {
    pub(super) fn fresh_state(&self) -> State {
        State::new(self.id.clone(), self.estimate, self.from_end, self.follow)
    }

    /// The view the rows are placed in, for a view of each height.
    pub(super) fn framing(&self) -> impl Fn(f32) -> Frame + use<'a, Message> {
        let (padding, gap, eager) = (self.padding, self.spacing, self.source.is_eager());
        let end_aligned = self.from_end && !self.short_at_start;
        move |view| Frame {
            view,
            start: padding.top,
            end: view - padding.bottom,
            margin: if eager {
                f32::INFINITY
            } else {
                (view * BUILT_PAST).max(BUILT_PAST_LEAST)
            },
            gap,
            end_aligned,
        }
    }

    /// The pointer as the rows see it: none while the list holds it (a drag,
    /// the bar, an autoscroll), or while it's off the list or on its bar.
    pub(super) fn row_cursor(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Cursor {
        let held = state.gesture.dragging() || state.bar.is_dragged() || state.autoscroll.is_some();
        if held || !cursor.is_over(bounds) || self.on_bar(state, bounds, cursor) {
            mouse::Cursor::Unavailable
        } else {
            cursor
        }
    }

    /// Notes the rows in view for the owner's report, if it asked for one.
    pub(super) fn see(&self, state: &mut State) {
        if self.on_viewport.is_some() {
            let source = &self.source;
            let seen = viewport(state, &|index| source.key(index));
            state.reports.see(seen, self.pixels);
        }
    }

    /// The pointer is over the strip of a scrollbar that shows.
    pub(super) fn on_bar(&self, state: &State, bounds: Rectangle, cursor: mouse::Cursor) -> bool {
        self.scrollbar && Bar::is_over(bounds, state.extent(), cursor)
    }

    /// Finds the pressed or dragged row again by its key, wherever rows went
    /// in or out, and builds a dragged row apart while it's out of the rows
    /// built, so its copy follows the pointer wherever the list scrolled.
    /// The copy the owner builds, if it does, is built apart while the drag
    /// lasts, laid out in its ground.
    pub(super) fn build_ghost(
        &mut self,
        state: &mut State,
        renderer: &Renderer,
        width: f32,
        view: f32,
    ) {
        self.ghost = None;
        let source = &self.source;
        let find = |key, hint| anchor::find(&|at| source.key(at), source.count(), key, hint);
        let (key, index) = match &mut state.gesture {
            Gesture::Idle => {
                state.ghost = None;
                state.faded.clear();
                return;
            }
            Gesture::Pressed { key, index, .. } => {
                state.ghost = None;
                match find(*key, *index) {
                    Some(found) => *index = found,
                    // Gone before it was picked up: the owner heard nothing.
                    None => state.gesture = Gesture::Idle,
                }
                return;
            }
            Gesture::Dragging { key, index, .. } => (*key, index),
        };
        let Some(found) = find(key, *index) else {
            // Gone from the list while dragged: the owner hears it went
            // back at the next event, as a layout has no shell to tell it.
            state.lost = Some(*index);
            state.gesture = Gesture::Idle;
            state.ghost = None;
            return;
        };
        *index = found;
        let copy = self.reorder.as_ref().and_then(|reorder| {
            let copy = reorder.copy.as_ref()?;
            Some((copy(found), (reorder.ground)(found)))
        });
        let (mut row, limits) = if let Some((copy, ground)) = copy {
            let size = Size::new(
                (width - ground.left - ground.right).max(0.0),
                ground.height.unwrap_or_else(|| state.heights.height(key)),
            );
            (copy, Limits::new(size, size))
        } else if self.source.is_eager() || state.built.keys.contains(&key) {
            state.ghost = None;
            return;
        } else {
            (self.source.build(found), row_limits(width, view))
        };
        let mut tree = state.ghost.take().unwrap_or_else(|| Tree::new(&row));
        tree.diff(&mut row);
        row.layout(&mut tree, renderer, &limits);
        state.ghost = Some(tree);
        self.ghost = Some(row);
    }

    /// The drop line, and the dragged row's copy at the pointer, on its
    /// ground and as faint as it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_drag(
        &self,
        state: &State,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        clip: &Rectangle,
    ) {
        let Gesture::Dragging {
            key,
            index: from,
            grab,
            pointer,
            over,
        } = state.gesture
        else {
            return;
        };
        let Some(reorder) = &self.reorder else {
            return;
        };
        let rows = || {
            layout
                .iter(&tree.children)
                .map(|(child, tree)| (moved(child, state.shift), tree))
        };
        if reorder.line
            && let Some(to) = over
            && to != from
            && (reorder.may_drop)(from, to)
            && let Some((row, _)) = to
                .checked_sub(state.built.first)
                .and_then(|at| rows().nth(at))
        {
            let first_at_top =
                to == 0 && row.bounds().y <= layout.bounds().y + state.frame.start + 0.5;
            drag::draw_line(renderer, theme, row.bounds(), from, to, first_at_top);
        }
        // Onto rows that are targets the copy hangs below the pointer, off
        // the row it would drop on; with the line it's held where grabbed.
        let y = if reorder.line {
            pointer.y - grab
        } else {
            pointer.y + ghost::BELOW_POINTER
        };
        // The copy is as faint as its ground, as Slint fades the two as one.
        let ground = (reorder.ground)(from);
        let faded = state.faded.get(theme, ground.alpha);
        let style = renderer::Style {
            text_color: style.text_color.scale_alpha(ground.alpha),
        };
        // The owner's copy was laid out in its ground; the row lies on it.
        let owners = reorder.copy.is_some();
        let cursor = mouse::Cursor::Unavailable;
        let mut draw = |copy: &Element<'a, Message>, copy_tree: &Tree, ghost: Layout| {
            let under = if owners {
                ghost.bounds()
            } else {
                ghost::ghost_ground(ghost.bounds(), ground)
            };
            ghost::draw_ground(renderer, theme, under, ground);
            copy.draw(copy_tree, renderer, &faded, &style, ghost, cursor, clip);
        };
        let built = state.built.keys.iter().position(|built| *built == key);
        if !owners
            && let Some(at) = built
            && let Some((row, row_tree)) = rows().nth(at)
        {
            draw(
                &self.built[at],
                row_tree,
                row.move_to(Point::new(row.position().x, y)),
            );
        } else if let (Some(copy), Some(copy_tree)) = (&self.ghost, &state.ghost) {
            let left = if owners { ground.left } else { 0.0 };
            let x = layout.bounds().x + self.padding.left + left;
            draw(
                copy,
                copy_tree,
                Layout::new(copy_tree.size).move_to(Point::new(x, y)),
            );
        }
    }
}

/// The rows in view and where the view is.
pub fn viewport(state: &State, key: &dyn Fn(usize) -> u64) -> Viewport {
    let built = &state.built;
    let view = state.frame.view;
    if built.keys.is_empty() {
        return Viewport {
            following: state.following(),
            height: view,
            ..Viewport::default()
        };
    }
    let shown = |at: &usize| {
        let (top, height) = state.row(*at);
        top < view && top + height > 0.0
    };
    let first_at = (0..built.keys.len()).find(shown).unwrap_or(0);
    let last_at = (first_at..built.keys.len())
        .rev()
        .find(shown)
        .unwrap_or(first_at);
    let (first, last) = (built.first + first_at, built.first + last_at);
    let to = (last + MARGIN).min(built.count - 1);
    let extent = state.extent();
    Viewport {
        first,
        last,
        keys: (first.saturating_sub(MARGIN)..=to).map(key).collect(),
        top: built.keys[first_at],
        near_start: extent.value < view,
        near_end: extent.maximum - extent.value < view,
        at_end: state.at_end(),
        following: state.following(),
        offset: extent.value,
        height: view,
    }
}
