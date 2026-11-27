//! A virtualized list's rows, kept by key ([`Rows`]).

use std::{cell::RefCell, collections::HashMap};

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
};

use super::Renderer;

/// The rows of a list built near its viewport, each at its own place in the
/// list's full height (so no spacers), with their widget state kept by key.
/// iced's columns pair state with children by position: when the rows built
/// shift by one, as each new message does, every row got another row's
/// state, and every paragraph was shaped again; iced's keyed column only
/// follows one insertion or removal, not a window that slides.
///
/// A measured list ([`Rows::measured`]) lays each row out at the height its
/// content takes, stacking them down from the first row's place, and
/// reports the rows whose height differs from the one it was given, once
/// drawn, so the app can keep the real heights: a message's Markdown,
/// pictures and reactions are hard to know ahead.
pub struct Rows<'a, M> {
    keys: Vec<u64>,
    children: Vec<Element<'a, M>>,
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// A placed child's left and width ([`Rows::place`]); a row spans the
    /// list.
    spans: Vec<Option<(f32, f32)>>,
    total: f32,
    measured: Option<Measure<M>>,
}

/// How a measured list reports its rows' heights, by key.
pub type Report<M> = fn(Vec<(u64, f32)>) -> M;

/// Heights a measured list found, by key, for the app to take in.
pub type Found = std::rc::Rc<RefCell<Vec<(u64, f32)>>>;

/// How a list measures its rows: see [`Rows::measured`].
struct Measure<M> {
    report: Report<M>,
    found: Found,
    /// The viewport height to fill above a short list.
    viewport_height: f32,
    /// The row (by its place among those pushed) at the view's top while
    /// someone reads earlier ones; none while the list follows its end.
    pivot: Option<usize>,
}

/// What a list keeps between builds: its rows' keys, in order, and the
/// heights measured that differ from those given, not yet reported.
#[derive(Default)]
struct RowsState {
    keys: Vec<u64>,
    differ: Vec<(u64, f32)>,
    /// A list that shrank may fit now and stop reporting its viewport.
    shrank: bool,
}

impl<'a, M> Rows<'a, M> {
    pub fn new(total: f32) -> Self {
        Self {
            keys: Vec::new(),
            children: Vec::new(),
            tops: Vec::new(),
            heights: Vec::new(),
            spans: Vec::new(),
            total,
            measured: None,
        }
    }

    /// Rows take the height their content does. The list grows by what
    /// rows above `pivot` (all of them without one) grew, so what's in view
    /// stays put in a scrollable anchored to its end, and their heights go
    /// in `found` for the app to take in when it next updates, at no cost
    /// of a message and a view of their own. Rows from `pivot` on, from
    /// the top of the view of someone reading earlier ones, run on into
    /// the room below them instead, and their heights are reported with
    /// `report` once drawn, so the app can keep what's read in place.
    pub fn measured(
        mut self,
        report: Report<M>,
        found: Found,
        pivot: Option<usize>,
        viewport_height: f32,
    ) -> Self {
        self.measured = Some(Measure {
            report,
            found,
            pivot,
            viewport_height,
        });
        self
    }

    pub fn push(&mut self, key: u64, top: f32, height: f32, child: impl Into<Element<'a, M>>) {
        self.keys.push(key);
        self.tops.push(top);
        self.heights.push(height);
        self.spans.push(None);
        self.children.push(child.into());
    }

    /// A child in its own box, `at` from the list's corner, rather than a
    /// row across it: a picker's tile, beside others in its row or column.
    /// Not for a measured list.
    pub fn place(&mut self, key: u64, at: Rectangle, child: impl Into<Element<'a, M>>) {
        self.push(key, at.y, at.height, child);
        if let Some(span) = self.spans.last_mut() {
            *span = Some((at.x, at.width));
        }
    }
}

