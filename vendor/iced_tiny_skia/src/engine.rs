use crate::Primitive;
use crate::core::renderer::Quad;
use crate::core::{
    Background, Color, Gradient, Rectangle, Size, Transformation, Vector,
};
use crate::graphics::{Image, Text};
use crate::text;

#[derive(Debug)]
pub struct Engine {
    text_pipeline: text::Pipeline,
    /// Yapfast port (patch 17): shadows worked out lately, by their shape.
    shadows: Vec<(ShadowKey, tiny_skia::Pixmap)>,

    #[cfg(feature = "image")]
    pub(crate) raster_pipeline: crate::raster::Pipeline,
    #[cfg(feature = "svg")]
    pub(crate) vector_pipeline: crate::vector::Pipeline,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            text_pipeline: text::Pipeline::new(),
            shadows: Vec::new(),
            #[cfg(feature = "image")]
            raster_pipeline: crate::raster::Pipeline::new(),
            #[cfg(feature = "svg")]
            vector_pipeline: crate::vector::Pipeline::new(),
        }
    }

    pub fn draw_quad(
        &mut self,
        quad: &Quad,
        background: &Background,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
    ) {
        let physical_bounds = quad.bounds * transformation;

        // Yapfast port (patch 11): a quad whose shadow alone reaches into
        // the clip still draws (its shadow). Stock, it was skipped, and a
        // damaged region over only the shadow lost it.
        let reach = shadow_reach(quad).map_or(physical_bounds, |reach| reach * transformation);

        if !clip_bounds.intersects(&physical_bounds.union(&reach)) {
            return;
        }

        // Yapfast port: a plain rectangle of one colour, as most panels and
        // backgrounds are, is filled as its part inside the clip: no clip
        // mask and no anti-aliased path, which cost several times more per
        // pixel (a window-sized background was masked for every damaged
        // region).
        if let Background::Color(color) = background
            && <[f32; 4]>::from(quad.border.radius) == [0.0; 4]
            && (quad.border.width == 0.0 || quad.border.color.a == 0.0)
            && quad.shadow.color.a == 0.0
        {
            if let Some(visible) = physical_bounds.intersection(&clip_bounds)
                && let Some(rect) = tiny_skia::Rect::from_ltrb(
                    visible.x.round(),
                    visible.y.round(),
                    (visible.x + visible.width).round(),
                    (visible.y + visible.height).round(),
                )
            {
                pixels.fill_rect(
                    rect,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            *color,
                        )),
                        anti_alias: false,
                        ..tiny_skia::Paint::default()
                    },
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            return;
        }

        // Yapfast port: a linear gradient on whole pixels (the nameplates'
        // fades and tints) is filled directly, only inside the clip; see
        // `crate::blit::gradient`.
        if let Background::Gradient(Gradient::Linear(linear)) = background
            && (quad.border.width == 0.0 || quad.border.color.a == 0.0)
            && quad.shadow.color.a == 0.0
            && let Some((x, y, width, height)) = whole_pixels(physical_bounds)
        {
            let (start, end) = linear.angle.to_distance(&quad.bounds);
            let (start, end) = (start * transformation, end * transformation);
            let mut stops: Vec<(f32, [f32; 4])> = Vec::with_capacity(8);
            for stop in linear.stops.into_iter().flatten() {
                // Ascending, as tiny-skia makes them.
                let offset = stop.offset.clamp(0.0, 1.0).max(stops.last().map_or(0.0, |last| last.0));
                let color = stop.color;
                stops.push((offset, [color.b, color.g, color.r, color.a]));
            }
            let scale = transformation.scale_factor();
            crate::blit::gradient(
                x,
                y,
                width,
                height,
                clip_bounds,
                <[f32; 4]>::from(quad.border.radius).map(|radius| radius * scale),
                (start.x, start.y),
                (end.x, end.y),
                &stops,
                pixels,
            );
            return;
        }

        let clip_mask = (!physical_bounds.is_within(&clip_bounds))
            .then_some(clip_mask as &_);

        let transform = into_transform(transformation);

        // Make sure the border radius is not larger than the bounds
        let border_width = quad
            .border
            .width
            .min(quad.bounds.width / 2.0)
            .min(quad.bounds.height / 2.0);

        let mut fill_border_radius = <[f32; 4]>::from(quad.border.radius);

        for radius in &mut fill_border_radius {
            *radius = (*radius)
                .min(quad.bounds.width / 2.0)
                .min(quad.bounds.height / 2.0);
        }

        let path = rounded_rectangle(quad.bounds, fill_border_radius);

        let shadow = quad.shadow;

        // Yapfast port (patch 11): the shadow is worked out and drawn only
        // inside the clip rectangle (the damaged region and the layer).
        // Stock, its whole rectangle was drawn with no clip for every
        // damaged region: a menu's or the picker's shadow, half-opaque
        // black under all of it, was laid again over what was already
        // drawn, darkening it, and every pixel of it worked out each time.
        // The quad's body, under an opaque fill, gets no shadow at all:
        // the fill covers it. Only the bands of the clip outside it are
        // worked out (a scrolled list in a raised panel damages the panel,
        // and working the shadow out under all of it every frame cost more
        // than drawing the list).
        let corner = fill_border_radius
            .iter()
            .fold(0.0f32, |most, radius| most.max(*radius))
            * transformation.scale_factor();
        let body = {
            let (left, top) = (
                (physical_bounds.x + corner).ceil(),
                (physical_bounds.y + corner).ceil(),
            );
            let (right, bottom) = (
                (physical_bounds.x + physical_bounds.width - corner).floor(),
                (physical_bounds.y + physical_bounds.height - corner).floor(),
            );
            (matches!(background, Background::Color(color) if color.a >= 1.0)
                && right > left
                && bottom > top)
                .then(|| Rectangle {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                })
        };
        if shadow.color.a > 0.0
            && let Some(visible) = reach.intersection(&clip_bounds)
        {
            // Whole pixels inside the clip, as the blit takes them: none
            // spills onto a neighbouring region already drawn.
            let (left, top) =
                (visible.x.round().max(0.0), visible.y.round().max(0.0));
            let visible = Rectangle {
                x: left,
                y: top,
                width: (visible.x + visible.width).round() - left,
                height: (visible.y + visible.height).round() - top,
            };
            let mut bands = Vec::with_capacity(4);
            match body.and_then(|body| body.intersection(&visible)) {
                Some(inner) => {
                    let (right, bottom) =
                        (visible.x + visible.width, visible.y + visible.height);
                    let (inner_right, inner_bottom) =
                        (inner.x + inner.width, inner.y + inner.height);
                    bands.extend([
                        Rectangle { x: visible.x, y: visible.y, width: visible.width, height: inner.y - visible.y },
                        Rectangle { x: visible.x, y: inner_bottom, width: visible.width, height: bottom - inner_bottom },
                        Rectangle { x: visible.x, y: inner.y, width: inner.x - visible.x, height: inner.height },
                        Rectangle { x: inner_right, y: inner.y, width: right - inner_right, height: inner.height },
                    ]);
                }
                None => bands.push(visible),
            }

            let radii = fill_border_radius
                .into_iter()
                .map(|radius| radius * transformation.scale_factor())
                .collect::<Vec<_>>();
            // Yapfast port (patch 17): a small shadow is worked out once for
            // its whole reach, kept by its shape (size, place within a
            // pixel, corners, blur, offset, colour), and each band is a
            // copy from it. The hover bar moving with a scroll had every
            // pixel of its shadow worked out again each frame.
            let origin = (reach.x.floor(), reach.y.floor());
            let key = ShadowKey::new(
                physical_bounds,
                origin,
                (
                    (reach.x + reach.width).ceil() - origin.0,
                    (reach.y + reach.height).ceil() - origin.1,
                ),
                &radii,
                shadow,
                transformation.scale_factor(),
            );
            let found = self.shadows.iter().position(|(known, _)| *known == key);
            // A large shadow (a menu's, the picker's) isn't kept: it would
            // hold a megabyte after its panel closed, and it moves seldom.
            // Only the bands the clip leaves outside its panel are worked
            // out, as patch 11 has them.
            let small = key.pixels.0 * key.pixels.1 <= SHADOW_KEPT_PIXELS;
            if found.is_none() && !small {
                for band in bands {
                    if band.width < 1.0 || band.height < 1.0 {
                        continue;
                    }
                    let corner = (band.x, band.y);
                    let size = (band.width as u32, band.height as u32);
                    if let Some(pixmap) =
                        shadow_pixmap(physical_bounds, corner, size, &radii, shadow, transformation)
                    {
                        pixels.draw_pixmap(
                            band.x as i32,
                            band.y as i32,
                            pixmap.as_ref(),
                            &tiny_skia::PixmapPaint::default(),
                            tiny_skia::Transform::default(),
                            None,
                        );
                    }
                }
            } else {
            let at = found.or_else(|| {
                let pixmap = shadow_pixmap(
                    physical_bounds,
                    origin,
                    key.pixels,
                    &radii,
                    shadow,
                    transformation,
                )?;
                if self.shadows.len() == SHADOWS_KEPT {
                    let _ = self.shadows.remove(0);
                }
                self.shadows.push((key, pixmap));
                Some(self.shadows.len() - 1)
            });
            if let Some(pixmap) = at.map(|at| &self.shadows[at].1) {
            let paint = tiny_skia::Paint {
                shader: tiny_skia::Pattern::new(
                    pixmap.as_ref(),
                    tiny_skia::SpreadMode::Pad,
                    tiny_skia::FilterQuality::Nearest,
                    1.0,
                    tiny_skia::Transform::from_translate(origin.0, origin.1),
                ),
                anti_alias: false,
                ..tiny_skia::Paint::default()
            };
            for band in bands {
                if band.width < 1.0 || band.height < 1.0 {
                    continue;
                }
                if let Some(rect) =
                    tiny_skia::Rect::from_xywh(band.x, band.y, band.width, band.height)
                {
                    pixels.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
                }
            }
            }
            }
        }

        // Yapfast port (patch 12): a one-colour quad with round corners on
        // whole pixels and no border (cards, menus, the picker, buttons and
        // chips) is filled as patch 8's gradient of one colour: only inside
        // the clip rectangle, corners by coverage, no mask. Stock, it went
        // through the anti-aliased path and the window-sized clip mask
        // (`mask_u8`), the picker's whole panel every frame it scrolled.
        if let Background::Color(color) = background
            && (quad.border.width == 0.0 || quad.border.color.a == 0.0)
            && let Some((x, y, width, height)) = whole_pixels(physical_bounds)
        {
            let straight = [color.b, color.g, color.r, color.a];
            crate::blit::gradient(
                x,
                y,
                width,
                height,
                clip_bounds,
                fill_border_radius
                    .map(|radius| radius * transformation.scale_factor()),
                (0.0, 0.0),
                (1.0, 0.0),
                &[(0.0, straight), (1.0, straight)],
                pixels,
            );
            return;
        }
        // The same with an opaque border round an opaque fill (the picker,
        // menus): the border's colour over the whole quad, then the fill
        // inside it, as the stroke would leave it.
        let scale = transformation.scale_factor();
        let border = quad.border.width * scale;
        if let Background::Color(color) = background
            && color.a >= 1.0
            && quad.border.color.a >= 1.0
            && border > 0.0
            && (border - border.round()).abs() < 0.01
            && let Some((x, y, width, height)) = whole_pixels(physical_bounds)
            && (border.round() as i32) * 2 < width.min(height)
        {
            let border = border.round() as i32;
            let edge = quad.border.color;
            let edge = [edge.b, edge.g, edge.r, edge.a];
            let fill = [color.b, color.g, color.r, color.a];
            let radius = fill_border_radius.map(|radius| radius * scale);
            crate::blit::gradient(
                x, y, width, height, clip_bounds, radius,
                (0.0, 0.0), (1.0, 0.0), &[(0.0, edge), (1.0, edge)], pixels,
            );
            crate::blit::gradient(
                x + border,
                y + border,
                width - border * 2,
                height - border * 2,
                clip_bounds,
                radius.map(|radius| (radius - border as f32).max(0.0)),
                (0.0, 0.0), (1.0, 0.0), &[(0.0, fill), (1.0, fill)], pixels,
            );
            return;
        }

        pixels.fill_path(
            &path,
            &tiny_skia::Paint {
                shader: match background {
                    Background::Color(color) => {
                        tiny_skia::Shader::SolidColor(into_color(*color))
                    }
                    Background::Gradient(Gradient::Linear(linear)) => {
                        let (start, end) =
                            linear.angle.to_distance(&quad.bounds);

                        let stops: Vec<tiny_skia::GradientStop> = linear
                            .stops
                            .into_iter()
                            .flatten()
                            .map(|stop| {
                                tiny_skia::GradientStop::new(
                                    stop.offset,
                                    tiny_skia::Color::from_rgba(
                                        stop.color.b,
                                        stop.color.g,
                                        stop.color.r,
                                        stop.color.a,
                                    )
                                    .expect("Create color"),
                                )
                            })
                            .collect();

                        tiny_skia::LinearGradient::new(
                            tiny_skia::Point {
                                x: start.x,
                                y: start.y,
                            },
                            tiny_skia::Point { x: end.x, y: end.y },
                            if stops.is_empty() {
                                vec![tiny_skia::GradientStop::new(
                                    0.0,
                                    tiny_skia::Color::BLACK,
                                )]
                            } else {
                                stops
                            },
                            tiny_skia::SpreadMode::Pad,
                            tiny_skia::Transform::identity(),
                        )
                        .expect("Create linear gradient")
                    }
                },
                anti_alias: true,
                ..tiny_skia::Paint::default()
            },
            tiny_skia::FillRule::EvenOdd,
            transform,
            clip_mask,
        );

        if border_width > 0.0 {
            // Border path is offset by half the border width
            let border_bounds = Rectangle {
                x: quad.bounds.x + border_width / 2.0,
                y: quad.bounds.y + border_width / 2.0,
                width: quad.bounds.width - border_width,
                height: quad.bounds.height - border_width,
            };

            // Make sure the border radius is correct
            let mut border_radius = <[f32; 4]>::from(quad.border.radius);
            let mut is_simple_border = true;

            for radius in &mut border_radius {
                *radius = if *radius == 0.0 {
                    // Path should handle this fine
                    0.0
                } else if *radius > border_width / 2.0 {
                    *radius - border_width / 2.0
                } else {
                    is_simple_border = false;
                    0.0
                }
                .min(border_bounds.width / 2.0)
                .min(border_bounds.height / 2.0);
            }

            // Stroking a path works well in this case
            if is_simple_border {
                let border_path =
                    rounded_rectangle(border_bounds, border_radius);

                pixels.stroke_path(
                    &border_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            quad.border.color,
                        )),
                        anti_alias: true,
                        ..tiny_skia::Paint::default()
                    },
                    &tiny_skia::Stroke {
                        width: border_width,
                        ..tiny_skia::Stroke::default()
                    },
                    transform,
                    clip_mask,
                );
            } else {
                // Draw corners that have too small border radii as having no border radius,
                // but mask them with the rounded rectangle with the correct border radius.
                let mut temp_pixmap = tiny_skia::Pixmap::new(
                    quad.bounds.width as u32,
                    quad.bounds.height as u32,
                )
                .unwrap();

                let mut quad_mask = tiny_skia::Mask::new(
                    quad.bounds.width as u32,
                    quad.bounds.height as u32,
                )
                .unwrap();

                let zero_bounds = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: quad.bounds.width,
                    height: quad.bounds.height,
                };
                let path = rounded_rectangle(zero_bounds, fill_border_radius);

                quad_mask.fill_path(
                    &path,
                    tiny_skia::FillRule::EvenOdd,
                    true,
                    transform,
                );
                let path_bounds = Rectangle {
                    x: border_width / 2.0,
                    y: border_width / 2.0,
                    width: quad.bounds.width - border_width,
                    height: quad.bounds.height - border_width,
                };

                let border_radius_path =
                    rounded_rectangle(path_bounds, border_radius);

                temp_pixmap.stroke_path(
                    &border_radius_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(
                            quad.border.color,
                        )),
                        anti_alias: true,
                        ..tiny_skia::Paint::default()
                    },
                    &tiny_skia::Stroke {
                        width: border_width,
                        ..tiny_skia::Stroke::default()
                    },
                    transform,
                    Some(&quad_mask),
                );

                pixels.draw_pixmap(
                    quad.bounds.x as i32,
                    quad.bounds.y as i32,
                    temp_pixmap.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    transform,
                    clip_mask,
                );
            }
        }
    }

    pub fn draw_text(
        &mut self,
        text: &Text,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        _clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
    ) {
        match text {
            Text::Paragraph {
                paragraph,
                position,
                color,
                clip_bounds: local_clip_bounds,
                transformation: local_transformation,
            } => {
                let transformation = transformation * *local_transformation;
                let Some(clip_bounds) = clip_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let physical_bounds =
                    Rectangle::new(*position, paragraph.min_bounds)
                        * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                self.text_pipeline.draw_paragraph(
                    paragraph,
                    *position,
                    *color,
                    pixels,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Editor {
                editor,
                position,
                color,
                clip_bounds: local_clip_bounds,
                transformation: local_transformation,
            } => {
                let transformation = transformation * *local_transformation;
                let Some(clip_bounds) = clip_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let physical_bounds =
                    Rectangle::new(*position, editor.bounds) * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                self.text_pipeline.draw_editor(
                    editor,
                    *position,
                    *color,
                    pixels,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Cached {
                content,
                bounds,
                color,
                size,
                line_height,
                font,
                align_x,
                align_y,
                shaping,
                clip_bounds: local_clip_bounds,
            } => {
                let physical_bounds = *local_clip_bounds * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                self.text_pipeline.draw_cached(
                    content,
                    *bounds,
                    *color,
                    *size,
                    *line_height,
                    *font,
                    *align_x,
                    *align_y,
                    *shaping,
                    pixels,
                    clip_bounds,
                    transformation,
                );
            }
            Text::Raw {
                raw,
                transformation: local_transformation,
            } => {
                let Some(buffer) = raw.buffer.upgrade() else {
                    return;
                };

                let transformation = transformation * *local_transformation;
                let (width, height) = buffer.size();

                let physical_bounds = Rectangle::new(
                    raw.position,
                    Size::new(
                        width.unwrap_or(clip_bounds.width),
                        height.unwrap_or(clip_bounds.height),
                    ),
                ) * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                self.text_pipeline.draw_raw(
                    &buffer,
                    raw.position,
                    raw.color,
                    pixels,
                    clip_bounds,
                    transformation,
                );
            }
        }
    }

    pub fn draw_primitive(
        &mut self,
        primitive: &Primitive,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        clip_bounds: Rectangle,
    ) {
        match primitive {
            Primitive::Fill { path, paint, rule } => {
                let physical_bounds = {
                    let bounds = path.bounds();

                    Rectangle {
                        x: bounds.x(),
                        y: bounds.y(),
                        width: bounds.width(),
                        height: bounds.height(),
                    } * transformation
                };

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&clip_bounds))
                    .then_some(clip_mask as &_);

                pixels.fill_path(
                    path,
                    paint,
                    *rule,
                    into_transform(transformation),
                    clip_mask,
                );
            }
            Primitive::Stroke {
                path,
                paint,
                stroke,
            } => {
                let physical_bounds = {
                    let bounds = path.bounds();

                    Rectangle {
                        x: bounds.x() - stroke.width / 2.0,
                        y: bounds.y() - stroke.width / 2.0,
                        width: bounds.width() + stroke.width,
                        height: bounds.height() + stroke.width,
                    } * transformation
                };

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&clip_bounds))
                    .then_some(clip_mask as &_);

                pixels.stroke_path(
                    path,
                    paint,
                    stroke,
                    into_transform(transformation),
                    clip_mask,
                );
            }
        }
    }

    pub fn draw_image(
        &mut self,
        image: &Image,
        _transformation: Transformation,
        _pixels: &mut tiny_skia::PixmapMut<'_>,
        _clip_mask: &mut tiny_skia::Mask,
        _clip_bounds: Rectangle,
    ) {
        match image {
            #[cfg(feature = "image")]
            Image::Raster { image, bounds, .. } => {
                let physical_bounds = *bounds * _transformation;

                if !_clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&_clip_bounds))
                    .then_some(_clip_mask as &_);

                let center = physical_bounds.center();
                let radians = f32::from(image.rotation);

                let transform = into_transform(_transformation).post_rotate_at(
                    radians.to_degrees(),
                    center.x,
                    center.y,
                );

                self.raster_pipeline.draw(
                    &image.handle,
                    image.filter_method,
                    *bounds,
                    image.opacity,
                    _pixels,
                    transform,
                    clip_mask,
                    // Yapfast port: for the one-to-one copy; each corner's
                    // own radius (round 3).
                    _clip_bounds,
                    {
                        let radius = image.border_radius;
                        [
                            radius.top_left,
                            radius.top_right,
                            radius.bottom_right,
                            radius.bottom_left,
                        ]
                        .map(|radius| radius * _transformation.scale_factor())
                    },
                );
            }
            #[cfg(feature = "svg")]
            Image::Vector { svg, bounds, .. } => {
                let physical_bounds = *bounds * _transformation;

                if !_clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask = (!physical_bounds.is_within(&_clip_bounds))
                    .then_some(_clip_mask as &_);

                let center = physical_bounds.center();
                let radians = f32::from(svg.rotation);

                let transform = into_transform(_transformation).post_rotate_at(
                    radians.to_degrees(),
                    center.x,
                    center.y,
                );

                self.vector_pipeline.draw(
                    &svg.handle,
                    svg.color,
                    *bounds,
                    svg.opacity,
                    _pixels,
                    transform,
                    clip_mask,
                );
            }
            #[cfg(not(feature = "image"))]
            Image::Raster { .. } => {
                log::warn!(
                    "Unsupported primitive in `iced_tiny_skia`: {image:?}",
                );
            }
            #[cfg(not(feature = "svg"))]
            Image::Vector { .. } => {
                log::warn!(
                    "Unsupported primitive in `iced_tiny_skia`: {image:?}",
                );
            }
        }
    }

    pub fn trim(&mut self) {
        self.text_pipeline.trim_cache();

        #[cfg(feature = "image")]
        self.raster_pipeline.trim_cache();

        #[cfg(feature = "svg")]
        self.vector_pipeline.trim_cache();
    }
}

