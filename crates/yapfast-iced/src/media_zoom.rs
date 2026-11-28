use std::time::{Duration, Instant};

use iced::{
    Alignment, Color, Element, Event, Point, Rectangle, Size, Transformation,
    advanced::{Renderer as _, Shell},
    mouse, touch,
    widget::{button, row, text, tooltip},
    window,
};

use crate::{
    controls,
    look::Look,
    pictures::Icon,
    widgets::{At, Hooks, Inner, Renderer, Stateless, Wrap},
};

#[derive(Clone, Copy, Debug)]
pub enum Change {
    Out,
    In,
    Reset,
    Toggle,
}

#[derive(Clone, Copy)]
pub struct Zoom {
    target: u16,
    transition: Option<(Instant, f32)>,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            target: 100,
            transition: None,
        }
    }
}

impl Zoom {
    const MIN: u16 = 50;
    const MAX: u16 = 400;
    const STEP: u16 = 25;
    const DURATION: Duration = Duration::from_millis(180);
    const DOUBLE_ZOOM: u16 = 200;

    pub fn change(&mut self, change: Change, animate: bool) -> bool {
        self.change_at(change, animate, Instant::now())
    }

    fn change_at(&mut self, change: Change, animate: bool, now: Instant) -> bool {
        let target = match change {
            Change::Out => (self.target - Self::STEP).max(Self::MIN),
            Change::In => (self.target + Self::STEP).min(Self::MAX),
            Change::Reset => 100,
            Change::Toggle if self.target > 100 => 100,
            Change::Toggle => Self::DOUBLE_ZOOM,
        };
        if target == self.target {
            return false;
        }
        self.transition = animate.then(|| (now, self.factor_at(now)));
        self.target = target;
        true
    }

    fn factor_at(self, now: Instant) -> f32 {
        let target = self.factor();
        let Some((start, from)) = self.transition else {
            return target;
        };
        let elapsed = now.saturating_duration_since(start);
        if elapsed >= Self::DURATION {
            return target;
        }
        let t = elapsed.as_secs_f32() / Self::DURATION.as_secs_f32();
        let eased = 1.0 - (1.0 - t).powi(3);
        from + (target - from) * eased
    }

    pub fn moving(self, now: Instant) -> bool {
        self.transition
            .is_some_and(|(start, _)| now.saturating_duration_since(start) < Self::DURATION)
    }

    pub fn settle(&mut self, start: Instant) {
        if self.transition.is_some_and(|(current, _)| current == start) {
            self.transition = None;
        }
    }

    pub fn picture_scale(self) -> f32 {
        self.transition
            .map_or(self.factor(), |(_, from)| self.factor().max(from))
    }

    pub fn needs_picture_scaling(self) -> bool {
        self.target != 100 || self.transition.is_some()
    }

    /// Scale around the fitted picture's center, keeping the viewport fixed.
    pub fn bounds(self, fitted: Rectangle) -> Rectangle {
        let scale = if self.transition.is_some() {
            self.factor_at(Instant::now())
        } else {
            self.factor()
        };
        if scale == 1.0 {
            return fitted;
        }
        let size = Size::new(fitted.width * scale, fitted.height * scale);
        let center = fitted.center();
        Rectangle::new(
            Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
            size,
        )
    }

    pub fn factor(self) -> f32 {
        f32::from(self.target) / 100.0
    }

    pub fn picture<'a, M: 'a>(
        self,
        picture: Element<'a, M>,
        settled: fn(Instant) -> M,
    ) -> Element<'a, M> {
        Wrap::new(
            picture,
            PictureZoom {
                zoom: self,
                settled,
            },
        )
        .into()
    }

    pub fn controls<'a, M: Clone + 'a>(
        self,
        look: Look<'a>,
        message: impl Fn(Change) -> M,
    ) -> Element<'a, M> {
        let ground = crate::tile::GLASS;
        let hover = crate::theme::mix(ground, look.theme.overlay, 0.9);
        let step = |icon, label, enabled: bool, change| {
            let ink = Color::from_rgba(1.0, 1.0, 1.0, if enabled { 1.0 } else { 0.4 });
            look.face_button(
                look.glyph(icon, 18, ink),
                32.0,
                label,
                enabled.then(|| message(change)),
            )
            .fill(ground)
            .below()
        };
        let reset = button(
            text(format!("{}%", self.target))
                .size(13)
                .color(Color::WHITE)
                .center(),
        )
        .padding(0)
        .width(46)
        .height(32)
        .on_press(message(Change::Reset))
        .style(controls::lit_style(Some(ground), hover, 6.0));
        row![
            step(
                Icon::Minus,
                "Zoom out",
                self.target > Self::MIN,
                Change::Out
            ),
            tooltip(
                reset,
                controls::tip(look.theme, "Reset zoom"),
                tooltip::Position::Bottom
            )
            .gap(6),
            step(Icon::Plus, "Zoom in", self.target < Self::MAX, Change::In),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .into()
    }
}

