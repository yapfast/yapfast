//! The client's own widgets: pictures drawn one to one (still and moving),
//! a virtualized list's rows kept by key, a member row with its nameplate
//! behind it, overlays placed in the window, a layer of its own, and the
//! timeline's presses.
//!
//! Each wrapper forwards `overlay()` to its children: iced collects
//! tooltips through it, and a wrapper that didn't would hide every tooltip
//! inside.

use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};

use iced::{
    Background, Border, Color, Element, Length, Point, Rectangle, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget,
        image::{self as raster, Handle},
        layout, mouse, renderer,
        widget::{Operation, Tree, tree},
    },
    gradient,
    widget::{Container, container},
    window,
};
use yapfast_core::chat::Picture as CorePicture;

use crate::{app::Message, paint, pictures::Player};

type Renderer = iced::Renderer;

/// A picture drawn in `bounds`, scaled to it if it isn't its size.
pub fn draw_picture(
    renderer: &mut Renderer,
    handle: &Handle,
    bounds: Rectangle,
    radius: [f32; 4],
    opacity: f32,
) {
    use iced::advanced::image::Renderer as _;
    let [top_left, top_right, bottom_right, bottom_left] = radius;
    renderer.draw_image(
        raster::Image {
            handle: handle.clone(),
            filter_method: raster::FilterMethod::Linear,
            rotation: iced::Radians(0.0),
            border_radius: iced::border::Radius {
                top_left,
                top_right,
                bottom_right,
                bottom_left,
            },
            opacity,
            snap: true,
        },
        bounds,
        bounds,
    );
}

// ---------------------------------------------------------------------------
// Pictures

/// A picture drawn one to one (the patched renderer copies it in), with
/// round corners and an opacity. It takes `size` in the layout but may be
/// drawn elsewhere around it (`at`, from the layout's corner), as a
/// decoration overhangs its avatar.
pub struct Pic {
    handle: Handle,
    size: Size,
    at: Rectangle,
    radius: [f32; 4],
    opacity: f32,
}

impl Pic {
    pub fn new(handle: Handle, size: Size) -> Self {
        Self {
            handle,
            size,
            at: Rectangle::with_size(size),
            radius: [0.0; 4],
            opacity: 1.0,
        }
    }

    /// A square picture, `side` across.
    pub fn square(handle: Handle, side: f32) -> Self {
        Self::new(handle, Size::new(side, side))
    }

    /// Centred in a box `side` across, as an icon on a button's face or a
    /// badge; the box can be styled further.
    pub fn boxed<'a>(self, side: impl Into<Length>) -> Container<'a, Message, Theme, Renderer> {
        container(self).center(side)
    }

    pub fn at(mut self, at: Rectangle) -> Self {
        self.at = at;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = [radius; 4];
        self
    }

    pub fn radius_each(mut self, radius: [f32; 4]) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Pic {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.size.width),
            Length::Fixed(self.size.height),
        )
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        draw_picture(renderer, &self.handle, bounds, self.radius, self.opacity);
    }
}

impl<'a> From<Pic> for Element<'a, Message> {
    fn from(picture: Pic) -> Self {
        Element::new(picture)
    }
}

/// A moving picture on screen: its decoder and the frame showing.
pub struct Playing {
    player: Player,
    frame: Option<Handle>,
}

/// A moving picture by its core picture and the size it's drawn at.
pub type PlayKey = (usize, u32, u32);

/// A moving picture: on each of the window's redraws while it's in view, it
/// shows the frame that has fallen due (fitted to the size it's drawn at)
/// and asks for a redraw when the next will. Every moving picture asks the
/// same window, so frames due together cost one repaint.
pub struct Animated<'a> {
    picture: Arc<CorePicture>,
    size: Size,
    at: Rectangle,
    cover: bool,
    radius: [f32; 4],
    opacity: f32,
    /// What shows until the first frame.
    still: Option<Handle>,
    playing: &'a RefCell<HashMap<PlayKey, Playing>>,
}

impl<'a> Animated<'a> {
    pub fn new(
        picture: Arc<CorePicture>,
        size: Size,
        at: Rectangle,
        cover: bool,
        playing: &'a RefCell<HashMap<PlayKey, Playing>>,
    ) -> Self {
        Self {
            picture,
            size,
            at,
            cover,
            radius: [0.0; 4],
            opacity: 1.0,
            still: None,
            playing,
        }
    }

    pub fn key(&self) -> PlayKey {
        (
            Arc::as_ptr(&self.picture) as usize,
            self.at.width as u32,
            self.at.height as u32,
        )
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = [radius; 4];
        self
    }

    pub fn radius_each(mut self, radius: [f32; 4]) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn still(mut self, still: Option<Handle>) -> Self {
        self.still = still;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Animated<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.size.width),
            Length::Fixed(self.size.height),
        )
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let iced::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        // Rows built in the margin around the viewport don't play.
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        if !bounds.intersects(viewport) {
            return;
        }
        let key = self.key();
        let mut playing = self.playing.borrow_mut();
        let Some(entry) = (match playing.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                Player::new(&self.picture).map(|player| {
                    entry.insert(Playing {
                        player,
                        frame: None,
                    })
                })
            }
        }) else {
            return;
        };
        let now = *now;
        if let Some(due) = entry.player.due()
            && due <= now + Duration::from_millis(4)
            && let Some(mut rgba) = entry.player.next(due.max(now))
        {
            paint::to_native(&mut rgba);
            let (width, height) = (self.picture.width, self.picture.height);
            let (to_width, to_height) = (self.at.width as u32, self.at.height as u32);
            entry.frame = Some(if (width, height) == (to_width, to_height) {
                Handle::from_rgba(width, height, rgba)
            } else {
                // Fitted to the size drawn, so it's copied in one to one.
                let fitted =
                    paint::fitted(&rgba, (width, height), (to_width, to_height), self.cover);
                Handle::from_rgba(to_width, to_height, fitted)
            });
        }
        if let Some(due) = entry.player.due() {
            shell.request_redraw_at(due);
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        let playing = self.playing.borrow();
        let frame = playing
            .get(&self.key())
            .and_then(|entry| entry.frame.as_ref())
            .or(self.still.as_ref());
        if let Some(frame) = frame {
            draw_picture(renderer, frame, bounds, self.radius, self.opacity);
        }
    }
}

impl<'a> From<Animated<'a>> for Element<'a, Message> {
    fn from(animated: Animated<'a>) -> Self {
        Element::new(animated)
    }
}

// ---------------------------------------------------------------------------
// A list's rows, kept by key

/// The rows of a list built near its viewport, each at its own place in the
/// list's full height (so no spacers), with their widget state kept by key.
/// iced's columns pair state with children by position: when the rows built
/// shift by one, as each new message does, every row got another row's
/// state, and every paragraph was shaped again; iced's keyed column only
/// follows one insertion or removal, not a window that slides.
///
/// A measured list ([`Rows::measured`]) lays each row out at the height its
/// content takes, stacking them down from the first row's place, and
/// reports the rows whose height differs from the one it was given, once
/// drawn, so the app can keep the real heights: a message's Markdown,
/// pictures and reactions are hard to know ahead.
pub struct Rows<'a> {
    keys: Vec<u64>,
    children: Vec<Element<'a, Message>>,
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// A placed child's left and width ([`Rows::place`]); a row spans the
    /// list.
    spans: Vec<Option<(f32, f32)>>,
    total: f32,
    measured: Option<Measure>,
}

/// How a measured list reports its rows' heights, by key.
pub type Report = fn(Vec<(u64, f32)>) -> Message;

/// Heights a measured list found, by key, for the app to take in.
pub type Found = std::rc::Rc<RefCell<Vec<(u64, f32)>>>;

/// How a list measures its rows: see [`Rows::measured`].
struct Measure {
    report: Report,
    found: Found,
    /// The row (by its place among those pushed) at the view's top while
    /// someone reads earlier ones; none while the list follows its end.
    pivot: Option<usize>,
}

/// What a list keeps between builds: its rows' keys, in order, and the
/// heights measured that differ from those given, not yet reported.
#[derive(Default)]
struct RowsState {
    keys: Vec<u64>,
    differ: Vec<(u64, f32)>,
}

impl<'a> Rows<'a> {
    pub fn new(total: f32) -> Self {
        Self {
            keys: Vec::new(),
            children: Vec::new(),
            tops: Vec::new(),
            heights: Vec::new(),
            spans: Vec::new(),
            total,
            measured: None,
        }
    }