/// Yapfast port: `bounds` as whole pixels, when they're whole pixels.
fn whole_pixels(bounds: Rectangle) -> Option<(i32, i32, i32, i32)> {
    let values = [bounds.x, bounds.y, bounds.width, bounds.height];
    values
        .iter()
        .all(|value| (value - value.round()).abs() < 0.01)
        .then(|| {
            let [x, y, width, height] = values.map(|value| value.round() as i32);
            (x, y, width, height)
        })
}

pub fn into_color(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(color.b, color.g, color.r, color.a)
        .expect("Convert color from iced to tiny_skia")
}

fn into_transform(transformation: Transformation) -> tiny_skia::Transform {
    let translation = transformation.translation();

    tiny_skia::Transform {
        sx: transformation.scale_factor(),
        kx: 0.0,
        ky: 0.0,
        sy: transformation.scale_factor(),
        tx: translation.x,
        ty: translation.y,
    }
}

/// Yapfast port: `pub(crate)` for `raster.rs`'s rounded pictures (patch 10).
pub(crate) fn rounded_rectangle(
    bounds: Rectangle,
    border_radius: [f32; 4],
) -> tiny_skia::Path {
    let [top_left, top_right, bottom_right, bottom_left] = border_radius;

    if top_left == 0.0
        && top_right == 0.0
        && bottom_right == 0.0
        && bottom_left == 0.0
    {
        return tiny_skia::PathBuilder::from_rect(
            tiny_skia::Rect::from_xywh(
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            )
            .expect("Build quad rectangle"),
        );
    }

    if top_left == top_right
        && top_left == bottom_right
        && top_left == bottom_left
        && top_left == bounds.width / 2.0
        && top_left == bounds.height / 2.0
    {
        return tiny_skia::PathBuilder::from_circle(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
            top_left,
        )
        .expect("Build circle path");
    }

    let mut builder = tiny_skia::PathBuilder::new();

    builder.move_to(bounds.x + top_left, bounds.y);
    builder.line_to(bounds.x + bounds.width - top_right, bounds.y);

    if top_right > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bounds.width - top_right,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + top_right,
            top_right,
        );
    }

    maybe_line_to(
        &mut builder,
        bounds.x + bounds.width,
        bounds.y + bounds.height - bottom_right,
    );

    if bottom_right > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bounds.width,
            bounds.y + bounds.height - bottom_right,
            bounds.x + bounds.width - bottom_right,
            bounds.y + bounds.height,
            bottom_right,
        );
    }

    maybe_line_to(
        &mut builder,
        bounds.x + bottom_left,
        bounds.y + bounds.height,
    );

    if bottom_left > 0.0 {
        arc_to(
            &mut builder,
            bounds.x + bottom_left,
            bounds.y + bounds.height,
            bounds.x,
            bounds.y + bounds.height - bottom_left,
            bottom_left,
        );
    }

    maybe_line_to(&mut builder, bounds.x, bounds.y + top_left);

    if top_left > 0.0 {
        arc_to(
            &mut builder,
            bounds.x,
            bounds.y + top_left,
            bounds.x + top_left,
            bounds.y,
            top_left,
        );
    }

    builder.finish().expect("Build rounded rectangle path")
}

