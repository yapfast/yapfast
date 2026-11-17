//! One choice in a menu a screen lays out itself, as the composer's +, the
//! status menu or the message menu: `MenuItem` (`ui/controls.slint:1156`).
//! It lights under the pointer (or when told), sends its row's intent when
//! clicked, or when it has the keyboard's focus and Enter or Space is
//! pressed, and draws a ring while focused. Its arrow turns down while what
//! it opens shows below it (`expanded`), over `QUICK` on `ARRIVE`; its box
//! fills over `QUICK` as it's ticked.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Id, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::keyboard::key::Named;
use iced::{Event, Length, Rectangle, Size, keyboard, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::menu::Row;

use super::row::{self, Paint, Shape};

pub struct Item<'a, M> {
    row: &'a Row<M>,
    arrow: bool,
    expanded: bool,
    lit: Option<bool>,
    on_hover: Option<M>,
    id: Option<Id>,
    width: Length,
    motion: Motion,
}

/// A row of a menu laid out by hand, as wide as its column.
pub fn item<M>(row: &Row<M>, motion: Motion) -> Item<'_, M> {
    Item {
        row,
        arrow: !row.submenu.is_empty(),
        expanded: false,
        lit: None,
        on_hover: None,
        id: None,
        width: Length::Fill,
        motion,
    }
}

impl<M> Item<'_, M> {
    /// It opens a submenu, which an arrow at its end shows.
    pub fn arrow(mut self, arrow: bool) -> Self {
        self.arrow = arrow;
        self
    }

    /// What its arrow opens shows below it: the arrow points down.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Drawn lit whatever the pointer does, as a row whose submenu shows.
    pub fn lit(mut self, lit: bool) -> Self {
        self.lit = Some(lit);
        self
    }

    /// What the pointer arriving on it sends.
    pub fn on_hover(mut self, message: M) -> Self {
        self.on_hover = Some(message);
        self
    }

    /// Names it for `focusable::focus`.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

struct State {
    hovered: bool,
    pressed: bool,
    focused: bool,
    turn: Animation<bool>,
    tick: Animation<bool>,
    now: Instant,
    /// The row's measure, shaped again only when the row or its width
    /// changes.
    measure: Option<Measure>,
}

#[derive(Clone, Copy)]
struct Measure {
    /// The row's [`row::key`].
    key: u64,
    /// The width it wants.
    wanted: f32,
    /// The width it was given, its height there, and its subtitle's height
    /// there and its icon.
    width: f32,
    height: f32,
    shape: Shape,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<M> widget::Meta for Item<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Item<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            pressed: false,
            focused: false,
            turn: Animation::new(self.expanded),
            tick: Animation::new(self.row.checked),
            now: Instant::now(),
            measure: None,
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        let key = row::key(self.row, self.arrow);
        let kept = state.measure.filter(|measure| measure.key == key);
        let wanted = kept.map_or_else(|| row::width(self.row, self.arrow), |kept| kept.wanted);
        let width = limits
            .width(self.width)
            .resolve(self.width, Length::Shrink, Size::new(wanted, 0.0))
            .width;
        let measure = match kept {
            Some(kept) if kept.width == width => kept,
            _ => {
                let below = row::below(self.row, self.arrow, width);
                let icon = kept.map_or_else(|| row::icon(self.row), |kept| kept.shape.icon);
                Measure {
                    key,
                    wanted,
                    width,
                    height: row::height(self.row, below),
                    shape: Shape { below, icon },
                }
            }
        };
        state.measure = Some(measure);
        tree.size = Size::new(width, measure.height);
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
        let over = cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != state.hovered {
                    state.hovered = over;
                    if over && let Some(hover) = &self.on_hover {
                        shell.publish(hover.clone());
                    }
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if std::mem::take(&mut state.pressed) && over {
                    self.choose(shell);
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.focused => {
                let pressed = matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Enter | Named::Space)
                        | keyboard::Key::Character(" ")
                );
                if pressed {
                    self.choose(shell);
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let mut moving = false;
                for (animation, target, easing) in [
                    (&mut state.turn, self.expanded, Some(ARRIVE)),
                    (&mut state.tick, self.row.checked, None),
                ] {
                    if animation.value() != target {
                        let next = Animation::new(!target).duration(self.motion.duration(QUICK));
                        let next = match easing {
                            Some(easing) => next.easing(easing),
                            None => next,
                        };
                        *animation = next.go(target, *now);
                    }
                    moving |= animation.is_animating(*now);
                }
                if moving {
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
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let paint = Paint {
            lit: self.lit.unwrap_or(cursor.is_over(bounds)),
            arrow: self.arrow,
            turned: state.turn.interpolate(0.0, 1.0, state.now),
            ticked: state.tick.interpolate(0.0, 1.0, state.now),
            focused: state.focused,
            alpha: 1.0,
        };
        let shape = state
            .measure
            .map(|measure| measure.shape)
            .unwrap_or_default();
        row::draw(renderer, theme, self.row, (bounds, shape), paint, *viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        match cursor.is_over(layout.bounds()) {
            true if !self.row.disabled => mouse::Interaction::Pointer,
            true => mouse::Interaction::Idle,
            false => mouse::Interaction::None,
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
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
        operation.text(
            self.id.as_ref(),
            layout.bounds(),
            &mut self.row.text.clone(),
        );
    }
}

impl<M: Clone> Item<'_, M> {
    fn choose(&self, shell: &mut Shell<'_, M>) {
        if self.row.disabled {
            return;
        }
        if let Some(intent) = &self.row.intent {
            shell.publish(intent.clone());
        }
    }
}
