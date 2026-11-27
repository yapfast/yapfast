//! An installed theme's channel-change transition. Slint could retain a
//! compositor snapshot of the whole conversation; iced 0.14 exposes no
//! window-snapshot command to application code, so this keeps the previous
//! timeline and fades it through the theme's chat ground. Dissolve uses the
//! same 96-block idea as the Slint renderer, with ground blocks uncovering
//! the new conversation in a stable shuffled order.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use iced::advanced::{Layout, Shell, Widget, layout, mouse, renderer, widget::Tree};
use iced::{Color, Element, Length, Rectangle, Size};
use yapfast_core::themes::Performance;
use yapfast_core::themes::package::art::{Transition, TransitionKind};

use crate::{
    app::{App, Message},
    timeline::{Lanes, Timeline},
};

const FRAME: Duration = Duration::from_millis(16);
const BLOCKS_X: usize = 12;
const BLOCKS_Y: usize = 8;
const BLOCKS: usize = BLOCKS_X * BLOCKS_Y;

#[derive(Default)]
pub struct State {
    active: Option<Active>,
}

struct Active {
    started: Instant,
    duration: Duration,
    kind: TransitionKind,
    timeline: Timeline,
    in_server: bool,
}

impl State {
    /// Releases the retained conversation at the end of this exact
    /// transition. A late redraw from an older overlay must not clear a newer
    /// room change.
    pub fn finish(&mut self, started: Instant) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.started == started)
        {
            self.active = None;
        }
    }
}

/// Retains the conversation that is about to be replaced, if the installed
/// theme asks for a transition and the person's motion choices allow this
/// heavy effect. Called by the central room-switch sync while the old
/// timeline is still present.
pub fn begin(app: &mut App) {
    if app.selected.is_none() {
        app.theme_transition.active = None;
        return;
    }
    let Some(spec) = wanted(app) else {
        app.theme_transition.active = None;
        return;
    };
    app.theme_transition.active = Some(Active {
        started: Instant::now(),
        duration: Duration::from_secs_f32(spec.duration / 1000.0),
        kind: spec.kind,
        timeline: timeline_copy(&app.timeline),
        in_server: crate::looks::in_server(app.selected.as_deref()),
    });
}

/// The shown theme's transition, from what [`crate::themes::Themes`]
/// already resolved for it, when the person's choices allow it.
fn wanted(app: &App) -> Option<Transition> {
    if !allowed(
        app.layout.theme_transitions,
        app.layout.theme_performance,
        app.layout.reduced_motion(),
        app.win.settings.is_open(),
    ) {
        return None;
    }
    app.themes.transition(&app.theme.id)
}

fn allowed(enabled: bool, performance: Performance, reduced: bool, settings: bool) -> bool {
    enabled && performance == Performance::Full && !reduced && !settings
}

/// Draws the retained old timeline over `current` while its transition is
/// live. Its widget deliberately never forwards input to the retained tree.
pub fn chat<'a>(app: &'a App, current: Element<'a, Message>) -> Element<'a, Message> {
    let Some(active) = app.theme_transition.active.as_ref() else {
        return current;
    };
    if progress(active.started, active.duration, Instant::now()) >= 1.0 {
        return current;
    }
    let mut ctx = app.ctx();
    ctx.lanes = Lanes {
        timeline: &active.timeline,
        aside: None,
        keyed: None,
    };
    ctx.in_server = active.in_server;
    let old = crate::timeline::view::timeline(ctx);
    iced::widget::stack![
        current,
        Element::new(Overlay {
            old,
            started: active.started,
            duration: active.duration,
            kind: active.kind,
            ground: app.theme.package_colour("chat").unwrap_or(app.theme.chat),
        })
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn progress(started: Instant, duration: Duration, now: Instant) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (now.saturating_duration_since(started).as_secs_f32() / duration.as_secs_f32()).min(1.0)
}

fn ground_coverage(amount: f32) -> f32 {
    if amount < 0.5 {
        amount * 2.0
    } else {
        (1.0 - amount) * 2.0
    }
    .clamp(0.0, 1.0)
}

fn timeline_copy(timeline: &Timeline) -> Timeline {
    Timeline {
        messages: timeline.messages.clone(),
        heights: timeline.heights.clone(),
        tops: timeline.tops.clone(),
        looks: timeline.looks.clone(),
        edge: timeline.edge.clone(),
        found: timeline.found.clone(),
        revealed: timeline.revealed.clone(),
        jumped: timeline.jumped.clone(),
        unread_jumped: timeline.unread_jumped,
        jump_lit: timeline.jump_lit.clone(),
        pressed_link: None,
        marks_for: timeline.marks_for.clone(),
        edge_changed: false,
        from_end: timeline.from_end,
        height: timeline.height,
        width: timeline.width,
        at_end: timeline.at_end,
        place: Cell::new(timeline.place.get()),
        keyed_press: timeline.keyed_press,
        latest_due: false,
        left_at: timeline.left_at.clone(),
        selection: None,
        selecting: false,
        clocks: Default::default(),
    }
}

struct Overlay<'a> {
    old: Element<'a, Message>,
    started: Instant,
    duration: Duration,
    kind: TransitionKind,
    ground: Color,
}