    /// Rows take the height their content does. The list grows by what
    /// rows above `pivot` (all of them without one) grew, so what's in view
    /// stays put in a scrollable anchored to its end, and their heights go
    /// in `found` for the app to take in when it next updates, at no cost
    /// of a message and a view of their own. Rows from `pivot` on, from
    /// the top of the view of someone reading earlier ones, run on into
    /// the room below them instead, and their heights are reported with
    /// `report` once drawn, so the app can keep what's read in place.
    pub fn measured(mut self, report: Report, found: Found, pivot: Option<usize>) -> Self {
        self.measured = Some(Measure {
            report,
            found,
            pivot,
        });
        self
    }

    pub fn push(
        &mut self,
        key: u64,
        top: f32,
        height: f32,
        child: impl Into<Element<'a, Message>>,
    ) {
        self.keys.push(key);
        self.tops.push(top);
        self.heights.push(height);
        self.spans.push(None);
        self.children.push(child.into());
    }

    /// A child in its own box, `at` from the list's corner, rather than a
    /// row across it: a picker's tile, beside others in its row or column.
    /// Not for a measured list.
    pub fn place(&mut self, key: u64, at: Rectangle, child: impl Into<Element<'a, Message>>) {
        self.push(key, at.y, at.height, child);
        if let Some(span) = self.spans.last_mut() {
            *span = Some((at.x, at.width));
        }
    }
}

impl Widget<Message, Theme, Renderer> for Rows<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RowsState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(RowsState {
            keys: self.keys.clone(),
            ..RowsState::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let keys = &mut tree.state.downcast_mut::<RowsState>().keys;
        if *keys == self.keys {
            for (child, state) in self.children.iter().zip(&mut tree.children) {
                state.diff(child);
            }
            return;
        }
        let mut old: HashMap<u64, Tree> = keys.drain(..).zip(tree.children.drain(..)).collect();
        for (key, child) in self.keys.iter().zip(&self.children) {
            tree.children.push(match old.remove(key) {
                Some(mut state) => {
                    state.diff(child);
                    state
                }
                None => Tree::new(child),
            });
        }
        keys.clone_from(&self.keys);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        if let Some(measure) = &self.measured {
            let loose = layout::Limits::new(Size::new(width, 0.0), Size::new(width, f32::INFINITY));
            let pivot = measure.pivot.unwrap_or(usize::MAX);
            let mut differ = Vec::new();
            let mut found = Vec::new();
            let mut grown = 0.0;
            let mut next = self.tops.first().copied().unwrap_or(0.0);
            let nodes = self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(self.keys.iter().zip(&self.heights))
                .enumerate()
                .map(|(place, ((child, state), (&key, &height)))| {
                    let node = child
                        .as_widget_mut()
                        .layout(state, renderer, &loose)
                        .move_to((0.0, next));
                    // Whole pixels: a row's height in halves would put the
                    // rows after it, and in a list held to its end every
                    // row, between pixels, where pictures and round
                    // corners take the renderer's slow way.
                    let measured = node.size().height.round();
                    if (measured - height).abs() > 0.5 {
                        if place < pivot {
                            grown += measured - height;
                            found.push((key, measured));
                        } else {
                            differ.push((key, measured));
                        }
                    }
                    next += measured;
                    node
                })
                .collect();
            if !found.is_empty() {
                measure.found.borrow_mut().extend(found);
            }
            tree.state.downcast_mut::<RowsState>().differ = differ;
            return layout::Node::with_children(Size::new(width, self.total + grown), nodes);
        }
        let nodes = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(self.tops.iter().zip(&self.heights).zip(&self.spans))
            .map(|((child, state), ((&top, &height), span))| {
                let (left, wide) = span.unwrap_or((0.0, width));
                let size = Size::new(wide, height);
                child
                    .as_widget_mut()
                    .layout(state, renderer, &layout::Limits::new(size, size))
                    .move_to((left, top))
            })
            .collect();
        layout::Node::with_children(Size::new(width, self.total), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // The heights measured at the last layout, once it's drawn.
        if let Some(measure) = &self.measured
            && let iced::Event::Window(iced::window::Event::RedrawRequested(_)) = event
        {
            let differ = std::mem::take(&mut tree.state.downcast_mut::<RowsState>().differ);
            if !differ.is_empty() {
                shell.publish((measure.report)(differ));
            }
        }
        for ((child, state), layout) in self
            .children
            .iter_mut()
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
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            if layout.bounds().intersects(viewport) {
                child
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        }
    }

    // A tooltip inside shows (iced asks each widget for its children's overlays).
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        iced::advanced::overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Rows<'a>> for Element<'a, Message> {
    fn from(rows: Rows<'a>) -> Self {
        Element::new(rows)
    }
}

// ---------------------------------------------------------------------------
// A member row and its nameplate

/// A member's row with the nameplate behind it, as the Slint client's
/// `Nameplate` draws one: the picture at the row's right end, faded to .3
/// of its strength under the text and in full 50 px after it, and the
/// palette's colour from nothing at the left edge. `content` is the row:
/// the avatar, then the text column, whose width says where the fade ends.
pub struct PlateRow<'a> {
    content: Element<'a, Message>,
    plate: Option<Element<'a, Message>>,
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
}

impl<'a> PlateRow<'a> {
    pub fn new(
        content: impl Into<Element<'a, Message>>,
        plate: Option<Element<'a, Message>>,
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
        }
    }

    /// The account's own plate, on its card or a tile: the picture isn't
    /// faded under the text, and the palette runs from .1 to .4.
    pub fn account(mut self) -> Self {
        self.account = true;
        self
    }

    fn children_mut(&mut self) -> impl Iterator<Item = &mut Element<'a, Message>> {
        std::iter::once(&mut self.content).chain(self.plate.as_mut())
    }
}

fn quad(renderer: &mut Renderer, bounds: Rectangle, stops: &[(f32, Color)]) {
    use iced::advanced::Renderer as _;
    let mut linear = gradient::Linear::new(iced::Degrees(90.0));
    for (offset, color) in stops {
        linear = linear.add_stop(offset.clamp(0.0, 1.0), *color);
    }
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: Border::default().rounded(8),
            ..Default::default()
        },
        Background::Gradient(linear.into()),
    );
}

impl Widget<Message, Theme, Renderer> for PlateRow<'_> {
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
        let children: Vec<&Element<'_, Message>> = std::iter::once(&self.content)
            .chain(self.plate.as_ref())
            .collect();
        tree.diff_children(&children);
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
        // Slint's `content-width`: the text column's width, from the plate's left.
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
        shell: &mut Shell<'_, Message>,
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
                quad(
                    renderer,
                    bounds,
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
                    quad(renderer, bounds, &[(0.0, tint(0.1)), (1.0, tint(0.4))]);
                } else {
                    quad(
                        renderer,
                        bounds,
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
}

impl<'a> From<PlateRow<'a>> for Element<'a, Message> {
    fn from(row: PlateRow<'a>) -> Self {
        Element::new(row)
    }
}

// ---------------------------------------------------------------------------
// A card placed in the window

/// Its child at `at`, moved up or left as far as it must be to stay
/// `margin` inside the window, as the Slint client clamps a profile card.
/// With [`Place::beside`], a second child stands beside a part of the
/// first (a submenu beside its row): on its right, or its left without
/// room, its top level with that part's, moved up to stay in the window;
/// the first doesn't move for it.
pub struct Place<'a> {
    child: Element<'a, Message>,
    at: Point,
    margin: f32,
    anchor: Anchor,
    beside: Option<Beside<'a>>,
}

/// What stands beside a [`Place`]'s child.
struct Beside<'a> {
    child: Element<'a, Message>,
    /// The part of the first child it lines up with, by the indices of its
    /// layout's children from the first child's root.
    path: Vec<usize>,
    /// Above the part's top, as a submenu's padding lifts it.
    lift: f32,
    gap: f32,
}

/// How far [`Anchor::Span`] keeps its child from the button.
const SPAN_GAP: f32 = 4.0;

/// How `Place` reads its point.
#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    /// The child's top left corner, moved in to fit.
    Corner,
    /// As a menu opens at the pointer: below it if it fits, else above it.
    Pointer,
    /// The child's bottom left corner, as suggestions sit over the composer.
    Bottom,
    /// Under a button `height` tall whose top left is the point, a gap
    /// from it, or above it without room there: as a popout from its
    /// button.
    Span(f32),
}

