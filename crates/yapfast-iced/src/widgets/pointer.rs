//! What the pointer does to a child: the timeline's presses ([`Watch`]),
//! a right press ([`RightPress`]), what shows under it ([`Hover`]), where a
//! button is as it's pressed ([`Spot`]), and the wheel with Ctrl
//! ([`CtrlWheel`]).

use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
    window,
};

use crate::app::Message;

use super::{At, Hooks, Inner, Renderer, Stateless, Wrap};

/// Its child, and what a left press nothing in it took and the left
/// button's release say: the timeline's selection is cleared by the first
/// and ends with the second, wherever the pointer is then.
pub struct Watch {
    on_blank: Option<Message>,
    on_release: Option<Message>,
}

impl Watch {
    pub fn new<'a>(
        child: impl Into<Element<'a, Message>>,
        on_blank: Option<Message>,
        on_release: Option<Message>,
    ) -> Wrap<'a, Self> {
        Wrap::new(
            child,
            Self {
                on_blank,
                on_release,
            },
        )
    }
}

impl Hooks for Watch {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_>,
    ) {
        child.update(shell);
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if !shell.is_event_captured() && at.cursor.is_over(at.bounds) =>
            {
                if let Some(message) = &self.on_blank {
                    shell.publish(message.clone());
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(message) = &self.on_release {
                    shell.publish(message.clone());
                }
            }
            _ => {}
        }
    }
}

/// Its child, and a message with the point a right press on it gives, from
/// the corner of the viewport it's drawn in, for a context menu at the
/// pointer.
pub struct RightPress<'a> {
    on: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    spot: Option<&'a std::cell::Cell<Point>>,
}

impl<'a> RightPress<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        on: impl Fn(Point) -> Message + 'a,
    ) -> Wrap<'a, Self> {
        Wrap::new(
            child,
            Self {
                on: Some(Box::new(on)),
                spot: None,
            },
        )
    }

    /// Around the whole window: notes where in it each press is, before
    /// what's pressed hears of it, so a button that opens a menu can open
    /// it there without a message for each move of the pointer.
    pub fn spot(
        child: impl Into<Element<'a, Message>>,
        spot: &'a std::cell::Cell<Point>,
    ) -> Wrap<'a, Self> {
        Wrap::new(
            child,
            Self {
                on: None,
                spot: Some(spot),
            },
        )
    }
}

impl Hooks for RightPress<'_> {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_>,
    ) {
        if let Some(spot) = self.spot
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(position) = at.cursor.position()
        {
            spot.set(position);
        }
        child.update(shell);
        if let Some(on) = &self.on
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && !shell.is_event_captured()
            && let Some(position) = at.cursor.position_over(at.bounds)
            && at.viewport.contains(position)
        {
            // From the corner of what shows of the list it's in: a
            // scrollable's children see the pointer in the list's own
            // space, scrolled.
            shell.publish(on(position - Vector::new(at.viewport.x, at.viewport.y)));
            shell.capture_event();
        }
    }
}

// ---------------------------------------------------------------------------
// What shows under the pointer

/// Its child, with `over` laid on it, in a layer of its own, only while
/// the pointer is on it (or while `kept`): a tile's buttons, a picture in
/// picture's title, a video's controls. The pointer coming and going costs
/// a redraw, not a message.
pub struct Hover<'a> {
    base: Element<'a, Message>,
    over: Element<'a, Message>,
    kept: bool,
}

impl<'a> Hover<'a> {
    pub fn new(
        base: impl Into<Element<'a, Message>>,
        over: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            base: base.into(),
            over: over.into(),
            kept: false,
        }
    }

    /// Shows `over` whether the pointer is on it or not.
    pub fn kept(mut self, kept: bool) -> Self {
        self.kept = kept;
        self
    }
}

/// Whether the pointer is on a [`Hover`].
#[derive(Default)]
struct Hovered(bool);

impl Hover<'_> {
    fn shown(&self, tree: &Tree) -> bool {
        self.kept || tree.state.downcast_ref::<Hovered>().0
    }
}

