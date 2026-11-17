//! Keeping the focus inside the top layer, as the Slint layer's
//! `FocusScope` takes it:
//!
//! - An operation passed to what lies under it sees no focusable widget
//!   there, so `focus`, `focus_next` and `unfocus` neither reach nor change
//!   it. Everything else (scrolling, text, finding a widget) goes through as
//!   it was.
//! - What had the focus under it loses it while the layer is open, so its
//!   caret and ring don't show and it asks for no input method, and has it
//!   again once the layer closes. A layer closing puts its own focus away
//!   the same way, and has it back if it opens again.

use iced::advanced::Layout;
use iced::advanced::widget::operation::{Focusable, Outcome, Scrollable, Text, TextInput};
use iced::advanced::widget::{Id, Operation, Tree, Widget as _};
use iced::{Rectangle, Size, Vector};

use anomi_theme::Renderer;

use super::Layers;
use super::state::State;

/// An operation that skips focusable widgets.
pub(super) struct Unfocusable<'a>(pub &'a mut dyn Operation);

impl Operation for Unfocusable<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        self.0
            .traverse(&mut |operation| operate(&mut Unfocusable(operation)));
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, viewport: &Rectangle) {
        self.0.container(id, bounds, viewport);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Size,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        self.0.scrollable(id, bounds, content, translation, state);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn Focusable) {}

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
        self.0.text_input(id, bounds, state);
    }

    fn text(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Text) {
        self.0.text(id, bounds, state);
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        self.0.custom(id, bounds, state);
    }

    fn finish(&self) -> Outcome<()> {
        self.0.finish()
    }
}

/// The focus of the base or of a layer, while it's put away.
#[derive(Debug, Default)]
pub(super) struct Kept {
    away: bool,
    had: Option<Had>,
}

/// What had the focus: its id; for one without, its place on the screen
/// and among the focusable widgets, as the tree may change under a layer.
#[derive(Debug)]
struct Had {
    id: Option<Id>,
    bounds: Rectangle,
    at: usize,
}

/// Puts the focus away, or gives it back, when `away` changes; `operate`
/// runs an operation over what the focus is kept for.
fn turn(kept: &mut Kept, away: bool, operate: &mut dyn FnMut(&mut dyn Operation)) {
    if kept.away == away {
        return;
    }
    kept.away = away;
    if away {
        let mut put = PutAway { at: 0, had: None };
        operate(&mut put);
        kept.had = put.had;
        return;
    }
    let Some(had) = kept.had.take() else {
        return;
    };
    let target = match had.id {
        Some(id) => Target::Id(id),
        None => {
            let mut find = Find {
                at: 0,
                had,
                pick: None,
            };
            operate(&mut find);
            match find.pick {
                Some(at) => Target::At(at),
                None => return,
            }
        }
    };
    operate(&mut GiveBack { at: 0, target });
}

/// Unfocuses what has the focus, noting which it was.
struct PutAway {
    at: usize,
    had: Option<Had>,
}

impl Operation for PutAway {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            let at = self.at;
            self.had.get_or_insert_with(|| Had {
                id: id.cloned(),
                bounds,
                at,
            });
            state.unfocus();
        }
        self.at += 1;
    }
}

/// Finds the widget without an id that had the focus: where it was, at its
/// place among the others if that still holds.
struct Find {
    at: usize,
    had: Had,
    pick: Option<usize>,
}

impl Operation for Find {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if bounds == self.had.bounds && (self.pick.is_none() || self.at == self.had.at) {
            self.pick = Some(self.at);
        }
        self.at += 1;
    }
}

enum Target {
    Id(Id),
    At(usize),
}

/// Focuses again what had the focus.
struct GiveBack {
    at: usize,
    target: Target,
}

impl Operation for GiveBack {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        let it = match &self.target {
            Target::Id(target) => id == Some(target),
            Target::At(at) => self.at == *at,
        };
        if it {
            state.focus();
        }
        self.at += 1;
    }
}

impl<M> Layers<'_, M> {
    /// Puts away the focus of what the top layer covers and of a layer
    /// closing, and gives it back to what is uncovered again. Only a change
    /// does any work.
    pub(super) fn keep_focus(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let top = state.top(&self.layers);
        let mut children = layout.iter_mut(children);
        let (base_layout, base_tree) = children.next().expect("the base");
        turn(&mut state.base_focus, top.is_some(), &mut |operation| {
            self.base
                .operate(base_tree, base_layout, viewport, renderer, operation);
        });
        for (index, (layer, (child_layout, child_tree))) in
            self.layers.iter_mut().zip(children).enumerate()
        {
            let away = !state.inside(index, top);
            turn(&mut state.slots[index].focus, away, &mut |operation| {
                layer
                    .content
                    .operate(child_tree, child_layout, viewport, renderer, operation);
            });
        }
    }
}