impl<'a> Place<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, at: Point, margin: f32) -> Self {
        Self {
            child: child.into(),
            at,
            margin,
            anchor: Anchor::Corner,
            beside: None,
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// `child` beside the part of the first at `path` (its layout's
    /// children's indices), `gap` from it, its top `lift` above the part's.
    pub fn beside(
        mut self,
        child: impl Into<Element<'a, Message>>,
        path: Vec<usize>,
        lift: f32,
        gap: f32,
    ) -> Self {
        self.beside = Some(Beside {
            child: child.into(),
            path,
            lift,
            gap,
        });
        self
    }

    fn children_mut(&mut self) -> impl Iterator<Item = &mut Element<'a, Message>> {
        std::iter::once(&mut self.child).chain(self.beside.as_mut().map(|beside| &mut beside.child))
    }

    fn children_ref(&self) -> impl Iterator<Item = &Element<'a, Message>> {
        std::iter::once(&self.child).chain(self.beside.as_ref().map(|beside| &beside.child))
    }
}

impl Widget<Message, Theme, Renderer> for Place<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        self.children_ref().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let children: Vec<&Element<'_, Message>> = self.children_ref().collect();
        tree.diff_children(&children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let room = limits.max();
        let free = layout::Limits::new(Size::ZERO, room);
        let node = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &free);
        let size = node.size();
        let x = self
            .at
            .x
            .min(room.width - size.width - self.margin)
            .max(self.margin);
        let y = match self.anchor {
            Anchor::Corner => self.at.y.min(room.height - size.height - self.margin),
            Anchor::Pointer if self.at.y + 4.0 + size.height <= room.height - self.margin => {
                self.at.y
            }
            // Above it, kept inside the window where it fits neither way.
            Anchor::Pointer if self.at.y - size.height - 4.0 >= self.margin => {
                self.at.y - size.height - 4.0
            }
            Anchor::Pointer => room.height - size.height - self.margin,
            Anchor::Bottom => self.at.y - size.height,
            Anchor::Span(height)
                if self.at.y + height + SPAN_GAP + size.height <= room.height - self.margin =>
            {
                self.at.y + height + SPAN_GAP
            }
            Anchor::Span(_) => self.at.y - size.height - SPAN_GAP,
        }
        .max(self.margin);
        let mut nodes = vec![node.move_to((x, y))];
        if let Some(beside) = &mut self.beside {
            let sub = beside
                .child
                .as_widget_mut()
                .layout(&mut tree.children[1], renderer, &free);
            let sub_size = sub.size();
            // The part's top within the first child.
            let mut part = &nodes[0];
            let mut top = 0.0;
            for &index in &beside.path {
                let Some(child) = part.children().get(index) else {
                    break;
                };
                top += child.bounds().y;
                part = child;
            }
            let right = x + size.width + beside.gap;
            let sub_x = if right + sub_size.width <= room.width - self.margin {
                right
            } else {
                (x - beside.gap - sub_size.width).max(self.margin)
            };
            let sub_y = (y + top - beside.lift)
                .min(room.height - sub_size.height - self.margin)
                .max(self.margin);
            nodes.push(sub.move_to((sub_x, sub_y)));
        }
        layout::Node::with_children(room, nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((child, state), layout) in self
            .children_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .operate(state, layout, renderer, operation);
        }
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
        // The one beside first: it's drawn over the first.
        let layouts: Vec<Layout<'_>> = layout.children().collect();
        let mut pairs: Vec<_> = self
            .children_mut()
            .zip(&mut tree.children)
            .zip(layouts)
            .collect();
        for ((child, state), layout) in pairs.iter_mut().rev() {
            child.as_widget_mut().update(
                state, event, *layout, cursor, renderer, clipboard, shell, viewport,
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
        self.children_ref()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for ((child, state), layout) in self
            .children_ref()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
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
        let child = layout.children().next().expect("child's");
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            child,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Place<'a>> for Element<'a, Message> {
    fn from(place: Place<'a>) -> Self {
        Element::new(place)
    }
}

// ---------------------------------------------------------------------------
// A layer of its own

/// Its child drawn in a layer of its own, within `reach` pixels around it:
/// iced draws each layer's quads, then its pictures, then its text, so a
/// round quad (an avatar's ring) laid over a picture (a banner) needs one.
pub struct Above<'a> {
    child: Element<'a, Message>,
    reach: f32,
}

impl<'a> Above<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, reach: f32) -> Self {
        Self {
            child: child.into(),
            reach,
        }
    }
}

impl Widget<Message, Theme, Renderer> for Above<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        renderer.with_layer(layout.bounds().expand(self.reach), |renderer| {
            self.child.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Above<'a>> for Element<'a, Message> {
    fn from(above: Above<'a>) -> Self {
        Element::new(above)
    }
}

// ---------------------------------------------------------------------------
// Under the title bar

/// Its child in the room `top` pixels below the window's top, laid out and
/// pointed at as if that room were the whole window: the window's own title
/// bar takes the top, and what's below it (overlays placed by the layout's
/// sums or at the pointer, sizes taken from the window's) needn't know.
pub struct Below<'a> {
    child: Element<'a, Message>,
    top: f32,
}

impl<'a> Below<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, top: f32) -> Self {
        Self {
            child: child.into(),
            top,
        }
    }

    fn shift(&self) -> Vector {
        Vector::new(0.0, self.top)
    }

    /// The pointer as the child sees it.
    fn cursor(&self, cursor: mouse::Cursor) -> mouse::Cursor {
        cursor - self.shift()
    }

    fn viewport(&self, viewport: &Rectangle) -> Rectangle {
        Rectangle {
            y: viewport.y - self.top,
            ..*viewport
        }
    }
}

impl Widget<Message, Theme, Renderer> for Below<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let room = limits.max();
        let inside = Size::new(room.width, (room.height - self.top).max(0.0));
        let child = self.child.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, inside),
        );
        layout::Node::with_children(room, vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let child = layout.children().next().expect("child");
        if self.top == 0.0 {
            self.child
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
            return;
        }
        // Told as a scrollable that can't scroll, moved down by the bar, so
        // what an operation finds (the tests' clicks, for one) is where it
        // shows.
        operation.scrollable(
            None,
            layout.bounds(),
            child.bounds(),
            -self.shift(),
            &mut Still,
        );
        operation.traverse(&mut |operation| {
            self.child
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let child = layout.children().next().expect("child");
        let had_input_method = shell.input_method().is_enabled();
        let (cursor, viewport) = (self.cursor(cursor), self.viewport(viewport));
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            &viewport,
        );
        // The caret the input method follows, back in the window's place.
        if !had_input_method
            && let iced::advanced::InputMethod::Enabled { cursor, .. } = shell.input_method_mut()
        {
            *cursor = *cursor + self.shift();
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().expect("child"),
            self.cursor(cursor),
            &self.viewport(viewport),
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
        let child = layout.children().next().expect("child");
        renderer.with_translation(self.shift(), |renderer| {
            self.child.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child,
                self.cursor(cursor),
                &self.viewport(viewport),
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let shift = self.shift();
        let viewport = self.viewport(viewport);
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().expect("child"),
            renderer,
            &viewport,
            translation + shift,
        )
    }
}

/// A [`Below`] to operations: nothing scrolls it.
struct Still;

impl iced::advanced::widget::operation::Scrollable for Still {
    fn snap_to(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::RelativeOffset<Option<f32>>,
    ) {
    }

    fn scroll_to(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::AbsoluteOffset<Option<f32>>,
    ) {
    }

    fn scroll_by(
        &mut self,
        _: iced::advanced::widget::operation::scrollable::AbsoluteOffset,
        _: Rectangle,
        _: Rectangle,
    ) {
    }
}

impl<'a> From<Below<'a>> for Element<'a, Message> {
    fn from(below: Below<'a>) -> Self {
        Element::new(below)
    }
}

// ---------------------------------------------------------------------------
// The timeline's presses and releases

/// Its child, and what a left press nothing in it took and the left
/// button's release say: the timeline's selection is cleared by the first
/// and ends with the second, wherever the pointer is then.
pub struct Watch<'a> {
    child: Element<'a, Message>,
    on_blank: Option<Message>,
    on_release: Option<Message>,
}

impl<'a> Watch<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        on_blank: Option<Message>,
        on_release: Option<Message>,
    ) -> Self {
        Self {
            child: child.into(),
            on_blank,
            on_release,
        }
    }
}

