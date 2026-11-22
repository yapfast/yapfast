//! Installed-theme art behind ordinary iced surfaces: fitted and tiled image
//! layers, sprite sheets and drift, fake glass, particles and nine-slice list
//! art. These widgets deliberately have no input handling, so they cannot
//! change a surface's behaviour.

use std::time::{Duration, Instant};

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer, text,
    widget::{self, Operation, Tree},
};
use iced::{Background, Color, Element, Length, Point, Rectangle, Size, Theme, Vector};
use yapfast_core::themes::Performance;
use yapfast_core::themes::layout::{Border as ThemeBorder, BorderPlace, Gel, Stroke};
use yapfast_core::themes::package::art::{
    ArtSurface, Depth, Emitter, Fit, Overlay, OverlayKind, Preset, Trigger,
};

use crate::{
    app::Message,
    themes::{ArtLayer, GlassPane, Pace, ParticleEvent, ThemeParticles, TileArtImage},
    widgets::gradient_quad,
};

struct Art<'a> {
    layers: &'a [ArtLayer],
    performance: Performance,
    reduced_motion: bool,
    running: bool,
    pace: Pace,
    pins: Pins,
    /// Whether it draws a surface's rims ([`ArtLayer::rim`]): under the
    /// content it draws the rest, over it the rims alone.
    rims: Rims,
    /// Slint's `ArtLayer` clips its pictures to the surface's rounded
    /// rectangle: the radius, and whether it rounds every corner (`true`)
    /// or only those it shares with the pane it is in.
    round: (f32, bool),
}

/// Which of a surface's layers an [`Art`] draws by their rims.
#[derive(Clone, Copy, PartialEq)]
enum Rims {
    With,
    Without,
    Only,
}

/// Which of a surface's layers an [`Art`] draws: tape that pins the
/// surface down ([`ArtLayer::pin`]) lies over its top edge, outside it, so
/// a surface whose content scrolls draws it apart, over the content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pins {
    With,
    Without,
    Only,
}

struct Procedural<'a> {
    overlays: &'a [Overlay],
    performance: Performance,
    reduced_motion: bool,
    running: bool,
    pace: Pace,
}

#[derive(Default)]
struct MotionClock {
    elapsed: Duration,
    last: Option<Instant>,
    running: bool,
}

impl MotionClock {
    fn redraw(&mut self, now: Instant, running: bool) {
        if running
            && self.running
            && let Some(last) = self.last
        {
            self.elapsed += now.saturating_duration_since(last);
        }
        self.last = Some(now);
        self.running = running;
    }

    fn seconds(&self) -> f32 {
        self.elapsed.as_secs_f32()
    }
}

fn clock_tag() -> widget::tree::Tag {
    widget::tree::Tag::of::<MotionClock>()
}

fn clock_state() -> widget::tree::State {
    widget::tree::State::new(MotionClock::default())
}

fn tick_clock(tree: &mut Tree, event: &iced::Event, running: bool) {
    if let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
        tree.state
            .downcast_mut::<MotionClock>()
            .redraw(*now, running);
    }
}

fn clock_seconds(tree: &Tree) -> f32 {
    tree.state.downcast_ref::<MotionClock>().seconds()
}

fn animation_running(performance: Performance, plays: bool) -> bool {
    performance == Performance::Full || (performance == Performance::Balanced && plays)
}

/// Whether a layer marked `heavy` moves: only on Full, and there only on
/// the GPU. A heavy layer is a big picture that changes (Hurt's breathing
/// room, a sheet over the whole conversation); on the CPU renderer each of
/// its frames draws that whole area again, a fifth of a core for a slow
/// breath, so there it holds still, as on Balanced and as grain does.
fn heavy_plays(performance: Performance, heavy: bool) -> bool {
    !heavy || (performance == Performance::Full && yapfast_iced_kit::renderer::gpu())
}

/// How long a moving layer can wait between frames: a sprite sheet that
/// doesn't slide only needs a frame when its picture changes (Hurt's 12
/// breaths over 9 s), so it isn't redrawn 60 times a second.
fn layer_wait(layer: &yapfast_core::themes::package::art::Layer, fps: u64) -> u64 {
    let smooth = 1000 / fps.max(1);
    match layer.frames.filter(|frames| frames.count > 1) {
        Some(frames) if layer.drift == (0.0, 0.0) => {
            ((frames.period.max(1.0) / frames.count as f32) as u64).max(smooth)
        }
        _ => smooth,
    }
}

/// How long a moving procedural overlay can wait between frames: grain
/// changes once a period (Zine's every 180 ms), a band and a flare move
/// smoothly.
fn overlay_wait(overlay: &Overlay, fps: u64) -> u64 {
    let smooth = 1000 / fps.max(1);
    match overlay.kind {
        OverlayKind::Grain => (overlay.period as u64).max(smooth),
        OverlayKind::Image => overlay
            .layer
            .as_ref()
            .map_or(smooth, |layer| layer_wait(layer, fps)),
        _ => smooth,
    }
}

/// Whether an overlay moves here. Grain changes the whole window each
/// period, which costs the CPU renderer two whole-window draws a step (its
/// buffers swap): there it holds still, as on Balanced, and moves on the
/// GPU, as in Slint.
fn overlay_moves(overlay: &Overlay) -> bool {
    overlay.moves() && (overlay.kind != OverlayKind::Grain || yapfast_iced_kit::renderer::gpu())
}

/// Pictures made once and kept by what made them, the latest last.
type Kept<K> = Vec<(K, iced::advanced::image::Handle)>;

/// Grain pictures a cycle, and blocks (4 px each) a side of one.
const GRAIN_PHASES: u32 = 8;
const GRAIN_SIDE: u32 = yapfast_core::themes::check::art::TEXTURE_SIDE;

/// The grain's picture for `phase` in `colour`, as Slint's grain
/// texture: every pixel at a hash's share of the colour, 256 px a side,
/// drawn a pixel a pixel. Each phase seeds the hash afresh (Slint shifts
/// one picture instead).
fn grain_tile(phase: u32, colour: Color) -> iced::advanced::image::Handle {
    use std::cell::RefCell;
    thread_local! {
        static TILES: RefCell<Kept<(u32, [u8; 4])>> = const { RefCell::new(Vec::new()) };
    }
    let key = (phase, colour.into_rgba8());
    TILES.with_borrow_mut(|tiles| {
        if let Some((_, handle)) = tiles.iter().find(|(at, _)| *at == key) {
            return handle.clone();
        }
        let side = GRAIN_SIDE;
        let [r, g, b, _] = colour.into_rgba8();
        let mut pixels = Vec::with_capacity((side * side * 4) as usize);
        for y in 0..side {
            for x in 0..side {
                let amount = grain_hash(x, y, 1 + phase);
                let alpha = (colour.a * amount * 255.0).round() as u8;
                pixels.extend_from_slice(&[r, g, b, alpha]);
            }
        }
        crate::paint::to_native(&mut pixels);
        let handle = iced::advanced::image::Handle::from_rgba(side, side, pixels);
        // Room for every phase in four colours, so three or four grain
        // colours on screen don't make new pictures every frame.
        if tiles.len() >= (GRAIN_PHASES * 4) as usize {
            tiles.remove(0);
        }
        tiles.push((key, handle.clone()));
        handle
    })
}

/// Slint's texture hash: 0 to 1 for a pixel.
fn grain_hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut value =
        x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca77) ^ seed.wrapping_mul(0xc2b2_ae3d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x2c1b_3c6d);
    value ^= value >> 12;
    value = value.wrapping_mul(0x297a_2d39);
    value ^= value >> 15;
    (value & 0xffff) as f32 / 65535.0
}

// Every theme motion wakes the window on one grid (`themes`): stepped
// loops and particles on its 100 ms beat, smooth ones at 60 Hz.
pub(crate) use crate::themes::{STEP_BEAT, on_beat, wake_at};

fn frame_fps(performance: Performance, pace: Pace) -> u64 {
    let base = if performance == Performance::Full {
        60
    } else {
        30
    };
    pace.cap_fps(base).max(1)
}

#[derive(Clone, Copy)]
struct DashedOutline {
    border: ThemeBorder,
    width: f32,
    colour: Color,
    radius: f32,
}

struct ThemeRule {
    border: Option<ThemeBorder>,
    width: f32,
    colour: Color,
}

/// A package's IRC separator repeated through every wrapped line. It wraps
/// the message body so its height comes from the content it sits beside,
/// without measuring hundreds of separator lines on every layout pass.
struct IrcRule<'a> {
    separator: String,
    repeated: String,
    size: f32,
    line_height: f32,
    font: iced::Font,
    colour: Color,
    content: Element<'a, Message>,
}

impl Widget<Message, Theme, iced::Renderer> for IrcRule<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        use iced::advanced::text::Paragraph as _;

        const GAP: f32 = 6.0;

        let paragraph = <iced::Renderer as text::Renderer>::Paragraph::with_text(text::Text {
            content: self.separator.as_str(),
            bounds: Size::new(f32::INFINITY, self.line_height),
            size: self.size.into(),
            line_height: text::LineHeight::Absolute(self.line_height.into()),
            font: self.font,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Center,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        });
        let rule_width = paragraph.min_bounds().width;
        let content = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &limits.shrink(Size::new(rule_width + GAP, 0.0)),
        );
        let content_bounds = content.bounds();
        layout::Node::with_children(
            Size::new(
                rule_width + GAP + content_bounds.width,
                content_bounds.height,
            ),
            vec![
                layout::Node::new(Size::new(rule_width, content_bounds.height)),
                content.move_to(Point::new(rule_width + GAP, 0.0)),
            ],
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout
                .children()
                .nth(1)
                .expect("the IRC rule's message body"),
            renderer,
            operation,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout
                .children()
                .nth(1)
                .expect("the IRC rule's message body"),
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
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout
                .children()
                .nth(1)
                .expect("the IRC rule's message body"),
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::text::Renderer as _;

        let rule = layout
            .children()
            .next()
            .expect("the IRC rule's separator")
            .bounds();
        // As in the Slint build, three copies a third of a glyph apart make a
        // line-character separator join cleanly between wrapped lines.
        for shift in 0..3 {
            renderer.fill_text(
                text::Text {
                    content: self.repeated.clone(),
                    bounds: rule.size(),
                    size: self.size.into(),
                    line_height: text::LineHeight::Absolute(self.line_height.into()),
                    font: self.font,
                    align_x: text::Alignment::Default,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::None,
                },
                Point::new(rule.x, rule.y + self.size * shift as f32 / 3.0),
                self.colour,
                rule,
            );
        }

        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout
                .children()
                .nth(1)
                .expect("the IRC rule's message body"),
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout
                .children()
                .nth(1)
                .expect("the IRC rule's message body"),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<IrcRule<'a>> for Element<'a, Message> {
    fn from(rule: IrcRule<'a>) -> Self {
        Element::new(rule)
    }
}

/// The Slint theme's crisp gel drawn at the resolved size of one bubble.
/// The child retains all of its own layout, input and overlay behaviour.
struct BubbleGel<'a> {
    content: Element<'a, Message>,
    background: Background,
    radius: f32,
    gel: Gel,
    /// A gel button's state: lit a touch under the pointer (1), darker
    /// while pressed (-1), else 0.
    lift: f32,
    /// The gloss lies over the content (a picture under the gel) rather
    /// than under it (words on the gel).
    gloss_over: bool,
    /// A button: it brightens under the pointer and darkens pressed.
    interactive: bool,
}

/// A gel button's pointer: over it, and pressed on it.
#[derive(Default)]
struct GelPointer {
    over: bool,
    pressed: bool,
}