fn maybe_line_to(path: &mut tiny_skia::PathBuilder, x: f32, y: f32) {
    if path.last_point() != Some(tiny_skia::Point { x, y }) {
        path.line_to(x, y);
    }
}

fn arc_to(
    path: &mut tiny_skia::PathBuilder,
    x_from: f32,
    y_from: f32,
    x_to: f32,
    y_to: f32,
    radius: f32,
) {
    let svg_arc = kurbo::SvgArc {
        from: kurbo::Point::new(f64::from(x_from), f64::from(y_from)),
        to: kurbo::Point::new(f64::from(x_to), f64::from(y_to)),
        radii: kurbo::Vec2::new(f64::from(radius), f64::from(radius)),
        x_rotation: 0.0,
        large_arc: false,
        sweep: true,
    };

    match kurbo::Arc::from_svg_arc(&svg_arc) {
        Some(arc) => {
            arc.to_cubic_beziers(0.1, |p1, p2, p| {
                path.cubic_to(
                    p1.x as f32,
                    p1.y as f32,
                    p2.x as f32,
                    p2.y as f32,
                    p.x as f32,
                    p.y as f32,
                );
            });
        }
        None => {
            path.line_to(x_to, y_to);
        }
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let x = ((x - a) / (b - a)).clamp(0.0, 1.0);

    x * x * (3.0 - 2.0 * x)
}

fn rounded_box_sdf(
    to_center: Vector,
    size: tiny_skia::Size,
    radii: &[f32],
) -> f32 {
    let radius = match (to_center.x > 0.0, to_center.y > 0.0) {
        (true, true) => radii[2],
        (true, false) => radii[1],
        (false, true) => radii[3],
        (false, false) => radii[0],
    };

    let x = (to_center.x.abs() - size.width() + radius).max(0.0);
    let y = (to_center.y.abs() - size.height() + radius).max(0.0);

    (x.powf(2.0) + y.powf(2.0)).sqrt() - radius
}

pub fn adjust_clip_mask(clip_mask: &mut tiny_skia::Mask, bounds: Rectangle) {
    // Yapfast port: clip masks here are only ever rectangles, set many times
    // a frame (each layer, each partly clipped text); clearing and refilling
    // the whole window-sized mask each time cost a memset of it every time.
    // Only the rows of the previous rectangle are cleared and the new one's
    // filled, and setting the same rectangle again does nothing.
    use std::cell::Cell;

    thread_local! {
        /// The mask last set (its pixels' address and size) and its rectangle.
        static LAST: Cell<Option<(usize, usize, [usize; 4])>> = const { Cell::new(None) };
    }

    let width = clip_mask.width() as usize;
    let height = clip_mask.height() as usize;
    let edge = |value: f32, limit: usize| (value.round().max(0.0) as usize).min(limit);
    let rect = [
        edge(bounds.x, width),
        edge(bounds.y, height),
        edge(bounds.x + bounds.width, width),
        edge(bounds.y + bounds.height, height),
    ];
    let data = clip_mask.data_mut();
    let identity = (data.as_ptr() as usize, data.len());
    let last = LAST.get();

    match last {
        Some((address, length, last)) if (address, length) == identity => {
            if last == rect {
                return;
            }
            let [x0, y0, x1, y1] = last;
            for row in y0..y1 {
                data[row * width + x0..row * width + x1].fill(0);
            }
        }
        _ => data.fill(0),
    }

    let [x0, y0, x1, y1] = rect;
    if x0 < x1 {
        for row in y0..y1 {
            data[row * width + x0..row * width + x1].fill(255);
        }
    }

    LAST.set(Some((identity.0, identity.1, rect)));
}

/// Yapfast port (patches 11 and 15): where a quad's shadow reaches, if it
/// has one: the quad moved by the shadow's offset and grown by its blur.
/// Drawing takes the shadow inside it; the layer's damage counts it too.
/// How many shadows the engine keeps worked out: the hover bar's, a menu's,
/// a popup's, with room for the sizes they take as they change.
const SHADOWS_KEPT: usize = 8;
/// The most pixels a kept shadow has: a hover bar's, 128 KiB.
const SHADOW_KEPT_PIXELS: u32 = 32 * 1024;

/// A shadow's shape, which its pixels follow from wholly.
#[derive(Debug, Clone, PartialEq)]
struct ShadowKey {
    /// The quad's size, its place within its pixel from the shadow's
    /// corner, and the shadow's size in whole pixels.
    size: (u32, u32),
    within: (u32, u32),
    pixels: (u32, u32),
    radii: [u32; 4],
    blur: u32,
    offset: (u32, u32),
    color: [u32; 4],
}

impl ShadowKey {
    fn new(
        bounds: Rectangle,
        origin: (f32, f32),
        pixels: (f32, f32),
        radii: &[f32],
        shadow: crate::core::Shadow,
        scale: f32,
    ) -> Self {
        let mut corners = [0; 4];
        for (corner, radius) in corners.iter_mut().zip(radii) {
            *corner = radius.to_bits();
        }
        Self {
            size: (bounds.width.to_bits(), bounds.height.to_bits()),
            within: ((bounds.x - origin.0).to_bits(), (bounds.y - origin.1).to_bits()),
            pixels: (pixels.0 as u32, pixels.1 as u32),
            radii: corners,
            blur: (shadow.blur_radius * scale).to_bits(),
            offset: ((shadow.offset.x * scale).to_bits(), (shadow.offset.y * scale).to_bits()),
            color: [
                shadow.color.r.to_bits(),
                shadow.color.g.to_bits(),
                shadow.color.b.to_bits(),
                shadow.color.a.to_bits(),
            ],
        }
    }
}

/// A shadow's pixels over `width` by `height` from `origin` (its whole
/// reach, or a band of it), as stock iced works them out pixel by pixel.
fn shadow_pixmap(
    physical_bounds: Rectangle,
    origin: (f32, f32),
    (width, height): (u32, u32),
    radii: &[f32],
    shadow: crate::core::Shadow,
    transformation: Transformation,
) -> Option<tiny_skia::Pixmap> {
    let half_width = physical_bounds.width / 2.0;
    let half_height = physical_bounds.height / 2.0;
    let size = tiny_skia::Size::from_wh(half_width, half_height)?;
    let scale = transformation.scale_factor();
    let colors: Vec<tiny_skia::PremultipliedColorU8> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (origin.0 + x as f32, origin.1 + y as f32)))
        .map(|(x, y)| {
            let shadow_distance = rounded_box_sdf(
                Vector::new(
                    x - physical_bounds.position().x - (shadow.offset.x * scale) - half_width,
                    y - physical_bounds.position().y - (shadow.offset.y * scale) - half_height,
                ),
                size,
                radii,
            )
            .max(0.0);
            let shadow_alpha = 1.0
                - smoothstep(
                    -shadow.blur_radius * scale,
                    shadow.blur_radius * scale,
                    shadow_distance,
                );
            let mut color = into_color(shadow.color);
            color.apply_opacity(shadow_alpha);
            color.to_color_u8().premultiply()
        })
        .collect();
    tiny_skia::Pixmap::from_vec(
        bytemuck::cast_vec(colors),
        tiny_skia::IntSize::from_wh(width, height)?,
    )
}

pub(crate) fn shadow_reach(quad: &Quad) -> Option<Rectangle> {
    let shadow = quad.shadow;
    (shadow.color.a > 0.0).then(|| Rectangle {
        x: quad.bounds.x + shadow.offset.x - shadow.blur_radius,
        y: quad.bounds.y + shadow.offset.y - shadow.blur_radius,
        width: quad.bounds.width + shadow.blur_radius * 2.0,
        height: quad.bounds.height + shadow.blur_radius * 2.0,
    })
}
