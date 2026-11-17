//! The card's avatar and name as one thing to point at: under the pointer
//! the name's box fills with a see-through wash (Discord's own, so the
//! nameplate shows through) and the username takes the status's place.
//! The Slint side: `account-touch` and `avatar-touch`
//! (`ui/yapfast.slint:3055`-`:3080`).
//!
//! It sends nothing itself: wrap it in `menu::opener` for the click, which
//! tells where the card is. It asks for frames only while the wash eases.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme, metrics};

/// Between the avatar and the name's box (`spacing: 5px`).
const GAP: f32 = 5.0;
/// The box's room above and below it (`y: 6px; height: parent - 12px`).
const MARGIN: f32 = 6.0;
/// The words' room inside the box (`padding-left: 6px`).
const INSET: f32 = 6.0;
/// The wash's alpha under the pointer and pressed (`Theme.overlay`).
const WASH: f32 = 0.08;
const WASH_PRESSED: f32 = 0.12;

struct State {
    over: bool,
    pressed: bool,
    lit: Animation<bool>,
    duration: Duration,
    now: Instant,
}

pub struct Pointed<'a, Message> {
    /// The avatar's cell at rest, the status, the username, then the avatar's
    /// cell playing (it shows while the pointer is on the card).
    parts: [Element<'a, Message>; 4],
    /// It reacts to the pointer: a linked account's.
    enabled: bool,
    motion: Motion,
}

/// `lead` beside the name's box, which shows `rest` and, while the pointer
/// is on the pair, `hovered` in its place and `playing` in `lead`'s: the
/// same avatar, built to animate, as Slint plays it while the card is hot.
pub fn pointed<'a, Message>(
    lead: Element<'a, Message>,
    playing: Element<'a, Message>,
    rest: Element<'a, Message>,
    hovered: Element<'a, Message>,
    enabled: bool,
    motion: Motion,
) -> Pointed<'a, Message> {
    Pointed {
        parts: [lead, rest, hovered, playing],
        enabled,
        motion,
    }
}

impl<Message> widget::Meta for Pointed<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Pointed<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            over: false,
            pressed: false,
            lit: Animation::new(false)
                .duration(self.motion.duration(QUICK))
                .easing(Easing::EaseOut),
            duration: self.motion.duration(QUICK),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let bounds = limits.bounds();
        let loose = layout::Limits::new(Size::ZERO, bounds);
        let mut lead = Size::ZERO;
        for index in [0, 3] {
            self.parts[index].layout(&mut tree.children[index], renderer, &loose);
            lead = tree.children[index].size;
            tree.children[index].translation =
                Vector::new(0.0, (bounds.height - lead.height) / 2.0);
        }
        let x = lead.width + GAP;
        let room = Size::new(
            (bounds.width - x - 2.0 * INSET).max(0.0),
            (bounds.height - 2.0 * MARGIN).max(0.0),
        );
        for index in [1, 2] {
            // Both are laid out as the box is, one of them shows.
            self.parts[index].layout(
                &mut tree.children[index],
                renderer,
                &layout::Limits::new(room, room),
            );
            tree.children[index].translation = Vector::new(x + INSET, MARGIN);
        }
        tree.size = bounds;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        // What shows is what operations see: the avatar and the status, or
        // the username while the pointer is on it.
        let (lead, shown) = shown(tree.state.downcast_ref::<State>().over);
        operation.traverse(&mut |operation| {
            for (index, (layout, tree)) in layout.iter_mut(&mut tree.children).enumerate() {
                if index == lead || index == shown {
                    self.parts[index].operate(tree, layout, viewport, renderer, operation);
                }
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let over = self.enabled && cursor.is_over(self.hit(layout));
        if over != state.over {
            state.over = over;
            shell.request_redraw();
        }
        // The parts on show advance: the avatar, its decoration and the
        // status's emoji play from the frames they are given.
        let (lead, text) = shown(over);
        for (index, (layout, tree)) in layout.iter_mut(&mut tree.children).enumerate() {
            if index == lead || index == text {
                self.parts[index].update(tree, event, layout, cursor, renderer, shell, viewport);
            }
        }
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = true;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if state.lit.value() != state.over {
                    let duration = self.motion.duration(QUICK);
                    if duration != state.duration {
                        state.duration = duration;
                        state.lit = std::mem::replace(&mut state.lit, Animation::new(false))
                            .duration(duration);
                    }
                    state.lit.go_mut(state.over, *now);
                }
                if state.lit.is_animating(*now) {
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
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<State>();
        let mut children = layout.iter(&tree.children);
        let (lead, lead_tree) = children.next().expect("the avatar");
        let (rest, rest_tree) = children.next().expect("the status");
        let (hovered, hovered_tree) = children.next().expect("the username");
        let (playing, playing_tree) = children.next().expect("the playing avatar");
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        if lit > 0.0 {
            let alpha = if state.pressed { WASH_PRESSED } else { WASH } * lit;
            let overlay = theme.palette().overlay;
            let wash = Rectangle {
                x: rest.bounds().x - INSET,
                y: layout.bounds().y + MARGIN,
                width: rest.bounds().width + INSET * 2.0,
                height: rest.bounds().height,
            };
            renderer.fill_quad(
                Quad {
                    bounds: wash,
                    border: Border::default().rounded(metrics::RADIUS_ROW),
                    ..Quad::default()
                },
                Color {
                    a: alpha,
                    ..overlay
                },
            );
        }
        if state.over {
            self.parts[3].draw(
                playing_tree,
                renderer,
                theme,
                style,
                playing,
                cursor,
                viewport,
            );
        } else {
            self.parts[0].draw(lead_tree, renderer, theme, style, lead, cursor, viewport);
        }
        // The username is shown with the wash, not before it has landed: it
        // follows the pointer.
        if state.over {
            self.parts[2].draw(
                hovered_tree,
                renderer,
                theme,
                style,
                hovered,
                cursor,
                viewport,
            );
        } else {
            self.parts[1].draw(rest_tree, renderer, theme, style, rest, cursor, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let over = self.enabled && cursor.is_over(self.hit(layout));
        let (lead, text) = shown(over);
        let inner = layout
            .iter(&tree.children)
            .enumerate()
            .filter(|(index, _)| *index == lead || *index == text)
            .map(|(index, (layout, tree))| {
                self.parts[index].mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default();
        if inner == mouse::Interaction::None && over {
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
    ) -> Vec<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let (lead, _) = shown(tree.state.downcast_ref::<State>().over);
        let Some((layout, tree)) = layout.iter_mut(&mut tree.children).nth(lead) else {
            return Vec::new();
        };
        self.parts[lead].overlay(tree, layout, renderer, viewport, translation, window)
    }
}

/// The avatar's cell and the name's box on show, by their places: at rest
/// the still avatar and the status, under the pointer the playing avatar
/// and the username.
fn shown(over: bool) -> (usize, usize) {
    if over { (3, 2) } else { (0, 1) }
}

impl<Message> Pointed<'_, Message> {
    /// What answers the pointer: the avatar's cell and the name's box, the
    /// whole of the widget.
    fn hit(&self, layout: Layout) -> Rectangle {
        layout.bounds()
    }
}
