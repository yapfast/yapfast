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
        }
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
                renderer.with_layer(bounds, |renderer| above(renderer, true));
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
pub struct Place<'a> {
    child: Element<'a, Message>,
    at: Point,
    margin: f32,
    anchor: Anchor,
}

/// How `Place` reads its point.
#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    /// The child's top left corner, moved in to fit.
    Corner,
    /// As a menu opens at the pointer: below it if it fits, else above it.
    Pointer,
    /// The child's bottom left corner, as suggestions sit over the composer.
    Bottom,
}

impl<'a> Place<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, at: Point, margin: f32) -> Self {
        Self {
            child: child.into(),
            at,
            margin,
            anchor: Anchor::Corner,
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Place<'_> {
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
        let node = self.child.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, room),
        );
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
            Anchor::Pointer => self.at.y - size.height - 4.0,
            Anchor::Bottom => self.at.y - size.height,
        }
        .max(self.margin);
        layout::Node::with_children(room, vec![node.move_to((x, y))])
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
}

impl<'a> Corner<'a> {
    pub fn new(child: impl Into<Element<'a, Message>>, right: f32, top: f32) -> Self {
        Self {
            child: child.into(),
            right,
            top,
        }
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
        let child = self.child.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY)),
        );
        let x = (width - self.right - child.size().width).max(0.0);
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
}

impl<'a> Spot<'a> {
    pub fn new(
        child: impl Into<Element<'a, Message>>,
        spot: &'a std::cell::Cell<Rectangle>,
    ) -> Self {
        Self {
            child: child.into(),
            spot,
        }
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
            && cursor.is_over(layout.bounds())
        {
            self.spot.set(layout.bounds());
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