impl Widget<Message, Theme, iced::Renderer> for BubbleGel<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<GelPointer>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(GelPointer::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if self.interactive {
            let state = tree.state.downcast_mut::<GelPointer>();
            let over = cursor.is_over(layout.bounds());
            let pressed = match event {
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => over,
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => false,
                _ => state.pressed,
            };
            if over != state.over || pressed != state.pressed {
                state.over = over;
                state.pressed = pressed;
                shell.request_redraw();
            }
        }
        self.content.as_widget_mut().update(
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
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
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
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let shadow = self
            .gel
            .shadow
            .map_or_else(iced::Shadow::default, |shadow| iced::Shadow {
                color: crate::themes::argb(shadow.colour),
                offset: Vector::new(shadow.x, shadow.y),
                blur_radius: shadow.blur,
            });
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border::default().rounded(self.radius),
                shadow,
                ..Default::default()
            },
            self.background,
        );

        // Quads in one layer draw in order: the shine needs no layer of its
        // own over the ground (a layer each is a cost every frame).
        let paint = |renderer: &mut iced::Renderer| {
            {
                if let Some(glow) = self.gel.glow {
                    // Slint draws the glow along the bottom clipped to the
                    // bubble's round corners, fading to the glow's own colour
                    // at no alpha (not to clear black, which greys it): one
                    // gradient over the whole bubble, clear above the glow.
                    let height = glow.height.min(bounds.height).max(0.0);
                    if height > 0.0 && bounds.height > 0.0 {
                        let colour = crate::themes::argb(glow.colour);
                        let clear = Color { a: 0.0, ..colour };
                        let start = (1.0 - height / bounds.height).clamp(0.0, 1.0);
                        gradient_quad(
                            renderer,
                            bounds,
                            self.radius.min(bounds.height / 2.0).min(bounds.width / 2.0),
                            180.0,
                            &[(0.0, clear), (start, clear), (1.0, colour)],
                        );
                    }
                }
                if let Some(mut gloss) = self.gel.gloss {
                    if self.interactive {
                        // A round gel button's gloss is a pill inside its top,
                        // clear of the rounded edge, as a gel orb's.
                        let round = self.radius.min(bounds.height / 2.0);
                        gloss.inset = (
                            gloss.inset.0.max(round * 0.34),
                            gloss.inset.1.max(bounds.height * 0.07),
                        );
                        gloss.max = gloss.max.max(bounds.height * 0.42);
                        gloss.height = gloss.height.max(0.42);
                        gloss.radius = None;
                    }
                    let height = (bounds.height * gloss.height).min(gloss.max).max(0.0);
                    let width = (bounds.width - gloss.inset.0 * 2.0).max(0.0);
                    if width > 0.0 && height > 0.0 {
                        let colour = crate::themes::argb(gloss.colour);
                        let at = |share: f32| Color {
                            a: colour.a * share.clamp(0.0, 1.0),
                            ..colour
                        };
                        let radius = gloss
                            .radius
                            .unwrap_or((self.radius - gloss.inset.0).max(0.0))
                            .min(width.min(height) / 2.0);
                        gradient_quad(
                            renderer,
                            Rectangle::new(
                                Point::new(bounds.x + gloss.inset.0, bounds.y + gloss.inset.1),
                                Size::new(width, height),
                            ),
                            radius,
                            180.0,
                            &[(0.0, at(gloss.from)), (1.0, at(gloss.to))],
                        );
                    }
                }
            }
        };
        // Over a picture (drawn after quads in a layer) the shine needs a
        // layer of its own; under words it doesn't.
        let shine = |renderer: &mut iced::Renderer| {
            if self.gloss_over {
                renderer.with_layer(bounds, |renderer| paint(renderer));
            } else {
                paint(renderer);
            }
        };
        if !self.gloss_over {
            shine(renderer);
        }

        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if self.gloss_over {
            shine(renderer);
        }
        let lift = if self.interactive {
            let state = tree.state.downcast_ref::<GelPointer>();
            if state.pressed && state.over {
                -1.0
            } else if state.over {
                1.0
            } else {
                self.lift
            }
        } else {
            self.lift
        };
        if lift != 0.0 {
            // Under the pointer the gel brightens; pressed, it darkens.
            let wash = if lift > 0.0 {
                Color::from_rgba(1.0, 1.0, 1.0, 0.16 * lift)
            } else {
                Color::from_rgba(0.0, 0.08, 0.2, 0.18 * -lift)
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: iced::Border::default().rounded(self.radius),
                    ..Default::default()
                },
                wash,
            );
        }

        if let Some(light) = self.gel.rim_light {
            let light = crate::themes::argb(light);
            let faded = |alpha: f32| Color {
                a: light.a * alpha,
                ..light
            };
            let inset = Rectangle::new(
                Point::new(bounds.x + 1.0, bounds.y + 1.0),
                Size::new(
                    (bounds.width - 2.0).max(0.0),
                    (bounds.height - 2.0).max(0.0),
                ),
            );
            if inset.width > 0.0 && inset.height > 0.0 {
                // The rim follows the bubble's round corners, as Slint's
                // rounded border: its top band lit, its bottom band at 65%,
                // the sides fading between them.
                let radius = (self.radius - 1.0).clamp(0.0, inset.height.min(inset.width) / 2.0);
                let band = radius.max(1.0);
                let ring = |renderer: &mut iced::Renderer, clip: Rectangle, colour: Color| {
                    renderer.with_layer(clip, |renderer| {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: inset,
                                border: iced::Border {
                                    color: colour,
                                    width: 1.0,
                                    radius: radius.into(),
                                },
                                ..Default::default()
                            },
                            Color::TRANSPARENT,
                        );
                    });
                };
                ring(
                    renderer,
                    Rectangle::new(inset.position(), Size::new(inset.width, band)),
                    light,
                );
                ring(
                    renderer,
                    Rectangle::new(
                        Point::new(inset.x, inset.y + inset.height - band),
                        Size::new(inset.width, band),
                    ),
                    faded(0.65),
                );
                let side = (inset.height - band * 2.0).max(0.0);
                if side > 0.0 {
                    for x in [inset.x, inset.x + inset.width - 1.0] {
                        gradient_quad(
                            renderer,
                            Rectangle::new(Point::new(x, inset.y + band), Size::new(1.0, side)),
                            0.0,
                            180.0,
                            &[
                                (0.0, light),
                                (0.4, faded(0.3)),
                                (0.7, faded(0.15)),
                                (1.0, faded(0.65)),
                            ],
                        );
                    }
                }
            }
        }
        if let Some(dark) = self.gel.rim_dark {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: iced::Border {
                        color: crate::themes::argb(dark),
                        width: 1.0,
                        radius: self.radius.into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<iced::advanced::overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<BubbleGel<'a>> for Element<'a, Message> {
    fn from(bubble: BubbleGel<'a>) -> Self {
        Element::new(bubble)
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for ThemeRule {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.width))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::new(limits.max().width, self.width))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        match self.border {
            Some(border) if border.stroke != Stroke::Solid => {
                let dotted = border.stroke == Stroke::Dotted;
                let dash = if dotted {
                    self.width
                } else {
                    border.dash.max(self.width)
                };
                draw_dashes(
                    renderer,
                    bounds.position(),
                    bounds.width,
                    self.width,
                    dash,
                    border.gap,
                    true,
                    dotted,
                    self.colour,
                );
            }
            _ => quad(renderer, bounds, self.colour, 0.0),
        }
    }
}

impl<'a, M: 'a> From<ThemeRule> for Element<'a, M> {
    fn from(rule: ThemeRule) -> Self {
        Element::new(rule)
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for DashedOutline {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let width = self.width.max(1.0);
        if self.border.stroke == Stroke::Solid {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: iced::Border {
                        color: self.colour,
                        width,
                        radius: self.radius.into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
            return;
        }
        let dotted = self.border.stroke == Stroke::Dotted;
        let dash = if dotted {
            width
        } else {
            self.border.dash.max(width)
        };
        let gap = self.border.gap.max(0.0);
        let inset = width / 2.0;
        let horizontal = (bounds.width - width).max(0.0);
        let vertical = (bounds.height - width).max(0.0);
        draw_dashes(
            renderer,
            Point::new(bounds.x + inset, bounds.y),
            horizontal,
            width,
            dash,
            gap,
            true,
            dotted,
            self.colour,
        );
        draw_dashes(
            renderer,
            Point::new(bounds.x + inset, bounds.y + bounds.height - width),
            horizontal,
            width,
            dash,
            gap,
            true,
            dotted,
            self.colour,
        );
        draw_dashes(
            renderer,
            Point::new(bounds.x, bounds.y + inset),
            vertical,
            width,
            dash,
            gap,
            false,
            dotted,
            self.colour,
        );
        draw_dashes(
            renderer,
            Point::new(bounds.x + bounds.width - width, bounds.y + inset),
            vertical,
            width,
            dash,
            gap,
            false,
            dotted,
            self.colour,
        );
    }
}

impl<'a, M: 'a> From<DashedOutline> for Element<'a, M> {
    fn from(outline: DashedOutline) -> Self {
        Element::new(outline)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
fn draw_dashes(
    renderer: &mut iced::Renderer,
    start: Point,
    length: f32,
    width: f32,
    dash: f32,
    gap: f32,
    horizontal: bool,
    dotted: bool,
    colour: Color,
) {
    let mut at = 0.0;
    while at < length {
        let run = dash.min(length - at);
        let origin = if horizontal {
            Point::new(start.x + at, start.y)
        } else {
            Point::new(start.x, start.y + at)
        };
        let size = if horizontal {
            Size::new(run, width)
        } else {
            Size::new(width, run)
        };
        quad(
            renderer,
            Rectangle::new(origin, size),
            colour,
            if dotted { width / 2.0 } else { 0.0 },
        );
        at += dash + gap;
    }
}

/// Adds a theme's dashed or dotted outline without changing the kit
/// Theme boundary. Missing border declarations return the original content.
pub fn outlined<'a>(
    content: impl Into<Element<'a, Message>>,
    cx: crate::ctx::Ctx<'a>,
    place: BorderPlace,
    surface: &str,
    stock_radius: f32,
    stock_width: f32,
    stock_colour: Color,
) -> Element<'a, Message> {
    let content = content.into();
    let Some(border) = cx
        .themes
        .package_layout(&cx.look.theme.id)
        .and_then(|layout| layout.borders.get(&place))
        .copied()
    else {
        return content;
    };
    let fallback = cx
        .look
        .theme
        .surface_border(surface, stock_radius, stock_width, stock_colour);
    let width = border.width.unwrap_or(fallback.width).max(1.0);
    let colour = border
        .colour
        .map(crate::themes::argb)
        .unwrap_or(fallback.color);
    iced::widget::stack![
        content,
        DashedOutline {
            border,
            width,
            colour,
            radius: cx.look.theme.surface_radius(surface, stock_radius),
        }
    ]
    .into()
}

/// A horizontal rule whose dash pattern comes from a layout border place.
pub fn rule<'a>(
    cx: crate::ctx::Ctx<'a>,
    place: BorderPlace,
    stock_colour: Color,
    stock_width: f32,
) -> Element<'a, Message> {
    let border = cx
        .themes
        .package_layout(&cx.look.theme.id)
        .and_then(|layout| layout.borders.get(&place))
        .copied();
    let width = border
        .and_then(|border| border.width)
        .unwrap_or(stock_width)
        .max(1.0);
    let colour = border
        .and_then(|border| border.colour)
        .map(crate::themes::argb)
        .unwrap_or(stock_colour);
    ThemeRule {
        border,
        width,
        colour,
    }
    .into()
}

/// The configured separator down every visual IRC line. A false `rule`
/// continues to use the ordinary one-off text widget in the message row.
pub fn irc_rule<'a>(
    separator: String,
    size: f32,
    line_height: f32,
    font: iced::Font,
    colour: Color,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    const RULE_LINES: usize = 240;
    let repeated = vec![separator.as_str(); RULE_LINES].join("\n");
    IrcRule {
        separator,
        repeated,
        size,
        line_height,
        font,
        colour,
        content,
    }
    .into()
}

/// Draws a non-empty theme gel around one bubble without changing the kit
/// theme boundary. Callers retain the stock container when the gel is empty.
pub fn gel_bubble<'a>(
    content: impl Into<Element<'a, Message>>,
    background: Background,
    radius: f32,
    gel: Gel,
) -> Element<'a, Message> {
    BubbleGel {
        content: content.into(),
        background,
        radius,
        gel,
        lift: 0.0,
        gloss_over: false,
        interactive: false,
    }
    .into()
}

/// A call control (mute, deafen, camera, share, leave) as a gel button
/// with its tooltip, `size` wide and tall, in a gel theme; `None` else.
/// The words on it are white on a lit or danger gel, the text colour on a
/// plain one.
pub fn gel_call_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    icon: crate::icons::Icon,
    glyph: u32,
    label: impl iced::widget::text::IntoFragment<'a>,
    tone: GelTone,
    size: (impl Into<Length>, f32),
    message: Option<Message>,
) -> Option<Element<'a, Message>> {
    if !has_gel(ctx) {
        return None;
    }
    let theme = ctx.look.theme;
    let ink = match tone {
        GelTone::Plain => theme.text,
        GelTone::Lit => theme.on_accent,
        GelTone::Danger => theme.on_danger,
    };
    // Not ready yet (joining), its glyph is faded.
    let ink = if message.is_some() {
        ink
    } else {
        Color { a: 0.5, ..ink }
    };
    // In the button's own ink even where the icon pack keeps colours: a
    // gel-blue glyph would sink into the gel.
    let face = ctx
        .look
        .tinted_glyph(icon, glyph, ink)
        .boxed(Length::Fill)
        .width(size.0.into())
        .height(size.1);
    let gel = gel_button(ctx, face, size.1 / 2.0, tone, 0.0, false)?;
    let face = iced::widget::button(gel)
        .padding(0)
        .on_press_maybe(message)
        .style(|_, _| iced::widget::button::Style::default());
    Some(
        iced::widget::tooltip(
            face,
            crate::controls::tip(theme, label),
            iced::widget::tooltip::Position::Top,
        )
        .gap(6)
        .into(),
    )
}

