#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
pub mod window;

mod blit;
mod engine;
mod layer;
mod primitive;
mod settings;
mod text;
// Yapfast port (patch 13): the app draws its video frames itself; see video.rs.
pub mod video;
// Yapfast port (patch 22): frame timings for the app's trace.
pub mod timing;
// Yapfast port (patch 26): whether this renderer may start.
mod enabled;
pub use enabled::set_enabled;

#[cfg(feature = "image")]
mod raster;

#[cfg(feature = "svg")]
mod vector;

#[cfg(feature = "geometry")]
pub mod geometry;

use iced_debug as debug;
pub use iced_graphics as graphics;
pub use iced_graphics::core;

pub use layer::Layer;
pub use primitive::Primitive;
pub use settings::Settings;

#[cfg(feature = "geometry")]
pub use geometry::Geometry;

use crate::core::renderer;
use crate::core::{
    Background, Color, Font, Pixels, Point, Rectangle, Size, Transformation,
};
use crate::engine::Engine;
use crate::graphics::Viewport;
use crate::graphics::compositor;
use crate::graphics::text::{Editor, Paragraph};

/// A [`tiny-skia`] graphics renderer for [`iced`].
///
/// [`tiny-skia`]: https://github.com/RazrFalcon/tiny-skia
/// [`iced`]: https://github.com/iced-rs/iced
#[derive(Debug)]
pub struct Renderer {
    default_font: Font,
    default_text_size: Pixels,
    layers: layer::Stack,
    engine: Engine, // TODO: Shared engine
    // Yapfast port (patch 32): what lies under the layers that change, kept.
    backdrop: Backdrop,
}

/// Yapfast port (patch 32): regions of the frame as drawn up to the first
/// layer that changed in them, kept from one frame to the next. A list
/// scrolled over a theme's art and glass changed only its own layer, yet
/// every frame drew the window's art, the panes' glass and grounds under it
/// again; from the second frame those come back as one copy.
#[derive(Debug, Default)]
struct Backdrop {
    /// The frame's size (physical pixels) the regions were kept at.
    width: u32,
    height: u32,
    /// The regions kept, none overlapping another, so together never more
    /// than the frame.
    kept: Vec<Kept>,
    /// What each of this frame's layers changed since the last frame, in
    /// logical pixels.
    changes: Vec<Vec<Rectangle>>,
    /// Frames drawn at its size.
    frame: u64,
    /// Whether `changes` are for the frame about to be drawn: set by
    /// `layer_changes`, used up by `draw`. A `screenshot` draws with none.
    fresh: bool,
}

/// One region of the frame kept, as drawn up to its first `layers`.
#[derive(Debug)]
struct Kept {
    /// Physical left, top, right, bottom.
    region: [i32; 4],
    /// How many of the frame's first layers it holds.
    layers: usize,
    /// Its pixels, row after row, in the frame's byte order.
    pixels: Vec<u8>,
    /// The frame it was last copied in or out at.
    used: u64,
}

impl Backdrop {
    /// The most regions it keeps: a list scrolling damages the same one.
    const REGIONS: usize = 8;
    /// Frames drawn without using a region before it goes.
    const IDLE: u64 = 120;

    /// The first layer that changed inside `region` (physical pixels).
    fn first_change(&self, region: Rectangle, scale_factor: f32) -> usize {
        self.changes
            .iter()
            .position(|changes| {
                changes
                    .iter()
                    .any(|change| (*change * scale_factor).intersects(&region))
            })
            .unwrap_or(self.changes.len())
    }

