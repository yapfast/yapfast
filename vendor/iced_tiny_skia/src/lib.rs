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
}

impl Renderer {
    pub fn new(default_font: Font, default_text_size: Pixels) -> Self {
        Self {
            default_font,
            default_text_size,
            layers: layer::Stack::new(),
            engine: Engine::new(),
        }
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

        for &damage_bounds in damage {
            let damage_bounds = damage_bounds * scale_factor;

            // Yapfast port (patch 20): what's under the last opaque quad
            // that covers the whole region isn't drawn: the layers before
            // its own, the quads before it in it, and the clear. A video
            // under Settings' card was drawn, and the call round it, for
            // every frame the card hid.
            let (first_layer, first_quad) =
                covered(self.layers.as_slice(), damage_bounds, scale_factor);

            // Yapfast port (patch 19): the region is cleared to the
            // background directly, as its rasterized path covered it:
            // the pixels whose centres are inside.
            if first_layer == 0 && first_quad == 0 {
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
        self.layers.push_clip(bounds);
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
    use super::{Rectangle, within_corners};

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
}
