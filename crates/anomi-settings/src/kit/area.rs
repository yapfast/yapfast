//! A text of a settings page several lines long, such as a server's
//! description or a channel's topic: at least four lines high, taller as the
//! text grows, with how many characters are left below it when that's
//! counted. The Slint side: `SettingsArea` (`ui/settings.slint:214`): a
//! sunken well with a rim that the keys' accent ring takes the place of, the
//! counter 12 px, red once the text is too long.
//!
//! The field is the composer's, which edits a copy of the page's text and
//! reports each change. Enter there only sends, so a new line in an area is
//! Shift+Enter until the composer has a plain mode (the composer widget is
//! F-wave's, not S01's).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{Quad, Style};
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree, tree};
use iced::advanced::{Renderer as _, Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::widget::{column, container, text};
use iced::{Border, Event, Length, Padding, Rectangle, Size, Vector, alignment, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, catalog, typography};
use anomi_widgets::composer::composer;
use yapfast_lang::tr;

use super::press::mix;

/// The well's least height, corners and the text's room above and below
/// (`height: max(96px, input.preferred-height + 16px)`).
const LEAST: f32 = 96.0;
const RADIUS: f32 = 6.0;
const PAD: f32 = 8.0;
/// The text sits 10 px in, the composer's own inset is 6.
const SIDE: f32 = 4.0;
/// The ring while the keys are in (`Theme.accent.with-alpha(0.8)`).
const RING_ALPHA: f32 = 0.8;
const GAP: f32 = 4.0;

pub struct Area<'a, M> {
    text: &'a str,
    placeholder: &'a str,
    left: Option<i32>,
    on_edit: Option<Box<dyn Fn(String) -> M + 'a>>,
    key: u64,
    id: Option<Id>,
    motion: Motion,
}

/// An area showing `text`; chain [`Area::on_edit`] or it takes nothing, as
/// a disabled one, and [`Area::left`] for the counter.
pub fn area<'a, M>(text: &'a str, motion: Motion) -> Area<'a, M> {
    Area {
        text,
        placeholder: "",
        left: None,
        on_edit: None,
        key: 0,
        id: None,
        motion,
    }
}

impl<'a, M: Clone + 'a> Area<'a, M> {
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// How many characters are left; below zero the counter says by how
    /// many it is too long. None draws no counter (`counted: false`).
    pub fn left(mut self, left: Option<i32>) -> Self {
        self.left = left;
        self
    }

    pub fn on_edit(mut self, on_edit: impl Fn(String) -> M + 'a) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    /// Whose text it shows: a new key shows its text afresh.
    pub fn key(mut self, key: u64) -> Self {
        self.key = key;
        self
    }

    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn view(self) -> Element<'a, M> {
        let mut field = composer(self.text, self.motion)
            .size(typography::size::SMALL)
            .pad(PAD)
            .min_height(LEAST)
            .placeholder(self.placeholder)
            .muted_placeholder()
            .key(self.key);
        if let Some(id) = self.id {
            field = field.id(id);
        }
        if let Some(on_edit) = self.on_edit {
            field = field.on_edit(move |text, _caret| on_edit(text));
        }
        let well = Rim {
            content: container(field).padding(Padding::from([0.0, SIDE])).boxed(),
            motion: self.motion,
        };
        let mut parts: Vec<Element<'a, M>> = vec![well.boxed()];
        if let Some(left) = self.left {
            let (words, style): (String, fn(&Theme) -> text::Style) = if left < 0 {
                (tr!("Too long by {}", -left), catalog::text::danger)
            } else {
                (tr!("{} left", left), catalog::text::muted)
            };
            parts.push(
                container(text(words).size(typography::size::CAPTION).style(style))
                    .width(Length::Fill)
                    .align_x(alignment::Horizontal::Right)
                    .boxed(),
            );
        }
        column(parts).spacing(GAP).boxed()
    }
}

/// The well round the field: it draws the ground and the rim, and the
/// accent ring in the rim's place while the field has the keys.
struct Rim<'a, M> {
    content: Element<'a, M>,
    motion: Motion,
}

/// Finds whether the first focusable under it has the keys.
struct Probe(Option<bool>);

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.0.is_none() {
            self.0 = Some(state.is_focused());
        }
    }
}

struct State {
    focused: bool,
    ring: Animation<bool>,
    now: Instant,
}

impl<M> widget::Meta for Rim<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Rim<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            focused: false,
            ring: Animation::new(false),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
        tree.size = child.size;
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
        let mut probe = Probe(None);
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .operate(child, child_layout, viewport, renderer, &mut probe);
        let state = tree.state.downcast_mut::<State>();
        let focused = probe.0.unwrap_or(false);
        if focused != state.focused {
            state.focused = focused;
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            state.ring = if self.motion.still() {
                Animation::new(state.focused)
            } else if state.ring.value() != state.focused {
                state
                    .ring
                    .clone()
                    .duration(QUICK)
                    .easing(Easing::Linear)
                    .go(state.focused, *now)
            } else {
                state.ring.clone()
            };
            if state.ring.is_animating(*now) {
                shell.request_redraw();
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
        let ring = state.ring.interpolate(0.0f32, 1.0, state.now);
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default().rounded(RADIUS).width(1).color(mix(
                    palette.line,
                    palette.accent.scale_alpha(RING_ALPHA),
                    ring,
                )),
                ..Quad::default()
            },
            palette.sunken,
        );
        let (child_layout, child) = layout.iter(&tree.children).next().expect("child");
        renderer.with_layer(layout.bounds(), |renderer| {
            self.content.draw(
                child,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
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
