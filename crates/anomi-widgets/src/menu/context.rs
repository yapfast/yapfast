//! A menu of choices, as a right click or a menu button opens, and the
//! submenus its rows open beside it: `ContextMenu` (`ui/controls.slint:1332`).
//!
//! It fills the window (a layer's) and places itself: the menu at its
//! [`Spot`], below the pointer or above it, kept 8 px inside the window and
//! scrolling when taller; a submenu beside its row, on the menu's right or,
//! without room there, its left, moved up to stay in the window. Each is as
//! wide as its rows' text within 255 and 320 px. A submenu fades in over
//! `QUICK`; the menu's own way in and out is its layer's.
//!
//! It takes the keyboard (see [`super::keys`]). A row with a submenu opens
//! it on hover; clicked, it opens it too, unless the row has an intent of
//! its own, which it then sends. Choosing a row sends its intent and then
//! `on_close`; a box to tick sends its intent and the menu stays; a
//! disabled row sends nothing. A click outside the panels isn't the menu's:
//! the layer under it closes the menu.

mod act;
mod state;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector, keyboard, touch, window};

use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::menu::{Row, Spot};

use self::state::{State, Target};
use super::keys::Move;
use super::panel::{NARROWEST, Panel};
use super::place::{self, MARGIN};
use super::row::{Look, Paint};

/// Between the menu and a submenu (`root.menu-width + 4px`, `:1490`).
const GAP: f32 = 4.0;

pub struct ContextMenu<'a, M> {
    rows: &'a [Row<M>],
    spot: Spot,
    narrowest: f32,
    headed: bool,
    shown: Option<usize>,
    on_close: Option<M>,
    motion: Motion,
}

/// A menu of `rows` at `spot`, in the window's coordinates.
pub fn context_menu<M>(rows: &[Row<M>], spot: Spot, motion: Motion) -> ContextMenu<'_, M> {
    ContextMenu {
        rows,
        spot,
        narrowest: NARROWEST,
        headed: false,
        shown: None,
        on_close: None,
        motion,
    }
}

impl<'a, M> ContextMenu<'a, M> {
    /// The menu's and its submenus' least width: 255 px unless set.
    pub fn narrowest(mut self, width: f32) -> Self {
        self.narrowest = width;
        self
    }

    /// Something sits right on top of the menu as one card: its top
    /// corners are square.
    pub fn headed(mut self, headed: bool) -> Self {
        self.headed = headed;
        self
    }

    /// The row whose submenu shows from the start, as a shot of an open
    /// submenu needs.
    pub fn submenu_shown(mut self, row: Option<usize>) -> Self {
        self.shown = row;
        self
    }

    /// What closing it sends: after a choice, and on Escape.
    pub fn on_close(mut self, message: M) -> Self {
        self.on_close = Some(message);
        self
    }

    fn submenu(&self, row: usize) -> &'a [Row<M>] {
        self.rows.get(row).map_or(&[], |row| row.submenu.as_slice())
    }
}

