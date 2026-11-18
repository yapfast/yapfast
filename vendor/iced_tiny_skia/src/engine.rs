use crate::Primitive;
use crate::core::renderer::Quad;
use crate::core::{Background, Color, Gradient, Rectangle, Size, Transformation, Vector};
use crate::graphics::{Image, Text};
use crate::text;

#[derive(Debug)]
pub struct Engine {
    text_pipeline: text::Pipeline,
    // Anomi port (patch 4): the rectangle the clip mask holds.
    pub(crate) clip: crate::clip::Clip,
    // Anomi port (patches 17, 18): shadows worked out lately.
    shadows: crate::shadow::Shadows,

    #[cfg(feature = "image")]
    pub(crate) raster_pipeline: crate::raster::Pipeline,
    #[cfg(feature = "svg")]
    pub(crate) vector_pipeline: crate::vector::Pipeline,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            text_pipeline: text::Pipeline::new(),
            clip: crate::clip::Clip::default(),
            shadows: crate::shadow::Shadows::default(),
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
        let shadow = quad.shadow;

        // Anomi port (patch 11): a quad whose shadow alone reaches into the
        // clip still draws its shadow; stock skipped it, so a damaged region
        // over the shadow alone lost it.
        let shadowed = shadow.color.a > 0.0
            && clip_bounds
                .intersects(&(crate::shadow::reach(quad.bounds, shadow) * transformation));
        let seen = clip_bounds.intersects(&physical_bounds);

        if !seen && !shadowed {
            return;
        }

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

        if shadowed {
            // Anomi port (patches 11, 17, 18): see `crate::shadow`. Under
            // an opaque fill, the pixels clear of its corners are the
            // fill's whatever lies under them.
            let scale = transformation.scale_factor();
            let corner = fill_border_radius
                .iter()
                .fold(0.0f32, |most, radius| most.max(*radius))
                * scale;
            let opaque = match background {
                Background::Color(color) => color.a >= 1.0,
                Background::Gradient(Gradient::Linear(linear)) => {
                    linear.stops.iter().flatten().count() > 0
                        && linear
                            .stops
                            .iter()
                            .flatten()
                            .all(|stop| stop.color.a >= 1.0)
                }
            };
            let body = (
                (physical_bounds.x + corner).ceil() as i32,
                (physical_bounds.y + corner).ceil() as i32,
                (physical_bounds.x + physical_bounds.width - corner).floor() as i32,
                (physical_bounds.y + physical_bounds.height - corner).floor() as i32,
            );
            let body = (opaque && body.2 > body.0 && body.3 > body.1).then_some(body);
            self.shadows.draw(
                quad.bounds,
                fill_border_radius.map(|radius| radius * scale),
                shadow,
                transformation,
                clip_bounds,
                body,
                pixels,
            );
        }

        if !seen {
            return;
        }

        // Anomi port (patches 3, 19): a plain rectangle of one colour on
        // whole pixels, as most grounds and panels are, is filled directly
        // inside the clip, as tiny-skia's anti-aliased fill of it through
        // the mask leaves it; a border of no colour leaves it as it is.
        if let Background::Color(color) = background
            && fill_border_radius == [0.0; 4]
            && (border_width == 0.0 || quad.border.color.a == 0.0)
            && let Some(edges) = whole_pixels(physical_bounds, clip_bounds)
        {
            crate::blit::fill(edges, into_color(*color), pixels);
            return;
        }

        let path = rounded_rectangle(quad.bounds, fill_border_radius);

        // Anomi port (patch 21): a fill of no colour (an outline over a
        // card) leaves every pixel as it was, so it isn't drawn.
        let clear = matches!(background, Background::Color(color) if color.a <= 0.0);

        let clip_mask = (!physical_bounds.is_within(&clip_bounds)).then_some(clip_mask as &_);