impl Widget<Message, iced::Theme, iced::Renderer> for Overlay<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.old)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.old));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self.old.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &limits.width(Length::Fill).height(Length::Fill),
        );
        layout::Node::with_children(limits.max(), vec![child])
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            if progress(self.started, self.duration, *now) < 1.0 {
                shell.request_redraw_at(*now + FRAME);
            } else {
                shell.publish(Message::ThemeTransitionDone(self.started));
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let amount = progress(self.started, self.duration, Instant::now());
        if amount >= 1.0 {
            return;
        }
        let bounds = layout.bounds();
        renderer.with_layer(bounds, |renderer| {
            if amount < 0.5
                && let Some(old_layout) = layout.children().next()
            {
                self.old.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    old_layout,
                    cursor,
                    viewport,
                );
            }
            match self.kind {
                TransitionKind::Fade => {
                    let alpha = ground_coverage(amount);
                    fill(
                        renderer,
                        bounds,
                        Color {
                            a: self.ground.a * alpha,
                            ..self.ground
                        },
                    );
                }
                TransitionKind::Dissolve => {
                    let covered = ground_coverage(amount);
                    blocks(renderer, bounds, self.ground, covered);
                }
            }
        });
    }
}

fn fill(renderer: &mut iced::Renderer, bounds: Rectangle, colour: Color) {
    use iced::advanced::Renderer as _;
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            ..Default::default()
        },
        colour,
    );
}

fn blocks(renderer: &mut iced::Renderer, bounds: Rectangle, colour: Color, covered: f32) {
    if covered <= 0.0 {
        return;
    }
    let width = bounds.width / BLOCKS_X as f32;
    let height = bounds.height / BLOCKS_Y as f32;
    for index in 0..BLOCKS {
        let order = ((index * 37) % BLOCKS) as f32 / (BLOCKS - 1) as f32;
        if order > covered {
            continue;
        }
        let x = index % BLOCKS_X;
        let y = index / BLOCKS_X;
        fill(
            renderer,
            Rectangle::new(
                iced::Point::new(bounds.x + x as f32 * width, bounds.y + y as f32 * height),
                Size::new(width.ceil(), height.ceil()),
            ),
            colour,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_heavy_and_respect_every_personal_gate() {
        assert!(allowed(true, Performance::Full, false, false));
        assert!(!allowed(false, Performance::Full, false, false));
        assert!(!allowed(true, Performance::Balanced, false, false));
        assert!(!allowed(true, Performance::Light, false, false));
        assert!(!allowed(true, Performance::Full, true, false));
        assert!(!allowed(true, Performance::Full, false, true));
    }

    #[test]
    fn transition_progress_follows_its_resolved_duration() {
        let start = Instant::now();
        let duration = Duration::from_millis(400);
        assert_eq!(progress(start, duration, start), 0.0);
        assert!(
            (progress(start, duration, start + Duration::from_millis(100)) - 0.25).abs() < 0.001
        );
        assert_eq!(progress(start, duration, start + duration), 1.0);
        assert_eq!(
            progress(start, duration, start + Duration::from_secs(2)),
            1.0
        );
        assert_eq!(ground_coverage(0.0), 0.0);
        assert_eq!(ground_coverage(0.5), 1.0);
        assert!((ground_coverage(0.75) - 0.5).abs() < 0.001);
        assert_eq!(ground_coverage(1.0), 0.0);
    }

    #[test]
    fn retained_timeline_keeps_the_old_view_position_but_not_interaction() {
        let mut old = Timeline::new(480.0, 720.0);
        old.from_end = 64.0;
        old.at_end = false;
        old.selecting = true;
        old.place.set(Rectangle::new(
            iced::Point::new(12.0, 80.0),
            Size::new(720.0, 480.0),
        ));
        let retained = timeline_copy(&old);
        assert_eq!(retained.from_end, 64.0);
        assert!(!retained.at_end);
        assert_eq!(retained.place.get(), old.place.get());
        assert!(retained.selection.is_none());
        assert!(!retained.selecting);
    }

    #[test]
    fn dissolve_order_covers_every_block_once() {
        let mut order: Vec<_> = (0..BLOCKS).map(|index| (index * 37) % BLOCKS).collect();
        order.sort_unstable();
        assert_eq!(order, (0..BLOCKS).collect::<Vec<_>>());
    }

    #[test]
    fn only_the_matching_transition_can_release_retained_state() {
        let started = Instant::now();
        let mut state = State {
            active: Some(Active {
                started,
                duration: Duration::from_millis(250),
                kind: TransitionKind::Fade,
                timeline: Timeline::new(480.0, 720.0),
                in_server: true,
            }),
        };
        state.finish(started - Duration::from_millis(1));
        assert!(state.active.is_some());
        state.finish(started);
        assert!(state.active.is_none());
    }
}