/// The tone of a gel button: the theme's other bubble (plain), its own
/// bubble (lit, as a selected or active button), or a red gel for a
/// button that ends something (leave the call).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GelTone {
    Plain,
    Lit,
    Danger,
}

/// Whether the theme draws gel (its bubbles have gloss, glow or rim), and
/// so its buttons may be gel too.
pub fn has_gel(ctx: crate::ctx::Ctx<'_>) -> bool {
    ctx.themes
        .package_layout(&ctx.look.theme.id)
        .is_some_and(|layout| {
            !layout.bubbles.own_gel.is_empty() || !layout.bubbles.other_gel.is_empty()
        })
}

/// `content` on a glossy gel button made from the theme's bubble gel, as
/// asked for a gel theme's server tiles and call buttons: the
/// bubble's ground and gloss, glow and rims, lit under the pointer
/// (`lift` 1) or darker pressed (-1). `gloss_over` lays the gloss over the
/// content, for a picture. `None` when the theme has no gel.
pub fn gel_button<'a>(
    ctx: crate::ctx::Ctx<'a>,
    content: impl Into<Element<'a, Message>>,
    radius: f32,
    tone: GelTone,
    lift: f32,
    gloss_over: bool,
) -> Option<Element<'a, Message>> {
    if !has_gel(ctx) {
        return None;
    }
    let theme = ctx.look.theme;
    let bubbles = &ctx.themes.package_layout(&theme.id)?.bubbles;
    let (paint, gel) = match tone {
        GelTone::Plain => (bubbles.other.as_ref(), &bubbles.other_gel),
        GelTone::Lit | GelTone::Danger => (bubbles.own.as_ref(), &bubbles.own_gel),
    };
    let mut gel = if gel.is_empty() {
        bubbles.own_gel.clone()
    } else {
        gel.clone()
    };
    let background = match tone {
        GelTone::Plain => paint
            .map(crate::themes::paint)
            .unwrap_or_else(|| theme.raised.into()),
        GelTone::Lit => paint
            .map(crate::themes::paint)
            .unwrap_or_else(|| theme.accent.into()),
        GelTone::Danger => {
            // The theme's danger red as a gel: lighter at the top, deeper
            // at the foot, as its own bubble shades its blue.
            let red = theme.danger;
            let mix = |by: f32, to: f32| Color {
                r: red.r + (to - red.r) * by,
                g: red.g + (to - red.g) * by,
                b: red.b + (to - red.b) * by,
                a: 1.0,
            };
            let linear = iced::gradient::Linear::new(iced::Degrees(180.0))
                .add_stop(0.0, mix(0.22, 1.0))
                .add_stop(0.45, red)
                .add_stop(0.8, mix(0.3, 0.0))
                .add_stop(1.0, mix(0.15, 0.0));
            if let Some(glow) = gel.glow.as_mut() {
                let light = mix(0.45, 1.0);
                glow.colour = (u32::from((0.9 * 255.0) as u8) << 24)
                    | (u32::from((light.r * 255.0) as u8) << 16)
                    | (u32::from((light.g * 255.0) as u8) << 8)
                    | u32::from((light.b * 255.0) as u8);
            }
            if gel.rim_dark.is_some() {
                let dark = mix(0.45, 0.0);
                gel.rim_dark = Some(
                    0xd900_0000
                        | (u32::from((dark.r * 255.0) as u8) << 16)
                        | (u32::from((dark.g * 255.0) as u8) << 8)
                        | u32::from((dark.b * 255.0) as u8),
                );
            }
            Background::Gradient(iced::Gradient::Linear(linear))
        }
    };
    Some(
        BubbleGel {
            content: content.into(),
            background,
            radius,
            gel,
            lift,
            gloss_over,
            interactive: true,
        }
        .into(),
    )
}

impl<M> Widget<M, Theme, iced::Renderer> for Procedural<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> widget::tree::Tag {
        clock_tag()
    }

    fn state(&self) -> widget::tree::State {
        clock_state()
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let moving = self.running
            && !self.reduced_motion
            && self.performance != Performance::Light
            && self.overlays.iter().any(|overlay| {
                overlay_moves(overlay) && (self.performance == Performance::Full || !overlay.heavy)
            });
        tick_clock(tree, event, moving);
        // Grain rides on frames drawn for other reasons: each of its steps
        // would draw the whole window again (iced has no partial redraw),
        // a sixth of a core for a texture that only flickers. It still
        // changes with every frame the window draws, and rests when idle.
        let drives = |overlay: &&Overlay| {
            overlay_moves(overlay)
                && overlay.kind != OverlayKind::Grain
                && (self.performance == Performance::Full || !overlay.heavy)
        };
        if moving
            && self.overlays.iter().any(|overlay| drives(&overlay))
            && let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event
        {
            let fps = frame_fps(self.performance, self.pace);
            let wait = self
                .overlays
                .iter()
                .filter(drives)
                .map(|overlay| overlay_wait(overlay, fps))
                .min()
                .unwrap_or(1000 / fps);
            shell.request_redraw_at(wake_at(*now, Duration::from_millis(wait)));
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let elapsed = clock_seconds(tree);
        renderer.with_layer(bounds, |renderer| {
            for overlay in self.overlays {
                if overlay.heavy && self.performance == Performance::Light {
                    continue;
                }
                let runs = self.running
                    && !self.reduced_motion
                    && self.performance != Performance::Light
                    && (self.performance == Performance::Full || !overlay.heavy);
                draw_overlay(renderer, overlay, bounds, elapsed, runs);
            }
        });
    }
}

impl<'a, M: 'a> From<Procedural<'a>> for Element<'a, M> {
    fn from(overlays: Procedural<'a>) -> Self {
        Element::new(overlays)
    }
}

fn overlay_colour(overlay: &Overlay) -> Color {
    let mut colour = encoded_colour(overlay.colour);
    colour.a *= overlay.opacity;
    colour
}

fn quad(renderer: &mut iced::Renderer, bounds: Rectangle, colour: Color, radius: f32) {
    use iced::advanced::Renderer as _;
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: iced::Border {
                radius: radius.into(),
                ..Default::default()
            },
            ..Default::default()
        },
        colour,
    );
}

fn draw_overlay(
    renderer: &mut iced::Renderer,
    overlay: &Overlay,
    bounds: Rectangle,
    elapsed: f32,
    runs: bool,
) {
    let colour = overlay_colour(overlay);
    match overlay.kind {
        OverlayKind::Image => {}
        OverlayKind::Scanlines => {
            let spacing = overlay.size.max(2.0);
            let mut y = bounds.y + spacing - 1.0;
            while y < bounds.y + bounds.height {
                quad(
                    renderer,
                    Rectangle::new(Point::new(bounds.x, y), Size::new(bounds.width, 1.0)),
                    colour,
                    0.0,
                );
                y += spacing;
            }
        }
        OverlayKind::Lcd => {
            let size = overlay.size.max(2.0);
            let stripe = size * 3.0;
            let tints = [
                Color::from_rgb(1.0, 0.16, 0.16),
                Color::from_rgb(0.16, 1.0, 0.16),
                Color::from_rgb(0.16, 0.16, 1.0),
            ];
            let mut x = bounds.x;
            let mut index = 0;
            while x < bounds.x + bounds.width {
                let mut tint = tints[index % 3];
                tint.a = colour.a / 5.0;
                quad(
                    renderer,
                    Rectangle::new(
                        Point::new(x, bounds.y),
                        Size::new((size - 1.0).max(1.0), bounds.height),
                    ),
                    tint,
                    0.0,
                );
                x += size;
                index += 1;
            }
            let mut y = bounds.y + size - 1.0;
            while y < bounds.y + bounds.height {
                quad(
                    renderer,
                    Rectangle::new(Point::new(bounds.x, y), Size::new(bounds.width, 1.0)),
                    colour,
                    0.0,
                );
                y += size;
            }
            let _ = stripe;
        }
        OverlayKind::Grain => {
            let phase = if runs && overlay_moves(overlay) && overlay.period > 0.0 {
                (elapsed * 1000.0 / overlay.period).floor() as u32
            } else {
                0
            };
            // One noise picture a phase, tiled: a frame of grain is a few
            // pictures, not 1,600 scattered specks (each its own damaged
            // region, the whole window drawn again for each).
            let tile = grain_tile(phase % GRAIN_PHASES, colour);
            let side = GRAIN_SIDE as f32;
            let across = (bounds.width / side).ceil() as usize;
            let down = (bounds.height / side).ceil() as usize;
            iced::advanced::Renderer::with_layer(renderer, bounds, |renderer| {
                use iced::advanced::image::Renderer as _;
                for y in 0..down {
                    for x in 0..across {
                        let at = Rectangle::new(
                            Point::new(bounds.x + x as f32 * side, bounds.y + y as f32 * side),
                            Size::new(side, side),
                        );
                        renderer.draw_image(
                            iced::advanced::image::Image {
                                handle: tile.clone(),
                                filter_method: iced::advanced::image::FilterMethod::Nearest,
                                rotation: iced::Radians(0.0),
                                border_radius: Default::default(),
                                opacity: 1.0,
                                snap: true,
                            },
                            at,
                            at,
                        );
                    }
                }
            });
        }
        OverlayKind::Vignette => {
            let reach = (bounds.width.min(bounds.height) * 0.22).max(1.0);
            for step in 0..16 {
                let at = step as f32 / 16.0;
                let width = reach / 16.0 + 0.5;
                let alpha = (1.0 - at) * 0.11;
                let inset = at * reach;
                for edge in [
                    Rectangle::new(
                        Point::new(bounds.x + inset, bounds.y + inset),
                        Size::new((bounds.width - inset * 2.0).max(0.0), width),
                    ),
                    Rectangle::new(
                        Point::new(bounds.x + inset, bounds.y + bounds.height - inset - width),
                        Size::new((bounds.width - inset * 2.0).max(0.0), width),
                    ),
                    Rectangle::new(
                        Point::new(bounds.x + inset, bounds.y + inset),
                        Size::new(width, (bounds.height - inset * 2.0).max(0.0)),
                    ),
                    Rectangle::new(
                        Point::new(bounds.x + bounds.width - inset - width, bounds.y + inset),
                        Size::new(width, (bounds.height - inset * 2.0).max(0.0)),
                    ),
                ] {
                    quad(renderer, edge, colour.scale_alpha(alpha), 0.0);
                }
            }
        }
        OverlayKind::Band => {
            let height = overlay.size.max(1.0);
            let progress = if runs && overlay.period > 0.0 {
                (elapsed * 1000.0 / overlay.period).fract()
            } else {
                0.5
            };
            let top = bounds.y - height + progress * (bounds.height + height);
            for step in 0..12 {
                let band = height / 12.0 + 0.5;
                let distance = ((step as f32 + 0.5) / 12.0 - 0.5).abs() * 2.0;
                quad(
                    renderer,
                    Rectangle::new(
                        Point::new(bounds.x, top + step as f32 * height / 12.0),
                        Size::new(bounds.width, band),
                    ),
                    colour.scale_alpha(1.0 - distance),
                    0.0,
                );
            }
        }
        OverlayKind::Flare => {
            let pulse = if runs && overlay.period > 0.0 {
                0.8 + 0.2 * (elapsed * std::f32::consts::TAU * 1000.0 / overlay.period).sin()
            } else {
                1.0
            };
            let diameter = overlay.size.max(1.0) * pulse;
            let center = Point::new(
                bounds.x + bounds.width * overlay.at.0,
                bounds.y + bounds.height * overlay.at.1,
            );
            for step in (1..=16).rev() {
                let scale = step as f32 / 16.0;
                let side = diameter * scale;
                quad(
                    renderer,
                    Rectangle::new(
                        Point::new(center.x - side / 2.0, center.y - side / 2.0),
                        Size::new(side, side),
                    ),
                    colour.scale_alpha((1.0 - scale) * 0.12 + 0.01),
                    side / 2.0,
                );
            }
        }
    }
}

