//! Discord's bar once a draft differs from what Discord has: Reset puts it
//! back, and Save Changes sends it. Each time leaving is refused for the
//! changes, `nudge` goes up, and the bar shakes red for a moment, as
//! Discord's does; with a still interface only the red says it. The Slint
//! side: `UnsavedBar` (`ui/settings.slint:311`): 60 px tall, the raised
//! ground under a line, its words 14 px semibold, a ghost Reset and a
//! primary Save Changes.
//!
//! The bar's coming up from its edge (`PopupEntry`) isn't drawn: iced has no
//! layer transform, so it appears whole.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Style};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Renderer as _, Shell, mouse};
use iced::widget::{container, row, space, text};
use iced::{Color, Event, Length, Padding, Rectangle, Size, Vector, alignment, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, catalog, typography};
use anomi_vm::settings::frame::Unsaved;
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

use super::press::mix;

const HEIGHT: f32 = 60.0;
/// The bar's insets (`padding: 12px`) and the gap between its parts.
const INSET: f32 = 12.0;
const GAP: f32 = 8.0;
/// Six shakes 60 ms apart, 6 px each way, and the red for 1.2 s.
const SHAKES: u128 = 6;
const BEAT: Duration = Duration::from_millis(60);
const RED: Duration = Duration::from_millis(1200);
const SWAY: f32 = 6.0;

/// The bar for `bar`; `pad` is its left inset, 12 px unless a page lines it
/// up with its column. `undo` and `save` are what the buttons send.
pub fn unsaved_bar<'a, M: Clone + 'a>(
    bar: &'a Unsaved,
    pad: f32,
    undo: M,
    save: M,
    motion: Motion,
) -> Element<'a, M> {
    let words = text(tr!("Careful — you have unsaved changes!"))
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .style(catalog::text::inherit);
    let reset = action_button(tr!("Reset"), Kind::Ghost, motion)
        .enabled(bar.may_reset && !bar.saving)
        .on_press(undo);
    let saving = if bar.saving {
        tr!("Saving...")
    } else {
        tr!("Save Changes")
    };
    let save = action_button(saving, Kind::Primary, motion)
        .enabled(bar.may_save && !bar.saving)
        .on_press(save);
    let content = row![
        container(words)
            .align_y(alignment::Vertical::Center)
            .height(Length::Fill),
        space().width(Length::Fill),
        reset,
        save,
    ]
    .spacing(GAP)
    .align_y(alignment::Vertical::Center)
    .padding(Padding {
        left: pad,
        ..Padding::new(INSET)
    })
    .height(HEIGHT);
    Shake {
        content: content.boxed(),
        nudge: bar.nudge,
        motion,
    }
    .boxed()
}

struct Shake<'a, M> {
    content: Element<'a, M>,
    nudge: u32,
    motion: Motion,
}

struct State {
    /// The last nudge seen, and when the shake it started began.
    seen: u32,
    since: Option<Instant>,
    now: Instant,
}

impl State {
    fn elapsed(&self) -> Option<Duration> {
        self.since
            .map(|since| self.now.saturating_duration_since(since))
    }

    /// How red the bar is: it comes in and goes out over `QUICK`.
    fn red(&self) -> f32 {
        let Some(elapsed) = self.elapsed().filter(|elapsed| *elapsed < RED) else {
            return 0.0;
        };
        let (rise, fall) = (
            elapsed.as_secs_f32() / QUICK.as_secs_f32(),
            (RED - elapsed).as_secs_f32() / QUICK.as_secs_f32(),
        );
        rise.min(fall).clamp(0.0, 1.0)
    }

    /// How far it is moved aside: a shake is over after six beats.
    fn sway(&self) -> f32 {
        match self.elapsed() {
            Some(elapsed) if elapsed.as_millis() / BEAT.as_millis() < SHAKES => {
                let left = SHAKES - elapsed.as_millis() / BEAT.as_millis();
                if left % 2 == 0 { -SWAY } else { SWAY }
            }
            _ => 0.0,
        }
    }
}

impl<M> widget::Meta for Shake<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Shake<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            seen: self.nudge,
            since: None,
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = Size::new(limits.max.width, HEIGHT);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .operate(child, layout, viewport, renderer, operation);
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
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content.update(
            child,
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        let state = tree.state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if state.seen != self.nudge {
                state.seen = self.nudge;
                state.since = Some(*now);
            }
            if state.elapsed().is_some_and(|elapsed| elapsed < RED) {
                shell.request_redraw();
            } else {
                state.since = None;
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let red = state.red();
        let bounds = layout.bounds();
        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            mix(palette.raised, palette.danger, red),
        );
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    height: 1.0,
                    ..bounds
                },
                ..Quad::default()
            },
            palette.edge,
        );
        let sway = if self.motion.still() {
            0.0
        } else {
            state.sway()
        };
        let ink = renderer::Style {
            text_color: mix(palette.text, palette.on_danger, red),
        };
        let _ = style;
        let (child_layout, child) = layout.iter(&tree.children).next().expect("child");
        renderer.with_translation(Vector::new(sway, 0.0), |renderer| {
            self.content
                .draw(child, renderer, theme, &ink, child_layout, cursor, viewport);
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
        let (layout, child) = layout.iter(&tree.children).next().expect("child");
        self.content
            .mouse_interaction(child, layout, cursor, viewport, renderer)
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
