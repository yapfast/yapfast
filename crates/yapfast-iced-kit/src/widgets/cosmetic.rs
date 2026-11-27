//! What's drawn round a child: a member row's nameplate ([`PlateRow`]) and
//! a profile frame round a card ([`Framed`]).

use iced::{
    Color, Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        image::Handle,
        layout, mouse, renderer,
        widget::{Operation, Tree},
    },
};

use super::{At, Hooks, Renderer, Stateless, Wrap, draw_picture, gradient_quad};

/// A member's row with the nameplate behind it: the picture at the row's
/// right end, faded to .3 of its strength under the text and in full 50 px
/// after it, and the palette's colour from nothing at the left edge.
/// `content` is the row: the avatar, then the text column, whose width says
/// where the fade ends.
pub struct PlateRow<'a, M> {
    content: Element<'a, M>,
    plate: Option<Element<'a, M>>,
    palette: Option<Color>,
    /// What lies under the plate, which the fade lets through.
    ground: Color,
    /// The palette's strength at the right end.
    end: f32,
    /// The whole plate's, .5 for someone offline.
    opacity: f32,
    fade_from: f32,
    /// The account card's plate: no fade under the text, and the palette
    /// from .1 to .4.
    account: bool,
    /// The corners of the row it fills, which its tints take.
    radius: f32,
}

impl<'a, M> PlateRow<'a, M> {
    pub fn new(
        content: impl Into<Element<'a, M>>,
        plate: Option<Element<'a, M>>,
        palette: Option<Color>,
        ground: Color,
        end: f32,
        opacity: f32,
    ) -> Self {
        Self {
            content: content.into(),
            plate,
            palette,
            ground,
            end,
            opacity,
            fade_from: 0.0,
            account: false,
            radius: PLATE_RADIUS,
        }
    }

    /// The corners of the row the plate fills, where they aren't 8 px.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// The account's own plate, on its card or a tile: the picture isn't
    /// faded under the text, and the palette runs from .1 to .4.
    pub fn account(mut self) -> Self {
        self.account = true;
        self
    }

    fn children_mut(&mut self) -> impl Iterator<Item = &mut Element<'a, M>> {
        std::iter::once(&mut self.content).chain(self.plate.as_mut())
    }
}

/// The corners of a row's nameplate and its tints, unless the row says.
const PLATE_RADIUS: f32 = 8.0;

impl<M> Widget<M, Theme, Renderer> for PlateRow<'_, M> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.content)
            .chain(self.plate.as_ref())
            .map(Tree::new)
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        match &self.plate {
            Some(plate) => tree.diff_children(&[&self.content, plate]),
            None => tree.diff_children(std::slice::from_ref(&self.content)),
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let mut trees = tree.children.iter_mut();
        let content =
            self.content
                .as_widget_mut()
                .layout(trees.next().expect("content's"), renderer, limits);
        let size = content.size();
        // How much of the plate is faded: the text column's width, from the
        // plate's left.
        self.fade_from = content
            .children()
            .get(1)
            .map_or(0.0, |column| column.size().width + 10.0)
            .min(size.width);
        let mut nodes = vec![content];
        if let Some(plate) = &mut self.plate {
            let node = plate.as_widget_mut().layout(
                trees.next().expect("plate's"),
                renderer,
                &layout::Limits::new(Size::ZERO, Size::new(f32::INFINITY, size.height)),
            );
            let x = size.width - node.size().width;
            nodes.push(node.move_to((x, 0.0)));
        }
        layout::Node::with_children(size, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
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
        for ((child, state), layout) in self
            .children_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().expect("content's"),
            cursor,
            viewport,
            renderer,
        )
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
        let bounds = layout.bounds();
        let mut layouts = layout.children();
        let content_layout = layouts.next().expect("content's");
        let above = |renderer: &mut Renderer, faded: bool| {
            if faded {
                // The fade under the text.
                let width = bounds.width.max(1.0);
                let from = self.fade_from / width;
                let to = ((self.fade_from + 50.0) / width).min(1.0);
                let ground = |alpha: f32| Color {
                    a: alpha * self.opacity,
                    ..self.ground
                };
                gradient_quad(
                    renderer,
                    bounds,
                    self.radius,
                    90.0,
                    &[
                        (0.0, ground(0.7)),
                        (from, ground(0.7)),
                        (to, ground(0.0)),
                        (1.0, ground(0.0)),
                    ],
                );
            }
            if let Some(palette) = self.palette {
                let tint = |alpha: f32| Color {
                    a: alpha * self.opacity,
                    ..palette
                };
                if self.account {
                    gradient_quad(
                        renderer,
                        bounds,
                        self.radius,
                        90.0,
                        &[(0.0, tint(0.1)), (1.0, tint(0.4))],
                    );
                } else {
                    gradient_quad(
                        renderer,
                        bounds,
                        self.radius,
                        90.0,
                        &[
                            (0.0, tint(0.0)),
                            (0.2, tint(0.08)),
                            (0.5, tint(0.08)),
                            (1.0, tint(self.end)),
                        ],
                    );
                }
            }
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                content_layout,
                cursor,
                viewport,
            );
        };
        match (&self.plate, layouts.next()) {
            (Some(plate), Some(plate_layout)) => {
                plate.as_widget().draw(
                    &tree.children[1],
                    renderer,
                    theme,
                    style,
                    plate_layout,
                    cursor,
                    viewport,
                );
                // iced draws a layer's quads, then its pictures, then its
                // text: what goes over the plate's picture is a layer above.
                renderer.with_layer(bounds, |renderer| above(renderer, !self.account));
            }
            _ => above(renderer, false),
        }
    }

    /// The row's own overlays: its tooltips (a booster's gem).
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, M, Theme, Renderer>> {
        let content = layout.children().next()?;
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            content,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: 'a> From<PlateRow<'a, M>> for Element<'a, M> {
    fn from(row: PlateRow<'a, M>) -> Self {
        Element::new(row)
    }
}