impl<M> Widget<M, Theme, iced::Renderer> for Art<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> widget::tree::Tag {
        clock_tag()
    }

    fn state(&self) -> widget::tree::State {
        clock_state()
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let moving = self.running
            && !self.reduced_motion
            && self.performance != Performance::Light
            && self
                .layers
                .iter()
                .any(|art| art.layer.moves() && heavy_plays(self.performance, art.layer.heavy));
        tick_clock(tree, event, moving);
        if moving && let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let fps = frame_fps(self.performance, self.pace);
            let wait = self
                .layers
                .iter()
                .filter(|art| art.layer.moves() && heavy_plays(self.performance, art.layer.heavy))
                .map(|art| layer_wait(&art.layer, fps))
                .min()
                .unwrap_or(1000 / fps);
            shell.request_redraw_at(wake_at(*now, Duration::from_millis(wait)));
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let elapsed = clock_seconds(tree);
        let pins = self.pins;
        // Under a rim, every other layer is cut to the rim's round corner,
        // so nothing (an LCD grid) shows past its arc.
        let rim = rim_round(self.layers, bounds);
        renderer.with_layer(bounds, |renderer| {
            for art in self.layers {
                if art.pin.is_some() || pins == Pins::Only {
                    continue;
                }
                if (self.rims == Rims::Without && art.rim) || (self.rims == Rims::Only && !art.rim)
                {
                    continue;
                }
                let runs = self.running
                    && !self.reduced_motion
                    && self.performance != Performance::Light
                    && heavy_plays(self.performance, art.layer.heavy);
                let drift = if runs {
                    Vector::new(art.layer.drift.0 * elapsed, art.layer.drift.1 * elapsed)
                } else {
                    Vector::new(0.0, 0.0)
                };
                let source = Size::new(art.width.max(1) as f32, art.height.max(1) as f32);
                if art.layer.fit == Fit::Tile {
                    // On a surface with round corners a still tiling is
                    // laid out once at the surface's size and drawn as one
                    // picture cut to its corners, as Slint clips it: tiles
                    // drawn one by one are cut to a square box, and showed
                    // past a bubble's or a card's round corners.
                    let radii = {
                        let most = bounds.width.min(bounds.height) / 2.0;
                        // Inside a rounded pane a surface without corners of
                        // its own still takes the corners it shares with it.
                        let declared = if self.round.1 {
                            own_in_pane(bounds, self.round.0.max(0.0))
                        } else {
                            crate::widgets::pane_radii(bounds, self.round.0.max(0.0))
                        };
                        with_rim(declared, rim)
                            .map(|radius| radius.min(most))
                    };
                    if radii.iter().any(|radius| *radius > 0.0)
                        && drift == Vector::new(0.0, 0.0)
                        && (art.layer.scale - 1.0).abs() < 0.001
                        && art.frames.is_empty()
                        && art.sheet.is_none()
                        && let Some(whole) = tiled_whole(&art.handle, bounds)
                    {
                        crate::widgets::draw_picture(
                            renderer,
                            &whole,
                            bounds,
                            radii,
                            art.layer.opacity,
                        );
                        continue;
                    }
                    // The layer's `scale` sizes each tile, as in Slint.
                    let tile = source * art.layer.scale.clamp(0.25, 4.0);
                    let start = Point::new(
                        bounds.x - ((bounds.x - drift.x).rem_euclid(tile.width)),
                        bounds.y - ((bounds.y - drift.y).rem_euclid(tile.height)),
                    );
                    let across = (bounds.width / tile.width).ceil() as usize + 2;
                    let down = (bounds.height / tile.height).ceil() as usize + 2;
                    for y in 0..down.min(128) {
                        for x in 0..across.min(128) {
                            draw(
                                renderer,
                                art,
                                Rectangle::new(
                                    start
                                        + Vector::new(
                                            x as f32 * tile.width,
                                            y as f32 * tile.height,
                                        ),
                                    tile,
                                ),
                                elapsed,
                                runs,
                                [0.0; 4],
                            );
                        }
                    }
                    continue;
                }
                let size = if let Some((width, height)) = art.layer.size {
                    Size::new(width, height)
                } else {
                    match art.layer.fit {
                        Fit::Stretch => bounds.size(),
                        Fit::Contain => {
                            source
                                * (bounds.width / source.width).min(bounds.height / source.height)
                        }
                        Fit::Cover => {
                            source
                                * (bounds.width / source.width).max(bounds.height / source.height)
                        }
                        Fit::Tile => unreachable!(),
                    }
                };
                let position = Point::new(
                    bounds.x + (bounds.width - size.width) * art.layer.position.0,
                    bounds.y + (bounds.height - size.height) * art.layer.position.1,
                ) + drift;
                let at = Rectangle::new(position, size);
                // A picture that covers the surface exactly takes its
                // corners, as Slint's clip to the rounded surface does.
                let fills = (at.x - bounds.x).abs() < 0.5
                    && (at.y - bounds.y).abs() < 0.5
                    && (at.width - bounds.width).abs() < 0.5
                    && (at.height - bounds.height).abs() < 0.5;
                let most = bounds.width.min(bounds.height) / 2.0;
                let radii = if art.rim {
                    [0.0; 4]
                } else if self.round.1 {
                    with_rim(own_in_pane(bounds, self.round.0.max(0.0)), rim)
                } else {
                    with_rim(crate::widgets::pane_radii(bounds, self.round.0.max(0.0)), rim)
                }
                .map(|radius| radius.min(most));
                // A still picture that overhangs a round-cornered surface
                // is cut to the surface's box once and drawn with its
                // corners; a smaller one, inside the box, is drawn as it is.
                let overhangs = !fills
                    && (at.x < bounds.x - 0.5
                        || at.y < bounds.y - 0.5
                        || at.x + at.width > bounds.x + bounds.width + 0.5
                        || at.y + at.height > bounds.y + bounds.height + 0.5);
                if overhangs
                    && radii.iter().any(|radius| *radius > 0.0)
                    && drift == Vector::new(0.0, 0.0)
                    && art.frames.is_empty()
                    && art.sheet.is_none()
                    && let Some(whole) = fitted_whole(&art.handle, at, bounds)
                {
                    crate::widgets::draw_picture(renderer, &whole, bounds, radii, art.layer.opacity);
                    continue;
                }
                let radii = if fills { radii } else { [0.0; 4] };
                draw(renderer, art, at, elapsed, runs, radii);
            }
        });
        if pins == Pins::Without {
            return;
        }
        // Pinning tape: across the surface's top edge, its middle on it,
        // over whatever's behind; never cut to the surface.
        for art in self.layers {
            let Some(middle) = art.pin else {
                continue;
            };
            let source = Size::new(art.width.max(1) as f32, art.height.max(1) as f32);
            let size = art.layer.size.map_or_else(
                || source * (bounds.width / source.width).min(bounds.height / source.height),
                |(width, height)| Size::new(width, height),
            );
            use iced::advanced::image::Renderer as _;
            // Two strips: one across each top corner, at 40 degrees, as
            // corner tape holds a note down.
            if art.corners.len() == 2 {
                // At most the size they have on a dialog of Slint's
                // standard width: a large surface (the full profile's
                // spread) keeps its corner tape the size of the small
                // dialogs', not stretched to its width.
                let scale = (size.width / source.width).min(CORNER_TAPE_WIDTH / source.width);
                for (index, (handle, strip, angle)) in art.corners.iter().enumerate() {
                    let (corner, target) = if index == 0 {
                        (Point::new(bounds.x, bounds.y), -40.0f32)
                    } else {
                        (Point::new(bounds.x + bounds.width, bounds.y), 40.0f32)
                    };
                    let strip = *strip * scale;
                    renderer.draw_image(
                        iced::advanced::image::Image {
                            handle: handle.clone(),
                            filter_method: iced::advanced::image::FilterMethod::Linear,
                            rotation: iced::Radians((target - angle).to_radians()),
                            border_radius: Default::default(),
                            opacity: art.layer.opacity,
                            snap: false,
                        },
                        Rectangle::new(
                            Point::new(corner.x - strip.width / 2.0, corner.y - strip.height / 2.0),
                            strip,
                        ),
                        *_viewport,
                    );
                }
                continue;
            }
            let at = Rectangle::new(
                Point::new(
                    bounds.x + (bounds.width - size.width) * art.layer.position.0,
                    bounds.y - size.height * middle,
                ),
                size,
            );
            renderer.draw_image(
                iced::advanced::image::Image {
                    handle: art.handle.clone(),
                    filter_method: iced::advanced::image::FilterMethod::Linear,
                    rotation: iced::Radians(0.0),
                    border_radius: Default::default(),
                    opacity: art.layer.opacity,
                    snap: false,
                },
                at,
                *_viewport,
            );
        }
    }
}

/// Tiled pictures made, by picture, size and tile.
type TiledCache =
    std::collections::HashMap<(u64, u32, u32, u32, u32), iced::advanced::image::Handle>;

thread_local! {
    /// Tilings laid out at a surface's size, by the tile's picture, the
    /// size and where the window's tile grid falls on it: made once per
    /// bubble or card size, not each frame.
    static TILED: std::cell::RefCell<TiledCache> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The most tilings [`TILED`] keeps before it starts again.
const TILED_KEPT: usize = 96;

/// `tile` repeated over `bounds` on the window's grid (as the tiles drawn
/// one by one lie), as one picture of the bounds' whole pixels.
fn tiled_whole(
    tile: &iced::advanced::image::Handle,
    bounds: Rectangle,
) -> Option<iced::advanced::image::Handle> {
    let iced::advanced::image::Handle::Rgba {
        width: tile_width,
        height: tile_height,
        pixels,
        ..
    } = tile
    else {
        return None;
    };
    let (tw, th) = (*tile_width, *tile_height);
    let (w, h) = (bounds.width.round() as u32, bounds.height.round() as u32);
    if tw == 0 || th == 0 || w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let ox = (bounds.x.round() as i64).rem_euclid(i64::from(tw)) as u32;
    let oy = (bounds.y.round() as i64).rem_euclid(i64::from(th)) as u32;
    let id = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tile.id().hash(&mut hasher);
        hasher.finish()
    };
    let key = (id, w, h, ox, oy);
    TILED.with_borrow_mut(|kept| {
        if let Some(found) = kept.get(&key) {
            return Some(found.clone());
        }
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for y in 0..h {
            let ty = (y + oy) % th;
            for x in 0..w {
                let tx = (x + ox) % tw;
                let from = (ty as usize * tw as usize + tx as usize) * 4;
                let to = (y as usize * w as usize + x as usize) * 4;
                out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
            }
        }
        if kept.len() >= TILED_KEPT {
            kept.clear();
        }
        let handle = iced::advanced::image::Handle::from_rgba(w, h, out);
        kept.insert(key, handle.clone());
        Some(handle)
    })
}

thread_local! {
    /// Pictures laid larger than their surface (`cover`), cut to its box
    /// once per size so they can be drawn with its round corners.
    static FITTED: std::cell::RefCell<std::collections::HashMap<(u64, [i32; 4], [i32; 4]), iced::advanced::image::Handle>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// `picture` drawn at `at` and seen through `bounds`, as one picture of the
/// bounds' whole pixels: a layer that overhangs its surface, made into one
/// that fills it exactly, so it takes the surface's round corners rather
/// than showing past them.
fn fitted_whole(
    picture: &iced::advanced::image::Handle,
    at: Rectangle,
    bounds: Rectangle,
) -> Option<iced::advanced::image::Handle> {
    let iced::advanced::image::Handle::Rgba {
        width: pw,
        height: ph,
        pixels,
        ..
    } = picture
    else {
        return None;
    };
    let (w, h) = (bounds.width.round() as u32, bounds.height.round() as u32);
    let (aw, ah) = (at.width.round() as u32, at.height.round() as u32);
    if *pw == 0 || *ph == 0 || w == 0 || h == 0 || w > 4096 || h > 4096 || aw > 8192 || ah > 8192 {
        return None;
    }
    let id = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        picture.id().hash(&mut hasher);
        hasher.finish()
    };
    let round = |r: Rectangle| {
        [r.x.round() as i32, r.y.round() as i32, r.width.round() as i32, r.height.round() as i32]
    };
    let key = (id, round(at), round(bounds));
    FITTED.with_borrow_mut(|kept| {
        if let Some(found) = kept.get(&key) {
            return Some(found.clone());
        }
        let source = image::RgbaImage::from_raw(*pw, *ph, pixels.to_vec())?;
        let scaled = image::imageops::resize(&source, aw.max(1), ah.max(1), image::imageops::FilterType::Triangle);
        let mut out = image::RgbaImage::new(w, h);
        let dx = (at.x - bounds.x).round() as i64;
        let dy = (at.y - bounds.y).round() as i64;
        image::imageops::overlay(&mut out, &scaled, dx, dy);
        if kept.len() >= TILED_KEPT {
            kept.clear();
        }
        let handle = iced::advanced::image::Handle::from_rgba(w, h, out.into_raw());
        kept.insert(key, handle.clone());
        Some(handle)
    })
}