    /// Keeps `region` of the frame as drawn up to its first `layers`, in
    /// place of any kept region it overlaps, reusing that one's memory.
    fn keep(&mut self, pixels: &mut tiny_skia::PixmapMut<'_>, region: [i32; 4], layers: usize) {
        let [left, top, right, bottom] = region;
        let row_bytes = (right - left) as usize * 4;
        let needed = row_bytes * (bottom - top) as usize;
        let mut buffer = Vec::new();
        self.kept.retain_mut(|kept| {
            let overlapping = overlaps(kept.region, region);
            if overlapping && kept.pixels.capacity() > buffer.capacity() {
                buffer = std::mem::take(&mut kept.pixels);
            }
            !overlapping
        });
        if self.kept.len() >= Self::REGIONS {
            let oldest = self.kept.remove(0);
            if buffer.capacity() < needed {
                buffer = oldest.pixels;
            }
        }
        // A buffer far larger than the region (a region that shrank) isn't
        // held on to.
        if buffer.capacity() > needed * 2 {
            buffer = Vec::new();
        }
        buffer.clear();
        buffer.reserve_exact(needed);
        let stride = pixels.width() as usize * 4;
        let frame = pixels.data_mut();
        for row in top..bottom {
            let at = row as usize * stride + left as usize * 4;
            buffer.extend_from_slice(&frame[at..at + row_bytes]);
        }
        self.kept.push(Kept {
            region,
            layers,
            pixels: buffer,
            used: self.frame,
        });
    }

    /// Copies `region` out of the kept region `index`, which holds it.
    fn restore(&mut self, pixels: &mut tiny_skia::PixmapMut<'_>, region: [i32; 4], index: usize) {
        let kept = &mut self.kept[index];
        kept.used = self.frame;
        let [left, top, right, bottom] = region;
        let kept_row = (kept.region[2] - kept.region[0]) as usize * 4;
        let row_bytes = (right - left) as usize * 4;
        let stride = pixels.width() as usize * 4;
        let frame = pixels.data_mut();
        for row in top..bottom {
            let at = row as usize * stride + left as usize * 4;
            let from = (row - kept.region[1]) as usize * kept_row
                + (left - kept.region[0]) as usize * 4;
            frame[at..at + row_bytes].copy_from_slice(&kept.pixels[from..from + row_bytes]);
        }
    }

    /// After a frame drawn: a region unused for [`Self::IDLE`] frames goes.
    fn age(&mut self) {
        let frame = self.frame;
        self.kept.retain(|kept| frame - kept.used <= Self::IDLE);
        self.frame += 1;
    }
}

impl Backdrop {
    /// `damage` (physical), each region overlapping a kept one cut into the
    /// part inside it and the parts round it, on whole pixels.
    fn split(
        &self,
        damage: impl Iterator<Item = Rectangle>,
        width: i32,
        height: i32,
    ) -> Vec<Rectangle> {
        let mut out = Vec::new();
        for region in damage {
            let whole = [
                (region.x.round() as i32).clamp(0, width),
                (region.y.round() as i32).clamp(0, height),
                ((region.x + region.width).round() as i32).clamp(0, width),
                ((region.y + region.height).round() as i32).clamp(0, height),
            ];
            let Some(kept) = self
                .kept
                .iter()
                .map(|kept| kept.region)
                .find(|&kept| overlaps(kept, whole) && !within(whole, kept))
            else {
                out.push(region);
                continue;
            };
            let [l, t, r, b] = whole;
            let inside = [l.max(kept[0]), t.max(kept[1]), r.min(kept[2]), b.min(kept[3])];
            let pieces = [
                inside,
                [l, t, r, inside[1]],
                [l, inside[3], r, b],
                [l, inside[1], inside[0], inside[3]],
                [inside[2], inside[1], r, inside[3]],
            ];
            for [l, t, r, b] in pieces {
                if l < r && t < b {
                    out.push(Rectangle::new(
                        Point::new(l as f32, t as f32),
                        Size::new((r - l) as f32, (b - t) as f32),
                    ));
                }
            }
        }
        out
    }
}

