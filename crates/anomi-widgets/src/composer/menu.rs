//! The field's own menu: Discord's for a text field, without its spelling
//! suggestions and with Select All, which Discord's lacks (`edit-menu`,
//! `ui/composer-field.slint:194`). A right click opens it at the pointer,
//! above it as the field sits at the window's foot, below when the window's
//! top is too near; it grows in as menus do while [`Motion`] plays. A
//! choice, Escape or a press outside closes it; while it's open, keys that
//! would type are its own.
//!
//! It is the field's overlay rather than a layer of the window, as the
//! Slint one is the field's `PopupWindow`: what its rows do happens to the
//! field's own text.

use std::time::Instant;

use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget as _};
use iced::advanced::{Shell, mouse, overlay, shell};
use iced::{Event, Point, Rectangle, Size, Transformation, Vector, keyboard, touch, window};

use anomi_theme::motion::{Pace, Popup};
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::menu::{Row, Side, Spot};
use yapfast_lang::tr;

use super::Composer;
use super::state::State;
use crate::icon::Icon;
use crate::menu::{context_menu, place};

/// A row of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Choice {
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// Escape, or a row chosen.
    Close,
}

/// The menu while it's open.
pub(super) struct Open {
    /// Where it opened: the press, from the field's top left.
    pub at: Vector,
    pub rows: Vec<Row<Choice>>,
    /// The menu widget's state.
    pub tree: Tree,
    pub popup: Popup,
}

impl Open {
    /// The menu at `at`, its rows for whether text is selected.
    pub fn new(at: Vector, selection: bool, motion: Motion, now: Instant) -> Self {
        let rows = rows(selection);
        let tree = Tree::new(&context_menu(&rows, Spot::default(), motion));
        Self {
            at,
            rows,
            tree,
            popup: Popup::entering(Pace::Popover, motion, now),
        }
    }
}

/// Cut and Copy with a selection, then Paste and Select All
/// (`menu-rows`, `:78`).
fn rows(selection: bool) -> Vec<Row<Choice>> {
    let row = |text: &str, choice, icon: Icon| Row::new(text, choice).icon(icon.file());
    let mut rows = Vec::with_capacity(4);
    if selection {
        rows.push(row(tr!("Cut"), Choice::Cut, Icon::Scissors));
        rows.push(row(tr!("Copy"), Choice::Copy, Icon::Copy));
    }
    rows.push(row(tr!("Paste"), Choice::Paste, Icon::ClipboardPaste));
    rows.push(row(tr!("Select All"), Choice::SelectAll, Icon::TextSelect));
    rows
}

/// The menu over the window, acting on the field it belongs to.
pub(super) struct Overlay<'b, 'a, M> {
    pub composer: &'b Composer<'a, M>,
    pub state: &'b mut State,
    /// The field's top left in the window.
    pub origin: Point,
    pub window: Size,
}

/// Where the menu opens: at the press, above it.
fn spot(origin: Point, at: Vector) -> Spot {
    Spot {
        side: Side::Above,
        ..Spot::pointer(origin + at)
    }
}

impl<'b, 'a, M> Overlay<'b, 'a, M> {
    /// The menu of the field at `origin` in a window this big, laid out.
    pub fn new(
        composer: &'b Composer<'a, M>,
        state: &'b mut State,
        origin: Point,
        window: Size,
        renderer: &Renderer,
    ) -> Self {
        if let Some(open) = state.menu.as_mut() {
            let spot = spot(origin, open.at);
            let mut menu = context_menu(&open.rows, spot, composer.motion);
            menu.layout(&mut open.tree, renderer, &Limits::new(Size::ZERO, window));
        }
        Self {
            composer,
            state,
            origin,
            window,
        }
    }

    fn spot(&self, at: Vector) -> Spot {
        spot(self.origin, at)
    }

    fn viewport(&self) -> Rectangle {
        Rectangle::with_size(self.window)
    }
}

impl<M: Clone> overlay::Overlay<M, Theme, Renderer> for Overlay<'_, '_, M> {
    fn update(
        &mut self,
        event: &Event,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
    ) {
        let viewport = self.viewport();
        let motion = self.composer.motion;
        let Some(at) = self.state.menu.as_ref().map(|open| open.at) else {
            return;
        };
        let spot = self.spot(at);
        let Some(open) = self.state.menu.as_mut() else {
            return;
        };
        let mut chosen = shell::Bus::new();
        let (captured, redraw) = {
            let mut local = shell.local(&mut chosen);
            let mut menu = context_menu(&open.rows, spot, motion).on_close(Choice::Close);
            let layout = Layout::new(open.tree.size);
            menu.update(
                &mut open.tree,
                event,
                layout,
                cursor,
                renderer,
                &mut local,
                &viewport,
            );
            (local.is_event_captured(), local.redraw_request())
        };
        shell.request_redraw_at(redraw);
        if captured {
            shell.capture_event();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            self.state.now = *now;
            if open.popup.is_animating(*now) {
                shell.request_redraw();
            }
        }
        let mut choices: Vec<Choice> = chosen.drain().map(|(choice, _)| choice).collect();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. })
                if !captured =>
            {
                choices.push(Choice::Close);
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. })
                if !modifiers.control() && !modifiers.alt() && !modifiers.logo() =>
            {
                shell.capture_event();
            }
            _ => {}
        }
        for choice in choices {
            self.composer.choose(self.state, choice, shell);
        }
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        let Some(open) = self.state.menu.as_ref() else {
            return;
        };
        let spot = self.spot(open.at);
        let menu = context_menu(&open.rows, spot, self.composer.motion);
        let layout = Layout::new(open.tree.size);
        let viewport = self.viewport();
        let grow = place::near(spot);
        let scale = open.popup.scale(self.state.now);
        let transformation = Transformation::translate(grow.x, grow.y)
            * Transformation::scale(scale)
            * Transformation::translate(-grow.x, -grow.y);
        renderer.with_layer(viewport, |renderer| {
            renderer.with_transformation(transformation, |renderer| {
                menu.draw(
                    &open.tree, renderer, theme, style, layout, cursor, &viewport,
                );
            });
        });
    }

    fn mouse_interaction(&self, cursor: mouse::Cursor, renderer: &Renderer) -> mouse::Interaction {
        let Some(open) = self.state.menu.as_ref() else {
            return mouse::Interaction::None;
        };
        let menu = context_menu(&open.rows, self.spot(open.at), self.composer.motion);
        let layout = Layout::new(open.tree.size);
        menu.mouse_interaction(&open.tree, layout, cursor, &self.viewport(), renderer)
    }

    fn operate(&mut self, renderer: &Renderer, operation: &mut dyn Operation) {
        let viewport = self.viewport();
        let motion = self.composer.motion;
        let Some(at) = self.state.menu.as_ref().map(|open| open.at) else {
            return;
        };
        let spot = self.spot(at);
        let Some(open) = self.state.menu.as_mut() else {
            return;
        };
        let mut menu = context_menu(&open.rows, spot, motion);
        let layout = Layout::new(open.tree.size);
        menu.operate(&mut open.tree, layout, &viewport, renderer, operation);
    }
}