impl<M> widget::Meta for ContextMenu<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for ContextMenu<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::showing(self.shown))
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let window = limits.bounds();
        tree.size = window;
        let state = tree.state.downcast_mut::<State>();
        state.measure(self.rows, self.narrowest, window.height - 2.0 * MARGIN);
        let main = &mut state.main;
        main.bounds = place::place(self.spot, main.bounds.size(), window, MARGIN).bounds;
        let widest = state
            .subs
            .iter()
            .map(|sub| sub.bounds.width)
            .fold(0.0, f32::max);
        let main = state.main.bounds;
        state.flip = widest > 0.0 && main.x + main.width + GAP + widest > window.width - MARGIN;
        state.window = window;
        let len = self.rows.len();
        let nav = &mut state.nav;
        nav.current = nav.current.filter(|&row| row < len);
        nav.open = nav.open.filter(|&row| !self.submenu(row).is_empty());
        if nav.open.is_none() {
            nav.sub = None;
        }
        state.place_sub();
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let point = cursor.position().map(|point| point - origin);
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                let target = point.and_then(|point| state.target(point));
                if target != state.pointer {
                    state.pointer = target;
                    self.hover(state, target);
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if point.is_some_and(|point| state.over(point)) {
                    if *button == mouse::Button::Left {
                        state.pressed = point.and_then(|point| state.target(point));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                let pressed = state.pressed.take();
                let target = point.and_then(|point| state.target(point));
                if let Some(target) = target.filter(|_| pressed == target) {
                    self.act(state, target, shell);
                }
                if point.is_some_and(|point| state.over(point)) {
                    shell.capture_event();
                }
            }
            Event::Touch(touch::Event::FingerPressed { .. }) => {
                if point.is_some_and(|point| state.over(point)) {
                    state.pressed = point.and_then(|point| state.target(point));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(point) = point.filter(|point| state.over(*point)) else {
                    return;
                };
                let in_sub = state.sub().is_some_and(|sub| sub.bounds.contains(point));
                let moved = match state.nav.open {
                    Some(open) if in_sub => state.subs[open].wheel(*delta),
                    _ => state.main.wheel(*delta),
                };
                if moved && !in_sub {
                    state.place_sub();
                }
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if let Some(key) = Move::of(key, *modifiers, state.flip) {
                    self.key(state, key, shell);
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if self.tick(state, *now) || state.fade.is_animating(*now) {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let ticked = |index: usize, checked: bool| {
            state
                .ticks
                .get(index)
                .map_or(if checked { 1.0 } else { 0.0 }, |tick| {
                    tick.interpolate(0.0, 1.0, state.now)
                })
        };
        renderer.with_translation(origin, |renderer| {
            state
                .main
                .draw(renderer, theme, self.rows, self.headed, 1.0, |index| {
                    Paint {
                        lit: state.nav.lit(index),
                        arrow: !self.submenu(index).is_empty(),
                        turned: 0.0,
                        ticked: ticked(index, self.rows[index].checked),
                        focused: false,
                        alpha: 1.0,
                        look: Look::Menu,
                    }
                });
            let (Some(open), Some(sub)) = (state.nav.open, state.sub()) else {
                return;
            };
            let rows = self.submenu(open);
            let alpha = state.fade.interpolate(0.0, 1.0, state.now);
            let first = state.main.len();
            renderer.with_layer(Rectangle::with_size(state.window), |renderer| {
                sub.draw(renderer, theme, rows, false, alpha, |index| Paint {
                    lit: state.nav.sub == Some(index),
                    arrow: false,
                    turned: 0.0,
                    ticked: ticked(first + index, rows[index].checked),
                    focused: false,
                    alpha,
                    look: Look::Menu,
                });
            });
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let Some(point) = cursor.position().map(|point| point - origin) else {
            return mouse::Interaction::None;
        };
        let row = match state.target(point) {
            Some(Target::Main(index)) => self.rows.get(index),
            Some(Target::Sub(index)) => state
                .nav
                .open
                .and_then(|open| self.submenu(open).get(index)),
            None => None,
        };
        match row {
            Some(row) if !row.disabled => mouse::Interaction::Pointer,
            _ if state.over(point) => mouse::Interaction::Idle,
            _ => mouse::Interaction::None,
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        // Each shown row's text, so a test clicks a row by what it says.
        let state = tree.state.downcast_ref::<State>();
        let origin = Vector::new(layout.position().x, layout.position().y);
        let mut report = |panel: &Panel, rows: &[Row<M>]| {
            for (index, row) in rows.iter().enumerate().take(panel.len()) {
                let bounds = panel.row(index);
                if let Some(shown) = bounds.intersection(&panel.bounds) {
                    operation.text(None, shown + origin, &mut row.text.clone());
                }
            }
        };
        report(&state.main, self.rows);
        if let (Some(open), Some(sub)) = (state.nav.open, state.sub()) {
            report(sub, self.submenu(open));
        }
    }
}