fn overlaps(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn within(inner: [i32; 4], outer: [i32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

impl Renderer {
    pub fn new(default_font: Font, default_text_size: Pixels) -> Self {
        Self {
            default_font,
            default_text_size,
            layers: layer::Stack::new(),
            engine: Engine::new(),
            backdrop: Backdrop::default(),
        }
    }

    /// Yapfast port (patch 32): what each of this frame's layers changed
    /// since the last frame, in logical pixels. A kept region a layer under
    /// its cut changed in goes.
    /// A frame of another size (`size`, physical pixels) keeps nothing.
    pub fn layer_changes(
        &mut self,
        changes: Vec<Vec<Rectangle>>,
        scale_factor: f32,
        size: Size<u32>,
    ) {
        let backdrop = &mut self.backdrop;
        if (backdrop.width, backdrop.height) != (size.width, size.height) {
            backdrop.kept.clear();
            (backdrop.width, backdrop.height) = (size.width, size.height);
        }
        backdrop.kept.retain(|kept| {
            let [left, top, right, bottom] = kept.region;
            let area = Rectangle::new(
                Point::new(left as f32, top as f32),
                Size::new((right - left) as f32, (bottom - top) as f32),
            );
            kept.layers <= changes.len()
                && !changes[..kept.layers]
                    .iter()
                    .flatten()
                    .any(|change| (*change * scale_factor).intersects(&area))
        });
        backdrop.changes = changes;
        backdrop.fresh = true;
    }

    pub fn layers(&mut self) -> &[Layer] {
        self.layers.flush();
        self.layers.as_slice()
    }

    pub fn draw(
        &mut self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        viewport: &Viewport,
        damage: &[Rectangle],
        background_color: Color,
    ) {
        let scale_factor = viewport.scale_factor();

        self.layers.flush();
        let layer_count = self.layers.as_slice().len();
        let (frame_width, frame_height) = (pixels.width() as i32, pixels.height() as i32);
        // Yapfast port (patch 32): only the window's frame, told its
        // layers' changes just before and of the size the regions were
        // kept at, uses them; a `screenshot` draws whole.
        let backdrop_fits = std::mem::take(&mut self.backdrop.fresh)
            && (self.backdrop.width, self.backdrop.height) == (pixels.width(), pixels.height());

        // Yapfast port (patch 32): a region partly kept is drawn as the part
        // kept and the parts round it, so a damaged area that grew or moved
        // a little still takes what's kept.
        let regions = if backdrop_fits {
            self.backdrop.split(
                damage.iter().map(|&damage| damage * scale_factor),
                frame_width,
                frame_height,
            )
        } else {
            damage.iter().map(|&damage| damage * scale_factor).collect()
        };
        for damage_bounds in regions {

            // Yapfast port (patch 20): what's under the last opaque quad
            // that covers the whole region isn't drawn: the layers before
            // its own, the quads before it in it, and the clear. A video
            // under Settings' card was drawn, and the call round it, for
            // every frame the card hid.
            let (first_layer, first_quad) =
                covered(self.layers.as_slice(), damage_bounds, scale_factor);

            // Yapfast port (patch 32): what's under the changed layers comes
            // from the backdrop when it holds the region; else it's drawn
            // and kept as the first changed layer is reached.
            let region = [
                (damage_bounds.x.round() as i32).clamp(0, frame_width),
                (damage_bounds.y.round() as i32).clamp(0, frame_height),
                ((damage_bounds.x + damage_bounds.width).round() as i32).clamp(0, frame_width),
                ((damage_bounds.y + damage_bounds.height).round() as i32).clamp(0, frame_height),
            ];
            let cut = self.backdrop.first_change(damage_bounds, scale_factor);
            let room = region[0] < region[2] && region[1] < region[3];
            let held = self
                .backdrop
                .kept
                .iter()
                .position(|kept| within(region, kept.region) && kept.layers <= cut)
                .filter(|&index| {
                    backdrop_fits && room && first_layer < self.backdrop.kept[index].layers
                });
            let restore = held.is_some();
            // Kept from here on: the layers that didn't change in it, while
            // one above them did.
            let capture = backdrop_fits
                && !restore
                && room
                && first_layer < cut
                && cut > 0
                && cut < layer_count;
            let (first_layer, first_quad) = if let Some(index) = held {
                self.backdrop.restore(pixels, region, index);
                (self.backdrop.kept[index].layers, 0)
            } else {
                (first_layer, first_quad)
            };

            if !restore && first_layer == 0 && first_quad == 0 {
                crate::blit::fill(
                    (
                        damage_bounds.x.round() as i32,
                        damage_bounds.y.round() as i32,
                        (damage_bounds.x + damage_bounds.width).round() as i32,
                        (damage_bounds.y + damage_bounds.height).round() as i32,
                    ),
                    engine::into_color(background_color),
                    true,
                    pixels,
                );
            }

            for (index, layer) in self.layers.iter().enumerate().skip(first_layer) {
                if capture && index == cut {
                    self.backdrop.keep(pixels, region, cut);
                }
                let Some(layer_bounds) =
                    damage_bounds.intersection(&(layer.bounds * scale_factor))
                else {
                    continue;
                };

                engine::adjust_clip_mask(clip_mask, layer_bounds);

                if !layer.quads.is_empty() {
                    let render_span = debug::render(debug::Primitive::Quad);
                    let skipped = if index == first_layer { first_quad } else { 0 };
                    for (quad, background) in &layer.quads[skipped..] {
                        self.engine.draw_quad(
                            quad,
                            background,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }
                    render_span.finish();
                }

                if !layer.primitives.is_empty() {
                    let render_span = debug::render(debug::Primitive::Triangle);

                    for group in &layer.primitives {
                        let Some(group_bounds) = (group.clip_bounds()
                            * scale_factor)
                            .intersection(&layer_bounds)
                        else {
                            continue;
                        };

                        engine::adjust_clip_mask(clip_mask, group_bounds);

                        for primitive in group.as_slice() {
                            self.engine.draw_primitive(
                                primitive,
                                Transformation::scale(scale_factor)
                                    * group.transformation(),
                                pixels,
                                clip_mask,
                                group_bounds,
                            );
                        }

                        engine::adjust_clip_mask(clip_mask, layer_bounds);
                    }

                    render_span.finish();
                }

                if !layer.images.is_empty() {
                    let render_span = debug::render(debug::Primitive::Image);

                    for image in &layer.images {
                        self.engine.draw_image(
                            image,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }

                    render_span.finish();
                }

                if !layer.text.is_empty() {
                    let render_span = debug::render(debug::Primitive::Image);

                    for group in &layer.text {
                        for text in group.as_slice() {
                            self.engine.draw_text(
                                text,
                                Transformation::scale(scale_factor)
                                    * group.transformation(),
                                pixels,
                                clip_mask,
                                layer_bounds,
                            );
                        }
                    }

                    render_span.finish();
                }
            }
        }

        self.engine.trim(self.layers.as_slice(), scale_factor);

        if backdrop_fits {
            self.backdrop.age();
        }
    }
}

/// Yapfast port (patch 20): the first layer and its first quad to draw in
/// `region` (physical pixels): those of the last opaque quad that covers
/// the whole of it, inside its corners and its layer, or (0, 0).
fn covered(layers: &[Layer], region: Rectangle, scale_factor: f32) -> (usize, usize) {
    for (index, layer) in layers.iter().enumerate().rev() {
        // A layer clips what's in it: one that leaves part of the region
        // out can't cover it.
        if !region.is_within(&(layer.bounds * scale_factor)) {
            continue;
        }
        let found = layer.quads.iter().rposition(|(quad, background)| {
            opaque(background)
                && within_corners(
                    region,
                    quad.bounds * scale_factor,
                    <[f32; 4]>::from(quad.border.radius).map(|radius| radius * scale_factor),
                )
        });
        if let Some(quad) = found {
            return (index, quad);
        }
    }
    (0, 0)
}

/// Opaque all over inside its corners, whatever its border (drawn over
/// its edge) and its shadow (drawn outside it).
fn opaque(background: &Background) -> bool {
    match background {
        Background::Color(color) => color.a >= 1.0,
        Background::Gradient(core::Gradient::Linear(linear)) => {
            linear.stops.iter().flatten().all(|stop| stop.color.a >= 1.0)
        }
    }
}

/// Whether `region` is inside `bounds` and clear of its rounded corners,
/// each a square of its radius (`[top left, top right, bottom right,
/// bottom left]`): a region along an edge, between its corners, is covered.
fn within_corners(region: Rectangle, bounds: Rectangle, radii: [f32; 4]) -> bool {
    let (right, bottom) = (bounds.x + bounds.width, bounds.y + bounds.height);
    let corners = [
        (bounds.x, bounds.y),
        (right - radii[1], bounds.y),
        (right - radii[2], bottom - radii[2]),
        (bounds.x, bottom - radii[3]),
    ];
    region.is_within(&bounds)
        && corners.into_iter().zip(radii).all(|((x, y), radius)| {
            radius <= 0.0
                || region
                    .intersection(&Rectangle { x, y, width: radius, height: radius })
                    .is_none_or(|part| part.width <= 0.0 || part.height <= 0.0)
        })
}

impl core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: Rectangle) {
        // Yapfast port (patch 28): a layer started inside another is cut to
        // it, as a clip inside a clip is. Stock, it replaced its parent's:
        // a picture cut to its box (`ContentFit::Cover`) or a themed
        // button's own layer inside a scrolled page drew over the page's
        // header once scrolled under it.
        let parent = self.layers.current_mut().0.bounds;
        self.layers.push_clip(bounds);
        if parent.width.is_finite() && parent.height.is_finite() {
            let (layer, _) = self.layers.current_mut();
            layer.bounds = layer.bounds.intersection(&parent).unwrap_or(Rectangle {
                width: 0.0,
                height: 0.0,
                ..layer.bounds
            });
        }
    }

    fn end_layer(&mut self) {
        self.layers.pop_clip();
    }

    fn start_transformation(&mut self, transformation: Transformation) {
        self.layers.push_transformation(transformation);
    }

    fn end_transformation(&mut self) {
        self.layers.pop_transformation();
    }

    fn fill_quad(
        &mut self,
        quad: renderer::Quad,
        background: impl Into<Background>,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_quad(quad, background.into(), transformation);
    }

    fn reset(&mut self, new_bounds: Rectangle) {
        self.layers.reset(new_bounds);
    }

    fn allocate_image(
        &mut self,
        _handle: &core::image::Handle,
        callback: impl FnOnce(Result<core::image::Allocation, core::image::Error>)
        + Send
        + 'static,
    ) {
        #[cfg(feature = "image")]
        #[allow(unsafe_code)]
        // TODO: Concurrency
        callback(self.engine.raster_pipeline.load(_handle));

        #[cfg(not(feature = "image"))]
        callback(Err(core::image::Error::Unsupported));
    }
}

impl core::text::Renderer for Renderer {
    type Font = Font;
    type Paragraph = Paragraph;
    type Editor = Editor;

    const ICON_FONT: Font = Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';
    const ICED_LOGO: char = '\u{e801}';
    const SCROLL_UP_ICON: char = '\u{e802}';
    const SCROLL_DOWN_ICON: char = '\u{e803}';
    const SCROLL_LEFT_ICON: char = '\u{e804}';
    const SCROLL_RIGHT_ICON: char = '\u{e805}';

    fn default_font(&self) -> Self::Font {
        self.default_font
    }

    fn default_size(&self) -> Pixels {
        self.default_text_size
    }

    fn fill_paragraph(
        &mut self,
        text: &Self::Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();

        layer.draw_paragraph(
            text,
            position,
            color,
            clip_bounds,
            transformation,
        );
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_editor(editor, position, color, clip_bounds, transformation);
    }

    fn fill_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text(text, position, color, clip_bounds, transformation);
    }
}

impl graphics::text::Renderer for Renderer {
    fn fill_raw(&mut self, raw: graphics::text::Raw) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text_raw(raw, transformation);
    }
}

#[cfg(feature = "geometry")]
impl graphics::geometry::Renderer for Renderer {
    type Geometry = Geometry;
    type Frame = geometry::Frame;

    fn new_frame(&self, bounds: Rectangle) -> Self::Frame {
        geometry::Frame::new(bounds)
    }

    fn draw_geometry(&mut self, geometry: Self::Geometry) {
        let (layer, transformation) = self.layers.current_mut();

        match geometry {
            Geometry::Live {
                primitives,
                images,
                text,
                clip_bounds,
            } => {
                layer.draw_primitive_group(
                    primitives,
                    clip_bounds,
                    transformation,
                );

                for image in images {
                    layer.draw_image(image, transformation);
                }

                layer.draw_text_group(text, clip_bounds, transformation);
            }
            Geometry::Cache(cache) => {
                layer.draw_primitive_cache(
                    cache.primitives,
                    cache.clip_bounds,
                    transformation,
                );

                for image in cache.images.iter() {
                    layer.draw_image(image.clone(), transformation);
                }

                layer.draw_text_cache(
                    cache.text,
                    cache.clip_bounds,
                    transformation,
                );
            }
        }
    }
}

impl graphics::mesh::Renderer for Renderer {
    fn draw_mesh(&mut self, _mesh: graphics::Mesh) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }

    fn draw_mesh_cache(&mut self, _cache: iced_graphics::mesh::Cache) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }
}