// ---------------------------------------------------------------------------
// A profile frame round a card

/// Where a frame's layer lies on the card, as Discord's stylesheet draws
/// it: across the card and past its sides by [`Framed`]'s `side`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// Once, past the end it's anchored to.
    Staple,
    /// The card's height, its picture at its anchor.
    Rail,
    /// Repeated down the card.
    Border,
}

/// The end of the card a frame's layer is anchored to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameAnchor {
    Top,
    Center,
    Bottom,
}

/// One layer of a profile frame: its picture at the frame's width and
/// `height`, and whether it lies over the card.
pub struct FrameLayer {
    pub picture: Handle,
    pub height: f32,
    pub kind: FrameKind,
    pub anchor: FrameAnchor,
    pub front: bool,
}

/// Discord draws its frames 1312 px wide, and a staple keeps the same part
/// over the card whatever its height: 412 at the top, 212 at the foot.
const FRAME_ART: f32 = 1312.0;
const STAPLE_TOP: f32 = 412.0;
const STAPLE_FOOT: f32 = 212.0;

/// A card with a profile frame round it: the back layers under the card,
/// the front ones over it, each `side` past its left and right; a staple at
/// the top sits `top_shift` lower (the full profile's 8 px). Its layout is
/// the card's; the frame draws past it and takes no clicks.
pub struct Framed {
    layers: Vec<FrameLayer>,
    side: f32,
    top_shift: f32,
}

impl Framed {
    pub fn new<'a, M>(
        child: impl Into<Element<'a, M>>,
        layers: Vec<FrameLayer>,
        side: f32,
        top_shift: f32,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                layers,
                side,
                top_shift,
            },
        )
    }

    fn draw_layers(&self, renderer: &mut Renderer, card: Rectangle, front: bool) {
        use iced::advanced::Renderer as _;
        let (x, width) = (card.x - self.side, card.width + 2.0 * self.side);
        let whole = Rectangle::new(Point::new(x, card.y), Size::new(width, card.height));
        let layers = || self.layers.iter().filter(move |layer| layer.front == front);
        let at = |y: f32, height: f32| {
            Rectangle::new(Point::new(x, card.y + y), Size::new(width, height))
        };
        // A staple reaches past the card's end; the renderer clips by layer,
        // so the rails and borders, which keep to the card's height, go in
        // one of the card's size.
        for layer in layers().filter(|layer| layer.kind == FrameKind::Staple) {
            let y = match layer.anchor {
                FrameAnchor::Bottom => card.height - width * STAPLE_FOOT / FRAME_ART,
                FrameAnchor::Center => (card.height - layer.height) / 2.0,
                FrameAnchor::Top => width * STAPLE_TOP / FRAME_ART + self.top_shift - layer.height,
            };
            draw_picture(renderer, &layer.picture, at(y, layer.height), [0.0; 4], 1.0);
        }
        if !layers().any(|layer| layer.kind != FrameKind::Staple) {
            return;
        }
        renderer.with_layer(whole, |renderer| {
            for layer in layers() {
                let height = layer.height;
                match layer.kind {
                    FrameKind::Staple => {}
                    FrameKind::Rail => {
                        let y = match layer.anchor {
                            FrameAnchor::Bottom => card.height - height,
                            FrameAnchor::Center => (card.height - height) / 2.0,
                            FrameAnchor::Top => 0.0,
                        };
                        draw_picture(renderer, &layer.picture, at(y, height), [0.0; 4], 1.0);
                    }
                    FrameKind::Border if height > 0.0 => {
                        let copies = (card.height / height).ceil() as usize + 1;
                        for copy in 0..copies {
                            draw_picture(
                                renderer,
                                &layer.picture,
                                at(copy as f32 * height, height),
                                [0.0; 4],
                                1.0,
                            );
                        }
                    }
                    FrameKind::Border => {}
                }
            }
        });
    }
}

impl<M> Hooks<M> for Framed {
    type State = Stateless;

    fn draw(
        &self,
        _: &Stateless,
        renderer: &mut Renderer,
        at: At,
        child: &mut dyn FnMut(&mut Renderer),
    ) {
        use iced::advanced::Renderer as _;
        let card = at.bounds;
        // The back layers under the card's ground, the card in a layer of
        // its own over them, and the front ones in another over it.
        self.draw_layers(renderer, card, false);
        let room = at.viewport.expand(self.side);
        renderer.with_layer(room, |renderer| child(renderer));
        if self.layers.iter().any(|layer| layer.front) {
            renderer.with_layer(room, |renderer| self.draw_layers(renderer, card, true));
        }
    }
}