fn draw(
    renderer: &mut iced::Renderer,
    art: &ArtLayer,
    bounds: Rectangle,
    elapsed: f32,
    runs: bool,
    radii: [f32; 4],
) {
    let period = art
        .layer
        .frames
        .map_or(1000.0, |frames| frames.period)
        .max(1.0);
    // A sprite sheet shows its frame by drawing the whole sheet, placed so
    // that frame lies on `bounds`, clipped to them: one picture throughout.
    if runs && let Some((sheet, columns, rows, count)) = &art.sheet {
        use iced::advanced::image::Renderer as _;
        let index = (elapsed * 1000.0 / period * *count as f32).floor() as u32 % (*count).max(1);
        let (column, row) = (index % columns, index / columns);
        let whole = Rectangle::new(
            Point::new(
                bounds.x - column as f32 * bounds.width,
                bounds.y - row as f32 * bounds.height,
            ),
            Size::new(bounds.width * *columns as f32, bounds.height * *rows as f32),
        );
        renderer.draw_image(
            iced::advanced::image::Image {
                handle: sheet.clone(),
                filter_method: iced::advanced::image::FilterMethod::Linear,
                rotation: iced::Radians(0.0),
                border_radius: Default::default(),
                opacity: art.layer.opacity,
                snap: true,
            },
            whole,
            bounds,
        );
        return;
    }
    let handle = if runs && !art.frames.is_empty() {
        let index = ((elapsed * 1000.0 / period * art.frames.len() as f32).floor() as usize)
            % art.frames.len();
        &art.frames[index]
    } else {
        &art.handle
    };
    crate::widgets::draw_picture(renderer, handle, bounds, radii, art.layer.opacity);
}

struct Glass<'a> {
    pane: &'a GlassPane,
    /// The surface's ground as the theme paints it (a colour or a
    /// gradient), which Slint lays over the blur at the glass's opacity.
    ground: Background,
    /// Its corners when it floats on its own; inside a pane it rounds only
    /// the pane's corners.
    radius: f32,
    /// Rounds its own corners wherever it is, as the composer's field does.
    own: bool,
}

impl<M> Widget<M, Theme, iced::Renderer> for Glass<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let style = &self.pane.style;
        let bounds = layout.bounds();
        crate::widgets::draw_glass(
            renderer,
            bounds,
            &self.pane.blurred,
            &crate::widgets::GlassLook {
                ground: self.ground,
                opacity: style.opacity,
                radii: if self.own {
                    [self.radius; 4]
                } else {
                    crate::widgets::pane_radii(bounds, self.radius)
                },
                rim: style
                    .rim
                    .map(|(colour, width)| (encoded_colour(colour), width)),
                highlight: style.highlight.map(encoded_colour),
                inset: style
                    .inset
                    .map(|shadow| (encoded_colour(shadow.colour), shadow.blur)),
            },
        );
    }
}

/// A surface's solid ground, rounded where it meets the pane it's in.
/// A fill at `stock` corners that also rounds the corners it shares with
/// the rounded pane it's drawn in, as the pane's ground does: a shade over
/// a server's banner at the top of a theme's floating list.
pub fn pane_fill<'a, M: 'a>(background: Background, stock: [f32; 4]) -> Element<'a, M> {
    Element::new(PaneFill { background, stock })
}

struct PaneFill {
    background: Background,
    stock: [f32; 4],
}

impl<M> Widget<M, Theme, iced::Renderer> for PaneFill {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let most = bounds.width.min(bounds.height) / 2.0;
        let pane = crate::widgets::pane_radii(bounds, 0.0);
        let [top_left, top_right, bottom_right, bottom_left] =
            std::array::from_fn(|corner| self.stock[corner].max(pane[corner]).min(most));
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    radius: iced::border::Radius {
                        top_left,
                        top_right,
                        bottom_right,
                        bottom_left,
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            self.background,
        );
    }
}

/// The round corner a surface's rim draws at `bounds`: its picture's
/// outer corner stretched with it (the lesser stretch, as the arc shrinks
/// with the narrower side), 0 without a rim.
fn rim_round(layers: &[ArtLayer], bounds: Rectangle) -> f32 {
    layers
        .iter()
        .filter(|art| art.rim && art.rim_radius > 0.0)
        .map(|art| {
            let across = bounds.width / art.width.max(1) as f32;
            let down = bounds.height / art.height.max(1) as f32;
            art.rim_radius * across.min(down)
        })
        .fold(0.0, f32::max)
}

/// `radius` on every corner, rounder where the surface shares a corner
/// with the rounded pane it's drawn in.
fn own_in_pane(bounds: Rectangle, radius: f32) -> [f32; 4] {
    crate::widgets::pane_radii(bounds, 0.0).map(|pane| pane.max(radius))
}

/// `radii`, each corner at least `rim` (a rim rounds all four).
fn with_rim(radii: [f32; 4], rim: f32) -> [f32; 4] {
    radii.map(|radius| radius.max(rim))
}

struct PaneGround<'a> {
    /// The surface's art, whose rim (if any) its corners follow.
    layers: &'a [ArtLayer],
    background: Background,
    radius: f32,
    /// Rounds all its corners wherever it is: it floats over the pane.
    own: bool,
}

impl<M> Widget<M, Theme, iced::Renderer> for PaneGround<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        // A radius past half the side draws a bar in iced: hold it there.
        let most = bounds.width.min(bounds.height) / 2.0;
        // A surface with corners of its own (the account card) still
        // takes the rounder corners of a pane it fills, or the pane's
        // ground shows round its corner.
        let radii = if self.own {
            own_in_pane(bounds, self.radius)
        } else {
            crate::widgets::pane_radii(bounds, self.radius)
        };
        let radii = with_rim(radii, rim_round(self.layers, bounds));
        let [top_left, top_right, bottom_right, bottom_left] = radii.map(|radius| radius.min(most));
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    radius: iced::border::Radius {
                        top_left,
                        top_right,
                        bottom_right,
                        bottom_left,
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            self.background,
        );
    }
}

impl<'a, M: 'a> From<PaneGround<'a>> for Element<'a, M> {
    fn from(ground: PaneGround<'a>) -> Self {
        Element::new(ground)
    }
}

fn encoded_colour(encoded: u32) -> iced::Color {
    let [alpha, red, green, blue] = encoded.to_be_bytes();
    iced::Color::from_rgba8(red, green, blue, f32::from(alpha) / 255.0)
}

impl<'a, M: 'a> From<Glass<'a>> for Element<'a, M> {
    fn from(glass: Glass<'a>) -> Self {
        Element::new(glass)
    }
}

struct Particles<'a> {
    sets: &'a [ThemeParticles],
    events: &'a std::sync::Arc<std::sync::Mutex<Vec<ParticleEvent>>>,
    performance: Performance,
    reduced_motion: bool,
    running: bool,
    pace: Pace,
    depth: Depth,
}

impl<M> Widget<M, Theme, iced::Renderer> for Particles<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> widget::tree::Tag {
        clock_tag()
    }

    fn state(&self) -> widget::tree::State {
        clock_state()
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        if self.reduced_motion || self.performance == Performance::Light {
            tick_clock(tree, event, false);
            return;
        }
        let runs = |set: &ThemeParticles| {
            set.spec.z == self.depth && (self.performance == Performance::Full || !set.spec.heavy)
        };
        let ambient = self
            .sets
            .iter()
            .any(|set| runs(set) && set.spec.on == Trigger::Ambient && set.spec.rate > 0.0);
        let now = Instant::now();
        let events = self.events.lock().is_ok_and(|mut events| {
            let longest = self
                .sets
                .iter()
                .map(|set| set.spec.life)
                .fold(0.0, f32::max)
                .max(1.0);
            events
                .retain(|event| now.duration_since(event.started).as_secs_f32() * 1000.0 < longest);
            !events.is_empty()
        });
        let moving = self.running && (ambient || events);
        tick_clock(tree, event, moving);
        if moving && let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            // Ambient drifting specks move slowly (dust at most ~12 px a
            // second): 10 frames a second step them about a pixel, at a
            // sixth of the redraws (each one draws the whole window's
            // widgets); a burst plays at the full rate. The frames fall on
            // one shared beat, so every surface's specks ask for the same
            // redraw rather than each its own.
            let fps = frame_fps(self.performance, self.pace);
            if events {
                shell.request_redraw_at(on_beat(*now, Duration::from_millis(1000 / fps)));
            } else {
                shell.request_redraw_at(on_beat(*now, STEP_BEAT));
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        if self.reduced_motion || self.performance == Performance::Light {
            return;
        }
        let bounds = layout.bounds();
        let elapsed = clock_seconds(tree);
        let events = self
            .events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone());
        let mut shown = 0usize;
        for (set_index, set) in self.sets.iter().enumerate() {
            if set.spec.z != self.depth
                || (self.performance == Performance::Balanced && set.spec.heavy)
            {
                continue;
            }
            let life = (set.spec.life / 1000.0).max(0.01);
            if set.spec.on == Trigger::Ambient && set.spec.rate > 0.0 {
                let count = set.spec.count.min(160) as usize;
                let cycle = life.max(count as f32 / set.spec.rate.max(0.01));
                for index in 0..count {
                    let age = (elapsed - index as f32 / set.spec.rate).rem_euclid(cycle);
                    if age < life && shown < 160 {
                        draw_particle(
                            renderer,
                            set,
                            bounds,
                            age,
                            set_index as u64 * 997 + index as u64,
                            None,
                            Trigger::Ambient,
                        );
                        shown += 1;
                    }
                }
            }
            for (event_index, event) in events.iter().enumerate() {
                if event.trigger != set.spec.on {
                    continue;
                }
                let age = Instant::now().duration_since(event.started).as_secs_f32();
                if age >= life {
                    continue;
                }
                for index in 0..set.spec.count.min(160) as usize {
                    if shown >= 160 {
                        break;
                    }
                    draw_particle(
                        renderer,
                        set,
                        bounds,
                        age,
                        ((event_index as u64 + 1) << 32) ^ (set_index as u64 * 997 + index as u64),
                        event.origin,
                        event.trigger,
                    );
                    shown += 1;
                }
            }
        }
    }
}

fn draw_particle(
    renderer: &mut iced::Renderer,
    set: &ThemeParticles,
    bounds: Rectangle,
    age: f32,
    seed: u64,
    event_origin: Option<(f32, f32)>,
    trigger: Trigger,
) {
    use iced::advanced::Renderer as _;
    let life = (set.spec.life / 1000.0).max(0.01);
    let progress = (age / life).clamp(0.0, 1.0);
    let r = |salt| particle_random(seed ^ salt);
    let near = |x: f32, y: f32| (x + (r(7) - 0.5) * 16.0, y + (r(11) - 0.5) * 16.0);
    let (mut x, mut y) = match set.spec.from {
        Emitter::Composer => (
            bounds.x + bounds.width * (0.1 + 0.8 * r(1)),
            bounds.y + bounds.height - 45.0 + 20.0 * r(2),
        ),
        Emitter::SendButton => near(
            bounds.x + bounds.width - 32.0,
            bounds.y + bounds.height - 36.0,
        ),
        Emitter::ApplyButton => near(bounds.center_x(), bounds.center_y()),
        Emitter::Pointer => event_origin.unwrap_or((bounds.center_x(), bounds.center_y())),
        Emitter::Window => match set.spec.preset {
            Preset::Bubbles => (
                bounds.x + bounds.width * r(1),
                bounds.y + bounds.height + 10.0,
            ),
            Preset::Scraps => (bounds.x + bounds.width * r(1), bounds.y - 10.0),
            _ => (
                bounds.x + bounds.width * r(1),
                bounds.y + bounds.height * r(2),
            ),
        },
    };
    let burst = !matches!(trigger, Trigger::Ambient | Trigger::ThemeApplied);
    let angle = r(3) * std::f32::consts::TAU;
    let (mut vx, mut vy) = match set.spec.preset {
        Preset::Bubbles => ((r(4) - 0.5) * 30.0, -60.0 - 80.0 * r(5)),
        Preset::Sparkles if burst => (
            angle.cos() * (80.0 + 120.0 * r(4)),
            angle.sin() * (80.0 + 120.0 * r(5)),
        ),
        Preset::Sparkles => ((r(4) - 0.5) * 10.0, (r(5) - 0.5) * 10.0),
        Preset::Dust => ((r(4) - 0.5) * 16.0, (r(5) - 0.5) * 10.0 - 4.0),
        Preset::Scraps if burst => ((r(4) - 0.5) * 220.0, -160.0 - 120.0 * r(5)),
        Preset::Scraps => ((r(4) - 0.5) * 30.0, 50.0 + 60.0 * r(5)),
    };
    match set.spec.preset {
        Preset::Bubbles => x += (age * 3.0 + r(13) * 100.0).sin() * 20.0,
        Preset::Sparkles => {
            let damping = (-2.5 * age).exp();
            vx *= damping;
            vy *= damping;
        }
        Preset::Scraps => {
            vy = (vy + 130.0 * age).min(90.0);
            x += (age * 2.0 + r(13) * 100.0).sin() * 40.0;
        }
        Preset::Dust => {}
    }
    x += vx * age;
    y += vy * age;
    let size = set.spec.size * (0.6 + 0.8 * r(17));
    let mut opacity = (progress * 8.0).min(1.0) * ((1.0 - progress) / 0.4).min(1.0);
    if set.spec.preset == Preset::Sparkles {
        opacity *= 0.6 + 0.4 * (age * 18.0 + r(19) * 100.0).sin();
    }
    let encoded = if set.spec.colours.is_empty() {
        0xffff_ffff
    } else {
        set.spec.colours[(r(23) * set.spec.colours.len() as f32) as usize % set.spec.colours.len()]
    };
    let colour = encoded_colour(encoded).scale_alpha(opacity.clamp(0.0, 1.0));
    let at = Rectangle::new(
        Point::new(x - size / 2.0, y - size / 2.0),
        Size::new(size, size),
    );
    if let Some(sprite) = &set.sprite {
        crate::widgets::draw_picture(renderer, sprite, at, [0.0; 4], opacity);
        return;
    }
    match set.spec.preset {
        Preset::Bubbles => renderer.fill_quad(
            renderer::Quad {
                bounds: at,
                border: iced::Border {
                    color: colour,
                    width: (size / 8.0).clamp(1.0, 3.0),
                    radius: (size / 2.0).into(),
                },
                ..Default::default()
            },
            colour.scale_alpha(0.12),
        ),
        Preset::Sparkles => {
            let thick = (size / 5.0).max(1.0);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(x - thick / 2.0, y - size / 2.0),
                        Size::new(thick, size),
                    ),
                    ..Default::default()
                },
                colour,
            );
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new(x - size / 2.0, y - thick / 2.0),
                        Size::new(size, thick),
                    ),
                    ..Default::default()
                },
                colour,
            );
        }
        Preset::Dust => renderer.fill_quad(
            renderer::Quad {
                bounds: at,
                border: iced::Border {
                    radius: (size / 2.0).into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            colour,
        ),
        Preset::Scraps => renderer.fill_quad(
            renderer::Quad {
                bounds: at,
                ..Default::default()
            },
            colour,
        ),
    }
}