#[cfg(feature = "image")]
impl core::image::Renderer for Renderer {
    type Handle = core::image::Handle;

    fn load_image(
        &self,
        handle: &Self::Handle,
    ) -> Result<core::image::Allocation, core::image::Error> {
        self.engine.raster_pipeline.load(handle)
    }

    fn measure_image(
        &self,
        handle: &Self::Handle,
    ) -> Option<crate::core::Size<u32>> {
        self.engine.raster_pipeline.dimensions(handle)
    }

    fn draw_image(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_raster(image, bounds, clip_bounds, transformation);
    }
}

#[cfg(feature = "svg")]
impl core::svg::Renderer for Renderer {
    fn measure_svg(
        &self,
        handle: &core::svg::Handle,
    ) -> crate::core::Size<u32> {
        self.engine.vector_pipeline.viewport_dimensions(handle)
    }

    fn draw_svg(
        &mut self,
        svg: core::Svg,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_svg(svg, bounds, clip_bounds, transformation);
    }
}

impl compositor::Default for Renderer {
    type Compositor = window::Compositor;
}

impl renderer::Headless for Renderer {
    async fn new(
        default_font: Font,
        default_text_size: Pixels,
        backend: Option<&str>,
    ) -> Option<Self> {
        // Yapfast port (patch 26): not while the app draws with the GPU.
        if !enabled::enabled()
            || backend.is_some_and(|backend| {
                !["tiny-skia", "tiny_skia"].contains(&backend)
            })
        {
            return None;
        }

        Some(Self::new(default_font, default_text_size))
    }

