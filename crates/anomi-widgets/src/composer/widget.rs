//! The field as a widget of its own: its state in the tree, the command's
//! pills as its one child, its height from its text
//! (`height: root.pills + max(48px, min(…))`, `ui/composer-field.slint:86`),
//! the focus and the caret given to the window's operations, the edit menu
//! as its overlay.

use iced::advanced::Renderer as _;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer;
use iced::advanced::text::editor::Editor as _;
use iced::advanced::text::{Alignment, LineHeight, Wrapping};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Pixels, Rectangle, Size, Vector, touch};

use anomi_theme::{Renderer, Theme, typography};

use super::menu::Overlay;
use super::state::State;
use super::{Composer, INSET, PILLS_GAP, PILLS_RAISE, TEXT_LINE};

impl<M> widget::Meta for Composer<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Composer<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.text, self.key))
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.state.downcast_mut::<State>().sync(self.text, self.key);
        tree.diff_children(self.pills.as_mut_slice());
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) {
        let width = limits.width(Length::Fill).bounds().width;
        let text_width = (width - 2.0 * INSET).max(0.0);
        let pills = match (self.pills.as_mut(), tree.children.first_mut()) {
            (Some(pills), Some(child)) => {
                let room = Size::new(text_width, f32::INFINITY);
                pills.layout(child, renderer, &Limits::new(Size::ZERO, room));
                child.translation = Vector::new(INSET, self.pad - PILLS_RAISE);
                child.size.height + PILLS_GAP
            }
            _ => 0.0,
        };
        let tallest = self.tallest();
        let state = tree.state.downcast_mut::<State>();
        let editor = state.editor.get_mut();
        editor.update(
            Size::new(text_width, tallest),
            typography::UI,
            Pixels(self.size),
            LineHeight::Relative(TEXT_LINE),
            Wrapping::WordOrGlyph,
            Alignment::Default,
            renderer.hint_factor(),
            state.marker.get_mut(),
        );
        let content = editor.min_bounds().height;
        let body = (content.min(tallest) + 2.0 * self.pad).max(self.min_height);
        state.pills = pills;
        state.visible = body - 2.0 * self.pad;
        state.overflows = content >= tallest - 0.5;
        if std::mem::take(&mut state.reveal) {
            state.show_caret(state.visible.min(tallest));
        }
        tree.size = Size::new(width, pills + body);
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
        // A widget before the field may have taken the event already: only
        // the pills' own capture keeps it from the field.
        let captured = shell.is_event_captured();
        if let (Some(pills), Some((child_layout, child))) = (
            self.pills.as_mut(),
            layout.iter_mut(&mut tree.children).next(),
        ) {
            pills.update(
                child,
                event,
                child_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
            if !captured && shell.is_event_captured() {
                // A pill clicked takes the field to it (`clicked(index)`,
                // `:91`): the field has the keys.
                let state = tree.state.downcast_mut::<State>();
                if self.enabled()
                    && matches!(
                        event,
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                            | Event::Touch(touch::Event::FingerPressed { .. })
                    )
                {
                    Focusable::focus(&mut state.keys);
                }
                self.settle(state, shell);
                return;
            }
        }
        let state = tree.state.downcast_mut::<State>();
        self.handle(state, event, layout.bounds(), cursor, shell);
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
        if let (Some(pills), Some((child_layout, child))) =
            (self.pills.as_ref(), layout.iter(&tree.children).next())
        {
            pills.draw(
                child,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
        }
        self.paint(state, renderer, theme, layout.bounds(), cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if let (Some(pills), Some((child_layout, child))) =
            (self.pills.as_ref(), layout.iter(&tree.children).next())
        {
            let over = pills.mouse_interaction(child, child_layout, cursor, viewport, renderer);
            if over != mouse::Interaction::None {
                return over;
            }
        }
        match (cursor.is_over(layout.bounds()), self.enabled()) {
            (true, true) => mouse::Interaction::Text,
            (true, false) => mouse::Interaction::Idle,
            (false, _) => mouse::Interaction::None,
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
        let bounds = layout.bounds();
        let id = self.id.as_ref();
        let state = tree.state.downcast_mut::<State>();
        if self.enabled() {
            operation.focusable(id, bounds, &mut state.keys);
            operation.text_input(id, bounds, state.editor.get_mut());
        }
        operation.custom(id, bounds, state);
        if let (Some(pills), Some((child_layout, child))) = (
            self.pills.as_mut(),
            layout.iter_mut(&mut tree.children).next(),
        ) {
            operation.traverse(&mut |operation| {
                pills.operate(child, child_layout, viewport, renderer, operation);
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        if state.menu.is_none() {
            return Vec::new();
        }
        let origin = layout.position() + translation;
        let menu = Overlay::new(&*self, state, origin, window, renderer);
        vec![overlay::Element::new(Box::new(menu))]
    }
}
