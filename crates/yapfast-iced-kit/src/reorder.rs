//! A list whose rows are put in another order by dragging, as a server's
//! roles are: a row is taken by its handle (a strip at its left), follows
//! the pointer up or down over the others, lifted on a ground of its own,
//! and where it's let go, rounded to whole rows, is where it moves to. The
//! list only says so (`on_move`, from and to); its owner moves it. Rows
//! that may not move have no handle.

use iced::{
    Background, Border, Color, Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
};

type Renderer = iced::Renderer;

/// A list of rows that dragging reorders.
pub struct Reorder<'a, M> {
    rows: Vec<Element<'a, M>>,
    movable: Vec<bool>,
    handle: f32,
    lifted: Color,
    radius: f32,
    on_move: Box<dyn Fn(usize, usize) -> M + 'a>,
}

impl<'a, M: Clone> Reorder<'a, M> {
    /// `rows`, each `movable` or not; `handle` is how wide the strip at a
    /// row's left is that takes it; a row let go elsewhere sends
    /// `on_move(from, to)`.
    pub fn new(
        rows: Vec<Element<'a, M>>,
        movable: Vec<bool>,
        handle: f32,
        on_move: impl Fn(usize, usize) -> M + 'a,
    ) -> Self {
        Self {
            rows,
            movable,
            handle,
            lifted: Color::TRANSPARENT,
            radius: 0.0,
            on_move: Box::new(on_move),
        }
    }

    /// The ground a row being dragged is drawn on, with round corners.
    pub fn lifted(mut self, ground: Color, radius: f32) -> Self {
        self.lifted = ground;
        self.radius = radius;
        self
    }

    fn movable(&self, index: usize) -> bool {
        self.movable.get(index).copied().unwrap_or(false)
    }
}

/// The row being dragged: its place, where the press was, how far it has
/// come.
#[derive(Default)]
struct Dragging(Option<(usize, f32, f32)>);

impl<M: Clone> Widget<M, Theme, Renderer> for Reorder<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Dragging>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Dragging::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.rows.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.rows);
        // A list of another length isn't the one being dragged.
        let dragging = tree.state.downcast_mut::<Dragging>();
        if dragging
            .0
            .is_some_and(|(index, _, _)| index >= self.rows.len())
        {
            dragging.0 = None;
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let mut top = 0.0;
        let mut nodes = Vec::with_capacity(self.rows.len());
        for (row, tree) in self.rows.iter_mut().zip(&mut tree.children) {
            let limits =
                layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
            let node = row
                .as_widget_mut()
                .layout(tree, renderer, &limits)
                .move_to((0.0, top));
            top += node.size().height;
            nodes.push(node);
        }
        layout::Node::with_children(Size::new(width, top), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((row, tree), layout) in self
            .rows
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            row.as_widget_mut()
                .operate(tree, layout, renderer, operation);
        }
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
        let dragging = tree.state.downcast_mut::<Dragging>();
        if let Some((index, start, offset)) = &mut dragging.0 {
            let index = *index;
            match event {
                // The cursor, as the press's start: in a scrolled page or
                // under the own title bar the raw position is elsewhere.
                iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if let Some(position) = cursor.position() {
                        *offset = position.y - *start;
                    }
                    shell.request_redraw();
                    shell.capture_event();
                }
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | iced::Event::Mouse(mouse::Event::CursorLeft) => {
                    let height = layout
                        .children()
                        .nth(index)
                        .map_or(1.0, |row| row.bounds().height);
                    let to = target(index, *offset, height, self.rows.len());
                    dragging.0 = None;
                    shell.request_redraw();
                    shell.capture_event();
                    if to != index {
                        shell.publish((self.on_move)(index, to));
                    }
                }
                _ => {}
            }
            return;
        }
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(position) = cursor.position()
        {
            for (index, row) in layout.children().enumerate() {
                let bounds = row.bounds();
                let handle =
                    Rectangle::new(bounds.position(), Size::new(self.handle, bounds.height));
                if self.movable(index) && handle.contains(position) {
                    dragging.0 = Some((index, position.y, 0.0));
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
        }
        for ((row, tree), layout) in self
            .rows
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            row.as_widget_mut().update(
                tree, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
            if shell.is_event_captured() {
                return;
            }
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
        if tree.state.downcast_ref::<Dragging>().0.is_some() {
            return mouse::Interaction::Grabbing;
        }
        for (index, ((row, tree), layout)) in self
            .rows
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let bounds = layout.bounds();
            let handle = Rectangle::new(bounds.position(), Size::new(self.handle, bounds.height));
            if self.movable(index) && cursor.is_over(handle) {
                return mouse::Interaction::Grab;
            }
            let interaction = row
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer);
            if interaction != mouse::Interaction::None {
                return interaction;
            }
        }
        mouse::Interaction::None
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
        use iced::advanced::Renderer as _;
        let dragging = tree.state.downcast_ref::<Dragging>().0;
        // The rows between the dragged one's place and where it would go
        // move aside for it, by its height.
        let aside = dragging.and_then(|(dragged, _, offset)| {
            let height = layout.children().nth(dragged)?.bounds().height;
            let to = target(dragged, offset, height, self.rows.len());
            Some((dragged, to, height))
        });
        for (index, ((row, tree), layout)) in self
            .rows
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .enumerate()
        {
            let shift = match aside {
                Some((dragged, _, _)) if dragged == index => continue,
                Some((dragged, to, height)) if dragged < index && index <= to => -height,
                Some((dragged, to, height)) if to <= index && index < dragged => height,
                _ => 0.0,
            };
            if shift == 0.0 {
                row.as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, viewport);
            } else {
                renderer.with_translation(Vector::new(0.0, shift), |renderer| {
                    row.as_widget()
                        .draw(tree, renderer, theme, style, layout, cursor, viewport);
                });
            }
        }
        let Some((index, _, offset)) = dragging else {
            return;
        };
        let (Some(row), Some(tree), Some(layout)) = (
            self.rows.get(index),
            tree.children.get(index),
            layout.children().nth(index),
        ) else {
            return;
        };
        let moved = layout.bounds() + Vector::new(0.0, offset);
        renderer.with_layer(*viewport, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: moved,
                    border: Border::default().rounded(self.radius),
                    ..Default::default()
                },
                Background::Color(self.lifted),
            );
            renderer.with_translation(Vector::new(0.0, offset), |renderer| {
                row.as_widget()
                    .draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        iced::advanced::overlay::from_children(
            &mut self.rows,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: Clone + 'a> From<Reorder<'a, M>> for Element<'a, M> {
    fn from(list: Reorder<'a, M>) -> Self {
        Element::new(list)
    }
}

/// Where row `index`, dragged `offset` down, would go among `rows`:
/// rounded to whole rows of its `height`.
fn target(index: usize, offset: f32, height: f32, rows: usize) -> usize {
    let steps = (offset / height.max(1.0)).round() as i64;
    let last = rows.saturating_sub(1) as i64;
    (index as i64 + steps).clamp(0, last) as usize
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_row_goes_to_the_whole_row_it_is_nearest() {
        assert_eq!(super::target(2, 0.0, 40.0, 5), 2);
        assert_eq!(super::target(2, 59.0, 40.0, 5), 3, "past half a row");
        assert_eq!(super::target(2, -100.0, 40.0, 5), 0);
        assert_eq!(
            super::target(2, 400.0, 40.0, 5),
            4,
            "no further than the last"
        );
    }
}