struct PictureZoom<M> {
    zoom: Zoom,
    settled: fn(Instant) -> M,
}

impl<M> Hooks<M> for PictureZoom<M> {
    type State = Stateless;

    fn update(
        &mut self,
        _state: &mut Stateless,
        event: &Event,
        _at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && let Some((start, _)) = self.zoom.transition
        {
            if self.zoom.moving(*now) {
                shell.request_redraw();
            } else {
                shell.publish((self.settled)(start));
            }
        }
        child.update(shell);
    }

    fn draw(
        &self,
        _: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        let scale = self.zoom.factor_at(Instant::now());
        let center = at.bounds.center();
        let transform = Transformation::translate(center.x, center.y)
            * Transformation::scale(scale)
            * Transformation::translate(-center.x, -center.y);
        renderer.with_layer(at.bounds, |renderer| {
            renderer.with_transformation(transform, child);
        });
    }
}

const DOUBLE_CLICK: Duration = Duration::from_millis(300);

pub fn media_click<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    double: M,
    single: Option<M>,
) -> Element<'a, M> {
    Wrap::new(content, MediaClick { double, single }).into()
}

struct MediaClick<M> {
    double: M,
    single: Option<M>,
}

#[derive(Default)]
struct LastPress(Option<(Instant, Point)>);

impl<M: Clone> Hooks<M> for MediaClick<M> {
    type State = LastPress;

    fn update(
        &mut self,
        state: &mut LastPress,
        event: &Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        child.update(shell);
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && let (Some((before, _)), Some(single)) = (state.0, &self.single)
        {
            let deadline = before + DOUBLE_CLICK;
            if *now >= deadline {
                state.0 = None;
                shell.publish(single.clone());
            } else {
                shell.request_redraw_at(deadline);
            }
        }
        if shell.is_event_captured() {
            return;
        }
        let position = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => at.cursor.position(),
            Event::Touch(touch::Event::FingerPressed { position, .. }) => Some(*position),
            _ => None,
        };
        let Some(position) = position
            .filter(|position| at.bounds.contains(*position) && at.viewport.contains(*position))
        else {
            return;
        };
        let now = Instant::now();
        match state.0.take() {
            Some((before, point))
                if now.duration_since(before) < DOUBLE_CLICK && point.distance(position) < 6.0 =>
            {
                shell.publish(self.double.clone());
            }
            previous => {
                if let Some(single) = &self.single {
                    if previous.is_some() {
                        shell.publish(single.clone());
                    }
                    shell.request_redraw_at(now + DOUBLE_CLICK);
                }
                state.0 = Some((now, position));
            }
        }
        shell.capture_event();
    }

    fn interaction(&self, _: &LastPress, at: At, child: mouse::Interaction) -> mouse::Interaction {
        if self.single.is_some()
            && at.cursor.is_over(at.bounds)
            && child == mouse::Interaction::None
        {
            mouse::Interaction::Pointer
        } else {
            child
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_bounded_and_keeps_the_fitted_center() {
        let fitted = Rectangle::new(iced::Point::new(20.0, 30.0), Size::new(80.0, 40.0));
        let mut zoom = Zoom::default();
        for _ in 0..20 {
            zoom.change(Change::In, false);
        }
        assert_eq!(zoom.bounds(fitted).size(), Size::new(320.0, 160.0));
        assert_eq!(zoom.bounds(fitted).center(), fitted.center());
        for _ in 0..20 {
            zoom.change(Change::Out, false);
        }
        assert_eq!(zoom.bounds(fitted).size(), Size::new(40.0, 20.0));
        zoom.change(Change::Reset, false);
        assert_eq!(zoom.bounds(fitted), fitted);
    }

    #[test]
    fn a_reversed_transition_starts_at_the_current_scale() {
        let start = Instant::now();
        let middle = start + Zoom::DURATION / 2;
        let mut zoom = Zoom::default();
        assert!(zoom.change_at(Change::Toggle, true, start));
        assert_eq!(zoom.factor_at(start), 1.0);
        assert_eq!(zoom.factor(), 2.0);
        let scale = zoom.factor_at(middle);
        assert!(scale > 1.0 && scale < 2.0);
        assert!(zoom.change_at(Change::Toggle, true, middle));
        assert_eq!(zoom.factor_at(middle), scale);
        assert_eq!(zoom.factor_at(middle + Zoom::DURATION), 1.0);
    }
}
