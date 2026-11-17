//! The card's inside: the way in on the left, a rule, the preview on the
//! right, laid out as Slint's row is (`ui/sign-in.slint:54-61`). iced's row
//! has one cross alignment and caps a fill-height child at its static
//! siblings, so this lays both sides out at their natural heights. The card
//! is as tall as the taller side. The left column, when it is the shorter,
//! stretches to that height and shares what is left equally among its
//! growable parts (each draws at the top of its share, as Slint's text
//! rows do). The preview is centred.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

/// One part of the left column: it takes a share of the leftover height
/// when `grows`.
pub type Part<'a, M> = (Element<'a, M>, bool);

pub struct Sides<'a, M> {
    /// The left column's parts, then the rule, then the preview.
    parts: Vec<Element<'a, M>>,
    grows: Vec<bool>,
    spacing: f32,
    gap: f32,
    right: f32,
    /// The inside width the card wants, and the least the left column keeps
    /// (`min-width`): the card is wider than the want when its parts need it.
    want: f32,
    min_left: f32,
    /// A copy of the widest row that never wraps, laid out only to learn
    /// the room it needs; it is the last of `parts` and is never drawn.
    floored: bool,
}

/// `left` stacked `spacing` apart, `gap` to the `rule`, `gap` again to the
/// `preview`, which is `right` wide.
pub fn sides<'a, M>(
    left: Vec<Part<'a, M>>,
    spacing: f32,
    rule: Element<'a, M>,
    preview: Element<'a, M>,
    gap: f32,
    right: f32,
    want: f32,
    min_left: f32,
    floor: Option<Element<'a, M>>,
) -> Sides<'a, M> {
    let (mut parts, grows): (Vec<_>, Vec<_>) = left.into_iter().unzip();
    parts.push(rule);
    parts.push(preview);
    let floored = floor.is_some();
    parts.extend(floor);
    Sides {
        parts,
        grows,
        spacing,
        gap,
        right,
        want,
        min_left,
        floored,
    }
}

impl<M> Sides<'_, M> {
    /// The parts that are drawn and answer events: all but the floor's copy.
    fn live(&self) -> usize {
        self.parts.len() - usize::from(self.floored)
    }
}

impl<M> widget::Meta for Sides<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Sides<'_, M> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, _limits: &layout::Limits) {
        let n = self.grows.len();
        let rule_at = self.gap;
        let mut min_left = self.min_left;
        if self.floored {
            let unbounded = layout::Limits::new(Size::ZERO, Size::INFINITE);
            self.parts[n + 2].layout(&mut tree.children[n + 2], renderer, &unbounded);
            min_left = min_left.max(tree.children[n + 2].size.width);
        }
        let width = self.want.max(min_left + self.right + 1.0 + 2.0 * rule_at);
        let left_width = width - self.right - 1.0 - 2.0 * rule_at;
        let loose = |width: f32| layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let mut left = 0.0;
        for (i, part) in self.parts[..n].iter_mut().enumerate() {
            part.layout(&mut tree.children[i], renderer, &loose(left_width));
            left += tree.children[i].size.height;
        }
        left += self.spacing * n.saturating_sub(1) as f32;
        self.parts[n + 1].layout(&mut tree.children[n + 1], renderer, &loose(self.right));
        let preview = tree.children[n + 1].size.height;
        let height = left.max(preview);
        let rule = Size::new(1.0, height);
        self.parts[n].layout(
            &mut tree.children[n],
            renderer,
            &layout::Limits::new(rule, rule),
        );
        let growing = self.grows.iter().filter(|grows| **grows).count();
        let share = if growing > 0 {
            (height - left).max(0.0) / growing as f32
        } else {
            0.0
        };
        let mut y = 0.0;
        for i in 0..n {
            tree.children[i].translation = Vector::new(0.0, y);
            y += tree.children[i].size.height + self.spacing;
            if self.grows[i] {
                y += share;
            }
        }
        tree.children[n].translation = Vector::new(left_width + self.gap, 0.0);
        tree.children[n + 1].translation =
            Vector::new(left_width + 2.0 * self.gap + 1.0, (height - preview) / 2.0);
        tree.size = Size::new(width, height);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            let live = self.live();
            for (part, (layout, tree)) in self.parts[..live]
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                part.operate(tree, layout, viewport, renderer, operation);
            }
        });
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
        let live = self.live();
        for (part, (layout, tree)) in self.parts[..live]
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            part.update(tree, event, layout, cursor, renderer, shell, viewport);
        }
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
        for (part, (layout, tree)) in self.parts[..self.live()]
            .iter()
            .zip(layout.iter(&tree.children))
        {
            part.draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        self.parts[..self.live()]
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(part, (layout, tree))| {
                part.mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        let live = self.live();
        overlay::from_children(
            &mut self.parts[..live],
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
