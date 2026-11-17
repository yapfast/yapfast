//! The keys that choose a row while the field has the focus. Slint's
//! `capture-key-pressed` on the dialog's `FocusScope` (`ui/dialogs.slint:412`):
//! Down and Tab choose the next place, Up and Shift+Tab the previous. The
//! text field would leave these to the focus walk (Tab) or ignore them, so
//! this wrapper takes them first, and takes them even with no place to
//! choose, as Slint does, so Tab never wanders off to a button behind.
//! Everything else, Enter and Escape included, goes to what it wraps.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

/// What a key chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Previous,
}

/// The step a key takes, if any.
pub fn step(key: &Key, modifiers: keyboard::Modifiers) -> Option<Step> {
    match key.as_ref() {
        Key::Named(Named::ArrowDown) => Some(Step::Next),
        Key::Named(Named::ArrowUp) => Some(Step::Previous),
        Key::Named(Named::Tab) if modifiers.shift() => Some(Step::Previous),
        Key::Named(Named::Tab) => Some(Step::Next),
        _ => None,
    }
}

/// Content whose Up, Down and Tab choose rows; see the module.
pub struct Keys<'a, M> {
    content: Element<'a, M>,
    next: Option<M>,
    previous: Option<M>,
}

impl<'a, M> Keys<'a, M> {
    /// What each key asks for; `None` for a key with nothing to choose,
    /// which is still taken.
    pub fn new(content: Element<'a, M>, next: Option<M>, previous: Option<M>) -> Self {
        Self {
            content,
            next,
            previous,
        }
    }
}

impl<M> widget::Meta for Keys<'_, M> {}

/// Keys keeps nothing of its own: its place in the tree.
struct Marker;

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Keys<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Marker>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Marker)
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
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
            let (layout, tree) = layout
                .iter_mut(&mut tree.children)
                .next()
                .expect("the content");
            self.content
                .operate(tree, layout, viewport, renderer, operation);
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
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && let Some(step) = step(key, *modifiers)
        {
            let message = match step {
                Step::Next => &self.next,
                Step::Previous => &self.previous,
            };
            if let Some(message) = message {
                shell.publish(message.clone());
            }
            shell.capture_event();
            return;
        }
        // A press outside the field would make it let go of the focus; in
        // Slint the card's areas take no focus, so the field keeps it.
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_)))
            && !cursor.is_over(layout.bounds())
        {
            return;
        }
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard::Modifiers;

    #[test]
    fn four_keys_choose() {
        let key = |named| Key::Named(named);
        assert_eq!(
            step(&key(Named::ArrowDown), Modifiers::empty()),
            Some(Step::Next)
        );
        assert_eq!(
            step(&key(Named::ArrowUp), Modifiers::empty()),
            Some(Step::Previous)
        );
        assert_eq!(step(&key(Named::Tab), Modifiers::empty()), Some(Step::Next));
        assert_eq!(
            step(&key(Named::Tab), Modifiers::SHIFT),
            Some(Step::Previous)
        );
        assert_eq!(step(&key(Named::Enter), Modifiers::empty()), None);
    }
}
