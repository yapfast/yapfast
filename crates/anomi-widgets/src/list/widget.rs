//! The list as an iced widget: its layout builds and places the rows (see
//! `build` and `place`), and drawing, operations, overlays and the pointer
//! go to the rows built, each where it is now: laid out, then moved up by
//! what the list scrolled since. Events are in `events`.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::motion::SLIDE;
use anomi_theme::{Renderer, Theme};

use super::anchor;
use super::build::Builder;
use super::place;
use super::rows::drawn;
use super::state::{self, Position, State};
use super::{List, events};

/// The view of a list in something that doesn't bound its height, as a
/// scrollable: all its rows are built, up to this.
const UNBOUNDED: f32 = 1.0e6;

impl<Message> widget::Meta for List<'_, Message> {}

impl<'a, Message: 'a> widget::Widget<Message, Theme, Renderer> for List<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(self.fresh_state())
    }

    fn diff(&mut self, tree: &mut Tree) {
        // The rows are diffed as they're built, in layout.
        let state = tree.state.downcast_mut::<State>();
        if state.id != self.id {
            *state = self.fresh_state();
            tree.children.clear();
        }
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let limits = limits.width(self.width).height(self.height);
        let bounds = limits.bounds();
        let width = limits.resolve_width(self.width, limits.max.width);
        // The most it may be; placed in that, it knows how tall it is.
        let most = if bounds.height.is_finite() {
            bounds.height
        } else {
            UNBOUNDED
        };
        let Tree {
            state,
            children,
            size,
            ..
        } = tree;
        let state = state.downcast_mut::<State>();
        state.from_end = self.from_end;
        state.follow = self.follow;
        let framing = self.framing();
        let mut frame = framing(most);
        state.frame = frame;
        let count = self.source.count();
        let seed = {
            let source = &self.source;
            state.seed(count, &|index| source.key(index))
        };
        let reveal = self.reveal.and_then(|(key, margin)| {
            let source = &self.source;
            anchor::find(&|index| source.key(index), count, key, state.built.first)
                .map(|index| (key, index, margin))
        });
        let row_width = (width - self.padding.left - self.padding.right).max(0.0);
        // In a list that makes room, where each row was drawn: those this
        // layout moves slide from there.
        let live = self
            .reorder
            .as_ref()
            .is_some_and(|reorder| reorder.onto.is_some());
        let drawn: Option<HashMap<u64, f32>> = live.then(|| {
            (0..state.built.keys.len())
                .map(|at| {
                    let key = state.built.keys[at];
                    (key, state.row(at).0 + state.slides.offset(key))
                })
                .collect()
        });
        let trees = std::mem::take(&mut state.built.keys)
            .into_iter()
            .zip(children.drain(..));
        let mut builder = Builder::new(
            &mut self.source,
            renderer,
            row_width,
            most,
            std::mem::take(&mut self.built),
            self.built_first,
            trees,
            &mut state.heights,
        );
        let mut placed = place::place(&mut builder, &frame, seed);
        // A list that shrinks to what it holds is placed again in that; one
        // holding nothing is as tall as its padding.
        let whole = count == 0 || (placed.first == 0 && placed.last() + 1 == count);
        let content = if whole {
            placed.bottom() - placed.top() + self.padding.top + self.padding.bottom
        } else {
            most
        };
        *size = limits.resolve(self.width, self.height, Size::new(width, content.min(most)));
        if (size.height - frame.view).abs() > 0.5 {
            frame = framing(size.height);
            placed = place::place(&mut builder, &frame, seed);
        }
        let mut moved_to = None;
        if let Some((key, index, margin)) = reveal
            && let Some(seed) =
                anchor::reveal_seed(state.revealed, key, index, margin, &placed, frame.view)
        {
            placed = place::place(&mut builder, &frame, seed);
            moved_to = placed
                .row(index)
                .map(|(top, height)| (key, index, top, height));
        }
        let (rows, trees, keys) = builder.install(&placed, self.padding.left);
        *children = trees;
        self.built = rows;
        self.built_first = placed.first;
        state.revealed = match reveal {
            Some((key, index, _)) => placed
                .row(index)
                .map(|(_, height)| (key, height))
                .or(state.revealed),
            None => None,
        };
        if let Some((key, index, top, height)) = moved_to {
            state.position = Position::Row(state::Anchor {
                key,
                index,
                top,
                height,
                before: None,
                keep_bottom: false,
            });
        }
        if let Some(drawn) = drawn {
            let now = Instant::now();
            let duration = self.motion.duration(SLIDE);
            state
                .slides
                .moved(&drawn, &keys, &placed.tops, duration, now);
        }
        state.placed(placed, keys, count, frame);
        self.build_ghost(state, renderer, row_width, frame.view);
        self.see(state);
        if state.heights.overfull(count) {
            let keys: HashSet<u64> = (0..count).map(|index| self.source.key(index)).collect();
            state.heights.prune(|key| keys.contains(&key));
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        events::update(self, tree, event, layout, cursor, renderer, shell, viewport);
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        let extent = state.extent();
        let cursor = self.row_cursor(state, bounds, cursor);
        // iced draws a layer's quads, then its pictures, then its text, and
        // a parent layer before the layers opened in it: what lies over the
        // rows goes in layers opened after theirs, or the rows' text would
        // show through it.
        renderer.with_layer(clip, |renderer| {
            let rows = self.built.iter().zip(layout.iter(&tree.children));
            for (at, (row, (child, child_tree))) in rows.enumerate() {
                let child = drawn(child, state, at);
                if child.bounds().intersects(&clip) {
                    row.draw(child_tree, renderer, theme, style, child, cursor, &clip);
                }
            }
            if state.gesture.dragging() {
                renderer.with_layer(clip, |renderer| {
                    self.draw_drag(state, tree, renderer, theme, style, layout, &clip);
                });
            }
        });
        let bar = self.scrollbar && extent.scrolls();
        let marker = state.autoscroll.filter(|autoscroll| autoscroll.shown());
        if bar || marker.is_some() {
            // Over the window, as Slint's marker is (`ui/yapfast.slint:5652`).
            renderer.with_layer(*viewport, |renderer| {
                if bar {
                    state.bar.draw(renderer, theme, bounds, extent);
                }
                if let Some(autoscroll) = marker {
                    autoscroll.draw(renderer, theme);
                }
            });
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        if let Some(autoscroll) = state.autoscroll.filter(|autoscroll| autoscroll.shown()) {
            return autoscroll.interaction();
        }
        if state.gesture.dragging() {
            return mouse::Interaction::Grabbing;
        }
        if state.bar.is_dragged() || self.on_bar(state, bounds, cursor) {
            return mouse::Interaction::Idle;
        }
        let clip = bounds.intersection(viewport).unwrap_or(bounds);
        let cursor = self.row_cursor(state, bounds, cursor);
        self.built
            .iter()
            .zip(layout.iter(&tree.children))
            .enumerate()
            .map(|(at, (row, (child, child_tree)))| {
                row.mouse_interaction(child_tree, drawn(child, state, at), cursor, &clip, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let bounds = layout.bounds();
        let clip = bounds.intersection(viewport).unwrap_or_default();
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let extent = state.extent();
        operation.custom(self.id.as_ref(), bounds, state);
        operation.scrollable(
            self.id.as_ref(),
            bounds,
            Size::new(bounds.width, extent.maximum + extent.page),
            Vector::ZERO,
            state,
        );
        operation.container(self.id.as_ref(), bounds, &clip);
        operation.traverse(&mut |operation| {
            let rows = self.built.iter_mut().zip(layout.iter_mut(children));
            for (at, (row, (child, child_tree))) in rows.enumerate() {
                row.operate(
                    child_tree,
                    drawn(child, state, at),
                    &clip,
                    renderer,
                    operation,
                );
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let state = tree.state.downcast_ref::<State>();
        let clip = layout.bounds().intersection(viewport).unwrap_or(*viewport);
        self.built
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
            .enumerate()
            .flat_map(|(at, (row, (child, child_tree)))| {
                row.overlay(
                    child_tree,
                    drawn(child, state, at),
                    renderer,
                    &clip,
                    translation,
                    window,
                )
            })
            .collect()
    }
}