fn particle_random(mut value: u64) -> f32 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value >> 40) as f32 / (1u64 << 24) as f32
}

impl<'a, M: 'a> From<Particles<'a>> for Element<'a, M> {
    fn from(particles: Particles<'a>) -> Self {
        Element::new(particles)
    }
}

struct NineSlice<'a> {
    image: &'a TileArtImage,
    /// A turn of the picture's own: its length scaled, moved along, and
    /// turned a little more (each server's tape on its print).
    jitter: Jitter,
    /// Drawn past its slot's edges, as a card's tape straddles its top.
    unclipped: bool,
    /// Degrees the slot leans (a tilted card's): the picture turns with
    /// it, about the slot's centre.
    lean: f32,
    /// It turns about its slot's top centre (a card's), not its centre (a
    /// rail tile's).
    pivot_top: bool,
    width: f32,
    height: f32,
    opacity: f32,
    performance: Performance,
    reduced_motion: bool,
    running: bool,
    pace: Pace,
}

impl<M> Widget<M, Theme, iced::Renderer> for NineSlice<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(fixed_or_fill(self.width), fixed_or_fill(self.height))
    }

    fn tag(&self) -> widget::tree::Tag {
        clock_tag()
    }

    fn state(&self) -> widget::tree::State {
        clock_state()
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(
            limits,
            fixed_or_fill(self.width),
            fixed_or_fill(self.height),
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let moving = self.running
            && !self.reduced_motion
            && self.performance != Performance::Light
            && !self.image.frames.is_empty()
            && (self.performance == Performance::Full || !self.image.art.layer.heavy);
        tick_clock(tree, event, moving);
        if moving && let iced::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let fps = frame_fps(self.performance, self.pace);
            let wait = layer_wait(&self.image.art.layer, fps);
            shell.request_redraw_at(wake_at(*now, Duration::from_millis(wait)));
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::{
            Renderer as _,
            image::{self as raster, Renderer as _},
        };
        let slot = layout.bounds();
        let size = self.image.art.size.map_or(slot.size(), |(width, height)| {
            Size::new(
                if width.is_finite() {
                    width * self.jitter.length
                } else {
                    slot.width
                },
                if height.is_finite() {
                    height
                } else {
                    slot.height
                },
            )
        });
        let clip = if self.unclipped { *_viewport } else { slot };
        let target = Rectangle::new(
            Point::new(
                slot.x
                    + (slot.width - size.width) * self.image.art.layer.position.0
                    + self.image.art.offset.0
                    + self.jitter.along,
                slot.y
                    + (slot.height - size.height) * self.image.art.layer.position.1
                    + self.image.art.offset.1,
            ),
            size,
        );
        let filter_method = if self.image.art.pixelated {
            raster::FilterMethod::Nearest
        } else {
            raster::FilterMethod::Linear
        };
        let runs = self.running
            && !self.reduced_motion
            && self.performance != Performance::Light
            && (self.performance == Performance::Full || !self.image.art.layer.heavy);
        let (handle, pieces) = if runs && !self.image.frames.is_empty() {
            let period = self
                .image
                .art
                .layer
                .frames
                .map_or(1000.0, |frames| frames.period)
                .max(1.0);
            let index = ((clock_seconds(tree) * 1000.0 / period * self.image.frames.len() as f32)
                .floor() as usize)
                % self.image.frames.len();
            let (handle, pieces) = &self.image.frames[index];
            (handle, pieces)
        } else {
            (&self.image.handle, &self.image.pieces)
        };
        let draw = |renderer: &mut iced::Renderer,
                    handle: &iced::advanced::image::Handle,
                    bounds: Rectangle| {
            renderer.draw_image(
                raster::Image {
                    handle: handle.clone(),
                    filter_method,
                    rotation: iced::Radians(0.0),
                    border_radius: Default::default(),
                    opacity: self.image.art.layer.opacity * self.opacity,
                    snap: self.image.art.pixelated,
                },
                bounds,
                clip,
            );
        };
        let (Some(slice), Some(pieces)) = (self.image.art.slice, pieces) else {
            if self.lean != 0.0 {
                // The picture turns with its leaning card: its centre goes
                // round the card's, and it turns by the lean and its own.
                let turn = self.lean.to_radians();
                // About the card's top centre, as its ground turns.
                let pivot = if self.pivot_top {
                    Point::new(slot.center_x(), slot.y)
                } else {
                    slot.center()
                };
                let from = target.center() - pivot;
                let (sin, cos) = turn.sin_cos();
                let centre =
                    pivot + Vector::new(from.x * cos - from.y * sin, from.x * sin + from.y * cos);
                let bounds = Rectangle::new(
                    Point::new(
                        centre.x - target.width / 2.0,
                        centre.y - target.height / 2.0,
                    ),
                    target.size(),
                );
                renderer.draw_image(
                    raster::Image {
                        handle: handle.clone(),
                        filter_method,
                        rotation: iced::Radians(
                            turn + (self.image.art.rotation + self.jitter.turn).to_radians(),
                        ),
                        border_radius: Default::default(),
                        opacity: self.image.art.layer.opacity * self.opacity,
                        snap: false,
                    },
                    bounds,
                    clip,
                );
                return;
            }
            draw(renderer, handle, target);
            return;
        };
        let density = self.image.art.density.max(0.01);
        let mut left = slice[3] / density;
        let mut right = slice[1] / density;
        let mut top = slice[0] / density;
        let mut bottom = slice[2] / density;
        if left + right > target.width && left + right > 0.0 {
            let scale = target.width / (left + right);
            left *= scale;
            right *= scale;
        }
        if top + bottom > target.height && top + bottom > 0.0 {
            let scale = target.height / (top + bottom);
            top *= scale;
            bottom *= scale;
        }
        let xs = [
            target.x,
            target.x + left,
            target.x + target.width - right,
            target.x + target.width,
        ];
        let ys = [
            target.y,
            target.y + top,
            target.y + target.height - bottom,
            target.y + target.height,
        ];
        renderer.with_layer(clip, |renderer| {
            for row in 0..3 {
                for column in 0..3 {
                    let bounds = Rectangle::new(
                        Point::new(xs[column], ys[row]),
                        Size::new(
                            (xs[column + 1] - xs[column]).max(0.0),
                            (ys[row + 1] - ys[row]).max(0.0),
                        ),
                    );
                    if bounds.width > 0.0
                        && bounds.height > 0.0
                        && let Some(handle) = &pieces[row * 3 + column]
                    {
                        draw(renderer, handle, bounds);
                    }
                }
            }
        });
    }
}

/// A message card's border on a row of its group: the sides always, its
/// top only on the group's first row and its bottom only on its last, so
/// the rows of one card show no seam.
struct CardEdges {
    colour: Color,
    width: f32,
    radius: f32,
    top: bool,
    bottom: bool,
}

impl<M> Widget<M, Theme, iced::Renderer> for CardEdges {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        // The edges the row doesn't own reach past it and are clipped off.
        let reach = self.width + self.radius + 1.0;
        let up = if self.top { 0.0 } else { reach };
        let down = if self.bottom { 0.0 } else { reach };
        let ring = Rectangle::new(
            Point::new(bounds.x, bounds.y - up),
            Size::new(bounds.width, bounds.height + up + down),
        );
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: ring,
                    border: iced::Border {
                        color: self.colour,
                        width: self.width,
                        radius: self.radius.into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        });
    }
}

/// Discord's outline on the channel sidebar, as Slint draws it under a
/// theme: its top and left edges in the outline colour, meeting in a round
/// corner; no line toward the conversation or the dock.
struct TopLeftEdges {
    colour: Color,
    width: f32,
    radius: f32,
}

impl<M> Widget<M, Theme, iced::Renderer> for TopLeftEdges {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        // The right and bottom edges reach past the pane and are cut off,
        // except where the sidebar is a rounded pane of its own (a glass
        // gap): there the edges follow the pane's corners, so the top line
        // bends down at its round corner instead of running past it.
        let pane = crate::widgets::pane_radii(bounds, 0.0);
        let reach = self.width + self.radius.max(pane[0]) + 1.0;
        let right = pane[1] > 0.0 || pane[2] > 0.0;
        let bottom = pane[2] > 0.0 || pane[3] > 0.0;
        let ring = Rectangle::new(
            bounds.position(),
            Size::new(
                bounds.width + if right { 0.0 } else { reach },
                bounds.height + if bottom { 0.0 } else { reach },
            ),
        );
        let radius = iced::border::Radius {
            top_left: self.radius.max(pane[0]),
            top_right: if right { pane[1] } else { self.radius },
            bottom_right: if right && bottom { pane[2] } else { self.radius },
            bottom_left: if bottom { pane[3] } else { self.radius },
        };
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: ring,
                    border: iced::Border {
                        color: self.colour,
                        width: self.width,
                        radius,
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        });
    }
}

/// [`TopLeftEdges`] over the sidebar.
pub fn top_left_edges<'a>(
    content: Element<'a, Message>,
    colour: Color,
    width: f32,
    radius: f32,
) -> Element<'a, Message> {
    iced::widget::stack![
        content,
        Element::new(TopLeftEdges {
            colour,
            width,
            radius,
        })
    ]
    .into()
}

/// [`CardEdges`] over a card's row.
pub fn card_edges<'a>(
    card: Element<'a, Message>,
    border: iced::Border,
    radius: f32,
    top: bool,
    bottom: bool,
) -> Element<'a, Message> {
    if border.width <= 0.0 || border.color.a <= 0.0 {
        return card;
    }
    iced::widget::stack![
        card,
        Element::new(CardEdges {
            colour: border.color,
            width: border.width,
            radius,
            top,
            bottom,
        })
    ]
    .into()
}

/// What a card's row lays on its card, under its words: the hover or
/// mention tint, a theme's highlighter, a mention's edge and the keyboard's.
#[derive(Clone, Copy, Debug, Default)]
pub struct CardMarks {
    pub tint: Option<Color>,
    /// A theme's row highlighter: a band across the middle 64 % of the
    /// row, 55 % strong, turned a little against the card, as Slint's
    /// `ArtHighlighter`.
    pub highlighter: Option<Color>,
    /// A bar on the card's left edge: its colour and width.
    pub edge: Option<(Color, f32)>,
}

/// A card's row's ground, shadow and marks. Slint turns the whole card, its
/// words aside, by its group's lean, and each row shows its slice of it.
/// Iced turns pictures, so the ground is a picture of its colour turned
/// about the card's top centre (`pivot` above this row's top), so the rows
/// of one card line up as one scrap; a row after the first reaches `bleed`
/// up under the one before so no seam shows. The words stay straight, as
/// Slint's ("text never rotates"), and the row's hit box is the row's.
/// With no lean the card's own container draws the ground and this draws
/// only the marks.
struct CardGround {
    colour: Option<Color>,
    shadow: iced::Shadow,
    lean: f32,
    pivot: f32,
    bleed: f32,
    marks: CardMarks,
}