impl Widget<Message, Theme, Renderer> for Watch<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(node.size(), vec![node])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let child = layout.children().next().expect("child's");
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], child, renderer, operation);
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
        let child = layout.children().next().expect("child's");
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if !shell.is_event_captured() && cursor.is_over(layout.bounds()) =>
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

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let child = layout.children().next().expect("child's");
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            child,
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
        let child = layout.children().next().expect("child's");
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let child = layout.children().next().expect("child's");
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            child,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Watch<'a>> for Element<'a, Message> {
    fn from(watch: Watch<'a>) -> Self {
        Element::new(watch)
    }
}

/// Its child, and a message with the point a right press on it gives, from
/// the corner of the viewport it's drawn in, for a context menu at the
/// pointer.
pub struct RightPress<'a> {
    child: Element<'a, Message>,
    on: Option<Box<dyn Fn(Point) -> Message + 'a>>,
    spot: Option<&'a std::cell::Cell<Point>>,
}

impl<'a> RightPress<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, on: impl Fn(Point) -> Message + 'a) -> Self {
        Self {
            child: child.into(),
            on: Some(Box::new(on)),
            spot: None,
        }
    }

    /// Around the whole window: notes where in it each press is, before
    /// what's pressed hears of it, so a button that opens a menu can open
    /// it there without a message for each move of the pointer.
    pub fn spot(child: impl Into<Element<'a, Message>>, spot: &'a std::cell::Cell<Point>) -> Self {
        Self {
            child: child.into(),
            on: None,
            spot: Some(spot),
        }
    }
}

impl Widget<Message, Theme, Renderer> for RightPress<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        if let Some(spot) = self.spot
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(at) = cursor.position()
        {
            spot.set(at);
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if let Some(on) = &self.on
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && !shell.is_event_captured()
            && let Some(at) = cursor.position_over(layout.bounds())
            && viewport.contains(at)
        {
            // From the corner of what shows of the list it's in: a
            // scrollable's children see the pointer in the list's own
            // space, scrolled.
            shell.publish(on(at - Vector::new(viewport.x, viewport.y)));
            shell.capture_event();
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<RightPress<'a>> for Element<'a, Message> {
    fn from(press: RightPress<'a>) -> Self {
        Element::new(press)
    }
}

// ---------------------------------------------------------------------------
// Something floating at a corner

/// Its child at its own size, its right edge `right` in from the right of
/// the space it's given and its top at `top` (which may be above that
/// space, over what's before it), drawn in a layer of its own so it covers
/// what it floats over: a message's hover bar over the row above.
pub struct Corner<'a> {
    child: Element<'a, Message>,
    right: f32,
    top: f32,
    free: bool,
}

impl<'a> Corner<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, right: f32, top: f32) -> Self {
        Self {
            child: child.into(),
            right,
            top,
            free: false,
        }
    }

    /// Wider than its space, and out past its left too: a slider over a
    /// button narrower than it.
    pub fn free(mut self) -> Self {
        self.free = true;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Corner<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let room = if self.free { f32::INFINITY } else { width };
        let child = self.child.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(room, f32::INFINITY)),
        );
        let x = width - self.right - child.size().width;
        let x = if self.free { x } else { x.max(0.0) };
        let child = child.move_to((x, self.top));
        layout::Node::with_children(Size::new(width, 0.0), vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(child) = layout.children().next() {
            self.child
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
        }
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
        if let Some(child) = layout.children().next() {
            self.child.as_widget_mut().update(
                &mut tree.children[0],
                event,
                child,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
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
        layout
            .children()
            .next()
            .map_or_else(Default::default, |child| {
                self.child.as_widget().mouse_interaction(
                    &tree.children[0],
                    child,
                    cursor,
                    viewport,
                    renderer,
                )
            })
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
        let Some(child) = layout.children().next() else {
            return;
        };
        // Room for its shadow.
        renderer.with_layer(child.bounds().expand(16.0), |renderer| {
            self.child.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child,
                cursor,
                viewport,
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        let child = layout.children().next()?;
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            child,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Corner<'a>> for Element<'a, Message> {
    fn from(corner: Corner<'a>) -> Self {
        Element::new(corner)
    }
}

// ---------------------------------------------------------------------------
// A long body folded

/// A body that folds once it's longer than `long` pixels: folded, it's cut
/// off at `cut`, and `more` (Show more, or Show less unfolded) shows under
/// it, `gap` below; a short body shows whole, without it.
pub struct Fold<'a> {
    body: Element<'a, Message>,
    more: Element<'a, Message>,
    folded: bool,
    long: f32,
    cut: f32,
    gap: f32,
}

impl<'a> Fold<'a> {
    pub fn new(
        body: impl Into<Element<'a, Message>>,
        more: impl Into<Element<'a, Message>>,
        folded: bool,
        (long, cut, gap): (f32, f32, f32),
    ) -> Self {
        Self {
            body: body.into(),
            more: more.into(),
            folded,
            long,
            cut,
            gap,
        }
    }
}

/// Whether the body was long at the last layout.
#[derive(Default)]
struct FoldState {
    long: bool,
}

impl Widget<Message, Theme, Renderer> for Fold<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<FoldState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(FoldState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.body), Tree::new(&self.more)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.body, &self.more]);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let loose = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
        let body = self
            .body
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &loose);
        let long = body.size().height > self.long;
        tree.state.downcast_mut::<FoldState>().long = long;
        if !long {
            let height = body.size().height;
            let more = layout::Node::new(Size::ZERO);
            return layout::Node::with_children(Size::new(width, height), vec![body, more]);
        }
        let shown = if self.folded {
            self.cut
        } else {
            body.size().height
        };
        let more = self
            .more
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &loose)
            .move_to((0.0, shown + self.gap));
        let height = shown + self.gap + more.size().height;
        layout::Node::with_children(Size::new(width, height), vec![body, more])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut children = layout.children();
        if let Some(body) = children.next() {
            self.body
                .as_widget_mut()
                .operate(&mut tree.children[0], body, renderer, operation);
        }
        if tree.state.downcast_ref::<FoldState>().long
            && let Some(more) = children.next()
        {
            self.more
                .as_widget_mut()
                .operate(&mut tree.children[1], more, renderer, operation);
        }
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
        let long = tree.state.downcast_ref::<FoldState>().long;
        let shown = layout.bounds();
        let mut children = layout.children();
        if let Some(body) = children.next() {
            // What's cut off takes no pointer.
            let cursor = match cursor.position() {
                Some(at) if self.folded && long && !shown.contains(at) => {
                    mouse::Cursor::Unavailable
                }
                _ => cursor,
            };
            self.body.as_widget_mut().update(
                &mut tree.children[0],
                event,
                body,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        if long && let Some(more) = children.next() {
            self.more.as_widget_mut().update(
                &mut tree.children[1],
                event,
                more,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
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
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        let body = children.next().map_or_else(Default::default, |body| {
            self.body.as_widget().mouse_interaction(
                &tree.children[0],
                body,
                cursor,
                viewport,
                renderer,
            )
        });
        let more = match children.next() {
            Some(more) if long => self.more.as_widget().mouse_interaction(
                &tree.children[1],
                more,
                cursor,
                viewport,
                renderer,
            ),
            _ => mouse::Interaction::default(),
        };
        body.max(more)
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
        let long = tree.state.downcast_ref::<FoldState>().long;
        let mut children = layout.children();
        let Some(body) = children.next() else {
            return;
        };
        if long && self.folded {
            let cut = Rectangle::new(body.position(), Size::new(body.bounds().width, self.cut));
            renderer.with_layer(cut, |renderer| {
                self.body.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    body,
                    cursor,
                    viewport,
                );
            });
        } else {
            self.body.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                body,
                cursor,
                viewport,
            );
        }
        if long && let Some(more) = children.next() {
            self.more.as_widget().draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                more,
                cursor,
                viewport,
            );
        }
    }
}

impl<'a> From<Fold<'a>> for Element<'a, Message> {
    fn from(fold: Fold<'a>) -> Self {
        Element::new(fold)
    }
}

#[cfg(all(test, feature = "cpu"))]
mod tests {
    use iced::{
        Border, Color, Font, Pixels, Rectangle, Size,
        advanced::{
            Renderer as _,
            renderer::{Headless as _, Quad},
        },
    };

