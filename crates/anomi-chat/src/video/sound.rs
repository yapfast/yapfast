//! Mute, with a slider over it for how loud while the pointer is on either
//! (`VideoControls`' sound, `ui/messages.slint:441`-`:507`): a dark panel
//! rises 112 px over the 28 px button, its upright white rail filled from
//! the bottom to the volume with a 12 px handle. A press or a drag on the
//! panel sets the volume there; the wheel over either moves it 5% a turn.
//! The panel fades in over `QUICK` and goes at once, as Slint's
//! `visible: sound.open` does.
//!
//! The panel lies over the video above the controls, outside this widget's
//! own 28 px: it is drawn in a layer of its own so it covers the frame, and
//! heard through the events every parent passes on.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Background, Border, Color, Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme};
use anomi_vm::chat::video::{Intent, VOLUME_STEP};

use super::look::{DARK, INK, RAIL};

/// The button's side, and how far the panel rises over it (`:445`).
pub const SLOT: f32 = 28.0;
const RISE: f32 = 112.0;
/// The panel stops 4 short of the button; its rail keeps 14 at each end.
const PANEL: f32 = RISE - 4.0;
const PANEL_RADIUS: f32 = 6.0;
const END: f32 = 14.0;
const THICK: f32 = 4.0;
const HANDLE: f32 = 12.0;

/// The mute `button` with the volume panel over it, showing `volume`.
pub fn sound<'a>(button: Element<'a, Intent>, volume: f32, motion: Motion) -> Sound<'a> {
    Sound {
        button,
        volume: volume.clamp(0.0, 1.0),
        motion,
    }
}

pub struct Sound<'a> {
    button: Element<'a, Intent>,
    volume: f32,
    motion: Motion,
}

struct State {
    hovered: bool,
    /// The panel is held down; `dragged` is where.
    pressed: bool,
    dragged: f32,
    fade: Animation<bool>,
    now: Instant,
}

impl State {
    fn open(&self) -> bool {
        self.hovered || self.pressed
    }
}

/// The panel over the button at `slot`.
pub fn panel(slot: Rectangle) -> Rectangle {
    Rectangle {
        y: slot.y - RISE,
        height: PANEL,
        ..slot
    }
}

/// Where the pointer keeps the panel open: the button, and while it's
/// open the panel and the gap under it too.
fn reach(slot: Rectangle, open: bool) -> Rectangle {
    if open {
        Rectangle {
            y: slot.y - RISE,
            height: slot.height + RISE,
            ..slot
        }
    } else {
        slot
    }
}

/// The upright rail inside the panel.
fn rail(slot: Rectangle) -> Rectangle {
    let panel = panel(slot);
    Rectangle {
        x: panel.x + (panel.width - THICK) / 2.0,
        y: panel.y + END,
        width: THICK,
        height: panel.height - 2.0 * END,
    }
}

/// The volume at the pointer's height on the rail of the button at `slot`.
pub fn level(slot: Rectangle, at: Point) -> f32 {
    let rail = rail(slot);
    (1.0 - (at.y - rail.y) / rail.height).clamp(0.0, 1.0)
}

impl Sound<'_> {
    fn shown(&self, state: &State) -> f32 {
        if state.pressed {
            state.dragged
        } else {
            self.volume
        }
    }
}