impl<M> Widget<M, Theme, iced::Renderer> for CardGround {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, image::Renderer as _};
        let bounds = layout.bounds();
        let marks = self.marks;
        let band = |bounds: Rectangle| {
            Rectangle::new(
                Point::new(bounds.x + 2.0, bounds.y + bounds.height * 0.18),
                Size::new((bounds.width - 4.0).max(0.0), bounds.height * 0.64),
            )
        };
        let edge = |bounds: Rectangle, width: f32| {
            Rectangle::new(bounds.position(), Size::new(width, bounds.height))
        };
        let Some(colour) = self.colour else {
            let mut quad = |colour: Color, at: Rectangle| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: at,
                        ..Default::default()
                    },
                    colour,
                );
            };
            if let Some(tint) = marks.tint {
                quad(tint, bounds);
            }
            if let Some(mark) = marks.highlighter {
                quad(mark.scale_alpha(0.55), band(bounds));
            }
            if let Some((colour, width)) = marks.edge {
                quad(colour, edge(bounds, width));
            }
            return;
        };
        let turn = self.lean.to_radians();
        let (sin, cos) = turn.sin_cos();
        // NaN: about its own centre (a rail print's window).
        let pivot = if self.pivot.is_nan() {
            bounds.center()
        } else {
            Point::new(bounds.center_x(), bounds.y - self.pivot)
        };
        // Where a rectangle of the straight card goes when the card turns
        // about its pivot: its centre goes round, and it turns about that.
        let turned = |at: Rectangle| {
            let from = at.center() - pivot;
            let centre =
                pivot + Vector::new(from.x * cos - from.y * sin, from.x * sin + from.y * cos);
            Rectangle::new(
                Point::new(centre.x - at.width / 2.0, centre.y - at.height / 2.0),
                at.size(),
            )
        };
        let mut paint = |colour: Color, at: Rectangle, extra: f32| {
            renderer.draw_image(
                iced::advanced::image::Image {
                    handle: solid(colour),
                    filter_method: iced::advanced::image::FilterMethod::Linear,
                    rotation: iced::Radians(turn + extra.to_radians()),
                    border_radius: Default::default(),
                    opacity: 1.0,
                    snap: false,
                },
                turned(at),
                *viewport,
            );
        };
        let ground = Rectangle::new(
            Point::new(bounds.x, bounds.y - self.bleed),
            Size::new(bounds.width, bounds.height + self.bleed),
        );
        if self.shadow.color.a > 0.0 {
            paint(self.shadow.color, ground + self.shadow.offset, 0.0);
        }
        paint(colour, ground, 0.0);
        if let Some(tint) = marks.tint {
            paint(tint, bounds, 0.0);
        }
        if let Some(mark) = marks.highlighter {
            paint(mark.scale_alpha(0.55), band(bounds), -0.6);
        }
        if let Some((colour, width)) = marks.edge {
            paint(colour, edge(bounds, width), 0.0);
        }
    }
}

/// A small picture of one colour, kept per colour: a leaning card's ground.
fn solid(colour: Color) -> iced::advanced::image::Handle {
    use std::cell::RefCell;
    thread_local! {
        static SOLIDS: RefCell<Kept<[u8; 4]>> = const { RefCell::new(Vec::new()) };
    }
    let key = colour.into_rgba8();
    SOLIDS.with_borrow_mut(|solids| {
        if let Some((_, handle)) = solids.iter().find(|(at, _)| *at == key) {
            return handle.clone();
        }
        let mut pixels = key.repeat(16);
        crate::paint::to_native(&mut pixels);
        let handle = iced::advanced::image::Handle::from_rgba(4, 4, pixels);
        if solids.len() >= 16 {
            solids.remove(0);
        }
        solids.push((key, handle.clone()));
        handle
    })
}

/// A card's row's ground (turned by `lean` degrees about the card's top
/// centre, `pivot` above the row) and marks, under its content.
pub fn card_ground<'a>(
    card: Element<'a, Message>,
    colour: Option<Color>,
    shadow: iced::Shadow,
    lean: f32,
    pivot: f32,
    bleed: f32,
    marks: CardMarks,
) -> Element<'a, Message> {
    iced::widget::Stack::with_children([card])
        .push_under(Element::new(CardGround {
            colour,
            shadow,
            lean,
            pivot,
            bleed,
            marks,
        }))
        .into()
}

/// A size given as infinite fills what there is, as a frame behind
/// content whose width is the content's own.
fn fixed_or_fill(size: f32) -> Length {
    if size.is_finite() {
        Length::Fixed(size)
    } else {
        Length::Fill
    }
}

/// The Slint build's framed list heading: the words in a frame that hugs them
/// (or spans the list, `fill`), `height` tall, centred in the heading's
/// own height, as Slint's `ListedMemberHeading` and channel heading do.
#[allow(
    clippy::too_many_arguments,
    reason = "each part it draws is its own input"
)]
pub fn framed_heading<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    tile: &yapfast_core::themes::lists::TileArt,
    words: Element<'a, Message>,
    row_height: f32,
    frame_height: f32,
    fill: bool,
    padding: iced::Padding,
) -> Element<'a, Message> {
    let frame_height = frame_height.min(row_height);
    let words = iced::widget::container(words)
        .padding(padding)
        .height(frame_height)
        .align_y(iced::Alignment::Center);
    let words: Element<'a, Message> = if fill {
        words.width(Length::Fill).into()
    } else {
        words.into()
    };
    let framed: Element<'a, Message> =
        match tile_art(ctx, surface, tile, f32::INFINITY, f32::INFINITY) {
            Some(art) => iced::widget::Stack::with_children([words])
                .push_under(art)
                .into(),
            None => words,
        };
    iced::widget::container(framed)
        .width(Length::Fill)
        .height(row_height)
        .align_y(iced::Alignment::Center)
        .into()
}

impl<'a, M: 'a> From<NineSlice<'a>> for Element<'a, M> {
    fn from(slice: NineSlice<'a>) -> Self {
        Element::new(slice)
    }
}

pub fn picked_tile<'a>(
    choices: &'a [yapfast_core::themes::lists::TileArt],
    id: &str,
) -> Option<&'a yapfast_core::themes::lists::TileArt> {
    (!choices.is_empty()).then(|| {
        let seed = crate::themes::slint_seed(id) as usize;
        &choices[seed % choices.len()]
    })
}

/// A message card's picture on its top or bottom edge (see
/// [`crate::themes::card_tiles`]), over the card and free to reach past it.
pub fn card_art<'a>(
    ctx: crate::ctx::Ctx<'a>,
    tile: &yapfast_core::themes::lists::TileArt,
    lean: f32,
) -> Option<Element<'a, Message>> {
    let image = ctx
        .themes
        .list_art(&ctx.look.theme.id, tile, ArtSurface::Chat)?;
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    Some(
        NineSlice {
            image,
            jitter: Jitter::NONE,
            unclipped: true,
            lean,
            pivot_top: true,
            width: f32::INFINITY,
            height: f32::INFINITY,
            opacity: 1.0,
            performance,
            reduced_motion,
            running: animation_running(performance, ctx.plays) && ctx.pace.runs(),
            pace: ctx.pace,
        }
        .into(),
    )
}

pub fn tile_art<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    tile: &yapfast_core::themes::lists::TileArt,
    width: f32,
    height: f32,
) -> Option<Element<'a, Message>> {
    tile_art_opacity(ctx, surface, tile, width, height, 1.0, 0.0)
}

/// [`tile_art`] turned `lean` degrees about its slot's centre, as a
/// leaning rail tile's frame, tape and stamp; drawn past its slot.
/// How a picture placed on a slot varies from its art's own place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jitter {
    /// Its length times this.
    pub length: f32,
    /// Pixels moved along its slot's top.
    pub along: f32,
    /// Degrees turned on top of its own.
    pub turn: f32,
}

impl Jitter {
    pub const NONE: Self = Self {
        length: 1.0,
        along: 0.0,
        turn: 0.0,
    };

    /// A piece of tape as a hand lays it, the same for a `seed` every time:
    /// left of centre, centred or right of it, a little longer or shorter,
    /// a few degrees either way.
    pub fn tape(seed: u32) -> Self {
        let pick = |shift: u32, steps: u32| (seed.rotate_right(shift) % steps) as f32;
        Self {
            length: 0.85 + pick(3, 7) * 0.05,
            along: (pick(7, 3) - 1.0) * 9.0 + (pick(11, 5) - 2.0),
            turn: pick(17, 9) - 4.0,
        }
    }
}

/// [`tile_art_turned`], the picture varied by `jitter`.
pub fn tile_art_jittered<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    tile: &yapfast_core::themes::lists::TileArt,
    width: f32,
    height: f32,
    lean: f32,
    jitter: Jitter,
) -> Option<Element<'a, Message>> {
    let image = ctx.themes.list_art(&ctx.look.theme.id, tile, surface)?;
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    let running = animation_running(performance, ctx.plays) && ctx.pace.runs();
    Some(
        NineSlice {
            image,
            jitter,
            unclipped: lean != 0.0 || jitter != Jitter::NONE,
            lean,
            pivot_top: false,
            width,
            height,
            opacity: 1.0,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
        }
        .into(),
    )
}

pub fn tile_art_turned<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    tile: &yapfast_core::themes::lists::TileArt,
    width: f32,
    height: f32,
    lean: f32,
) -> Option<Element<'a, Message>> {
    tile_art_opacity(ctx, surface, tile, width, height, 1.0, lean)
}

pub fn tile_art_opacity<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    tile: &yapfast_core::themes::lists::TileArt,
    width: f32,
    height: f32,
    opacity: f32,
    lean: f32,
) -> Option<Element<'a, Message>> {
    let image = ctx.themes.list_art(&ctx.look.theme.id, tile, surface)?;
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    let running = animation_running(performance, ctx.plays) && ctx.pace.runs();
    Some(
        NineSlice {
            image,
            jitter: Jitter::NONE,
            unclipped: lean != 0.0,
            lean,
            pivot_top: false,
            width,
            height,
            opacity,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
        }
        .into(),
    )
}

impl<'a, M: 'a> From<Art<'a>> for Element<'a, M> {
    fn from(art: Art<'a>) -> Self {
        Element::new(art)
    }
}

/// The corners a surface's art and glass are clipped to, as Slint's
/// `ArtLayer` takes its surface's shape (`Look.r(Look.<surface>, stock)`):
/// the theme's own for the surface, else the stock corner it rounds.
fn corner_radius(ctx: crate::ctx::Ctx<'_>, surface: ArtSurface) -> f32 {
    let stock = match surface {
        ArtSurface::Picker | ArtSurface::Composer => 10.0,
        // The account card's ground rounds its corners as the card does.
        ArtSurface::Menu
        | ArtSurface::Card
        | ArtSurface::Toast
        | ArtSurface::Popout
        | ArtSurface::Account => 8.0,
        ArtSurface::Actions => 8.0,
        ArtSurface::Dialog | ArtSurface::Settings => 12.0,
        ArtSurface::Embed | ArtSurface::Code | ArtSurface::Tooltip => 4.0,
        _ => return ctx.look.theme.surface_radius(surface.ground(), 0.0),
    };
    ctx.look.theme.surface_radius(surface.name(), stock)
}

