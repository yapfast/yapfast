//! [`Pointed`] as a widget: its parts' layout, the pointer and the keys,
//! and its overlays; `paint` draws it.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::{Element, Renderer, Theme};

use super::state::State;
use super::{CHEVRON, CHEVRON_GAP, LEADING, LEADING_GAP, PAD, Pointed};

/// A menu opened from the keyboard opens under the row, this far in
/// (`ui/rail.slint:549`).
const MENU_INSET: f32 = 16.0;

impl<M> widget::Meta for Pointed<'_, M> {}

impl<'a, M: 'a> Pointed<'a, M> {
    /// The parts in the tree's order: the body, then the trailing part and
    /// the card where there are.
    fn parts(&mut self) -> Vec<&mut Element<'a, M>> {
        let mut parts = vec![&mut self.body];
        parts.extend(self.trailing.as_mut());
        parts.extend(self.card.as_mut());
        parts
    }

    /// The trailing part is laid out: under the pointer, or chosen; one
    /// that fades with the pointer always, as Slint's "+" is there at no
    /// opacity (`ui/rail.slint:924`).
    fn shows_trailing(&self, state: &State) -> bool {
        self.trailing.is_some() && (state.hovered || self.look.selected || self.look.fade_trailing)
    }

    /// Presses it: a click let go over it, Space or Enter.
    fn press(&self, state: &State, bounds: Rectangle, shell: &mut Shell<'_, M>) {
        if let Some(on_press) = &self.on_press {
            let shown = Rectangle::new(bounds.position() + state.translation, bounds.size());
            shell.publish(on_press(shown));
            shell.capture_event();
        }
    }
}

