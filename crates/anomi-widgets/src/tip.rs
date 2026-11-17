//! The interface's tooltip, and the frame of the badge's. The Slint window
//! has one tooltip, drawn above everything (`tip-card`,
//! `ui/yapfast.slint:5633`), which whatever the pointer is on fills through
//! the `Tip` global (`ui/theme.slint:119`): its text, and its side, above,
//! right of or below what it names. A badge that levels up shows a larger
//! card above it the same way (`BadgeTip`, `:131`; the card itself is
//! P08's). `TipIcon` (`ui/controls.slint:1895`) is an icon with a tooltip.
//!
//! Here each widget that names something wraps it in a [`Tip`], which shows
//! the card as an overlay while the pointer is on it, until a click hides
//! it, and again once the pointer leaves and comes back. The card arrives
//! as `PopupEntry` has it, growing from the edge that faces what it names
//! over `POPUP`, and leaves at once; a still interface shows it whole.
//!
//! The text card: the tooltip colour, 6 px corners, a soft shadow, 14 px
//! bold text 10 px from its sides, 32 px tall; centred over or under what
//! it names, kept 8 px from the window's sides and 4 px from its top, or
//! 10 px right of it, centred on its height.

mod card;
#[cfg(test)]
mod tests;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{Pace, Popup};
use anomi_theme::{Element, Motion, Renderer, Theme, View};

use crate::icon::{Icon, icon};

/// Where the card sits against what it names: `Tip.side` 0, 1 and 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    Above,
    Right,
    Below,
}

impl Side {
    /// Between the card and what it names (`ui/yapfast.slint:5637`, `:5638`).
    fn gap(self) -> f32 {
        match self {
            Self::Right => 10.0,
            Self::Above | Self::Below => 6.0,
        }
    }
}

/// Between a badge and its card (`ui/yapfast.slint:5650`).
const BADGE_GAP: f32 = 10.0;

enum Face<'a, M> {
    Text(&'a str),
    Card(Element<'a, M>),
}

pub struct Tip<'a, M> {
    content: Element<'a, M>,
    face: Face<'a, M>,
    side: Side,
    gap: f32,
    motion: Motion,
}

/// `content` with `text` beside it while the pointer is on it; no tooltip
/// for empty text.
pub fn tip<'a, M>(
    content: impl View<M> + 'a,
    text: &'a str,
    side: Side,
    motion: Motion,
) -> Tip<'a, M> {
    Tip {
        content: content.boxed(),
        face: Face::Text(text),
        side,
        gap: side.gap(),
        motion,
    }
}

/// `content` with a card of its own above it while the pointer is on it,
/// as a badge that levels up shows its level (`BadgeTip`).
pub fn tip_card<'a, M: 'a>(
    content: impl View<M> + 'a,
    card: impl View<M> + 'a,
    motion: Motion,
) -> Tip<'a, M> {
    Tip {
        content: content.boxed(),
        face: Face::Card(card.boxed()),
        side: Side::Above,
        gap: BADGE_GAP,
        motion,
    }
}

/// An icon whose meaning shows above it on hover, as Discord's signals
/// have it: `TipIcon`, 18 px, tinted from the theme.
pub fn tip_icon<'a, M: 'a>(
    glyph: Icon,
    tint: impl Fn(&Theme) -> Color + 'a,
    text: &'a str,
    motion: Motion,
) -> Tip<'a, M> {
    let face =
        icon(glyph, TIP_ICON).style(move |theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(tint(theme)),
        });
    tip(face, text, Side::Above, motion)
}

/// `TipIcon`'s side (`ui/controls.slint:1897`).
const TIP_ICON: f32 = 18.0;

impl<M> Tip<'_, M> {
    /// Another space between the card and what it names.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    fn shows(&self) -> bool {
        match &self.face {
            Face::Text(text) => !text.is_empty(),
            Face::Card(_) => true,
        }
    }
}

struct State {
    hovered: bool,
    /// A click hid it until the pointer leaves.
    hidden: bool,
    entry: Popup,
    now: Instant,
    /// The text's width, with the hash of the text it was measured for:
    /// the card is built at each event and frame while it shows.
    measured: Option<(u64, f32)>,
}

impl State {
    /// The width of `text` on the card, measured once.
    fn width(&mut self, text: &str) -> f32 {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        match self.measured {
            Some((measured, width)) if measured == key => width,
            _ => {
                let width = card::text_width(text);
                self.measured = Some((key, width));
                width
            }
        }
    }
}

impl<M> widget::Meta for Tip<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Tip<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            hidden: false,
            entry: Popup::closed(Pace::Popover),
            now: Instant::now(),
            measured: None,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        match &mut self.face {
            Face::Text(_) => tree.diff_children(std::slice::from_mut(&mut self.content)),
            Face::Card(card) => tree.diff_children(&mut [&mut self.content, card]),
        }
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
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
        let over = cursor.is_over(layout.bounds());
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != state.hovered {
                    state.hovered = over;
                    state.hidden = false;
                    state.entry = if over {
                        Popup::entering(Pace::Popover, self.motion, Instant::now())
                    } else {
                        Popup::closed(Pace::Popover)
                    };
                    shell.invalidate_overlay();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(_)) if over && !state.hidden => {
                state.hidden = true;
                shell.invalidate_overlay();
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if state.entry.is_animating(*now) {
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
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
        let shows = self.shows();
        let Tree {
            state, children, ..
        } = tree;
        let state = state.downcast_mut::<State>();
        let (content_tree, card_tree) = match children.split_first_mut() {
            Some((first, rest)) => (first, rest.first_mut()),
            None => return Vec::new(),
        };
        let content_layout = layout
            .iter(std::slice::from_ref(content_tree))
            .next()
            .unwrap()
            .0;
        let mut overlays = self.content.overlay(
            content_tree,
            content_layout,
            renderer,
            viewport,
            translation,
            window,
        );
        if state.hovered && !state.hidden && shows {
            let amount = state.entry.amount(state.now);
            let place = card::Place {
                anchor,
                side: self.side,
                gap: self.gap,
                window,
                amount,
            };
            let face = match (&mut self.face, card_tree) {
                (Face::Text(text), _) => card::Face::Text(text, state.width(text)),
                (Face::Card(element), Some(tree)) => card::Face::Card(element, tree),
                (Face::Card(_), None) => return overlays,
            };
            overlays.push(card::overlay(face, place, renderer));
        }
        overlays
    }
}
