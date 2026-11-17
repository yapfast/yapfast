//! A widget with something that opens from it: suggestions over a field, a
//! dropdown under its button, a menu over the composer's +. The popup is
//! drawn in iced's overlay layer, over everything else, placed by the
//! widget's own bounds on screen (above it or below it, as wide as it or at
//! its own width, its left or right edge level with the widget's), and kept
//! inside the window. While it's open it takes the pointer over it; a press
//! outside both says so, for a popup that closes then, and goes no further.

use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree},
    },
};

type Renderer = iced::Renderer;

/// Where the popup opens from the widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Over it, its foot `gap` above the widget's top; under it if there's
    /// no room.
    Above,
    /// Under it, unless there's more room over it.
    Below,
}

/// How wide the popup is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// The widget's own width.
    Same,
    /// Its own, laid out at most this wide, with its left edge level with
    /// the widget's (or its right, `Popover::right`).
    Own(f32),
}

/// The space kept between a popup and the window's edges.
const MARGIN: f32 = 8.0;

pub struct Popover<'a, M> {
    base: Element<'a, M>,
    popup: Option<Element<'a, M>>,
    side: Side,
    width: Width,
    gap: f32,
    right: bool,
    outside: Option<M>,
}

impl<'a, M: Clone> Popover<'a, M> {
    pub fn new(base: impl Into<Element<'a, M>>, popup: Option<Element<'a, M>>, side: Side) -> Self {
        Self {
            base: base.into(),
            popup,
            side,
            width: Width::Same,
            gap: 8.0,
            right: false,
            outside: None,
        }
    }

    pub fn width(mut self, width: Width) -> Self {
        self.width = width;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// The popup's right edge level with the widget's.
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }

    /// What a press outside the popup and the widget says.
    pub fn outside(mut self, message: M) -> Self {
        self.outside = Some(message);
        self
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Popover<'_, M> {
    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.base.as_widget().size_hint()
    }

    fn children(&self) -> Vec<Tree> {
        let mut children = vec![Tree::new(&self.base)];
        if let Some(popup) = &self.popup {
            children.push(Tree::new(popup));
        }
        children
    }

    fn diff(&self, tree: &mut Tree) {
        match &self.popup {
            Some(popup) => tree.diff_children(&[&self.base, popup]),
            None => tree.diff_children(std::slice::from_ref(&self.base)),
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.base
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.base
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.base.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.base.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
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
        self.base.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        let (base_tree, rest) = tree.children.split_first_mut()?;
        let inner =
            self.base
                .as_widget_mut()
                .overlay(base_tree, layout, renderer, viewport, translation);
        let popup = match (&mut self.popup, rest.first_mut()) {
            (Some(popup), Some(tree)) => Some(overlay::Element::new(Box::new(Open {
                popup,
                tree,
                base: layout.bounds() + translation,
                side: self.side,
                width: self.width,
                gap: self.gap,
                right: self.right,
                outside: self.outside.clone(),
            }))),
            _ => None,
        };
        match (inner, popup) {
            (None, None) => None,
            (Some(one), None) | (None, Some(one)) => Some(one),
            (Some(inner), Some(popup)) => {
                Some(overlay::Group::with_children(vec![inner, popup]).overlay())
            }
        }
    }
}

impl<'a, M: Clone + 'a> From<Popover<'a, M>> for Element<'a, M> {
    fn from(popover: Popover<'a, M>) -> Self {
        Element::new(popover)
    }
}

/// The open popup, in the overlay layer.
struct Open<'a, 'b, M> {
    popup: &'b mut Element<'a, M>,
    tree: &'b mut Tree,
    /// The widget's bounds in the window.
    base: Rectangle,
    side: Side,
    width: Width,
    gap: f32,
    right: bool,
    outside: Option<M>,
}

impl<M: Clone> overlay::Overlay<M, Theme, Renderer> for Open<'_, '_, M> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let base = self.base;
        let above = base.y - self.gap - MARGIN;
        let below = bounds.height - (base.y + base.height + self.gap) - MARGIN;
        let over = match self.side {
            Side::Above => above >= below.min(120.0),
            Side::Below => below < 120.0 && above > below,
        };
        let room = if over { above } else { below }.max(0.0);
        let (min, max) = match self.width {
            Width::Same => (base.width, base.width),
            Width::Own(width) => (0.0, width.min(bounds.width - 2.0 * MARGIN)),
        };
        let limits = layout::Limits::new(Size::new(min, 0.0), Size::new(max, room));
        let node = self
            .popup
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let size = node.size();
        let x = if self.right {
            base.x + base.width - size.width
        } else {
            base.x
        };
        let x = x
            .min(bounds.width - MARGIN - size.width)
            .max(MARGIN.min(base.x));
        let y = if over {
            base.y - self.gap - size.height
        } else {
            base.y + base.height + self.gap
        };
        node.move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        self.popup
            .as_widget()
            .draw(self.tree, renderer, theme, style, layout, cursor, &bounds);
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.popup
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
    ) {
        let bounds = layout.bounds();
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(outside) = &self.outside
            && !cursor.is_over(bounds)
            && !cursor.is_over(self.base)
        {
            // The press closes the popup and goes no further: a dialog
            // under the list stays, a switch under it doesn't flip.
            shell.publish(outside.clone());
            shell.capture_event();
            return;
        }
        self.popup.as_widget_mut().update(
            self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds,
        );
        // The popup takes presses on it from what's under it.
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && cursor.is_over(bounds)
        {
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        self.popup
            .as_widget()
            .mouse_interaction(self.tree, layout, cursor, &bounds, renderer)
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, M, Theme, Renderer>> {
        self.popup.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}
