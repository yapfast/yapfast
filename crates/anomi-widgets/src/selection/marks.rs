//! What a selection finds in its content: [`row`] marks a list's row, by
//! its index and key (Slint found it as the repeater's instance inside the
//! flickable, `place`); [`whole`] a text drawn by a widget that selects
//! nothing of itself, as a styled name, which a selection covers and copies
//! whole; [`picture`] a picture, copied as `[Picture]` and tinted while
//! covered (`is_picture` and `tint_if_selected`, `items/image.rs:158`).
//! Each draws, lays out and answers as its content does.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, Widget as _, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};

use super::act::{CLEAR, UNKNOWN};

/// How strongly a covered picture is tinted: more than text's highlight,
/// to show over light pictures (`tint_if_selected`, `items/image.rs:167`).
const PICTURE_TINT: f32 = 0.5;

/// What a mark holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A text that selects itself.
    Text,
    /// A text covered and copied whole.
    Whole,
    /// A picture.
    Picture,
}

/// A row's state: what it tells a selection's walk, the version of the
/// selection it shows (none, built anew), and the fingerprint of the acts
/// its parts were last given ([`super::act::CLEAR`], built anew).
pub struct RowMark {
    pub index: usize,
    pub key: u64,
    pub shown: u64,
    pub acts: u64,
}

/// A whole text's or a picture's state: whether a selection covers it.
#[derive(Debug)]
pub struct Cover {
    pub kind: Kind,
    pub on: bool,
}

/// A list's row, for a selection: see [`row`].
pub struct Row<'a, Message> {
    index: usize,
    key: u64,
    content: Element<'a, Message>,
}

/// Marks `content` as the list's row `index`, keyed `key` as the list keys
/// it, so a selection running through it keeps its place there.
pub fn row<'a, Message>(
    index: usize,
    key: u64,
    content: impl View<Message> + 'a,
) -> Row<'a, Message> {
    Row {
        index,
        key,
        content: content.boxed(),
    }
}

/// A whole text or a picture, for a selection: see [`whole`], [`picture`].
pub struct Mark<'a, Message> {
    kind: Kind,
    radius: f32,
    content: Element<'a, Message>,
}

/// Marks `content`, a text drawn by a widget that selects nothing of
/// itself (a styled name), as one a selection covers and copies whole, by
/// the words of the texts inside it joined by spaces. A press on it starts
/// a selection, as on any text.
pub fn whole<'a, Message>(content: impl View<Message> + 'a) -> Mark<'a, Message> {
    Mark {
        kind: Kind::Whole,
        radius: 0.0,
        content: content.boxed(),
    }
}

/// Marks `content` as a picture, which a selection copies as `[Picture]`
/// and tints while it covers it.
pub fn picture<'a, Message>(content: impl View<Message> + 'a) -> Mark<'a, Message> {
    Mark {
        kind: Kind::Picture,
        radius: 0.0,
        content: content.boxed(),
    }
}

impl<Message> Mark<'_, Message> {
    /// The corners the tint rounds, as the picture's.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// Lays `content` out as its holder, at its own size.
fn lay_out<Message>(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    renderer: &Renderer,
    limits: &layout::Limits,
) {
    content.layout(&mut tree.children[0], renderer, limits);
    tree.children[0].translation = Vector::ZERO;
    tree.size = tree.children[0].size;
}

/// The holder's one child, with its layout.
fn only(layout: Layout, tree: &Tree) -> (Layout, &Tree) {
    layout
        .iter(&tree.children)
        .next()
        .expect("a mark holds one")
}

macro_rules! forward {
    () => {
        fn diff(&mut self, tree: &mut Tree) {
            tree.diff_children(std::slice::from_mut(&mut self.content));
        }

        fn size(&self) -> Size<Length> {
            self.content.size()
        }

        fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
            lay_out(&mut self.content, tree, renderer, limits);
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
            let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("one");
            self.content
                .update(child, event, layout, cursor, renderer, shell, viewport);
        }

        fn mouse_interaction(
            &self,
            tree: &Tree,
            layout: Layout,
            cursor: mouse::Cursor,
            viewport: &Rectangle,
            renderer: &Renderer,
        ) -> mouse::Interaction {
            let (layout, child) = only(layout, tree);
            self.content
                .mouse_interaction(child, layout, cursor, viewport, renderer)
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
            let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("one");
            self.content
                .overlay(child, layout, renderer, viewport, translation, window)
        }
    };
}

impl<Message> widget::Meta for Row<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Row<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RowMark>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(RowMark {
            index: self.index,
            key: self.key,
            shown: 0,
            acts: CLEAR,
        })
    }

    forward!();

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
        let (layout, child) = only(layout, tree);
        self.content
            .draw(child, renderer, theme, style, layout, cursor, viewport);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let mark = state.downcast_mut::<RowMark>();
        if mark.key != self.key {
            // Another row's state: it shows nothing of the selection, and
            // its texts may show anything.
            mark.shown = 0;
            mark.acts = UNKNOWN;
        }
        (mark.index, mark.key) = (self.index, self.key);
        operation.custom(None, layout.bounds(), mark);
        operation.traverse(&mut |operation| {
            let (layout, child) = layout.iter_mut(children).next().expect("one");
            self.content
                .operate(child, layout, viewport, renderer, operation);
        });
    }
}

impl<Message> widget::Meta for Mark<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Mark<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Cover>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Cover {
            kind: self.kind,
            on: false,
        })
    }

    forward!();

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
        let covered = tree.state.downcast_ref::<Cover>().on;
        let highlight = <Theme as iced::widget::text::Catalog>::selection(theme);
        let paint = |renderer: &mut Renderer, color: Color| {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border::default().rounded(self.radius),
                    ..Quad::default()
                },
                color,
            );
        };
        if covered && self.kind == Kind::Whole {
            paint(renderer, highlight);
        }
        let (child_layout, child) = only(layout, tree);
        self.content.draw(
            child,
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
        if covered && self.kind == Kind::Picture {
            paint(
                renderer,
                Color {
                    a: PICTURE_TINT,
                    ..highlight
                },
            );
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let cover = state.downcast_mut::<Cover>();
        cover.kind = self.kind;
        operation.custom(None, layout.bounds(), cover);
        operation.traverse(&mut |operation| {
            let (layout, child) = layout.iter_mut(children).next().expect("one");
            self.content
                .operate(child, layout, viewport, renderer, operation);
        });
    }
}
