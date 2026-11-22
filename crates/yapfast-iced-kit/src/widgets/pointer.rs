//! What the pointer does to a child: the timeline's presses ([`Watch`]),
//! a right press ([`RightPress`]), what shows under it ([`Hover`]), where a
//! button is as it's pressed ([`Spot`]), the wheel with Ctrl
//! ([`CtrlWheel`]), and a child that keeps the pointer's moves over it from
//! what it floats over ([`Holds`]).

use iced::{
    Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
    window,
};

use super::{At, Hooks, Inner, Renderer, Stateless, Wrap};

/// Its child, and what a left press nothing in it took and the left
/// button's release say: the timeline's selection is cleared by the first
/// and ends with the second, wherever the pointer is then.
pub struct Watch<M> {
    on_blank: Option<M>,
    on_release: Option<M>,
}

impl<M: Clone> Watch<M> {
    pub fn new<'a>(
        child: impl Into<Element<'a, M>>,
        on_blank: Option<M>,
        on_release: Option<M>,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                on_blank,
                on_release,
            },
        )
    }
}

impl<M: Clone> Hooks<M> for Watch<M> {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
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

/// Its child, which keeps the pointer's moves over it to itself: what it
/// floats over, drawn before it (a message's row under the hover bar that
/// reaches over it), doesn't take the pointer while it's over the child,
/// nor does what it's in say the pointer left. Lists that overlap so
/// ([`Rows`](super::Rows), iced's stack) hand events to what's drawn last
/// first.
pub struct Holds;

impl Holds {
    pub fn new<'a, M: Clone>(child: impl Into<Element<'a, M>>) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self)
    }
}

impl<M: Clone> Hooks<M> for Holds {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        child.update(shell);
        if let iced::Event::Mouse(mouse::Event::CursorMoved { .. }) = event
            && at.cursor.is_over(at.bounds)
        {
            shell.capture_event();
        }
    }
}

/// Its child, and a message with the point a right press on it gives, from
/// the corner of the viewport it's drawn in, for a context menu at the
/// pointer.
pub struct RightPress<'a, M> {
    on: Option<Box<dyn Fn(Point) -> M + 'a>>,
    spot: Option<&'a std::cell::Cell<Point>>,
    /// Sent with each press noted, for an app that acts on where it was.
    told: Option<M>,
}

impl<'a, M: Clone> RightPress<'a, M> {
    pub fn new(
        child: impl Into<Element<'a, M>>,
        on: impl Fn(Point) -> M + 'a,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                on: Some(Box::new(on)),
                spot: None,
                told: None,
            },
        )
    }

    /// Around the whole window: notes where in it each press is, before
    /// what's pressed hears of it, so a button that opens a menu can open
    /// it there without a message for each move of the pointer.
    pub fn spot(
        child: impl Into<Element<'a, M>>,
        spot: &'a std::cell::Cell<Point>,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                on: None,
                spot: Some(spot),
                told: None,
            },
        )
    }
}

impl<M: Clone> Wrap<'_, RightPress<'_, M>, M> {
    /// Sends `message` with each press noted, ahead of what the press
    /// itself sends: a press costs no message without it.
    pub fn telling(mut self, message: Option<M>) -> Self {
        self.hooks.told = message;
        self
    }
}

