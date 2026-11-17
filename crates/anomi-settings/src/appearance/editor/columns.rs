//! The colours' rows in one or two columns, read down: two when the width is
//! 520 px or more, with 24 px between, each row 46 px tall. The Slint side:
//! the `Rectangle` of `ThemeColourRow`s in `AppearanceSettings`
//! (`ui/settings.slint:1590`).

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

const WIDE: f32 = 520.0;
const GAP: f32 = 24.0;
const ROW: f32 = 46.0;

pub fn columns<'a, M: 'a>(rows: Vec<Element<'a, M>>) -> Element<'a, M> {
    Columns { rows }.boxed()
}

struct Columns<'a, M> {
    rows: Vec<Element<'a, M>>,
}

impl<M> widget::Meta for Columns<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Columns<'_, M> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.rows);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let width = limits.max.width;
        let count = if width >= WIDE { 2 } else { 1 };
        let rows = self.rows.len().div_ceil(count);
        let cell = (width - (count - 1) as f32 * GAP) / count as f32;
        let fixed = layout::Limits::new(Size::new(cell, ROW), Size::new(cell, ROW));
        for (index, (row, state)) in self.rows.iter_mut().zip(&mut tree.children).enumerate() {
            row.layout(state, renderer, &fixed);
            state.translation = Vector::new(
                (index / rows) as f32 * (cell + GAP),
                (index % rows) as f32 * ROW,
            );
        }
        tree.size = Size::new(width, rows as f32 * ROW);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            for (row, (layout, state)) in self
                .rows
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                row.operate(state, layout, viewport, renderer, operation);
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
        for (row, (layout, state)) in self
            .rows
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            row.update(state, event, layout, cursor, renderer, shell, viewport);
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
        self.rows
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(row, (layout, state))| {
                row.mouse_interaction(state, layout, cursor, viewport, renderer)
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
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for (row, (layout, state)) in self.rows.iter().zip(layout.iter(&tree.children)) {
            if layout.bounds().intersects(viewport) {
                row.draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        }
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
        overlay::from_children(
            &mut self.rows,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