    /// The pixel at `x`, `y` of a screenshot `width` wide.
    fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    #[test]
    fn a_round_quad_stays_inside_its_layers_clip() {
        let clip = Rectangle::new(iced::Point::new(10.0, 10.0), Size::new(40.0, 20.0));
        // Rounded, bordered and both; at whole and fractional scales; and
        // as wide as a line of text that doesn't wrap.
        let cases = [(4.0, 0.0), (4.0, 1.0), (0.0, 1.0)]
            .into_iter()
            .flat_map(|look| [(look, 1.0, 100.0), (look, 1.5, 100.0), (look, 1.25, 5000.0)]);
        for ((radius, border), scale, width) in cases {
            let mut renderer = iced_tiny_skia::Renderer::new(Font::default(), Pixels(16.0));
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            iced::Point::new(20.0, 12.0),
                            Size::new(width, 16.0),
                        ),
                        border: Border {
                            color: Color::from_rgb(1.0, 0.0, 0.0),
                            width: border,
                            radius: radius.into(),
                        },
                        ..Quad::default()
                    },
                    Color::WHITE,
                );
            });
            let rgba = renderer.screenshot(Size::new(200, 60), scale, Color::BLACK);
            let at = |x: f32| ((x * scale) as u32, (20.0 * scale) as u32);
            let (x, y) = at(30.0);
            assert_eq!(pixel(&rgba, 200, x, y)[..3], [255, 255, 255], "inside");
            for outside in [52.0, 80.0, 130.0] {
                let (x, y) = at(outside);
                assert_eq!(
                    pixel(&rgba, 200, x, y)[..3],
                    [0, 0, 0],
                    "at {outside}, outside the clip: radius {radius}, border {border}, scale {scale}, width {width}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A bar to move through something playing

/// Its child, across which a press says where it was pressed (from 0 at
/// its left to 1 at its right) with `seek`, and so does the pointer while
/// it's held; letting go says `released`: a voice message's waveform.
pub struct Scrub<'a> {
    child: Element<'a, Message>,
    seek: Box<dyn Fn(f32) -> Message + 'a>,
    released: Option<Message>,
}

impl<'a> Scrub<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, seek: impl Fn(f32) -> Message + 'a) -> Self {
        Self {
            child: child.into(),
            seek: Box::new(seek),
            released: None,
        }
    }

    pub fn released(mut self, released: Message) -> Self {
        self.released = Some(released);
        self
    }
}

/// Whether the button went down on it and is still down.
#[derive(Default)]
struct ScrubState {
    held: bool,
}

impl Widget<Message, Theme, Renderer> for Scrub<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ScrubState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ScrubState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<ScrubState>();
        let at = |x: f32| ((x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0);
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.held = true;
                    shell.publish((self.seek)(at(position.x)));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) if state.held => {
                shell.publish((self.seek)(at(position.x)));
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.held => {
                state.held = false;
                if let Some(released) = &self.released {
                    shell.publish(released.clone());
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
}

impl<'a> From<Scrub<'a>> for Element<'a, Message> {
    fn from(scrub: Scrub<'a>) -> Self {
        Element::new(scrub)
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
    child: Element<'a, Message>,
    spot: &'a std::cell::Cell<Rectangle>,
    from_press: bool,
}

impl<'a> Spot<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        spot: &'a std::cell::Cell<Rectangle>,
    ) -> Self {
        Self {
            child: child.into(),
            spot,
            from_press: false,
        }
    }

    /// Notes its bounds from the press instead, for a child in a list that
    /// scrolls, whose bounds aren't the window's: `App::pressed_spot` puts
    /// them in the window.
    pub fn by_press(mut self) -> Self {
        self.from_press = true;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Spot<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        if let iced::Event::Mouse(mouse::Event::ButtonPressed(_)) = event
            && let Some(at) = cursor.position_over(layout.bounds())
        {
            let bounds = layout.bounds();
            self.spot.set(if self.from_press {
                bounds - (at - Point::ORIGIN)
            } else {
                bounds
            });
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Spot<'a>> for Element<'a, Message> {
    fn from(spot: Spot<'a>) -> Self {
        Element::new(spot)
    }
}

// ---------------------------------------------------------------------------
// A step graph

/// A line of steps drawn across its box (a graph of the call's pings): each
/// step's run at its height, joined by risers, in a box of its own units
/// stretched to the widget's, the line `stroke` px wide whatever the
/// stretch. Its lines are axis-aligned, so they're quads: no path to fill.
pub struct Steps {
    steps: Vec<crate::call_details::Step>,
    units: Size,
    ink: Color,
    stroke: f32,
    height: f32,
}

impl Steps {
    pub fn new(
        steps: Vec<crate::call_details::Step>,
        units: Size,
        ink: Color,
        height: f32,
    ) -> Self {
        Self {
            steps,
            units,
            ink,
            stroke: 2.0,
            height,
        }
    }
}

impl Widget<Message, Theme, Renderer> for Steps {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, self.height, Size::ZERO))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let (sx, sy) = (
            bounds.width / self.units.width,
            bounds.height / self.units.height,
        );
        let half = self.stroke / 2.0;
        let mut line = |x: f32, y: f32, width: f32, height: f32| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(Point::new(x, y), Size::new(width, height)),
                    ..Default::default()
                },
                self.ink,
            );
        };
        let mut last: Option<f32> = None;
        for step in &self.steps {
            let x0 = bounds.x + step.x0 * sx;
            let x1 = bounds.x + step.x1 * sx;
            let y = bounds.y + step.y * sy;
            if let Some(was) = last {
                let (top, bottom) = (was.min(y), was.max(y));
                line(
                    x0 - half,
                    top - half,
                    self.stroke,
                    bottom - top + self.stroke,
                );
            }
            line(x0 - half, y - half, x1 - x0 + self.stroke, self.stroke);
            last = Some(y);
        }
    }
}

impl<'a> From<Steps> for Element<'a, Message> {
    fn from(steps: Steps) -> Self {
        Element::new(steps)
    }
}

// ---------------------------------------------------------------------------
// An edge dragged to resize

/// The edge between two panels, which a drag moves: 2 px wide, drawn as a
/// line of `edge` down its first, it takes the pointer `REACH` px either
/// side, and fills both under the pointer (`lit`, or `held` while dragged), says how far the
/// pointer has moved since the press with `dragged`, `released` as it's
/// let go, and `reset` on a double click: the thread beside's.
pub struct Splitter<'a> {
    edge: Color,
    lit: Color,
    held: Color,
    dragged: Box<dyn Fn(f32) -> Message + 'a>,
    released: Message,
    reset: Message,
}

/// How far either side of the edge it takes the pointer, and the most
/// time between a double click's clicks.
const REACH: f32 = 3.0;
/// Its width: the edge's line and the lit line's.
const EDGE: f32 = 2.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Default)]
struct SplitterState {
    /// Where the button went down, while it's held.
    from: Option<f32>,
    pointed: bool,
    last_click: Option<std::time::Instant>,
}

impl<'a> Splitter<'a> {
    pub fn new(
        edge: Color,
        lit: Color,
        held: Color,
        dragged: impl Fn(f32) -> Message + 'a,
        released: Message,
        reset: Message,
    ) -> Self {
        Self {
            edge,
            lit,
            held,
            dragged: Box::new(dragged),
            released,
            reset,
        }
    }

    fn reach(bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x - REACH,
            width: bounds.width + 2.0 * REACH,
            ..bounds
        }
    }
}

impl Widget<Message, Theme, Renderer> for Splitter<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(EDGE), Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SplitterState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SplitterState::default())
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(EDGE), Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SplitterState>();
        let reach = Self::reach(layout.bounds());
        match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let pointed = reach.contains(*position);
                if pointed != state.pointed {
                    state.pointed = pointed;
                    shell.request_redraw();
                }
                if let Some(from) = state.from {
                    shell.publish((self.dragged)(position.x - from));
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_over(reach) else {
                    return;
                };
                let now = std::time::Instant::now();
                if state
                    .last_click
                    .is_some_and(|last| now.duration_since(last) < DOUBLE_CLICK)
                {
                    state.last_click = None;
                    shell.publish(self.reset.clone());
                } else {
                    state.last_click = Some(now);
                    state.from = Some(at.x);
                }
                shell.capture_event();
                shell.request_redraw();
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.from.take().is_some() =>
            {
                shell.publish(self.released.clone());
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<SplitterState>();
        if state.from.is_some() || cursor.is_over(Self::reach(layout.bounds())) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<SplitterState>();
        let bounds = layout.bounds();
        let held = state.from.is_some();
        let (ink, width) = if held {
            (self.held, EDGE)
        } else if state.pointed {
            (self.lit, EDGE)
        } else {
            (self.edge, 1.0)
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { width, ..bounds },
                ..Default::default()
            },
            ink,
        );
    }
}

impl<'a> From<Splitter<'a>> for Element<'a, Message> {
    fn from(splitter: Splitter<'a>) -> Self {
        Element::new(splitter)
    }
}

// ---------------------------------------------------------------------------
// A row that dragging moves

