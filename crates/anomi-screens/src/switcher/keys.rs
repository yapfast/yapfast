//! The keys that choose a row while the field has the focus. Slint's
//! `capture-key-pressed` on the dialog's `FocusScope` (`ui/dialogs.slint:412`):
//! Down and Tab choose the next place, Up and Shift+Tab the previous. The
//! text field would leave these to the focus walk (Tab) or ignore them, so
//! this wrapper takes them first, and takes them even with no place to
//! choose, as Slint does, so Tab never wanders off to a button behind.
//! Enter goes to the place last chosen. The wrapper keeps that place itself,
//! as a text input keeps its value: iced runs every event that arrived
//! together against one view, so a second key must step from the first's
//! result, not from the place the view was built with. Everything else,
//! Escape included, goes to what it wraps.

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

/// Content whose Up, Down and Tab choose rows and Enter goes; see the module.
pub struct Keys<'a, M> {
    content: Element<'a, M>,
    places: usize,
    current: Option<usize>,
    on_choose: Box<dyn Fn(usize) -> M + 'a>,
    on_go: Box<dyn Fn(usize) -> M + 'a>,
}

impl<'a, M> Keys<'a, M> {
    /// `places` rows with `current` the one chosen (`None` with no rows);
    /// `on_choose` and `on_go` make the messages for a place.
    pub fn new(
        content: Element<'a, M>,
        places: usize,
        current: Option<usize>,
        on_choose: impl Fn(usize) -> M + 'a,
        on_go: impl Fn(usize) -> M + 'a,
    ) -> Self {
        Self {
            content,
            places,
            current,
            on_choose: Box::new(on_choose),
            on_go: Box::new(on_go),
        }
    }
}

/// The place after `step` from `at` among `places`, wrapping.
pub fn stepped(at: usize, places: usize, step: Step) -> usize {
    match step {
        Step::Next if at + 1 < places => at + 1,
        Step::Next => 0,
        Step::Previous => at.checked_sub(1).unwrap_or(places - 1),
    }
}

impl<M> widget::Meta for Keys<'_, M> {}

/// The place last chosen, and the view's inputs it was set from: a view
/// built with other inputs starts over from its own current place.
struct State {
    at: usize,
    seen: (usize, Option<usize>),
}

impl<M> widget::Widget<M, Theme, Renderer> for Keys<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            at: self.current.unwrap_or(0),
            seen: (self.places, self.current),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        let seen = (self.places, self.current);
        if state.seen != seen {
            state.seen = seen;
            state.at = self.current.unwrap_or(0);
        }
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
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
            let state = tree.state.downcast_mut::<State>();
            if let Some(step) = step(key, *modifiers) {
                if self.places > 0 {
                    state.at = stepped(state.at.min(self.places - 1), self.places, step);
                    shell.publish((self.on_choose)(state.at));
                }
                shell.capture_event();
                return;
            }
            if matches!(key.as_ref(), Key::Named(Named::Enter)) && self.places > 0 {
                shell.publish((self.on_go)(state.at.min(self.places - 1)));
                shell.capture_event();
                return;
            }
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