impl widget::Meta for Sound<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Sound<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            pressed: false,
            dragged: 0.0,
            fade: Animation::new(false),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.button));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SLOT), Length::Fixed(SLOT))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let size = Size::new(SLOT, SLOT);
        self.button.layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(size, size),
        );
        tree.size = limits.resolve(Length::Fixed(SLOT), Length::Fixed(SLOT), size);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Intent>,
        viewport: &Rectangle,
    ) {
        let slot = layout.bounds();
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let state = tree.state.downcast_mut::<State>();
        let was_open = state.open();
        let on_panel = was_open && cursor.is_over(panel(slot));
        // On the panel the button hears the pointer gone, so its hover and
        // tooltip leave as the pointer rises onto the panel.
        let button_cursor = if on_panel {
            mouse::Cursor::Unavailable
        } else {
            cursor
        };
        self.button.update(
            child_tree,
            event,
            child,
            button_cursor,
            renderer,
            shell,
            viewport,
        );
        if let Event::Mouse(_) = event {
            state.hovered = cursor.is_over(reach(slot, was_open));
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if on_panel => {
                if let Some(at) = cursor.position() {
                    state.pressed = true;
                    state.dragged = level(slot, at);
                    shell.publish(Intent::Volume(state.dragged));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.pressed => {
                state.dragged = level(slot, *position);
                shell.publish(Intent::Volume(state.dragged));
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.is_over(reach(slot, was_open)) =>
            {
                let turn = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        *y
                    }
                };
                if turn != 0.0 {
                    let step = if turn > 0.0 {
                        VOLUME_STEP
                    } else {
                        -VOLUME_STEP
                    };
                    let shown = self.shown(state);
                    shell.publish(Intent::Volume((shown + step).clamp(0.0, 1.0)));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if !state.open() || self.motion.still() {
                    state.fade = Animation::new(state.open());
                } else if !state.fade.value() {
                    state.fade = self.motion.animation(false, QUICK, ARRIVE).go(true, *now);
                }
                if state.fade.is_animating(*now) {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        if state.open() != was_open {
            shell.request_redraw();
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
        let slot = layout.bounds();
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.button
            .draw(child_tree, renderer, theme, style, child, cursor, viewport);
        let state = tree.state.downcast_ref::<State>();
        if !state.open() {
            return;
        }
        let shown = self.shown(state);
        let alpha = state.fade.interpolate(0.0f32, 1.0, state.now);
        let faded = |color: Color| Color {
            a: color.a * alpha,
            ..color
        };
        let quad = |bounds, radius: f32| Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Quad::default()
        };
        let panel = panel(slot);
        let rail = rail(slot);
        // A layer of its own, so it lies over the frame drawn before it.
        renderer.with_layer(panel.expand(1.0), |renderer| {
            renderer.fill_quad(quad(panel, PANEL_RADIUS), Background::Color(faded(DARK)));
            renderer.fill_quad(quad(rail, THICK / 2.0), Background::Color(faded(RAIL)));
            let filled = Rectangle {
                y: rail.y + rail.height * (1.0 - shown),
                height: rail.height * shown,
                ..rail
            };
            if filled.height > 0.0 {
                renderer.fill_quad(quad(filled, THICK / 2.0), Background::Color(faded(INK)));
            }
            let handle = Rectangle {
                x: rail.center_x() - HANDLE / 2.0,
                y: filled.y - HANDLE / 2.0,
                width: HANDLE,
                height: HANDLE,
            };
            renderer.fill_quad(quad(handle, HANDLE / 2.0), Background::Color(faded(INK)));
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let slot = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        if state.open() && cursor.is_over(panel(slot)) {
            return mouse::Interaction::Pointer;
        }
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.button
            .mouse_interaction(child_tree, child, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.button
            .operate(child_tree, child, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Intent, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.button
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT_AT: Rectangle = Rectangle {
        x: 300.0,
        y: 200.0,
        width: SLOT,
        height: SLOT,
    };

    #[test]
    fn the_rail_runs_from_full_at_its_top_to_silent_at_its_foot() {
        let rail = rail(SLOT_AT);
        assert_eq!(rail.height, PANEL - 2.0 * END);
        assert_eq!(level(SLOT_AT, Point::new(314.0, rail.y)), 1.0);
        assert_eq!(level(SLOT_AT, Point::new(314.0, rail.y + rail.height)), 0.0);
        assert_eq!(
            level(SLOT_AT, Point::new(314.0, rail.y + rail.height / 2.0)),
            0.5
        );
        assert_eq!(level(SLOT_AT, Point::new(314.0, 0.0)), 1.0);
    }

    #[test]
    fn the_panel_rises_over_the_button_and_keeps_it_open() {
        let panel = panel(SLOT_AT);
        assert_eq!(panel.y, SLOT_AT.y - RISE);
        assert!(reach(SLOT_AT, true).contains(Point::new(310.0, SLOT_AT.y - 2.0)));
        assert!(!reach(SLOT_AT, false).contains(Point::new(310.0, SLOT_AT.y - 2.0)));
    }
}