/// How far the pointer goes with the button down before a press becomes a
/// drag, as the Slint client's rows.
const DRAG_START: f32 = 4.0;

/// Its child, which a press and a move of [`DRAG_START`] px up or down
/// starts dragging (`drag`, with how far down the row it was pressed),
/// and a press let go on it without a drag clicks (`click`): a channel's
/// or a category's row in the sidebar. While a drag is on (`tracking`), it
/// says when the pointer comes onto it and leaves (`over`), and a release
/// on it clicks nothing; the drag itself
/// follows the pointer through the window's events, not through it.
pub struct Drags<'a> {
    child: Element<'a, Message>,
    click: Option<Message>,
    drag: Option<Box<dyn Fn(f32) -> Message + 'a>>,
    over: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    tracking: bool,
    head: Option<f32>,
}

impl<'a> Drags<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>) -> Self {
        Self {
            child: child.into(),
            click: None,
            drag: None,
            over: None,
            tracking: false,
            head: None,
        }
    }

    /// Only a press on its top `height` px clicks or drags: a voice
    /// channel's own row, not who is listed under it.
    pub fn head(mut self, height: f32) -> Self {
        self.head = Some(height);
        self
    }

    pub fn click(mut self, click: Message) -> Self {
        self.click = Some(click);
        self
    }

    pub fn drag(mut self, drag: impl Fn(f32) -> Message + 'a) -> Self {
        self.drag = Some(Box::new(drag));
        self
    }

    pub fn tracking(mut self, tracking: bool, over: impl Fn(bool) -> Message + 'a) -> Self {
        self.tracking = tracking;
        self.over = Some(Box::new(over));
        self
    }
}

#[derive(Default)]
struct DragsState {
    /// Where the button went down on it, while it's down.
    pressed: Option<Point>,
    /// The press moved far enough to be a drag.
    moved: bool,
    /// The pointer is over it while a drag is on.
    over: bool,
}

impl Widget<Message, Theme, Renderer> for Drags<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<DragsState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(DragsState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        let bounds = layout.bounds();
        let released = matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        );
        // A drag's end isn't the child's: a button under it would click.
        if !(released && self.tracking) {
            self.child.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        let state = tree.state.downcast_mut::<DragsState>();
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // Not a press a button inside took.
                let head = Rectangle {
                    height: self.head.unwrap_or(bounds.height),
                    ..bounds
                };
                state.pressed = cursor
                    .position_over(head)
                    .filter(|_| !shell.is_event_captured());
                state.moved = false;
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. })
                if let (Some(from), Some(at), Some(drag)) =
                    (state.pressed, cursor.position(), &self.drag)
                    && !state.moved
                    && (at.y - from.y).abs() > DRAG_START =>
            {
                state.moved = true;
                shell.publish(drag(from.y - bounds.y));
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = state.pressed.take();
                if pressed.is_some()
                    && !state.moved
                    && !self.tracking
                    && !shell.is_event_captured()
                    && cursor.is_over(bounds)
                    && let Some(click) = &self.click
                {
                    shell.publish(click.clone());
                    shell.capture_event();
                }
                state.moved = false;
            }
            _ => {}
        }
        // Over it while a drag is on; checked on each frame too, as the
        // list scrolling under a still pointer moves it.
        let over = self.tracking && cursor.is_over(bounds);
        if over != state.over {
            state.over = over;
            if let Some(on) = &self.over {
                shell.publish(on(over));
            }
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
        if self.tracking {
            return mouse::Interaction::Grabbing;
        }
        let child = self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if child == mouse::Interaction::default() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            child
        }
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        // No card beside a row while one is dragged.
        if self.tracking {
            return None;
        }
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Drags<'a>> for Element<'a, Message> {
    fn from(drags: Drags<'a>) -> Self {
        Element::new(drags)
    }
}

// ---------------------------------------------------------------------------
// Ctrl and the wheel

/// Around the whole window: the wheel with Ctrl held zooms (`on`, with the
/// step `zoom::Wheel` makes of it, a notch or 60 px of a touchpad), as
/// Slint's `zoom_keys` takes it, and nothing under it scrolls.
pub struct CtrlWheel<'a> {
    child: Element<'a, Message>,
    on: fn(crate::zoom::Step) -> Message,
}

impl<'a> CtrlWheel<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        on: fn(crate::zoom::Step) -> Message,
    ) -> Self {
        Self {
            child: child.into(),
            on,
        }
    }
}

#[derive(Default)]
struct CtrlWheelState {
    control: bool,
    wheel: crate::zoom::Wheel,
}

impl Widget<Message, Theme, Renderer> for CtrlWheel<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<CtrlWheelState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(CtrlWheelState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        let state = tree.state.downcast_mut::<CtrlWheelState>();
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
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<CtrlWheel<'a>> for Element<'a, Message> {
    fn from(wheel: CtrlWheel<'a>) -> Self {
        Element::new(wheel)
    }
}

// ---------------------------------------------------------------------------
// Autoscroll

/// How far the pointer can be from where it clicked without scrolling; how
/// fast it scrolls just past that, in px a second for each px; how much
/// faster the farther, for each px squared; and the fastest, in px a
/// second: Slint's `yapfast_autoscroll`.
const AUTOSCROLL_STILL: f32 = 10.0;
const AUTOSCROLL_SPEED: f32 = 8.0;
const AUTOSCROLL_ACCELERATION: f32 = 0.05;
const AUTOSCROLL_FASTEST: f32 = 12000.0;
/// The marker at the click.
const AUTOSCROLL_MARKER: f32 = 30.0;

/// How fast the pointer `distance` px from where it clicked scrolls, in px
/// a second.
pub fn autoscroll_speed(distance: f32) -> f32 {
    let past = distance.abs() - AUTOSCROLL_STILL;
    if past <= 0.0 {
        return 0.0;
    }
    (past * AUTOSCROLL_SPEED + past * past * AUTOSCROLL_ACCELERATION)
        .min(AUTOSCROLL_FASTEST)
        .copysign(distance)
}

/// The marker's look: its ground, edge and dot, and the arrows' pictures.
pub struct AutoscrollLook {
    pub ground: Color,
    pub edge: Color,
    pub dot: Color,
    pub up: Handle,
    pub down: Handle,
}

/// Around the whole window, as Slint's `yapfast_autoscroll` and browsers:
/// a middle click on a list that has more than fits scrolls it towards the
/// pointer, faster the farther the pointer is from the click, until the
/// next press, a key or the wheel; held and moved away, until it's let go.
/// Meanwhile the pointer's events go to nothing under it, the press that
/// ends it included, and a marker shows at the click. It scrolls on each
/// frame it asks for while it goes on, and asks for none after. Lists
/// anchored to their foot (`anchored`, by id) scroll the other way round.
pub struct Autoscroll<'a> {
    child: Element<'a, Message>,
    anchored: &'static [&'static str],
    look: AutoscrollLook,
}

impl<'a> Autoscroll<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        anchored: &'static [&'static str],
        look: AutoscrollLook,
    ) -> Self {
        Self {
            child: child.into(),
            anchored,
            look,
        }
    }

    /// Runs `operation` on the child.
    fn operate_child(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
}

#[derive(Default)]
struct AutoscrollState {
    scroll: Option<Scrolling>,
}

struct Scrolling {
    origin: Point,
    pointer: Point,
    /// The middle button is still down; moved away with it down, letting
    /// go ends it.
    held: bool,
    dragged: bool,
    /// The button whose press ended it, until it's let go.
    ending: Option<mouse::Button>,
    last: Option<std::time::Instant>,
}

/// Finds the innermost list under a point that has more than fits, by its
/// place in the traversal, and with `by` set scrolls it.
struct ScrollAt<'a> {
    at: Point,
    anchored: &'a [&'static str],
    seen: usize,
    found: Option<usize>,
    by: Option<(usize, f32)>,
}

impl Operation for ScrollAt<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        let index = self.seen;
        self.seen += 1;
        if bounds.contains(self.at) && content_bounds.height > bounds.height + 1.0 {
            self.found = Some(index);
        }
        if let Some((target, by)) = self.by
            && target == index
        {
            let anchored = id.is_some_and(|id| {
                self.anchored
                    .iter()
                    .any(|name| iced::advanced::widget::Id::new(name) == *id)
            });
            let by = if anchored { -by } else { by };
            state.scroll_by(
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: by },
                bounds,
                content_bounds,
            );
        }
    }
}