pub fn surface<'a>(
    content: impl Into<Element<'a, Message>>,
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
) -> Element<'a, Message> {
    let layers = ctx.themes.art(&ctx.look.theme.id, surface);
    let glass = ctx.themes.glass(&ctx.look.theme.id, surface);
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    let running = animation_running(performance, ctx.plays) && ctx.pace.runs();
    let mut content = content.into();
    if let Some(reserve) = ctx.themes.art_reserve(&ctx.look.theme.id, surface) {
        content = iced::widget::container(content)
            .padding(iced::Padding {
                top: reserve.top,
                right: reserve.right,
                bottom: reserve.bottom,
                left: reserve.left,
            })
            .into();
    }
    // A package brush may be a gradient, and the theme's extra surface
    // colours do not fit in Wired's intentionally stable Theme struct.
    // Put the effective theme ground below art and content. When no package
    // paint exists, `surface_paint` is exactly the prior Wired solid.
    let background = ctx.look.theme.surface_paint(surface.ground());
    // Inside a floating pane the ground takes the corners it shares with
    // the pane, as Slint's pane clips it.
    // A message's hover bar floats over the rows inside the conversation's
    // pane: its ground and pictures round all its corners, as Slint's pill.
    // The account card is a card of its own at the list's foot: all its
    // corners round, whatever pane it sits in.
    let floats = matches!(surface, ArtSurface::Actions | ArtSurface::Account);
    let base = PaneGround {
        layers,
        own: floats,
        background,
        radius: if ctx.look.theme.packaged() {
            corner_radius(ctx, surface)
        } else {
            ctx.look.theme.surface_radius(surface.ground(), 0.0)
        },
    };
    // Keep `content` as Stack's intrinsic base and insert decorations under
    // it. Main panes still fill because their content fills; cards, embeds
    // and code blocks retain their natural size instead of expanding to the
    // whole conversation.
    let mut decorated = iced::widget::Stack::with_children([content]);
    // The composer lays its art over its own field (see `art_layers`).
    if !layers.is_empty() && surface != ArtSurface::Composer {
        let rimmed = layers.iter().any(|layer| layer.rim);
        decorated = decorated.push_under(Art {
            rims: if rimmed { Rims::Without } else { Rims::With },
            layers,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
            pins: Pins::With,
            round: (corner_radius(ctx, surface), floats),
        });
        // A rim round the surface lies over what's in it, as a border
        // does: scrolled messages, pictures and bars pass under it rather
        // than over it (Bubblegum's plum rim).
        if rimmed {
            decorated = decorated.push(Art {
                rims: Rims::Only,
                layers,
                performance,
                reduced_motion,
                running,
                pace: ctx.pace,
                pins: Pins::Without,
                round: (corner_radius(ctx, surface), floats),
            });
        }
    }
    // As in Slint, the bar over the conversation is the conversation's own
    // ground and art (drawn once under the whole conversation) unless the
    // theme gives the bar a ground, art or glass of its own.
    let bare_header = surface == ArtSurface::Header
        && layers.is_empty()
        && glass.is_none()
        && ctx.look.theme.package_paint("header").is_none();
    // Likewise a heading's label is bare unless the theme dresses it.
    // The composer's field draws its own ground; round it the
    // conversation shows, as in Slint.
    let bare_composer = surface == ArtSurface::Composer && (!layers.is_empty() || glass.is_some());
    let bare_label = surface == ArtSurface::Label
        && layers.is_empty()
        && glass.is_none()
        && ctx.look.theme.package_paint("label").is_none();
    // The room a layer reserves at a glass pane's foot (Zine's "ISSUE Nº
    // 07" stub) keeps the pane's ground under its art, as Slint's: the
    // pane's content, which paints the rest, stops above it.
    if glass.is_some()
        && let Some(reserve) = ctx
            .themes
            .art_reserve(&ctx.look.theme.id, surface)
            .filter(|reserve| reserve.bottom > 0.0)
    {
        let ground = ctx.look.theme.surface_paint(surface.ground());
        decorated = decorated.push_under(
            iced::widget::column![
                iced::widget::Space::new().height(Length::Fill),
                iced::widget::container(iced::widget::Space::new())
                    .width(Length::Fill)
                    .height(reserve.bottom)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(ground),
                        ..Default::default()
                    })
            ]
            .width(Length::Fill)
            .height(Length::Fill),
        );
    }
    // The composer's glass lies under its field alone (see `glass_under`).
    if let Some(pane) = glass.filter(|_| surface != ArtSurface::Composer) {
        decorated = decorated.push_under(Glass {
            pane,
            ground: ctx.look.theme.surface_paint(surface.ground()),
            own: false,
            radius: corner_radius(ctx, surface),
        });
    }
    // A glass pane's ground is the glass's own, laid over the art; a
    // solid ground under it would hide the art at the rounded corners.
    let decorated: Element<'a, Message> =
        if glass.is_some() || bare_header || bare_label || bare_composer {
            decorated.into()
        } else {
            decorated.push_under(base).into()
        };
    let decorated = under_particles(decorated, ctx, surface);
    if surface == ArtSurface::Chat {
        overlay_layers(decorated, ctx, true)
    } else {
        decorated
    }
}

/// One of the floating panes of a theme whose glass has a gap (the
/// the Slint build lays them out `gap` apart over the window's art and clips
/// each to its rounded rectangle). The surfaces inside round only the
/// pane's corners; with `surface`, that surface's glass lies under the
/// whole pane, as the conversation's does under its header and composer.
pub fn pane<'a>(
    content: impl Into<Element<'a, Message>>,
    ctx: crate::ctx::Ctx<'a>,
    surface: Option<ArtSurface>,
) -> Element<'a, Message> {
    let content = content.into();
    let id = &ctx.look.theme.id;
    let Some(style) = ctx.themes.glass_style(id).filter(|style| style.gap > 0.0) else {
        return content;
    };
    let radius = style.radius;
    let content = match surface.and_then(|surface| Some((surface, ctx.themes.glass(id, surface)?)))
    {
        Some((surface, glass)) => iced::widget::stack![
            Glass {
                pane: glass,
                ground: ctx.look.theme.surface_paint(surface.ground()),
                own: false,
                radius,
            },
            content
        ]
        .into(),
        None => content,
    };
    crate::widgets::Wrap::new(content, crate::widgets::Pane { radius }).into()
}

/// A bubble's own pictures (a theme's `bubbles.own.art` or
/// `other.art`), rounded to the bubble, to lie over its ground and under
/// its words, as Slint's bubble `ArtLayer`.
pub fn bubble_layers<'a>(
    ctx: crate::ctx::Ctx<'a>,
    own: bool,
    radius: f32,
) -> Option<Element<'a, Message>> {
    let layers = ctx.themes.bubble_art(&ctx.look.theme.id, own);
    if layers.is_empty() {
        return None;
    }
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    Some(
        Art {
            rims: Rims::With,
            layers,
            performance,
            reduced_motion,
            running: animation_running(performance, ctx.plays) && ctx.pace.runs(),
            pace: ctx.pace,
            pins: Pins::With,
            round: (radius, true),
        }
        .into(),
    )
}

/// A surface's glass alone, rounded `radius` on every corner, for a
/// surface that lays it under a ground of its own, as Slint's composer box
/// does under its field.
pub fn glass_under<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    radius: f32,
) -> Option<Element<'a, Message>> {
    let pane = ctx.themes.glass(&ctx.look.theme.id, surface)?;
    Some(
        Glass {
            pane,
            ground: ctx.look.theme.surface_paint(surface.ground()),
            radius,
            own: true,
        }
        .into(),
    )
}

/// A surface's art alone, for a surface that lays it over a ground of its
/// own, as the composer does over its field (Bubblegum's keys at its ends).
pub fn art_layers<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    radius: f32,
) -> Option<Element<'a, Message>> {
    art_layers_of(ctx, surface, Pins::With, (radius, true))
}

/// A surface's art under its content, without its pinning tape, for a
/// surface whose content scrolls; [`art_pins`] lays the tape over it.
pub fn art_under<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
) -> Option<Element<'a, Message>> {
    art_layers_of(
        ctx,
        surface,
        Pins::Without,
        (corner_radius(ctx, surface), false),
    )
}

/// The widest surface a theme's corner tape is drawn for at its full
/// scale: Slint's standard dialog.
const CORNER_TAPE_WIDTH: f32 = 440.0;

/// A surface's pinning tape alone, over its top edge.
pub fn art_pins<'a>(ctx: crate::ctx::Ctx<'a>, surface: ArtSurface) -> Option<Element<'a, Message>> {
    ctx.themes
        .art(&ctx.look.theme.id, surface)
        .iter()
        .any(|layer| layer.pin.is_some())
        .then(|| art_layers_of(ctx, surface, Pins::Only, (0.0, false)))
        .flatten()
}

fn art_layers_of<'a>(
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
    pins: Pins,
    round: (f32, bool),
) -> Option<Element<'a, Message>> {
    let layers = ctx.themes.art(&ctx.look.theme.id, surface);
    if layers.is_empty() {
        return None;
    }
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    Some(
        Art {
            rims: Rims::With,
            layers,
            performance,
            reduced_motion,
            running: animation_running(performance, ctx.plays) && ctx.pace.runs(),
            pace: ctx.pace,
            pins,
            round,
        }
        .into(),
    )
}

/// Adds a theme's deliberately foreground art over a surface. Unlike
/// [`surface`], this does not paint a ground or reserve content room: the
/// package checker only permits `over` art on surfaces whose pictures are
/// cosmetic and take no input, such as the gloss over a profile card.
pub fn surface_over<'a>(
    content: impl Into<Element<'a, Message>>,
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
) -> Element<'a, Message> {
    let content = content.into();
    let layers = ctx.themes.art(&ctx.look.theme.id, surface);
    if layers.is_empty() {
        return content;
    }
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    let running = animation_running(performance, ctx.plays) && ctx.pace.runs();
    iced::widget::stack![
        content,
        Art {
            rims: Rims::With,
            layers,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
            pins: Pins::With,
            round: (0.0, false),
        }
    ]
    .into()
}

/// A regular app dialog whose theme art sits inside the existing kit
/// frame. The kit still owns sizing, focus shielding and Wired fallbacks;
/// this app-side boundary supplies the resolved package pictures and glass.
pub fn framed_dialog<'a>(
    ctx: crate::ctx::Ctx<'a>,
    body: impl IntoIterator<Item = Element<'a, Message>>,
    width: f32,
    close: Message,
) -> Element<'a, Message> {
    let body = crate::widgets::scroll_view(iced::widget::column(body).spacing(12).padding(20))
        .width(Length::Fill)
        .style(crate::style::scrollbar(ctx.look.theme));
    let body = surface(body, ctx, ArtSurface::Dialog);
    crate::dialog::dialog(ctx.look.theme, body, width, close)
}

/// A dialog over server/channel settings, with the same package decoration
/// but the kit's existing dimensions and dimming behavior.
pub fn panel_dialog<'a>(
    ctx: crate::ctx::Ctx<'a>,
    body: impl Into<Element<'a, Message>>,
    width: f32,
    height: Option<f32>,
    close: Message,
) -> Element<'a, Message> {
    let body = surface(body, ctx, ArtSurface::Dialog);
    crate::dialog::panel(ctx.look.theme, body, width, height, close)
}

fn under_particles<'a>(
    content: Element<'a, Message>,
    ctx: crate::ctx::Ctx<'a>,
    surface: ArtSurface,
) -> Element<'a, Message> {
    if surface != ArtSurface::Chat {
        return content;
    }
    let Some((sets, events, performance, reduced_motion)) =
        ctx.themes.particles(&ctx.look.theme.id)
    else {
        return content;
    };
    iced::widget::stack![
        Particles {
            sets,
            events,
            performance,
            reduced_motion,
            running: animation_running(performance, ctx.plays) && ctx.pace.runs(),
            pace: ctx.pace,
            depth: Depth::Under,
        },
        content
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn overlays<'a>(
    content: Element<'a, Message>,
    ctx: crate::ctx::Ctx<'a>,
) -> Element<'a, Message> {
    overlay_layers(content, ctx, false)
}

fn overlay_layers<'a>(
    content: Element<'a, Message>,
    ctx: crate::ctx::Ctx<'a>,
    chat: bool,
) -> Element<'a, Message> {
    let layers = ctx.themes.overlays(&ctx.look.theme.id, chat);
    let procedural = ctx.themes.procedural_overlays(&ctx.look.theme.id, chat);
    let (performance, reduced_motion) = ctx.themes.art_motion(&ctx.look.theme.id);
    let running = animation_running(performance, ctx.plays) && ctx.pace.runs();
    let particles = (!chat)
        .then(|| ctx.themes.particles(&ctx.look.theme.id))
        .flatten();
    if layers.is_empty() && procedural.is_empty() && particles.is_none() {
        return content;
    }
    let mut decorated = iced::widget::stack![content];
    if !layers.is_empty() {
        decorated = decorated.push(Art {
            rims: Rims::With,
            layers,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
            pins: Pins::With,
            round: (0.0, false),
        });
    }
    if !procedural.is_empty() {
        decorated = decorated.push(Procedural {
            overlays: procedural,
            performance,
            reduced_motion,
            running,
            pace: ctx.pace,
        });
    }
    if let Some((sets, events, particle_performance, particle_reduced)) = particles {
        decorated = decorated.push(Particles {
            sets,
            events,
            performance: particle_performance,
            reduced_motion: particle_reduced,
            running: animation_running(particle_performance, ctx.plays) && ctx.pace.runs(),
            pace: ctx.pace,
            depth: Depth::Over,
        });
    }
    decorated.width(Length::Fill).height(Length::Fill).into()
}

#[cfg(test)]
mod motion_tests {
    use super::*;

    #[test]
    fn balanced_art_pauses_when_the_window_animation_gate_rests() {
        assert!(animation_running(Performance::Full, false));
        assert!(animation_running(Performance::Balanced, true));
        assert!(!animation_running(Performance::Balanced, false));
        assert!(!animation_running(Performance::Light, true));

        let start = Instant::now();
        let mut clock = MotionClock::default();
        clock.redraw(start, true);
        clock.redraw(start + Duration::from_millis(100), true);
        clock.redraw(start + Duration::from_millis(200), false);
        clock.redraw(start + Duration::from_millis(300), true);
        clock.redraw(start + Duration::from_millis(350), true);
        assert_eq!(clock.elapsed, Duration::from_millis(150));
    }
}