impl Widget<Message, Theme, Renderer> for Hover<'_> {
    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hovered>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hovered::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.base), Tree::new(&self.over)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.base, &self.over]);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let base = self
            .base
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let size = base.size();
        let over = self.over.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        layout::Node::with_children(size, vec![base, over])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut children = layout.children();
        let (base, over) = (
            children.next().expect("base"),
            children.next().expect("over"),
        );
        self.base
            .as_widget_mut()
            .operate(&mut tree.children[0], base, renderer, operation);
        self.over
            .as_widget_mut()
            .operate(&mut tree.children[1], over, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let on = match event {
            iced::Event::Mouse(mouse::Event::CursorLeft)
            | iced::Event::Window(window::Event::Unfocused) => Some(false),
            iced::Event::Mouse(_) => Some(cursor.is_over(layout.bounds())),
            _ => None,
        };
        if let Some(on) = on {
            let hovered = tree.state.downcast_mut::<Hovered>();
            if hovered.0 != on {
                hovered.0 = on;
                shell.request_redraw();
            }
        }
        let mut children = layout.children();
        let (base, over) = (
            children.next().expect("base"),
            children.next().expect("over"),
        );
        if self.shown(tree) {
            self.over.as_widget_mut().update(
                &mut tree.children[1],
                event,
                over,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        self.base.as_widget_mut().update(
            &mut tree.children[0],
            event,
            base,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let mut children = layout.children();
        let (base, over) = (
            children.next().expect("base"),
            children.next().expect("over"),
        );
        if self.shown(tree) {
            let interaction = self.over.as_widget().mouse_interaction(
                &tree.children[1],
                over,
                cursor,
                viewport,
                renderer,
            );
            if interaction != mouse::Interaction::None {
                return interaction;
            }
        }
        self.base
            .as_widget()
            .mouse_interaction(&tree.children[0], base, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let mut children = layout.children();
        let (base, over) = (
            children.next().expect("base"),
            children.next().expect("over"),
        );
        self.base.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            base,
            cursor,
            viewport,
        );
        if self.shown(tree) {
            renderer.with_layer(layout.bounds(), |renderer| {
                self.over.as_widget().draw(
                    &tree.children[1],
                    renderer,
                    theme,
                    style,
                    over,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let shown = self.shown(tree);
        let mut children = layout.children();
        let (base, over) = (
            children.next().expect("base"),
            children.next().expect("over"),
        );
        let (base_tree, over_tree) = tree.children.split_at_mut(1);
        let mut overlays = Vec::new();
        if let Some(overlay) = self.base.as_widget_mut().overlay(
            &mut base_tree[0],
            base,
            renderer,
            viewport,
            translation,
        ) {
            overlays.push(overlay);
        }
        if shown
            && let Some(overlay) = self.over.as_widget_mut().overlay(
                &mut over_tree[0],
                over,
                renderer,
                viewport,
                translation,
            )
        {
            overlays.push(overlay);
        }
        (!overlays.is_empty())
            .then(|| iced::advanced::overlay::Group::with_children(overlays).overlay())
    }
}

impl<'a> From<Hover<'a>> for Element<'a, Message> {
    fn from(hover: Hover<'a>) -> Self {
        Element::new(hover)
    }
}

// ---------------------------------------------------------------------------
// Where a button is

/// Its child, noting its bounds in the window into `spot` as a press lands
/// on it, before the child hears of it: a button that opens a menu above
/// itself (a device toggle's caret, the call's share button) opens it
/// there, as the Slint client places its popups by the button's corner.
pub struct Spot<'a> {
    spot: &'a std::cell::Cell<Rectangle>,
    from_press: bool,
}

impl<'a> Spot<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        spot: &'a std::cell::Cell<Rectangle>,
    ) -> Wrap<'a, Self> {
        Wrap::new(
            child,
            Self {
                spot,
                from_press: false,
            },
        )
    }
}

impl Wrap<'_, Spot<'_>> {
    /// Notes its bounds from the press instead, for a child in a list that
    /// scrolls, whose bounds aren't the window's: `App::pressed_spot` puts
    /// them in the window.
    pub fn by_press(mut self) -> Self {
        self.hooks.from_press = true;
        self
    }
}

impl Hooks for Spot<'_> {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_>,
    ) {
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(position) = at.cursor.position_over(at.bounds)
        {
            self.spot.set(if self.from_press {
                at.bounds - (position - Point::ORIGIN)
            } else {
                at.bounds
            });
        }
        child.update(shell);
    }
}

// ---------------------------------------------------------------------------
// Ctrl and the wheel

/// Around the whole window: the wheel with Ctrl held zooms (`on`, with the
/// step `zoom::Wheel` makes of it, a notch or 60 px of a touchpad), as
/// Slint's `zoom_keys` takes it, and nothing under it scrolls.
pub struct CtrlWheel {
    on: fn(crate::zoom::Step) -> Message,
}

impl CtrlWheel {
    pub fn new<'a>(
        child: impl Into<Element<'a, Message>>,
        on: fn(crate::zoom::Step) -> Message,
    ) -> Wrap<'a, Self> {
        Wrap::new(child, Self { on })
    }
}

#[derive(Default)]
pub struct CtrlWheelState {
    control: bool,
    wheel: crate::zoom::Wheel,
}

impl Hooks for CtrlWheel {
    type State = CtrlWheelState;

    fn update(
        &mut self,
        state: &mut CtrlWheelState,
        event: &iced::Event,
        _: At,
        shell: &mut Shell<'_, Message>,
        child: &mut Inner<'_, '_>,
    ) {
        match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.control = modifiers.control();
                if !state.control {
                    state.wheel.reset();
                }
            }
            iced::Event::Window(window::Event::Unfocused) => {
                // Ctrl may be let go elsewhere.
                state.control = false;
                state.wheel.reset();
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if state.control => {
                let step = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => state.wheel.lines(f64::from(*y)),
                    mouse::ScrollDelta::Pixels { y, .. } => state.wheel.pixels(f64::from(*y)),
                };
                if let Some(step) = step {
                    shell.publish((self.on)(step));
                }
                shell.capture_event();
                return;
            }
            _ => {}
        }
        child.update(shell);
    }
}