impl Widget<Message, Theme, Renderer> for Autoscroll<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<AutoscrollState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(AutoscrollState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.operate_child(tree, layout, renderer, operation);
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
        let going = tree
            .state
            .downcast_ref::<AutoscrollState>()
            .scroll
            .is_some();
        if going {
            let state = tree.state.downcast_mut::<AutoscrollState>();
            let scroll = state.scroll.as_mut().expect("going");
            let mut take = true;
            match event {
                iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    scroll.pointer = *position;
                    if scroll.held && scroll.pointer.distance(scroll.origin) > AUTOSCROLL_STILL {
                        scroll.dragged = true;
                    }
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                    scroll.ending = Some(*button);
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                    if scroll.ending == Some(*button) {
                        state.scroll = None;
                    } else if *button == mouse::Button::Middle {
                        scroll.held = false;
                        if scroll.dragged {
                            state.scroll = None;
                        }
                    }
                    shell.request_redraw();
                }
                iced::Event::Mouse(mouse::Event::WheelScrolled { .. })
                | iced::Event::Keyboard(_)
                | iced::Event::Window(window::Event::Unfocused) => {
                    state.scroll = None;
                    take = false;
                    shell.request_redraw();
                }
                iced::Event::Window(window::Event::RedrawRequested(now))
                    if scroll.ending.is_none() =>
                {
                    let elapsed = scroll
                        .last
                        .map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.05));
                    scroll.last = Some(*now);
                    let by = autoscroll_speed(scroll.pointer.y - scroll.origin.y) * elapsed;
                    let origin = scroll.origin;
                    if by != 0.0 {
                        let mut find = ScrollAt {
                            at: origin,
                            anchored: self.anchored,
                            seen: 0,
                            found: None,
                            by: None,
                        };
                        self.operate_child(tree, layout, renderer, &mut find);
                        if let Some(found) = find.found {
                            let mut scroll = ScrollAt {
                                by: Some((found, by)),
                                seen: 0,
                                found: None,
                                ..find
                            };
                            self.operate_child(tree, layout, renderer, &mut scroll);
                        }
                    }
                    shell.request_redraw();
                    take = false;
                }
                _ => take = false,
            }
            if take {
                shell.capture_event();
                return;
            }
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if !going
            && let iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) = event
            && !shell.is_event_captured()
            && let Some(at) = cursor.position()
        {
            let mut find = ScrollAt {
                at,
                anchored: self.anchored,
                seen: 0,
                found: None,
                by: None,
            };
            self.operate_child(tree, layout, renderer, &mut find);
            if find.found.is_some() {
                tree.state.downcast_mut::<AutoscrollState>().scroll = Some(Scrolling {
                    origin: at,
                    pointer: at,
                    held: true,
                    dragged: false,
                    ending: None,
                    last: None,
                });
                shell.capture_event();
                shell.request_redraw();
            }
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
        if let Some(scroll) = &tree.state.downcast_ref::<AutoscrollState>().scroll {
            return if scroll.ending.is_some() {
                mouse::Interaction::default()
            } else {
                mouse::Interaction::ResizingVertically
            };
        }
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let Some(scroll) = tree
            .state
            .downcast_ref::<AutoscrollState>()
            .scroll
            .as_ref()
            .filter(|scroll| scroll.ending.is_none())
        else {
            return;
        };
        // A circle at the click with the arrows the list can go, as Slint's
        // `AutoscrollMarker`.
        let side = AUTOSCROLL_MARKER;
        let marker = Rectangle::new(
            scroll.origin - Vector::new(side / 2.0, side / 2.0),
            Size::new(side, side),
        );
        let look = &self.look;
        renderer.with_layer(marker.expand(12.0), |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: marker,
                    border: Border {
                        color: look.edge,
                        width: 1.0,
                        radius: (side / 2.0).into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba8(0, 0, 0, 0x60 as f32 / 255.0),
                        offset: Vector::new(0.0, 2.0),
                        blur_radius: 8.0,
                    },
                    ..Default::default()
                },
                look.ground,
            );
            let centre = marker.center();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(centre - Vector::new(2.0, 2.0), Size::new(4.0, 4.0)),
                    border: Border::default().rounded(2),
                    ..Default::default()
                },
                look.dot,
            );
            for (handle, y) in [
                (&look.up, marker.y + 3.0),
                (&look.down, marker.y + side - 13.0),
            ] {
                let at = Rectangle::new(Point::new(centre.x - 5.0, y), Size::new(10.0, 10.0));
                draw_picture(renderer, handle, at, [0.0; 4], 1.0);
            }
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Autoscroll<'a>> for Element<'a, Message> {
    fn from(autoscroll: Autoscroll<'a>) -> Self {
        Element::new(autoscroll)
    }
}

#[cfg(test)]
mod autoscroll_tests {
    use super::autoscroll_speed;

    #[test]
    fn autoscroll_is_still_near_the_click_and_faster_farther() {
        assert_eq!(autoscroll_speed(10.0), 0.0);
        assert_eq!(autoscroll_speed(-6.0), 0.0);
        // 10 px past: 80 px a second and 5 for the acceleration.
        assert!((autoscroll_speed(20.0) - 85.0).abs() < 0.001);
        assert!((autoscroll_speed(-20.0) + 85.0).abs() < 0.001);
        assert_eq!(autoscroll_speed(5000.0), 12000.0);
    }
}

// ---------------------------------------------------------------------------
// A ring that pulses

/// A round ring of `color`, `width` px, over its child, its opacity
/// pulsing between 0.05 and 0.85 once a second as the Slint client's
/// ringing card's (`0.45 + 0.4 × sin`), drawn anew on each frame it asks
/// for while it shows: a call ringing.
pub struct Pulse<'a> {
    child: Element<'a, Message>,
    color: Color,
    width: f32,
}

impl<'a> Pulse<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, color: Color, width: f32) -> Self {
        Self {
            child: child.into(),
            color,
            width,
        }
    }
}

/// When it began, and the time of the frame being drawn.
struct PulseState {
    began: Option<std::time::Instant>,
    now: Option<std::time::Instant>,
}

/// The ring's opacity `elapsed` into its pulse.
pub fn pulse_opacity(elapsed: Duration) -> f32 {
    let degrees = elapsed.as_secs_f32() * 1000.0 * 0.36;
    0.45 + 0.4 * degrees.to_radians().sin()
}

impl Widget<Message, Theme, Renderer> for Pulse<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PulseState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PulseState {
            began: None,
            now: None,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<PulseState>();
            state.began.get_or_insert(*now);
            state.now = Some(*now);
            shell.request_redraw();
        }
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        let state = tree.state.downcast_ref::<PulseState>();
        let elapsed = match (state.began, state.now) {
            (Some(began), Some(now)) => now.duration_since(began),
            _ => Duration::ZERO,
        };
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: Color {
                        a: self.color.a * pulse_opacity(elapsed),
                        ..self.color
                    },
                    width: self.width,
                    radius: (bounds.width.min(bounds.height) / 2.0).into(),
                },
                ..Default::default()
            },
            Color::TRANSPARENT,
        );
        self.child.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, Renderer>> {
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Pulse<'a>> for Element<'a, Message> {
    fn from(pulse: Pulse<'a>) -> Self {
        Element::new(pulse)
    }
}

#[cfg(test)]
mod pulse_tests {
    #[test]
    fn the_ring_pulses_once_a_second() {
        use std::time::Duration;
        let at = |ms| super::pulse_opacity(Duration::from_millis(ms));
        assert!((at(0) - 0.45).abs() < 0.001);
        assert!((at(250) - 0.85).abs() < 0.001);
        assert!((at(750) - 0.05).abs() < 0.001);
        assert!((at(1000) - 0.45).abs() < 0.001);
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

/// A card with a profile frame round it, as the Slint client's
/// `ProfileFrameLayer`s: the back layers under the card, the front ones
/// over it, each `side` past its left and right; a staple at the top sits
/// `top_shift` lower (the full profile's 8 px). Its layout is the card's;
/// the frame draws past it and takes no clicks.
pub struct Framed<'a> {
    child: Element<'a, Message>,
    layers: Vec<FrameLayer>,
    side: f32,
    top_shift: f32,
}

impl<'a> Framed<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        layers: Vec<FrameLayer>,
        side: f32,
        top_shift: f32,
    ) -> Self {
        Self {
            child: child.into(),
            layers,
            side,
            top_shift,
        }
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
            frame_picture(renderer, &layer.picture, at(y, layer.height));
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
                        frame_picture(renderer, &layer.picture, at(y, height));
                    }
                    FrameKind::Border if height > 0.0 => {
                        let copies = (card.height / height).ceil() as usize + 1;
                        for copy in 0..copies {
                            frame_picture(
                                renderer,
                                &layer.picture,
                                at(copy as f32 * height, height),
                            );
                        }
                    }
                    FrameKind::Border => {}
                }
            }
        });
    }
}