    fn name(&self) -> String {
        "tiny-skia".to_owned()
    }

    fn screenshot(
        &mut self,
        size: Size<u32>,
        scale_factor: f32,
        background_color: Color,
    ) -> Vec<u8> {
        let viewport = Viewport::with_physical_size(size, scale_factor);

        window::compositor::screenshot(self, &viewport, background_color)
    }
}

#[cfg(test)]
mod tests {
    use super::{Backdrop, Layer, Rectangle, Renderer, within_corners};
    use crate::core::renderer::{Quad, Renderer as _};
    use crate::core::{Color, Font, Pixels, Size};
    use crate::graphics::{Viewport, damage};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle { x, y, width, height }
    }

    #[test]
    fn a_region_along_an_edge_between_the_corners_is_within_them() {
        let card = rect(100.0, 100.0, 400.0, 300.0);
        let radii = [12.0, 8.0, 16.0, 0.0];
        // Along the right edge, clear of both right corners.
        assert!(within_corners(rect(300.0, 108.0, 200.0, 276.0), card, radii));
        // Into the top right corner's square, or the bottom right's.
        assert!(!within_corners(rect(300.0, 107.0, 200.0, 50.0), card, radii));
        assert!(!within_corners(rect(300.0, 300.0, 200.0, 85.0), card, radii));
        // A square corner is no corner; outside the card isn't covered.
        assert!(within_corners(rect(100.0, 300.0, 50.0, 100.0), card, radii));
        assert!(!within_corners(rect(90.0, 200.0, 50.0, 50.0), card, radii));
    }

    const WIDTH: u32 = 400;
    const HEIGHT: u32 = 200;

    /// A frame: the root layer's quads, then each upper layer's (each the
    /// window's size, as overlays are).
    type Frame = Vec<Vec<(Rectangle, Color)>>;

    fn build(renderer: &mut Renderer, frame: &Frame) {
        let window = rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        renderer.reset(window);
        for (index, quads) in frame.iter().enumerate() {
            if index > 0 {
                renderer.start_layer(window);
            }
            for &(bounds, color) in quads {
                renderer.fill_quad(Quad { bounds, ..Quad::default() }, color);
            }
            if index > 0 {
                renderer.end_layer();
            }
        }
    }

    /// The frame drawn whole by a new renderer.
    fn whole(frame: &Frame) -> Vec<u32> {
        let mut renderer = Renderer::new(Font::DEFAULT, Pixels(16.0));
        build(&mut renderer, frame);
        let mut pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut pixels), WIDTH, HEIGHT).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(WIDTH, HEIGHT).expect("mask");
        let viewport = Viewport::with_physical_size(Size::new(WIDTH, HEIGHT), 1.0);
        let window = rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        renderer.draw(&mut pixmap, &mut mask, &viewport, &[window], Color::BLACK);
        pixels
    }

    /// A window's renderer, drawing each frame's damage into one buffer, as
    /// the compositor does with one buffer.
    struct Window {
        renderer: Renderer,
        pixels: Vec<u32>,
        mask: tiny_skia::Mask,
        last: Option<Vec<Layer>>,
    }

    impl Window {
        fn new() -> Self {
            Self {
                renderer: Renderer::new(Font::DEFAULT, Pixels(16.0)),
                pixels: vec![0; (WIDTH * HEIGHT) as usize],
                mask: tiny_skia::Mask::new(WIDTH, HEIGHT).expect("mask"),
                last: None,
            }
        }

        fn draw(&mut self, frame: &Frame) {
            build(&mut self.renderer, frame);
            let current = self.renderer.layers().to_vec();
            let window = rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
            let (changes, damage) = match &self.last {
                Some(previous) => (
                    current
                        .iter()
                        .enumerate()
                        .map(|(index, layer)| match previous.get(index) {
                            Some(before) => Layer::damage(before, layer),
                            None => layer.extent(),
                        })
                        .collect(),
                    damage::group(damage::diff(previous, &current, Layer::extent, Layer::damage), window),
                ),
                None => (current.iter().map(|layer| vec![layer.bounds]).collect(), vec![window]),
            };
            self.renderer.layer_changes(changes, 1.0, Size::new(WIDTH, HEIGHT));
            let viewport = Viewport::with_physical_size(Size::new(WIDTH, HEIGHT), 1.0);
            let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut self.pixels), WIDTH, HEIGHT).expect("pixmap");
            if !damage.is_empty() {
                self.renderer.draw(&mut pixmap, &mut self.mask, &viewport, &damage, Color::BLACK);
            }
            self.last = Some(current);
        }
    }

    /// Art under a list and a tooltip over both, all partly see-through so
    /// a layer drawn twice or left out shows.
    fn frame(art: Rectangle, row: f32, tip: f32) -> Frame {
        let see_through = |r, g, b| Color { r, g, b, a: 0.5 };
        vec![
            vec![
                (rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32), Color::from_rgb(0.2, 0.2, 0.2)),
                (art, see_through(1.0, 0.0, 0.0)),
            ],
            vec![(rect(50.0, 50.0 + row, 100.0, 20.0), see_through(0.0, 0.0, 1.0))],
            vec![(rect(300.0 + tip, 10.0, 50.0, 20.0), see_through(0.0, 1.0, 0.0))],
        ]
    }

    #[test]
    fn a_layer_inside_another_is_cut_to_it() {
        let mut renderer = Renderer::new(Font::DEFAULT, Pixels(16.0));
        renderer.reset(rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32));
        renderer.start_layer(rect(0.0, 40.0, 400.0, 100.0));
        // A picture's box scrolled half out of its page.
        renderer.start_layer(rect(10.0, 120.0, 50.0, 50.0));
        renderer.fill_quad(Quad { bounds: rect(10.0, 120.0, 50.0, 50.0), ..Quad::default() }, Color::WHITE);
        renderer.end_layer();
        // And one wholly out of it.
        renderer.start_layer(rect(10.0, 150.0, 50.0, 20.0));
        renderer.end_layer();
        renderer.end_layer();
        let bounds: Vec<Rectangle> = renderer.layers().iter().map(|layer| layer.bounds).collect();
        assert!(bounds.contains(&rect(10.0, 120.0, 50.0, 20.0)), "{bounds:?}");
        assert!(
            bounds.iter().all(|bounds| bounds.y + bounds.height <= 140.0 || bounds.height == 0.0 || bounds.y == 0.0),
            "{bounds:?}"
        );
    }

    #[test]
    fn regions_kept_under_a_changing_layer_draw_as_the_whole_frame() {
        // Its edge runs through the list, so what's kept differs across.
        let art = rect(0.0, 0.0, 130.0, 120.0);
        let moved_art = rect(250.0, 150.0, 40.0, 40.0);
        let frames = [
            frame(art, 0.0, 0.0),
            // The list scrolls: what's under it is kept.
            frame(art, 5.0, 0.0),
            // And comes back as one copy.
            frame(art, 10.0, 0.0),
            // A mark comes on the list inside what's kept, away from its
            // left edge.
            {
                let mut marked = frame(art, 10.0, 0.0);
                marked[1].push((rect(120.0, 60.0, 20.0, 10.0), Color { r: 1.0, g: 1.0, b: 0.0, a: 0.5 }));
                marked
            },
            // The art changes away from the list, the list scrolls.
            frame(moved_art, 15.0, 0.0),
            // Only the tooltip moves.
            frame(moved_art, 15.0, 20.0),
            frame(moved_art, 20.0, 20.0),
            // The art changes under the list.
            frame(rect(40.0, 40.0, 200.0, 120.0), 25.0, 20.0),
            frame(rect(40.0, 40.0, 200.0, 120.0), 30.0, 20.0),
        ];
        let mut window = Window::new();
        let mut kept = 0;
        for (index, frame) in frames.iter().enumerate() {
            window.draw(frame);
            kept = kept.max(window.renderer.backdrop.kept.len());
            let expected = whole(frame);
            let differing = window.pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
            assert_eq!(differing, 0, "frame {index}: {differing} pixels differ");
        }
        assert!(kept > 0, "the backdrop was used");

        // A screenshot after: a new buffer drawn whole, with no changes told
        // for it, takes nothing from what's kept.
        let last = frames.last().expect("frames");
        window.draw(last);
        let mut shot = vec![0u32; (WIDTH * HEIGHT) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut shot), WIDTH, HEIGHT).expect("pixmap");
        let viewport = Viewport::with_physical_size(Size::new(WIDTH, HEIGHT), 1.0);
        let window_bounds = rect(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        window.renderer.draw(&mut pixmap, &mut window.mask, &viewport, &[window_bounds], Color::BLACK);
        assert!(shot == whole(last), "the screenshot is the whole frame");
    }

    #[test]
    fn a_kept_region_holds_its_own_pixels_and_goes_when_unused() {
        let art = rect(0.0, 0.0, 200.0, 120.0);
        let mut window = Window::new();
        window.draw(&frame(art, 0.0, 0.0));
        window.draw(&frame(art, 5.0, 0.0));
        let backdrop = &window.renderer.backdrop;
        assert_eq!(backdrop.kept.len(), 1);
        let kept = &backdrop.kept[0];
        let list = kept.region;
        let [left, top, right, bottom] = kept.region;
        assert_eq!(kept.pixels.len(), ((right - left) * (bottom - top) * 4) as usize);
        assert!(kept.pixels.len() < (WIDTH * HEIGHT * 4) as usize / 4, "{} bytes", kept.pixels.len());
        // Only the tooltip moves from here on, never over the list (what's
        // under the tooltip is kept in turn).
        for step in 0..Backdrop::IDLE {
            window.draw(&frame(art, 5.0, (step % 2 + 1) as f32 * 20.0));
        }
        let regions = |window: &Window| window.renderer.backdrop.kept.iter().map(|kept| kept.region).collect::<Vec<_>>();
        assert!(regions(&window).contains(&list), "kept while recent: {:?}", regions(&window));
        window.draw(&frame(art, 5.0, 0.0));
        window.draw(&frame(art, 5.0, 20.0));
        assert!(!regions(&window).contains(&list), "the list's region goes unused: {:?}", regions(&window));
    }
}