        if !clear {
            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: match background {
                        Background::Color(color) => {
                            tiny_skia::Shader::SolidColor(into_color(*color))
                        }
                        Background::Gradient(Gradient::Linear(linear)) => {
                            let (start, end) = quad.bounds.chord(linear.angle);

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
                                    vec![tiny_skia::GradientStop::new(0.0, tiny_skia::Color::BLACK)]
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
        }

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
                let border_path = rounded_rectangle(border_bounds, border_radius);

                pixels.stroke_path(
                    &border_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(quad.border.color)),
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
                let mut temp_pixmap =
                    tiny_skia::Pixmap::new(quad.bounds.width as u32, quad.bounds.height as u32)
                        .unwrap();

                let mut quad_mask =
                    tiny_skia::Mask::new(quad.bounds.width as u32, quad.bounds.height as u32)
                        .unwrap();

                let zero_bounds = Rectangle {
                    x: 0.0,
                    y: 0.0,
                    width: quad.bounds.width,
                    height: quad.bounds.height,
                };
                let path = rounded_rectangle(zero_bounds, fill_border_radius);

                quad_mask.fill_path(&path, tiny_skia::FillRule::EvenOdd, true, transform);
                let path_bounds = Rectangle {
                    x: border_width / 2.0,
                    y: border_width / 2.0,
                    width: quad.bounds.width - border_width,
                    height: quad.bounds.height - border_width,
                };

                let border_radius_path = rounded_rectangle(path_bounds, border_radius);

                temp_pixmap.stroke_path(
                    &border_radius_path,
                    &tiny_skia::Paint {
                        shader: tiny_skia::Shader::SolidColor(into_color(quad.border.color)),
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

    // Anomi port (patch 5): text is clipped by a rectangle as its glyphs
    // are copied in (`crate::blit::glyph`), with no mask; see
    // `crate::blit::text_clip`.
    pub fn draw_text(
        &mut self,
        text: &Text,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_bounds: Rectangle,
        damage_bounds: Rectangle,
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
                let Some(clip_bounds) =
                    clip_bounds.intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let physical_bounds =
                    Rectangle::new(*position, paragraph.min_bounds) * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_bounds =
                    crate::blit::text_clip(physical_bounds, clip_bounds, damage_bounds);

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

                let Some(clip_bounds) =
                    clip_bounds.intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

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
                wrapping,
                ellipsis,
                clip_bounds: local_clip_bounds,
            } => {
                let physical_bounds = *local_clip_bounds * transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_bounds =
                    crate::blit::text_clip(physical_bounds, clip_bounds, damage_bounds);

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
                    *wrapping,
                    *ellipsis,
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

                let clip_bounds =
                    crate::blit::text_clip(physical_bounds, clip_bounds, damage_bounds);

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

                let clip_mask =
                    (!physical_bounds.is_within(&clip_bounds)).then_some(clip_mask as &_);

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

                let clip_mask =
                    (!physical_bounds.is_within(&clip_bounds)).then_some(clip_mask as &_);

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
            Image::Raster {
                image,
                bounds,
                clip_bounds: local_clip_bounds,
            } => {
                let physical_bounds = *local_clip_bounds * _transformation;

                let Some(clip_bounds) = physical_bounds.intersection(&_clip_bounds) else {
                    return;
                };

                let radians = f32::from(image.rotation);
                // Anomi port (patches 2, 6, 7, 10, 19): the corners, as
                // iced's wgpu renderer cuts them: those of the picture's own
                // clip, each its own radius. A turned picture keeps square
                // corners, as stock drew every picture.
                let radius = <[f32; 4]>::from(image.border_radius)
                    .map(|radius| radius * _transformation.scale_factor());
                let round = radius.iter().any(|&radius| radius > 0.0);

                if radians == 0.0
                    && self.raster_pipeline.blit(
                        &image.handle,
                        *bounds * _transformation,
                        _clip_bounds,
                        &crate::blit::Rounded::new(crate::blit::whole(physical_bounds), radius),
                        image.opacity,
                        _pixels,
                    )
                {
                    return;
                }

                let rounded = (radians == 0.0 && round).then_some((physical_bounds, radius));
                // A rounded fill is held to the picture's rectangle, which
                // its pattern's padding would smear past.
                let mask_bounds = match rounded {
                    Some(_) => clip_bounds.intersection(&(*bounds * _transformation)),
                    None => Some(clip_bounds),
                };
                let Some(mask_bounds) = mask_bounds else {
                    return;
                };
                self.clip.set(_clip_mask, mask_bounds);

                let center = bounds.center();

                let transform = into_transform(_transformation).pre_rotate_at(
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
                    Some(_clip_mask),
                    rounded,
                );
            }
            #[cfg(feature = "svg")]
            Image::Vector { svg, bounds, .. } => {
                let physical_bounds = *bounds * _transformation;

                if !_clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let clip_mask =
                    (!physical_bounds.is_within(&_clip_bounds)).then_some(_clip_mask as &_);

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
                log::warn!("Unsupported primitive in `iced_tiny_skia`: {image:?}",);
            }
            #[cfg(not(feature = "svg"))]
            Image::Vector { .. } => {
                log::warn!("Unsupported primitive in `iced_tiny_skia`: {image:?}",);
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

/// Anomi port (patches 3, 19): the whole pixels (left, top, right, bottom)
/// tiny-skia's anti-aliased fill of `bounds` through a mask of `clip_bounds`
/// covers whole, when it covers no pixel in part: each edge of `bounds` on
/// a whole pixel or a pixel or more outside the clip, which then masks it.
fn whole_pixels(bounds: Rectangle, clip_bounds: Rectangle) -> Option<(i32, i32, i32, i32)> {
    let masked = !bounds.is_within(&clip_bounds);
    let (right, bottom) = (bounds.x + bounds.width, bounds.y + bounds.height);
    let (clip_right, clip_bottom) = (
        clip_bounds.x + clip_bounds.width,
        clip_bounds.y + clip_bounds.height,
    );
    let whole = |edge: f32, outside: bool| edge == edge.round() || masked && outside;
    (whole(bounds.x, bounds.x <= clip_bounds.x - 1.0)
        && whole(bounds.y, bounds.y <= clip_bounds.y - 1.0)
        && whole(right, right >= clip_right + 1.0)
        && whole(bottom, bottom >= clip_bottom + 1.0))
    .then(|| {
        (
            (bounds.x.ceil() as i32).max(clip_bounds.x.round() as i32),
            (bounds.y.ceil() as i32).max(clip_bounds.y.round() as i32),
            (right.floor() as i32).min(clip_right.round() as i32),
            (bottom.floor() as i32).min(clip_bottom.round() as i32),
        )
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

pub(crate) fn rounded_rectangle(bounds: Rectangle, border_radius: [f32; 4]) -> tiny_skia::Path {
    let [top_left, top_right, bottom_right, bottom_left] = border_radius;

    if top_left == 0.0 && top_right == 0.0 && bottom_right == 0.0 && bottom_left == 0.0 {
        return tiny_skia::PathBuilder::from_rect(
            tiny_skia::Rect::from_xywh(bounds.x, bounds.y, bounds.width, bounds.height)
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

pub(crate) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let x = ((x - a) / (b - a)).clamp(0.0, 1.0);

    x * x * (3.0 - 2.0 * x)
}

pub(crate) fn rounded_box_sdf(to_center: Vector, size: tiny_skia::Size, radii: &[f32]) -> f32 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Border, Point};

    /// A plain quad drawn as stock drew it: an anti-aliased path, through
    /// the mask when it isn't within the clip.
    fn stock(frame: &mut [u32], size: (u32, u32), quad: &Quad, color: Color, clip: Rectangle) {
        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
        crate::clip::Clip::default().set(&mut mask, clip);
        let mut pixels =
            tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(frame), size.0, size.1)
                .expect("frame");
        pixels.fill_path(
            &rounded_rectangle(quad.bounds, [0.0; 4]),
            &tiny_skia::Paint {
                shader: tiny_skia::Shader::SolidColor(into_color(color)),
                anti_alias: true,
                ..tiny_skia::Paint::default()
            },
            tiny_skia::FillRule::EvenOdd,
            tiny_skia::Transform::identity(),
            (!quad.bounds.is_within(&clip)).then_some(&mask),
        );
        if quad.border.width > 0.0 {
            let border = quad.border.width / 2.0;
            pixels.stroke_path(
                &rounded_rectangle(quad.bounds.shrink(border), [0.0; 4]),
                &tiny_skia::Paint {
                    shader: tiny_skia::Shader::SolidColor(into_color(quad.border.color)),
                    anti_alias: true,
                    ..tiny_skia::Paint::default()
                },
                &tiny_skia::Stroke {
                    width: quad.border.width,
                    ..tiny_skia::Stroke::default()
                },
                tiny_skia::Transform::identity(),
                (!quad.bounds.is_within(&clip)).then_some(&mask),
            );
        }
    }

    #[test]
    fn a_plain_quad_comes_out_as_stock_drew_it() {
        let size = (64_u32, 48_u32);
        let frame: Vec<u32> = (0..size.0 * size.1)
            .map(|at| 0xff00_0000 | at.wrapping_mul(2_654_435_761) >> 8)
            .collect();
        let quads = [
            Rectangle::new(Point::new(4.0, 3.0), Size::new(30.0, 20.0)),
            Rectangle::new(Point::new(-8.0, -2.0), Size::new(100.0, 60.0)),
            Rectangle::new(Point::new(10.5, 7.25), Size::new(20.0, 9.5)),
            Rectangle::new(Point::new(-0.5, 12.0), Size::new(40.0, 4.0)),
        ];
        let clips = [
            Rectangle::new(Point::ORIGIN, Size::new(64.0, 48.0)),
            Rectangle::new(Point::new(6.4, 2.5), Size::new(40.3, 30.0)),
        ];
        let colors = [
            Color::from_rgb(0.2, 0.5, 0.9),
            Color::from_rgba(0.9, 0.3, 0.1, 0.5),
            Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            Color::from_rgba(0.3, 0.3, 0.3, 0.0),
        ];
        let borders = [
            Border::default(),
            Border::default().width(1.0).color(Color::TRANSPARENT),
            Border::default()
                .width(2.0)
                .color(Color::from_rgb(1.0, 0.0, 0.0)),
        ];
        for bounds in quads {
            for clip in clips {
                for color in colors {
                    for border in borders {
                        let quad = Quad {
                            bounds,
                            border,
                            ..Quad::default()
                        };
                        let mut ours = frame.clone();
                        let mut theirs = frame.clone();
                        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
                        let mut engine = Engine::new();
                        engine.clip.set(&mut mask, clip);
                        engine.draw_quad(
                            &quad,
                            &Background::Color(color),
                            Transformation::IDENTITY,
                            &mut tiny_skia::PixmapMut::from_bytes(
                                bytemuck::cast_slice_mut(&mut ours),
                                size.0,
                                size.1,
                            )
                            .expect("frame"),
                            &mut mask,
                            clip,
                        );
                        stock(&mut theirs, size, &quad, color, clip);
                        assert!(
                            ours == theirs,
                            "{bounds:?} in {clip:?}, {color:?}, {border:?}"
                        );
                    }
                }
            }
        }
    }
}