fn frame_picture(renderer: &mut Renderer, handle: &Handle, bounds: Rectangle) {
    use iced::advanced::image::Renderer as _;
    renderer.draw_image(
        raster::Image {
            handle: handle.clone(),
            filter_method: raster::FilterMethod::Linear,
            rotation: iced::Radians(0.0),
            border_radius: Default::default(),
            opacity: 1.0,
            snap: true,
        },
        bounds,
        bounds,
    );
}

impl Widget<Message, Theme, Renderer> for Framed<'_> {
    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.child.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        self.child.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
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
        let card = layout.bounds();
        // The back layers under the card's ground, the card in a layer of
        // its own over them, and the front ones in another over it.
        self.draw_layers(renderer, card, false);
        let room = viewport.expand(self.side);
        renderer.with_layer(room, |renderer| {
            self.child.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        });
        if self.layers.iter().any(|layer| layer.front) {
            renderer.with_layer(room, |renderer| self.draw_layers(renderer, card, true));
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
        self.child.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Framed<'a>> for Element<'a, Message> {
    fn from(framed: Framed<'a>) -> Self {
        Element::new(framed)
    }
}

// ---------------------------------------------------------------------------
// A colour's saturation, brightness and hue

/// A colour as hue (0 to 360), saturation and brightness (0 to 1 each).
pub type Hsv = [f32; 3];

/// The colour of `hsv`, as 0xRRGGBB.
pub fn hsv_rgb([hue, sat, val]: Hsv) -> u32 {
    let hue = hue.rem_euclid(360.0) / 60.0;
    let chroma = val * sat;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let channel = |c: f32| (((c + val - chroma) * 255.0).round().clamp(0.0, 255.0)) as u32;
    channel(r) << 16 | channel(g) << 8 | channel(b)
}

/// The hue, saturation and brightness of 0xRRGGBB.
pub fn rgb_hsv(rgb: u32) -> Hsv {
    let [_, r, g, b] = rgb.to_be_bytes().map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let chroma = max - r.min(g).min(b);
    let hue = if chroma == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    };
    let sat = if max == 0.0 { 0.0 } else { chroma / max };
    [hue, sat, max]
}

/// The square and strip of the Slint client's `ColourPopover`: saturation
/// across and brightness down a square of the hue, a ring where the colour
/// is; under it the hues, a handle at the one chosen. Pressing or dragging
/// on either chooses; the colour is the caller's, kept as [`Hsv`] so a hue
/// isn't lost at grey or black.
pub struct ColourPad<'a> {
    hsv: Hsv,
    chosen: Box<dyn Fn(Hsv) -> Message + 'a>,
}

/// The square's height, the gap and the strip's.
const PAD_SQUARE: f32 = 150.0;
const PAD_GAP: f32 = 10.0;
const PAD_STRIP: f32 = 10.0;

impl<'a> ColourPad<'a> {
    pub fn new(hsv: Hsv, chosen: impl Fn(Hsv) -> Message + 'a) -> Self {
        Self {
            hsv,
            chosen: Box::new(chosen),
        }
    }

    fn parts(bounds: Rectangle) -> (Rectangle, Rectangle) {
        let square = Rectangle::new(bounds.position(), Size::new(bounds.width, PAD_SQUARE));
        let strip = Rectangle::new(
            Point::new(bounds.x, bounds.y + PAD_SQUARE + PAD_GAP),
            Size::new(bounds.width, PAD_STRIP),
        );
        (square, strip)
    }

    /// The colour a point in a part chooses.
    fn at(&self, part: PadPart, bounds: Rectangle, point: Point) -> Hsv {
        let (square, strip) = Self::parts(bounds);
        let [hue, sat, val] = self.hsv;
        let across = |part: Rectangle| ((point.x - part.x) / part.width.max(1.0)).clamp(0.0, 1.0);
        match part {
            PadPart::Square => [
                hue,
                across(square),
                1.0 - ((point.y - square.y) / square.height).clamp(0.0, 1.0),
            ],
            PadPart::Strip => [across(strip) * 360.0, sat, val],
        }
    }
}

#[derive(Clone, Copy)]
enum PadPart {
    Square,
    Strip,
}

/// The part the button went down on, while it's down.
#[derive(Default)]
struct PadState {
    held: Option<PadPart>,
}

impl Widget<Message, Theme, Renderer> for ColourPad<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fill,
            Length::Fixed(PAD_SQUARE + PAD_GAP + PAD_STRIP),
        )
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PadState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PadState::default())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fill,
            PAD_SQUARE + PAD_GAP + PAD_STRIP,
            Size::ZERO,
        ))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (square, strip) = Self::parts(bounds);
        let state = tree.state.downcast_mut::<PadState>();
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let part = if cursor.is_over(square) {
                    PadPart::Square
                } else if cursor.is_over(strip.expand(3.0)) {
                    PadPart::Strip
                } else {
                    return;
                };
                if let Some(position) = cursor.position() {
                    state.held = Some(part);
                    shell.publish((self.chosen)(self.at(part, bounds, position)));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(part) = state.held {
                    shell.publish((self.chosen)(self.at(part, bounds, *position)));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.held = None;
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let (square, strip) = Self::parts(layout.bounds());
        if cursor.is_over(square) || cursor.is_over(strip.expand(3.0)) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let (square, strip) = Self::parts(layout.bounds());
        let [hue, sat, val] = self.hsv;
        let colour = |rgb: u32| {
            let [_, r, g, b] = rgb.to_be_bytes();
            Color::from_rgb8(r, g, b)
        };
        let fill = |renderer: &mut Renderer,
                    bounds: Rectangle,
                    radius: f32,
                    angle: f32,
                    stops: &[(f32, Color)]| {
            let mut linear = gradient::Linear::new(iced::Degrees(angle));
            for (offset, colour) in stops {
                linear = linear.add_stop(*offset, *colour);
            }
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border::default().rounded(radius),
                    ..Default::default()
                },
                Background::Gradient(linear.into()),
            );
        };
        // White to the hue across, then clear to black down.
        fill(
            renderer,
            square,
            8.0,
            90.0,
            &[(0.0, Color::WHITE), (1.0, colour(hsv_rgb([hue, 1.0, 1.0])))],
        );
        fill(
            renderer,
            square,
            8.0,
            180.0,
            &[(0.0, Color::TRANSPARENT), (1.0, Color::BLACK)],
        );
        let ring = Rectangle::new(
            Point::new(
                square.x + sat * square.width - 6.0,
                square.y + (1.0 - val) * square.height - 6.0,
            ),
            Size::new(12.0, 12.0),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: ring,
                border: Border {
                    color: Color::WHITE,
                    width: 2.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
        let hues = [
            (0.0, 0xff0000),
            (0.17, 0xffff00),
            (0.33, 0x00ff00),
            (0.5, 0x00ffff),
            (0.67, 0x0000ff),
            (0.83, 0xff00ff),
            (1.0, 0xff0000),
        ]
        .map(|(offset, rgb)| (offset, colour(rgb)));
        fill(renderer, strip, 5.0, 90.0, &hues);
        let handle = Rectangle::new(
            Point::new(strip.x + hue / 360.0 * strip.width - 4.0, strip.y - 3.0),
            Size::new(8.0, 16.0),
        );
        renderer.fill_quad(
            renderer::Quad {
                bounds: handle,
                border: Border::default().rounded(4),
                ..Default::default()
            },
            Background::Color(Color::WHITE),
        );
    }
}

impl<'a> From<ColourPad<'a>> for Element<'a, Message> {
    fn from(pad: ColourPad<'a>) -> Self {
        Element::new(pad)
    }
}

#[cfg(test)]
mod colour_tests {
    use super::*;

    #[test]
    fn colours_go_to_hsv_and_back() {
        for rgb in [
            0xff0000, 0x00ff00, 0x0000ff, 0x5865f2, 0xfde2e4, 0x000000, 0xffffff, 0x808080,
        ] {
            assert_eq!(hsv_rgb(rgb_hsv(rgb)), rgb, "{rgb:06x}");
        }
        assert_eq!(rgb_hsv(0x00ff00), [120.0, 1.0, 1.0]);
        assert_eq!(hsv_rgb([240.0, 1.0, 0.5]), 0x000080);
    }
}