impl<'a, M: 'a> widget::Widget<M, Theme, Renderer> for Pointed<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.chevron.unwrap_or(false)))
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts());
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = limits.resolve(Length::Fill, self.height, Size::ZERO);
        let inner = Size::new(size.width, (size.height - self.top).max(0.0));
        let shows = self.shows_trailing(tree.state.downcast_ref::<State>());
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let mut right = 0.0;
        if let Some(trailing) = &mut self.trailing {
            let child = &mut children[1];
            if shows {
                trailing.layout(child, renderer, &layout::Limits::new(Size::ZERO, inner));
                let tail = child.size;
                child.translation = Vector::new(
                    size.width - PAD - tail.width,
                    self.top + (inner.height - tail.height) / 2.0,
                );
                right = tail.width + PAD;
            } else {
                child.size = Size::ZERO;
            }
        }
        state.trailing = shows;
        let chevron = if self.chevron.is_some() {
            CHEVRON + CHEVRON_GAP + PAD
        } else {
            0.0
        };
        let left = if self.leading.is_some() {
            PAD + LEADING + LEADING_GAP
        } else {
            0.0
        };
        let room = Size::new((size.width - left - right - chevron).max(0.0), inner.height);
        let body = &mut children[0];
        self.body
            .layout(body, renderer, &layout::Limits::new(Size::ZERO, room));
        body.translation = Vector::new(left, self.top + (inner.height - body.size.height) / 2.0);
        state.chevron_x = body.size.width + CHEVRON_GAP;
        tree.size = size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        if self.look.ring.is_some() && self.on_press.is_some() {
            operation.focusable(None, layout.bounds(), state);
        }
        // The card isn't laid out in the row.
        let mut parts = layout.iter_mut(children);
        let (child, child_tree) = parts.next().expect("the body");
        self.body
            .operate(child_tree, child, viewport, renderer, operation);
        if let (Some(trailing), Some((child, child_tree))) = (self.trailing.as_mut(), parts.next())
            && state.trailing
        {
            trailing.operate(child_tree, child, viewport, renderer, operation);
        }
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
        let bounds = layout.bounds();
        let over = cursor.is_over(bounds);
        let shows = tree.state.downcast_ref::<State>().trailing;
        {
            let mut children = layout.iter_mut(&mut tree.children);
            let (child, child_tree) = children.next().expect("the body");
            self.body
                .update(child_tree, event, child, cursor, renderer, shell, viewport);
            if let (Some(trailing), Some((child, child_tree))) =
                (self.trailing.as_mut(), children.next())
                && shows
            {
                trailing.update(child_tree, event, child, cursor, renderer, shell, viewport);
            }
        }
        let taken = shell.is_event_captured();
        let motion = self.motion;
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != state.hovered {
                    state.point(over, motion, Instant::now());
                    if self.trailing.is_some() && !self.look.fade_trailing {
                        shell.invalidate_layout();
                    }
                    shell.invalidate_overlay();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if over && !taken {
                    state.pressed = true;
                    state.dismissed = true;
                    shell.invalidate_overlay();
                    shell.request_redraw();
                } else if !over && state.focused {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if std::mem::take(&mut state.pressed) && over && !taken {
                    self.press(state, bounds, shell);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if over && !taken => {
                if let (Some(on_menu), Some(at)) = (&self.on_menu, cursor.position()) {
                    state.dismissed = true;
                    shell.invalidate_overlay();
                    shell.publish(on_menu(at + state.translation));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) if over && !state.dismissed => {
                // Scrolled, the row it's beside moves from under the
                // pointer (`ui/rail.slint:1340`).
                state.dismissed = true;
                shell.invalidate_overlay();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && !taken =>
            {
                let named = match key.as_ref() {
                    keyboard::Key::Named(named) => Some(named),
                    _ => None,
                };
                let menu = named == Some(Named::ContextMenu)
                    || (named == Some(Named::F10) && modifiers.shift());
                if matches!(named, Some(Named::Enter | Named::Space)) {
                    self.press(state, bounds, shell);
                } else if menu && let Some(on_menu) = &self.on_menu {
                    // From the keyboard the pointer may be elsewhere: its
                    // next visit shows the card.
                    state.dismissed = over;
                    shell.invalidate_overlay();
                    let at = Point::new(bounds.x + MENU_INSET, bounds.y + bounds.height);
                    shell.publish(on_menu(at + state.translation));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now))
                if state.advance(*now, self.chevron, motion) =>
            {
                shell.request_redraw();
            }
            _ => {}
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
        self.paint(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let mut children = layout.iter(&tree.children);
        let (child, child_tree) = children.next().expect("the body");
        let mut inner = self
            .body
            .mouse_interaction(child_tree, child, cursor, viewport, renderer);
        if let (Some(trailing), Some((child, child_tree))) = (&self.trailing, children.next())
            && state.trailing
        {
            inner = inner
                .max(trailing.mouse_interaction(child_tree, child, cursor, viewport, renderer));
        }
        if inner == mouse::Interaction::None
            && self.on_press.is_some()
            && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Pointer
        } else {
            inner
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
        let anchor = layout.bounds() + translation;
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        state.translation = translation;
        let card_shows = state.hovered && !state.dismissed;
        let amount = state.card.amount(state.now);
        let Self {
            body,
            trailing,
            card,
            ..
        } = self;
        let mut parts = layout.iter_mut(children);
        let (child, child_tree) = parts.next().expect("the body");
        let mut overlays = body.overlay(child_tree, child, renderer, viewport, translation, window);
        if let Some(trailing) = trailing {
            let (child, child_tree) = parts.next().expect("the trailing part");
            if state.trailing {
                overlays.extend(trailing.overlay(
                    child_tree,
                    child,
                    renderer,
                    viewport,
                    translation,
                    window,
                ));
            }
        }
        if let (Some(card), Some((_, card_tree))) = (card, parts.next())
            && card_shows
        {
            let place = super::card::Place {
                anchor,
                window,
                amount,
            };
            overlays.push(super::card::overlay(card, card_tree, place, renderer));
        }
        overlays
    }
}