impl<M> Widget<M, Theme, Renderer> for Rows<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RowsState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(RowsState {
            keys: self.keys.clone(),
            ..RowsState::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let keys = &mut tree.state.downcast_mut::<RowsState>().keys;
        if *keys == self.keys {
            for (child, state) in self.children.iter().zip(&mut tree.children) {
                state.diff(child);
            }
            return;
        }
        let mut old: HashMap<u64, Tree> = keys.drain(..).zip(tree.children.drain(..)).collect();
        for (key, child) in self.keys.iter().zip(&self.children) {
            tree.children.push(match old.remove(key) {
                Some(mut state) => {
                    state.diff(child);
                    state
                }
                None => Tree::new(child),
            });
        }
        keys.clone_from(&self.keys);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        if let Some(measure) = &self.measured {
            let loose = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
            let pivot = measure.pivot.unwrap_or(usize::MAX);
            let mut differ = Vec::new();
            let mut found = Vec::new();
            let mut grown = 0.0;
            let mut next = self.tops.first().copied().unwrap_or(0.0);
            let mut nodes: Vec<_> = self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(self.keys.iter().zip(&self.heights))
                .enumerate()
                .map(|(place, ((child, state), (&key, &height)))| {
                    let node = child
                        .as_widget_mut()
                        .layout(state, renderer, &loose)
                        .move_to((0.0, next));
                    // Whole pixels: a row's height in halves would put the
                    // rows after it, and in a list held to its end every
                    // row, between pixels, where pictures and round
                    // corners take the renderer's slow way.
                    let measured = node.size().height.round();
                    if (measured - height).abs() > 0.5 {
                        if place < pivot {
                            grown += measured - height;
                            found.push((key, measured));
                        } else {
                            differ.push((key, measured));
                        }
                    }
                    next += measured;
                    node
                })
                .collect();
            if !found.is_empty() {
                measure.found.borrow_mut().extend(found);
            }
            let state = tree.state.downcast_mut::<RowsState>();
            state.differ = differ;
            state.shrank = grown < -0.5;
            let height = self.total + grown;
            let lead = (measure.viewport_height - height).max(0.0);
            if lead > 0.0 {
                for node in &mut nodes {
                    node.move_to_mut((0.0, node.bounds().y + lead));
                }
            }
            return layout::Node::with_children(Size::new(width, height + lead), nodes);
        }
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(self.tops.iter().zip(&self.heights).zip(&self.spans))
            .map(|((child, state), ((&top, &height), span))| {
                let (left, wide) = span.unwrap_or((0.0, width));
                let size = Size::new(wide, height);
                child
                    .as_widget_mut()
                    .layout(state, renderer, &layout::Limits::new(size, size))
                    .move_to((left, top))
            })
            .collect();
        layout::Node::with_children(Size::new(width, self.total), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        // The heights measured at the last layout, once it's drawn.
        if let Some(measure) = &self.measured
            && let iced::Event::Window(iced::window::Event::RedrawRequested(_)) = event
        {
            let state = tree.state.downcast_mut::<RowsState>();
            let differ = std::mem::take(&mut state.differ);
            let shrank = std::mem::take(&mut state.shrank);
            // A report also takes in the heights found when the list shrank
            // enough to stop sending scroll reports.
            if !differ.is_empty() || shrank {
                shell.publish((measure.report)(differ));
            }
        }
        // The last drawn first, as iced's stack does: a row's hover bar
        // floats over the row before it, and takes the pointer first.
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .rev()
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            if layout.bounds().intersects(viewport) {
                child
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        }
    }

    // A tooltip inside shows (iced asks each widget for its children's overlays).
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        iced::advanced::overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<Rows<'a, M>> for Element<'a, M> {
    fn from(rows: Rows<'a, M>) -> Self {
        Element::new(rows)
    }
}

#[cfg(test)]
mod tests {
    use iced::{Font, Pixels, advanced::renderer::Headless as _, widget::Space};

    use super::*;
    use crate::testing::Message;

    #[test]
    fn measured_rows_fill_the_viewport_using_their_laid_out_height() {
        let renderer = iced_test::futures::futures::executor::block_on(Renderer::new(
            Font::DEFAULT,
            Pixels(16.0),
            None,
        ))
        .expect("a headless renderer");
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, f32::INFINITY));
        for (estimate, actual, height, top) in
            [(300.0, 80.0, 200.0, 120.0), (80.0, 300.0, 300.0, 0.0)]
        {
            let mut rows =
                Rows::new(estimate).measured(|_| Message::Nothing, Found::default(), None, 200.0);
            rows.push(1, 0.0, estimate, Space::new().height(actual));
            let mut tree = Tree::new(&rows as &dyn Widget<_, _, _>);
            let node = rows.layout(&mut tree, &renderer, &limits);
            assert_eq!(node.size().height, height);
            assert_eq!(node.children()[0].bounds().y, top);
        }
    }
}