impl<M: Clone> Hooks<M> for RightPress<'_, M> {
    type State = Stateless;

    fn update(
        &mut self,
        _: &mut Stateless,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if let Some(spot) = self.spot
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(position) = at.cursor.position()
        {
            spot.set(position);
            if let Some(told) = &self.told {
                shell.publish(told.clone());
            }
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
pub struct Hover<'a, M> {
    base: Element<'a, M>,
    over: Element<'a, M>,
    kept: bool,
}

impl<'a, M: Clone> Hover<'a, M> {
    pub fn new(base: impl Into<Element<'a, M>>, over: impl Into<Element<'a, M>>) -> Self {
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

impl<M: Clone> Hover<'_, M> {
    fn shown(&self, tree: &Tree) -> bool {
        self.kept || tree.state.downcast_ref::<Hovered>().0
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Hover<'_, M> {
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
        shell: &mut Shell<'_, M>,
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
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
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

impl<'a, M: Clone + 'a> From<Hover<'a, M>> for Element<'a, M> {
    fn from(hover: Hover<'a, M>) -> Self {
        Element::new(hover)
    }
}

// ---------------------------------------------------------------------------
// Where a button is

/// Its child, noting its bounds in the window into `spot` as a press lands
/// on it, before the child hears of it: a button that opens a menu above
/// itself (a device toggle's caret, the call's share button) opens it
/// there. A button whose place moves while the menu is open — a window
/// resized, a panel opened beside it — moves the place it noted with it, and
/// `refresh` has the view built again where it is now.
pub struct Spot<'a, M> {
    spot: &'a std::cell::Cell<Rectangle>,
    from_press: bool,
    /// Sent when the bounds it noted move, so a menu opened above it is laid
    /// out where the button is now.
    refresh: Option<M>,
}

/// What a [`Spot`] keeps: the bounds it last noted. One `spot` cell is shared
/// by every button that opens a menu from it (the call's device carets, its
/// share buttons), so a button only moves the cell the press put there — the
/// one that opened the menu — rather than the one another button noted.
#[derive(Default)]
pub struct Noted {
    bounds: Rectangle,
}

impl Noted {
    /// Where the child is now, `bounds`, answering whether that place is one
    /// this button noted itself: the cell is only moved by the button whose
    /// press put its bounds there. The first sight of a button opens nothing,
    /// so it moves nothing either.
    fn again(&mut self, spot: &std::cell::Cell<Rectangle>, bounds: Rectangle) -> bool {
        let before = std::mem::replace(&mut self.bounds, bounds);
        before != bounds && before.area() > 0.0 && spot.get() == before
    }
}

impl<'a, M: Clone> Spot<'a, M> {
    pub fn new(
        child: impl Into<Element<'a, M>>,
        spot: &'a std::cell::Cell<Rectangle>,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                spot,
                from_press: false,
                refresh: None,
            },
        )
    }
}

impl<M: Clone> Wrap<'_, Spot<'_, M>, M> {
    /// Notes its bounds from the press instead, for a child in a list that
    /// scrolls, whose bounds aren't the window's: `App::pressed_spot` puts
    /// them in the window.
    pub fn by_press(mut self) -> Self {
        self.hooks.from_press = true;
        self
    }

    /// Sends `message` when the place it noted moves, which is what has a
    /// menu opened above it follow a window that changed size: the place is
    /// noted again and the view built again with it.
    pub fn refreshing(mut self, message: M) -> Self {
        self.hooks.refresh = Some(message);
        self
    }
}

impl<M: Clone> Hooks<M> for Spot<'_, M> {
    type State = Noted;

    fn update(
        &mut self,
        state: &mut Noted,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(position) = at.cursor.position_over(at.bounds)
        {
            state.bounds = at.bounds;
            self.spot.set(if self.from_press {
                at.bounds - (position - Point::ORIGIN)
            } else {
                at.bounds
            });
        } else if !self.from_press && state.again(self.spot, at.bounds) {
            // Not where it was, and it's this button's place the cell holds:
            // the window was resized, or a panel took room beside it, and what
            // opened above the button goes where it is now.
            self.spot.set(at.bounds);
            if let Some(refresh) = &self.refresh {
                shell.publish(refresh.clone());
            }
        }
        child.update(shell);
    }
}

// ---------------------------------------------------------------------------
// Ctrl and the wheel

/// Around the whole window: the wheel with Ctrl held zooms, and nothing
/// under it scrolls. What a turn of the wheel or a touchpad's pixels come
/// to is `turn`'s, over a `W` kept from one to the next, which `reset`
/// clears as Ctrl goes up or the window loses the focus: a step, once
/// there's enough for one.
pub struct CtrlWheel<M, W> {
    turn: fn(&mut W, mouse::ScrollDelta) -> Option<M>,
    reset: fn(&mut W),
}

impl<M: Clone, W: Default + 'static> CtrlWheel<M, W> {
    pub fn new<'a>(
        child: impl Into<Element<'a, M>>,
        turn: fn(&mut W, mouse::ScrollDelta) -> Option<M>,
        reset: fn(&mut W),
    ) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self { turn, reset })
    }
}

pub struct CtrlWheelState<W> {
    control: bool,
    wheel: W,
}

impl<W: Default> Default for CtrlWheelState<W> {
    fn default() -> Self {
        Self {
            control: false,
            wheel: W::default(),
        }
    }
}

impl<M: Clone, W: Default + 'static> Hooks<M> for CtrlWheel<M, W> {
    type State = CtrlWheelState<W>;

    fn update(
        &mut self,
        state: &mut CtrlWheelState<W>,
        event: &iced::Event,
        _: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        match event {
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.control = modifiers.control();
                if !state.control {
                    (self.reset)(&mut state.wheel);
                }
            }
            iced::Event::Window(window::Event::Unfocused) => {
                // Ctrl may be let go elsewhere.
                state.control = false;
                (self.reset)(&mut state.wheel);
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if state.control => {
                if let Some(message) = (self.turn)(&mut state.wheel, *delta) {
                    shell.publish(message);
                }
                shell.capture_event();
                return;
            }
            _ => {}
        }
        child.update(shell);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_button_whose_press_noted_its_place_moves_it() {
        let spot = std::cell::Cell::new(Rectangle::default());
        let aimed = Rectangle::new(Point::new(10.0, 20.0), Size::new(52.0, 52.0));
        let beside = Rectangle::new(Point::new(90.0, 20.0), Size::new(52.0, 52.0));
        let mut noted = Noted::default();
        // A button seen for the first time opens nothing, so it moves nothing.
        assert!(!noted.again(&spot, aimed));
        assert!(!noted.again(&spot, beside), "a place nothing opened from");
        // The caret that was pressed put its own bounds in the cell: when it
        // moves, it takes the cell with it.
        spot.set(aimed);
        let mut pressed = Noted { bounds: aimed };
        assert!(pressed.again(&spot, aimed.expand(4.0)));
        // A button whose bounds aren't the ones the cell holds leaves the
        // caret's place alone, however much it moves.
        assert!(!noted.again(&spot, beside.expand(8.0)));
    }
}
