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
        //
        // Anomi: what a quad touches, by the pixels it and the clip cover
        // (`touches`).
        let shadowed = shadow.color.a > 0.0
            && touches(
                crate::shadow::reach(quad.bounds, shadow) * transformation,
                clip_bounds,
            );
        let seen = touches(physical_bounds, clip_bounds);

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
            crate::stats::mark(crate::stats::Kind::QuadShadow);
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
        //
        // Anomi: so is a rounded one whose part in the clip lies clear of its
        // corners, as a damaged region's edge passes a panel's ground: there
        // its path is its rectangle, which stock filled whole through the
        // mask. A border that reaches no pixel of the clip draws nothing
        // there, so isn't stroked.
        let scale = transformation.scale_factor();
        let radii = fill_border_radius.map(|radius| radius * scale);
        let inside = clip_pixels(clip_bounds);
        let square =
            fill_border_radius == [0.0; 4] || clear_of_corners(physical_bounds, radii, inside);
        let stroked = border_width > 0.0
            && quad.border.color.a > 0.0
            && !(square && clear_of_border(physical_bounds, border_width * scale, inside));

        if let Background::Color(color) = background
            && square
            && !stroked
            && let Some(edges) = whole_pixels(physical_bounds, clip_bounds)
        {
            crate::stats::mark(crate::stats::Kind::QuadFill);
            crate::blit::fill(edges, into_color(*color), pixels);
            return;
        }

        // Anomi: a quad square within the clip, as a row's ground on half a
        // pixel that a thin damaged region crosses, has its path cut to the
        // clip and a pixel round it: an edge farther out covers no pixel the
        // mask lets through, and stock rasterized the quad whole for them.
        let path = if square && !stroked && !unmasked(physical_bounds, clip_bounds) {
            rounded_rectangle(cut(quad.bounds, inside, scale), [0.0; 4])
        } else {
            rounded_rectangle(quad.bounds, fill_border_radius)
        };

        // Anomi port (patch 21): a fill of no colour (an outline over a
        // card) leaves every pixel as it was, so it isn't drawn.
        let clear = matches!(background, Background::Color(color) if color.a <= 0.0);

        let clip_mask = (!unmasked(physical_bounds, clip_bounds)).then_some(clip_mask as &_);

        if stroked {
            crate::stats::mark(crate::stats::Kind::QuadBorder);
        }
        if !clear {
            crate::stats::mark(match (background, clip_mask) {
                (Background::Gradient(_), _) => crate::stats::Kind::QuadGradient,
                (_, Some(_)) => crate::stats::Kind::QuadMasked,
                (_, None) => crate::stats::Kind::QuadPath,
            });
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

        if stroked {
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
    //
    // Anomi: `layer_bounds` is the layer's own, not cut by the damaged
    // region, so whether text is within its clip doesn't hang on where a
    // region ends: see `text_clip`.
    pub fn draw_text(
        &mut self,
        text: &Text,
        transformation: Transformation,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        layer_bounds: Rectangle,
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
                    layer_bounds.intersection(&(*local_clip_bounds * transformation))
                else {
                    return;
                };

                let bounds = Rectangle::new(*position, paragraph.min_bounds);

                let Some(clip_bounds) = crate::reach::text_clip(
                    bounds * transformation,
                    bounds.expand(crate::reach::REACH) * transformation,
                    clip_bounds,
                    damage_bounds,
                ) else {
                    return;
                };

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

                let reach = Rectangle::new(*position, editor.bounds).expand(crate::reach::REACH);
                let Some(clip_bounds) = layer_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                    .and_then(|clip| clip.intersection(&(reach * transformation)))
                    .and_then(|clip| clip.intersection(&damage_bounds))
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
                let Some(clip_bounds) = layer_bounds
                    .intersection(&(*local_clip_bounds * transformation))
                    .and_then(|clip| {
                        let bounds = crate::reach::aligned(*bounds, *align_x, *align_y);
                        crate::reach::text_clip(
                            bounds * transformation,
                            bounds.expand(crate::reach::REACH) * transformation,
                            clip,
                            damage_bounds,
                        )
                    })
                else {
                    return;
                };

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

                // Its damage is its clip (`crate::reach::text_reach`), so its
                // glyphs are cut there.
                let clip = raw.clip_bounds * transformation;
                let transformation = transformation * *local_transformation;
                let (width, height) = buffer.size();

                let bounds = Rectangle::new(
                    raw.position,
                    Size::new(
                        width.unwrap_or(raw.clip_bounds.width),
                        height.unwrap_or(raw.clip_bounds.height),
                    ),
                );

                let Some(clip_bounds) = layer_bounds
                    .intersection(&clip)
                    .and_then(|clip| clip.intersection(&damage_bounds))
                    .filter(|clip| clip.intersects(&(bounds * transformation)))
                else {
                    return;
                };

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

                if !touches(physical_bounds, clip_bounds) {
                    return;
                }

                let clip_mask =
                    (!unmasked(physical_bounds, clip_bounds)).then_some(clip_mask as &_);

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

                if !touches(physical_bounds, clip_bounds) {
                    return;
                }

                let clip_mask =
                    (!unmasked(physical_bounds, clip_bounds)).then_some(clip_mask as &_);

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

                let cut = crate::blit::Rounded::new(crate::blit::whole(physical_bounds), radius);
                if radians == 0.0
                    && self.raster_pipeline.blit(
                        &image.handle,
                        *bounds * _transformation,
                        _clip_bounds,
                        &cut,
                        image.opacity,
                        _pixels,
                    )
                {
                    return;
                }
                // Anomi port (patch 13): a video frame drawn at another size,
                // which the app's hook scales.
                if radians == 0.0
                    && self.raster_pipeline.video(
                        &image.handle,
                        image.filter_method,
                        *bounds * _transformation,
                        _clip_bounds,
                        &cut,
                        image.opacity,
                        _pixels,
                    )
                {
                    crate::stats::mark(crate::stats::Kind::ImageVideo);
                    return;
                }

                crate::stats::mark(crate::stats::Kind::ImageScaled);
                if let Some(size) = self.raster_pipeline.dimensions(&image.handle) {
                    let drawn = *bounds * _transformation;
                    crate::stats::scaled(
                        (size.width, size.height),
                        (drawn.width.round() as u32, drawn.height.round() as u32),
                    );
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
                crate::stats::mark(crate::stats::Kind::Svg);
                let physical_bounds = *bounds * _transformation;

                if !touches(physical_bounds, _clip_bounds) {
                    return;
                }

                let clip_mask =
                    (!unmasked(physical_bounds, _clip_bounds)).then_some(_clip_mask as &_);

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

/// Anomi: the pixels (left, top, right, bottom) a clip of `clip_bounds`
/// lets through, each edge rounded to the nearest as `Clip` sets the mask.
fn clip_pixels(clip_bounds: Rectangle) -> (f32, f32, f32, f32) {
    (
        clip_bounds.x.round(),
        clip_bounds.y.round(),
        (clip_bounds.x + clip_bounds.width).round(),
        (clip_bounds.y + clip_bounds.height).round(),
    )
}

/// Anomi: every pixel `bounds` touches, its edges anti-aliased, is one a
/// clip of `clip_bounds` lets through (`clip_pixels`), so the mask would
/// change nothing and the shape is drawn without it. Stock asked
/// `Rectangle::is_within` of the clip's own edges, so a shape within a clip
/// on half a pixel (a damaged region's edge) drew the pixel that edge cuts,
/// which the region's clearing and the mask leave out: there it was laid
/// again over what the frame before had drawn, a 3 px dot's edge in the
/// header growing lighter on each frame a region began on it.
pub(crate) fn unmasked(bounds: Rectangle, clip_bounds: Rectangle) -> bool {
    let (left, top, right, bottom) = clip_pixels(clip_bounds);
    bounds.x.floor() >= left
        && bounds.y.floor() >= top
        && (bounds.x + bounds.width).ceil() <= right
        && (bounds.y + bounds.height).ceil() <= bottom
}

/// Anomi: some pixel `bounds` touches, its edges anti-aliased, is one a
/// clip of `clip_bounds` lets through (`clip_pixels`). Stock asked whether
/// the two rectangles overlapped: a damaged region ending on y = 777.5 left
/// the clip the pixel row 777, which the region cleared, and the composer's
/// box, its top on 777.5 and so covering half of that row, wasn't drawn
/// there; the row stayed the ground's colour.
pub(crate) fn touches(bounds: Rectangle, clip_bounds: Rectangle) -> bool {
    let (left, top, right, bottom) = clip_pixels(clip_bounds);
    bounds.x.floor() < right
        && (bounds.x + bounds.width).ceil() > left
        && bounds.y.floor() < bottom
        && (bounds.y + bounds.height).ceil() > top
}

/// Anomi: `bounds`, logical, with each edge more than a pixel past `clip`
/// (from `clip_pixels`, the frame's pixels at `scale`) brought to a pixel
/// past it.
fn cut(bounds: Rectangle, clip: (f32, f32, f32, f32), scale: f32) -> Rectangle {
    let (left, top) = ((clip.0 - 1.0) / scale, (clip.1 - 1.0) / scale);
    let (right, bottom) = ((clip.2 + 1.0) / scale, (clip.3 + 1.0) / scale);
    let x = bounds.x.max(left);
    let y = bounds.y.max(top);
    Rectangle {
        x,
        y,
        width: ((bounds.x + bounds.width).min(right) - x).max(0.0),
        height: ((bounds.y + bounds.height).min(bottom) - y).max(0.0),
    }
}

/// Anomi: no pixel of `clip` (from `clip_pixels`) touches the square of any
/// rounded corner of `bounds`, its `radii` as iced orders them, all in the
/// frame's pixels: within the clip the quad's path is its rectangle, as
/// the arcs lie within their squares.
fn clear_of_corners(bounds: Rectangle, radii: [f32; 4], clip: (f32, f32, f32, f32)) -> bool {
    let (left, top) = (bounds.x, bounds.y);
    let (right, bottom) = (left + bounds.width, top + bounds.height);
    let [top_left, top_right, bottom_right, bottom_left] = radii;
    [
        (top_left, (left, top, left + top_left, top + top_left)),
        (top_right, (right - top_right, top, right, top + top_right)),
        (
            bottom_right,
            (right - bottom_right, bottom - bottom_right, right, bottom),
        ),
        (
            bottom_left,
            (left, bottom - bottom_left, left + bottom_left, bottom),
        ),
    ]
    .into_iter()
    .all(|(radius, (x0, y0, x1, y1))| {
        radius <= 0.0
            || x1.ceil() <= clip.0
            || x0.floor() >= clip.2
            || y1.ceil() <= clip.1
            || y0.floor() >= clip.3
    })
}

/// Anomi: a border `width` pixels wide inside `bounds` reaches no pixel of
/// `clip`, which lies within the rectangle the border's inner edge leaves
/// (clear of the corners, which `clear_of_corners` tells).
fn clear_of_border(bounds: Rectangle, width: f32, clip: (f32, f32, f32, f32)) -> bool {
    (bounds.x + width).ceil() <= clip.0
        && (bounds.y + width).ceil() <= clip.1
        && (bounds.x + bounds.width - width).floor() >= clip.2
        && (bounds.y + bounds.height - width).floor() >= clip.3
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

    /// A quad drawn as stock drew it: an anti-aliased path, through the
    /// mask when it isn't within the clip; its border stroked inside it,
    /// its corners `radius` (each more than half the border, or none).
    fn stock(frame: &mut [u32], size: (u32, u32), quad: &Quad, color: Color, clip: Rectangle) {
        stock_rounded(frame, size, quad, color, clip, [0.0; 4]);
    }

    fn stock_rounded(
        frame: &mut [u32],
        size: (u32, u32),
        quad: &Quad,
        color: Color,
        clip: Rectangle,
        radius: [f32; 4],
    ) {
        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
        crate::clip::Clip::default().set(&mut mask, clip);
        let mut pixels =
            tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(frame), size.0, size.1)
                .expect("frame");
        pixels.fill_path(
            &rounded_rectangle(quad.bounds, radius),
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
                &rounded_rectangle(
                    quad.bounds.shrink(border),
                    radius.map(|radius| if radius > 0.0 { radius - border } else { 0.0 }),
                ),
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

    #[test]
    fn a_rounded_quad_clipped_clear_of_its_corners_comes_out_as_stock_drew_it() {
        let size = (96_u32, 80_u32);
        let frame: Vec<u32> = (0..size.0 * size.1)
            .map(|at| 0xff00_0000 | at.wrapping_mul(2_654_435_761) >> 8)
            .collect();
        let quads = [
            (
                Rectangle::new(Point::new(4.0, 3.0), Size::new(60.0, 70.0)),
                [12.0, 0.0, 0.0, 0.0],
            ),
            (
                Rectangle::new(Point::new(-8.0, 6.0), Size::new(90.0, 60.0)),
                [8.0; 4],
            ),
            (
                Rectangle::new(Point::new(10.0, -20.0), Size::new(50.0, 90.0)),
                [6.0, 6.0, 0.0, 10.0],
            ),
            (
                Rectangle::new(Point::new(10.5, 7.25), Size::new(40.0, 50.5)),
                [5.0; 4],
            ),
        ];
        let clips = [
            // Along an edge, clear of the corners.
            Rectangle::new(Point::new(62.0, 20.0), Size::new(1.0, 30.0)),
            Rectangle::new(Point::new(30.0, 40.0), Size::new(30.0, 40.0)),
            Rectangle::new(Point::new(20.0, 20.0), Size::new(20.0, 20.0)),
            Rectangle::new(Point::new(0.0, 14.6), Size::new(96.0, 20.3)),
            // Thin, through a quad on half a pixel.
            Rectangle::new(Point::new(30.0, 0.0), Size::new(1.0, 80.0)),
            Rectangle::new(Point::new(0.0, 40.0), Size::new(96.0, 1.0)),
            // Over a corner, which stock still draws.
            Rectangle::new(Point::new(0.0, 0.0), Size::new(20.0, 20.0)),
            Rectangle::new(Point::new(50.0, 55.0), Size::new(40.0, 20.0)),
            Rectangle::new(Point::ORIGIN, Size::new(96.0, 80.0)),
        ];
        let colors = [
            Color::from_rgb(0.2, 0.5, 0.9),
            Color::from_rgba(0.9, 0.3, 0.1, 0.5),
            Color::from_rgba(0.3, 0.3, 0.3, 0.0),
        ];
        let borders = [
            Border::default(),
            Border::default().width(1.0).color(Color::TRANSPARENT),
            Border::default()
                .width(1.0)
                .color(Color::from_rgb(1.0, 0.0, 0.0)),
            Border::default()
                .width(2.0)
                .color(Color::from_rgba(0.0, 1.0, 0.0, 0.5)),
        ];
        for (bounds, radius) in quads {
            for clip in clips {
                for color in colors {
                    for border in borders {
                        let quad = Quad {
                            bounds,
                            border: border.rounded(crate::core::border::Radius {
                                top_left: radius[0],
                                top_right: radius[1],
                                bottom_right: radius[2],
                                bottom_left: radius[3],
                            }),
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
                        stock_rounded(&mut theirs, size, &quad, color, clip, radius);
                        assert!(
                            ours == theirs,
                            "{bounds:?} {radius:?} in {clip:?}, {color:?}, {border:?}"
                        );
                    }
                }
            }
        }
    }
}
