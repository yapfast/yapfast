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
    /// Yapfast port (patches 17 and 18): shadows worked out lately, by
    /// their shape, each as its nine slices.
    shadows: Vec<(ShadowKey, Sliced)>,

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

        // Yapfast port (patch 33): a quad with no area draws nothing;
        // square-cornered, its path couldn't be built at all (a theme's 0 px
        // corners on a card's empty banner panicked here).
        if !(physical_bounds.width > 0.0 && physical_bounds.height > 0.0) {
            return;
        }

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
            // Patch 19: written in directly (`blit::fill`).
            if let Some(visible) = physical_bounds.intersection(&clip_bounds) {
                crate::blit::fill(
                    (
                        visible.x.round() as i32,
                        visible.y.round() as i32,
                        (visible.x + visible.width).round() as i32,
                        (visible.y + visible.height).round() as i32,
                    ),
                    into_color(*color),
                    false,
                    pixels,
                );
            }
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

        let path = rounded_rectangle(quad.bounds, fill_border_radius);

        let shadow = quad.shadow;

        // Yapfast port (patch 11): the shadow is worked out and drawn only
        // inside the clip rectangle (the damaged region and the layer).
        // Stock, its whole rectangle was drawn with no clip for every
        // damaged region: a menu's or the picker's shadow, half-opaque
        // black under all of it, was laid again over what was already
        // drawn, darkening it, and every pixel of it worked out each time.
        // The quad's body gets no shadow at all (patch 34). Only the bands
        // of the clip outside it are worked out (a scrolled list in a
        // raised panel damages the panel, and working the shadow out under
        // all of it every frame cost more than drawing the list).
        // Yapfast port (patch 34): under a see-through quad, the band
        // between its body and its rounded outline takes only the shadow
        // outside its shape (`cut`), as iced_wgpu's quad shader does: the
        // shadow times one less the quad's coverage of each pixel. An
        // opaque quad hides that band, so its shadow is worked out whole,
        // as before, and its anti-aliased edge blends over it unchanged.
        let cut = !crate::opaque(background);
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
            // Yapfast port (patch 34): a shadow lies outside its quad, as
            // CSS's box-shadow and Slint's drop shadow do: a see-through
            // quad (a theme's menu, its ground drawn under it with art) no
            // longer shows its own shadow through itself. Its body is
            // wholly covered by the quad, so it's skipped whole.
            (right > left
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
            // Yapfast port (patch 17): a shadow is worked out once, kept by
            // its shape (size, place within a pixel, corners, blur, offset,
            // colour), and each band is a copy from it. The hover bar
            // moving with a scroll had every pixel of its shadow worked out
            // again each frame.
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
                cut,
            );
            // Yapfast port (patch 18): a large shadow (a card's, a menu's)
            // is kept as its nine slices: the corners whole, each edge one
            // row or column across, and the middle one pixel, which the
            // straight stretches of its edges and its middle repeat
            // exactly. Stock, and with patch 11, a 1400 px card's shadow
            // was worked out pixel by pixel each time anything on the card
            // was drawn again. Every kept shadow is copied in directly,
            // source over, rather than through tiny-skia's pattern
            // pipeline.
            let at = match self.shadows.iter().position(|(known, _)| *known == key) {
                Some(at) => Some(at),
                None => Sliced::new(
                    physical_bounds,
                    origin,
                    key.pixels,
                    &radii,
                    shadow,
                    transformation,
                    cut,
                )
                .map(|sliced| {
                    if self.shadows.len() == SHADOWS_KEPT {
                        let _ = self.shadows.remove(0);
                    }
                    self.shadows.push((key, sliced));
                    self.shadows.len() - 1
                }),
            };
            if let Some(sliced) = at.map(|at| &self.shadows[at].1) {
                let origin = (origin.0 as i32, origin.1 as i32);
                for band in bands {
                    if band.width >= 1.0 && band.height >= 1.0 {
                        sliced.draw(origin, band, pixels);
                    }
                }
            }
        }

        // Yapfast port: a linear gradient (the nameplates' fades and tints)
        // is filled directly, only inside the clip; see
        // `crate::blit::gradient`. Patch 43: after its shadow, as a quad of
        // one colour is, and at 100% off whole pixels too (a chat bubble as
        // wide as its text): a theme's glossy bubbles and cards, each with
        // a shadow, went through tiny-skia's pipeline and the clip mask.
        // Scaled, it's put on whole pixels, as before.
        if let Background::Gradient(Gradient::Linear(linear)) = background
            && (quad.border.width == 0.0 || quad.border.color.a == 0.0)
        {
            let scale = transformation.scale_factor();
            let bounds = if (scale - 1.0).abs() < 0.001 {
                Some(physical_bounds)
            } else {
                whole_pixels(physical_bounds, scale).map(pixel_rectangle)
            };
            if let Some(bounds) = bounds {
                let (start, end) = linear.angle.to_distance(&quad.bounds);
                let (start, end) = (start * transformation, end * transformation);
                let mut stops: Vec<(f32, [f32; 4])> = Vec::with_capacity(8);
                for stop in linear.stops.into_iter().flatten() {
                    // Ascending, as tiny-skia makes them.
                    let offset = stop.offset.clamp(0.0, 1.0).max(stops.last().map_or(0.0, |last| last.0));
                    let color = stop.color;
                    stops.push((offset, [color.b, color.g, color.r, color.a]));
                }
                crate::blit::gradient(
                    bounds,
                    clip_bounds,
                    <[f32; 4]>::from(quad.border.radius).map(|radius| radius * scale),
                    (start.x, start.y),
                    (end.x, end.y),
                    &stops,
                    pixels,
                );
                return;
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
            && let Some((x, y, width, height)) = whole_pixels(physical_bounds, transformation.scale_factor())
        {
            let straight = [color.b, color.g, color.r, color.a];
            crate::blit::gradient(
                pixel_rectangle((x, y, width, height)),
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
        // Patch 25: a scaled border is put on whole pixels too, a pixel at
        // the least.
        let border = if (scale - 1.0).abs() >= 0.001 && border > 0.0 {
            border.round().max(1.0)
        } else {
            border
        };
        if let Background::Color(color) = background
            && color.a >= 1.0
            && quad.border.color.a >= 1.0
            && border > 0.0
            && (border - border.round()).abs() < 0.01
            && let Some((x, y, width, height)) = whole_pixels(physical_bounds, transformation.scale_factor())
            && (border.round() as i32) * 2 < width.min(height)
        {
            let border = border.round() as i32;
            let edge = quad.border.color;
            let edge = [edge.b, edge.g, edge.r, edge.a];
            let fill = [color.b, color.g, color.r, color.a];
            let radius = fill_border_radius.map(|radius| radius * scale);
            crate::blit::gradient(
                pixel_rectangle((x, y, width, height)), clip_bounds, radius,
                (0.0, 0.0), (1.0, 0.0), &[(0.0, edge), (1.0, edge)], pixels,
            );
            crate::blit::gradient(
                pixel_rectangle((x + border, y + border, width - border * 2, height - border * 2)),
                clip_bounds,
                radius.map(|radius| (radius - border as f32).max(0.0)),
                (0.0, 0.0), (1.0, 0.0), &[(0.0, fill), (1.0, fill)], pixels,
            );
            return;
        }
        // Yapfast port (patch 44): test containment against the same whole
        // pixels as the clip mask and damage clear. A fractional stroke at
        // the clip edge can otherwise paint pixels that aren't cleared.
        let clip_mask = clip_bounds
            .snap()
            .is_none_or(|clip| !physical_bounds.is_within(&Rectangle::from(clip)))
            .then_some(clip_mask as &_);

        // Yapfast port (patch 21): a quad of no colour (an outline drawn over
        // a card, `controls::edged`) fills nothing; stock, its whole area
        // went through the anti-aliased path and the clip mask.
        let clear = matches!(background, Background::Color(color) if color.a <= 0.0);
        if !clear {
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
            Image::Raster {
                image,
                bounds,
                clip_bounds: own,
            } => {
                // Yapfast port (patch 30): a picture is cut to its own clip
                // as well, the box a `ContentFit::Cover` picture overflows.
                // Stock ignored it, and covered pictures spilled out of
                // their boxes over their neighbours.
                let Some(clip_bounds) = _clip_bounds.intersection(&(*own * _transformation)) else {
                    return;
                };
                let physical_bounds = *bounds * _transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let center = physical_bounds.center();
                let radians = f32::from(image.rotation);

                let transform = into_transform(_transformation).post_rotate_at(
                    radians.to_degrees(),
                    center.x,
                    center.y,
                );

                // Yapfast port (patch 30): only the direct copy (unrotated,
                // `raster.rs`) cuts by `clip_bounds`; a turned picture (a
                // Zine card's lean) or one filled through tiny-skia goes
                // by the mask, which is set to the narrowed clip while it
                // draws and put back to the layer's after.
                // Turned, it reaches past its unturned bounds.
                let reach = bounds.rotate(image.rotation) * _transformation;
                let narrowed = narrowed_mask(
                    _clip_mask,
                    &reach,
                    clip_bounds,
                    _clip_bounds,
                    !is_direct(transform),
                );
                let clip_mask = (!reach.is_within(&clip_bounds)).then_some(_clip_mask as &_);

                self.raster_pipeline.draw(
                    &image.handle,
                    image.filter_method,
                    *bounds,
                    image.opacity,
                    _pixels,
                    transform,
                    clip_mask,
                    // Yapfast port: for the one-to-one copy; each corner's
                    // own radius (round 3), at the picture's own clip
                    // (patch 38).
                    clip_bounds,
                    *own,
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

                if narrowed {
                    adjust_clip_mask(_clip_mask, _clip_bounds);
                }
            }
            #[cfg(feature = "svg")]
            Image::Vector {
                svg,
                bounds,
                clip_bounds: own,
            } => {
                // Yapfast port (patch 30): an SVG is cut to its own clip
                // too, through the mask (it's always drawn through
                // tiny-skia).
                let Some(clip_bounds) = _clip_bounds.intersection(&(*own * _transformation)) else {
                    return;
                };
                let physical_bounds = *bounds * _transformation;

                if !clip_bounds.intersects(&physical_bounds) {
                    return;
                }

                let reach = bounds.rotate(svg.rotation) * _transformation;
                let narrowed = narrowed_mask(_clip_mask, &reach, clip_bounds, _clip_bounds, true);
                let clip_mask = (!reach.is_within(&clip_bounds)).then_some(_clip_mask as &_);

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

                if narrowed {
                    adjust_clip_mask(_clip_mask, _clip_bounds);
                }
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

    /// After a frame of `layers` at `scale_factor`.
    pub fn trim(&mut self, _layers: &[crate::Layer], _scale_factor: f32) {
        self.text_pipeline.trim_cache();

        // Yapfast port (patch 24): the pictures in the frame's layers keep
        // their scaled copies, drawn this time or not.
        #[cfg(feature = "image")]
        {
            let shown = _layers
                .iter()
                .flat_map(|layer| &layer.images)
                .filter_map(|image| match image {
                    Image::Raster { image, bounds, .. } => {
                        Some(crate::raster::shown_at(image, *bounds, _scale_factor))
                    }
                    #[allow(unreachable_patterns)]
                    _ => None,
                })
                .collect();
            self.raster_pipeline.trim_cache(&shown);
        }

        #[cfg(feature = "svg")]
        self.vector_pipeline.trim_cache();
    }
}

/// Yapfast port: `bounds` as whole pixels, when they're whole pixels.
/// `bounds` (in the frame's pixels) as whole pixels, when they are: x, y,
/// width, height. Yapfast port (patch 25): at a scale other than 1 (a
/// zoom, a desktop's 125%), whose quads seldom fall on whole pixels, each
/// edge is put on the nearest pixel's, as the pictures are, so neighbours
/// still meet. Stock, every such quad went through the anti-aliased path
/// and the clip mask.
fn whole_pixels(bounds: Rectangle, scale: f32) -> Option<(i32, i32, i32, i32)> {
    let values = [bounds.x, bounds.y, bounds.width, bounds.height];
    if (scale - 1.0).abs() >= 0.001 {
        let (left, top) = (bounds.x.round() as i32, bounds.y.round() as i32);
        let right = (bounds.x + bounds.width).round() as i32;
        let bottom = (bounds.y + bounds.height).round() as i32;
        return (right > left && bottom > top).then_some((left, top, right - left, bottom - top));
    }
    values
        .iter()
        .all(|value| (value - value.round()).abs() < 0.01)
        .then(|| {
            let [x, y, width, height] = values.map(|value| value.round() as i32);
            (x, y, width, height)
        })
}

/// Whole pixels from [`whole_pixels`] as a rectangle.
fn pixel_rectangle((x, y, width, height): (i32, i32, i32, i32)) -> Rectangle {
    Rectangle { x: x as f32, y: y as f32, width: width as f32, height: height as f32 }
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
    // Yapfast port (patch 35): a hard edge (a shadow with no blur, as
    // themes' offset shadows are) steps at `a`. Stock, 0 / 0 gave NaN inside
    // the shape and the whole shadow came out clear.
    // Samples fall on pixel corners, so the edge itself (and float dust
    // on it) counts as inside: within half a pixel.
    if b <= a {
        return if x <= a + 0.5 { 0.0 } else { 1.0 };
    }
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

/// Yapfast port (patch 34): the signed distance from a point (`to_center`,
/// from the box's centre) to a rounded box's outline, negative inside.
/// `rounded_box_sdf` is iced's, right only outside (inside it's minus the
/// corner's radius throughout).
fn rounded_box_distance(to_center: Vector, size: tiny_skia::Size, radii: &[f32]) -> f32 {
    let radius = match (to_center.x > 0.0, to_center.y > 0.0) {
        (true, true) => radii[2],
        (true, false) => radii[1],
        (false, true) => radii[3],
        (false, false) => radii[0],
    };
    let x = to_center.x.abs() - size.width() + radius;
    let y = to_center.y.abs() - size.height() + radius;
    x.max(0.0).hypot(y.max(0.0)) + x.max(y).min(0.0) - radius
}

/// Yapfast port (patch 30): whether a picture drawn with `transform` takes
/// the direct copy in `raster.rs` (unrotated, unflipped), which cuts by its
/// clip rectangle rather than the mask.
#[cfg(feature = "image")]
fn is_direct(transform: tiny_skia::Transform) -> bool {
    transform.kx == 0.0 && transform.ky == 0.0 && transform.sx > 0.0 && transform.sy > 0.0
}

/// Yapfast port (patch 30): sets the mask to a picture's own `clip` when it
/// draws through the mask (`masked`), spills out of that clip, and the clip
/// is narrower than the layer's (`layer`, what the mask holds now). Returns
/// whether it did, so the caller puts the layer's back after drawing.
#[cfg(any(feature = "image", feature = "svg"))]
fn narrowed_mask(
    clip_mask: &mut tiny_skia::Mask,
    physical_bounds: &Rectangle,
    clip: Rectangle,
    layer: Rectangle,
    masked: bool,
) -> bool {
    let narrowed = masked && clip != layer && !physical_bounds.is_within(&clip);
    if narrowed {
        adjust_clip_mask(clip_mask, clip);
    }
    narrowed
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

    // A new mask can take a freed one's place in memory, at the same size
    // (`screenshot` makes one each time): it's all clear, so the rectangle
    // remembered for that place is only taken as set when its first pixel
    // is.
    let set = |[x0, y0, x1, y1]: [usize; 4]| x0 >= x1 || y0 >= y1 || data[y0 * width + x0] == 255;

    match last {
        Some((address, length, last)) if (address, length) == identity && set(last) => {
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
/// a popup's, a card's, with room for the sizes they take as they change.
const SHADOWS_KEPT: usize = 8;

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
    /// Patch 34: whether the quad's own shape is cut out of it.
    cut: bool,
}

impl ShadowKey {
    fn new(
        bounds: Rectangle,
        origin: (f32, f32),
        pixels: (f32, f32),
        radii: &[f32],
        shadow: crate::core::Shadow,
        scale: f32,
        cut: bool,
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
            cut,
        }
    }
}

/// Yapfast port (patch 18): one axis of a shadow's reach, as its slices
/// keep it. Its pixels from `start` to `end`, both in, lie along the
/// straight stretch of the shadow's edges, where every pixel across is the
/// same, and are kept as the one at `start`. Pixels are counted from the
/// reach's corner.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slices {
    start: u32,
    end: u32,
}

impl Slices {
    /// Along an axis of `pixels` from `origin`, of a box from `near` to
    /// `far` (offset included) whose corners take `corner` at most.
    fn new(origin: f32, pixels: u32, near: f32, far: f32, corner: f32) -> Self {
        // A pixel at `origin + i` is straight across when it's `corner` or
        // more inside both ends.
        let start = (near + corner - origin).ceil().max(0.0);
        let end = (far - corner - origin).floor().min(pixels as f32 - 1.0);
        if end > start {
            Self {
                start: start as u32,
                end: end as u32,
            }
        } else {
            Self { start: 0, end: 0 }
        }
    }

    /// Patch 34: the stretch straight in both.
    fn and(self, other: Self) -> Self {
        let (start, end) = (self.start.max(other.start), self.end.min(other.end));
        if end > start { Self { start, end } } else { Self { start: 0, end: 0 } }
    }

    /// How many pixels are kept of `pixels`.
    fn kept(self, pixels: u32) -> u32 {
        pixels - (self.end - self.start)
    }

    /// The pixel of the reach a kept one stands for.
    fn whole(self, kept: u32) -> u32 {
        if kept <= self.start {
            kept
        } else {
            kept + (self.end - self.start)
        }
    }

    /// The kept pixel that stands for a pixel of the reach.
    fn kept_at(self, whole: u32) -> u32 {
        if whole < self.start {
            whole
        } else if whole <= self.end {
            self.start
        } else {
            whole - (self.end - self.start)
        }
    }
}

/// Yapfast port (patches 17 and 18): a shadow's pixels, premultiplied in
/// the frame's byte order, as kept: its nine slices (whole when it has no
/// straight stretch).
#[derive(Debug)]
struct Sliced {
    pixels: Vec<u32>,
    width: u32,
    x: Slices,
    y: Slices,
}

impl Sliced {
    /// Works the shadow out over its reach, `width` by `height` pixels
    /// from `origin`, keeping one pixel of each straight stretch; when
    /// `cut`, only what lies outside the quad's rounded shape (patch 34).
    fn new(
        physical_bounds: Rectangle,
        origin: (f32, f32),
        (width, height): (u32, u32),
        radii: &[f32],
        shadow: crate::core::Shadow,
        transformation: Transformation,
        cut: bool,
    ) -> Option<Self> {
        let scale = transformation.scale_factor();
        let corner = radii.iter().fold(0.0f32, |most, radius| most.max(*radius));
        let (offset_x, offset_y) = (shadow.offset.x * scale, shadow.offset.y * scale);
        let x = Slices::new(
            origin.0,
            width,
            physical_bounds.x + offset_x,
            physical_bounds.x + offset_x + physical_bounds.width,
            corner,
        );
        let y = Slices::new(
            origin.1,
            height,
            physical_bounds.y + offset_y,
            physical_bounds.y + offset_y + physical_bounds.height,
            corner,
        );
        // Patch 34: the quad's coverage, sampled at pixel centres, is the
        // same across where a centre lies `corner` and half a pixel inside
        // both its ends: from `near` to `far - 1` as `Slices` counts.
        let (x, y) = if cut {
            (
                x.and(Slices::new(
                    origin.0,
                    width,
                    physical_bounds.x,
                    physical_bounds.x + physical_bounds.width - 1.0,
                    corner,
                )),
                y.and(Slices::new(
                    origin.1,
                    height,
                    physical_bounds.y,
                    physical_bounds.y + physical_bounds.height - 1.0,
                    corner,
                )),
            )
        } else {
            (x, y)
        };
        let (kept_width, kept_height) = (x.kept(width), y.kept(height));
        if kept_width == 0 || kept_height == 0 {
            return None;
        }
        let half_width = physical_bounds.width / 2.0;
        let half_height = physical_bounds.height / 2.0;
        let size = tiny_skia::Size::from_wh(half_width, half_height)?;
        let color = into_color(shadow.color);
        let mut pixels = Vec::with_capacity((kept_width * kept_height) as usize);
        for row in 0..kept_height {
            let at_y = origin.1 + y.whole(row) as f32;
            for column in 0..kept_width {
                let at_x = origin.0 + x.whole(column) as f32;
                let shadow_distance = rounded_box_sdf(
                    Vector::new(
                        at_x - physical_bounds.position().x - offset_x - half_width,
                        at_y - physical_bounds.position().y - offset_y - half_height,
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
                // Patch 34: none where the quad covers the pixel.
                let shadow_alpha = if cut {
                    let distance = rounded_box_distance(
                        Vector::new(
                            at_x + 0.5 - physical_bounds.position().x - half_width,
                            at_y + 0.5 - physical_bounds.position().y - half_height,
                        ),
                        size,
                        radii,
                    );
                    shadow_alpha * (1.0 - (0.5 - distance).clamp(0.0, 1.0))
                } else {
                    shadow_alpha
                };
                let mut color = color;
                color.apply_opacity(shadow_alpha);
                let pixel = color.to_color_u8().premultiply();
                pixels.push(u32::from_le_bytes([
                    pixel.red(),
                    pixel.green(),
                    pixel.blue(),
                    pixel.alpha(),
                ]));
            }
        }
        Some(Self {
            pixels,
            width: kept_width,
            x,
            y,
        })
    }

    /// Lays the shadow, its reach's corner at `origin`, over `band` (whole
    /// pixels inside the frame), source over.
    fn draw(&self, origin: (i32, i32), band: Rectangle, pixels: &mut tiny_skia::PixmapMut<'_>) {
        let (frame_width, frame_height) = (pixels.width() as i32, pixels.height() as i32);
        let left = (band.x as i32).max(origin.0).max(0);
        let top = (band.y as i32).max(origin.1).max(0);
        let right = ((band.x + band.width) as i32).min(frame_width);
        let bottom = ((band.y + band.height) as i32).min(frame_height);
        // Yapfast port (patch 37): every kept pixel plus the stretch the
        // straight one stands for.
        // (The last kept pixel's place undercounts when the stretch runs
        // to the reach's edge: a hard shadow on square corners, whose
        // reach is all stretch, drew one pixel of itself.)
        let reach_width = (self.width + (self.x.end - self.x.start)) as i32;
        let reach_height =
            (self.pixels.len() as u32 / self.width + (self.y.end - self.y.start)) as i32;
        let right = right.min(origin.0 + reach_width);
        let bottom = bottom.min(origin.1 + reach_height);
        if left >= right || top >= bottom {
            return;
        }
        let stride = frame_width as usize;
        let frame: &mut [u32] = bytemuck::cast_slice_mut(pixels.data_mut());
        let (first, last) = ((left - origin.0) as u32, (right - origin.0) as u32);
        // The run of the row before the straight stretch, along it, after it.
        let straight = (
            first.max(self.x.start).min(last),
            (self.x.end + 1).max(first).min(last),
        );
        let straight = if self.x.end > self.x.start {
            straight
        } else {
            (last, last)
        };
        for row in top..bottom {
            let kept_row = self.y.kept_at((row - origin.1) as u32) as usize;
            let source = &self.pixels[kept_row * self.width as usize..][..self.width as usize];
            let target = &mut frame[row as usize * stride..][..stride];
            let mut at = first;
            while at < last {
                if at == straight.0 && straight.1 > straight.0 {
                    let pixel = source[self.x.start as usize];
                    let from = (origin.0 + at as i32) as usize;
                    let to = (origin.0 + straight.1 as i32) as usize;
                    for below in &mut target[from..to] {
                        *below = crate::blit::over(pixel, *below);
                    }
                    at = straight.1;
                    continue;
                }
                let target = &mut target[(origin.0 + at as i32) as usize];
                *target = crate::blit::over(source[self.x.kept_at(at) as usize], *target);
                at += 1;
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pixel of a shadow's reach, worked out one by one as stock iced
    /// does, then laid over a dark ground, times one less the see-through
    /// quad's own coverage of the pixel (patch 34, as iced_wgpu's shader):
    /// the signed distance from the pixel's centre to its rounded outline,
    /// worked out here on its own.
    fn whole_shadow(bounds: Rectangle, radii: [f32; 4], shadow: crate::core::Shadow, size: (u32, u32)) -> Vec<u32> {
        let (half_width, half_height) = (bounds.width / 2.0, bounds.height / 2.0);
        let coverage = |x: f32, y: f32| {
            let (dx, dy) = (x + 0.5 - bounds.x - half_width, y + 0.5 - bounds.y - half_height);
            let radius = radii[match (dx > 0.0, dy > 0.0) {
                (false, false) => 0,
                (true, false) => 1,
                (true, true) => 2,
                (false, true) => 3,
            }];
            // How far past the box inset by the radius, on each axis.
            let (past_x, past_y) = (dx.abs() - half_width + radius, dy.abs() - half_height + radius);
            let distance = if past_x > 0.0 && past_y > 0.0 {
                past_x.hypot(past_y) - radius
            } else {
                past_x.max(past_y) - radius
            };
            (0.5 - distance).clamp(0.0, 1.0)
        };
        let quad = Quad { bounds, border: crate::core::Border::default(), shadow, ..Quad::default() };
        let reach = shadow_reach(&quad).expect("a shadow");
        let origin = (reach.x.floor(), reach.y.floor());
        let ground = 0xff20_1d1c_u32;
        let mut frame = vec![ground; (size.0 * size.1) as usize];
        let half = tiny_skia::Size::from_wh(half_width, half_height).expect("size");
        for y in 0..size.1 {
            for x in 0..size.0 {
                let (at_x, at_y) = (x as f32, y as f32);
                if at_x < origin.0
                    || at_y < origin.1
                    || at_x >= (reach.x + reach.width).ceil()
                    || at_y >= (reach.y + reach.height).ceil()
                {
                    continue;
                }
                let distance = rounded_box_sdf(
                    Vector::new(
                        at_x - bounds.x - shadow.offset.x - half_width,
                        at_y - bounds.y - shadow.offset.y - half_height,
                    ),
                    half,
                    &radii,
                )
                .max(0.0);
                let alpha = 1.0 - smoothstep(-shadow.blur_radius, shadow.blur_radius, distance);
                let alpha = alpha * (1.0 - coverage(at_x, at_y));
                let mut color = into_color(shadow.color);
                color.apply_opacity(alpha);
                let pixel = color.to_color_u8().premultiply();
                let pixel = u32::from_le_bytes([pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]);
                let at = (y * size.0 + x) as usize;
                frame[at] = crate::blit::over(pixel, frame[at]);
            }
        }
        frame
    }

    /// The same drawn by the engine, through its kept slices, in bands.
    fn drawn_shadow(bounds: Rectangle, radii: [f32; 4], shadow: crate::core::Shadow, size: (u32, u32)) -> (Vec<u32>, usize) {
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let mut frame = vec![ground; (size.0 * size.1) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size.0, size.1).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
        let quad = Quad {
            bounds,
            border: crate::core::Border { radius: crate::core::border::Radius { top_left: radii[0], top_right: radii[1], bottom_right: radii[2], bottom_left: radii[3] }, ..Default::default() },
            shadow,
            ..Quad::default()
        };
        // A see-through fill: the shadow shows under all of it.
        let background = Background::Color(Color::TRANSPARENT);
        // In two halves, as two damaged regions would draw it.
        let whole = Rectangle::new(crate::core::Point::ORIGIN, Size::new(size.0 as f32, size.1 as f32));
        let top = Rectangle { height: (size.1 / 2) as f32, ..whole };
        let rest = Rectangle { y: top.height, height: whole.height - top.height, ..whole };
        for clip in [top, rest] {
            engine.draw_quad(&quad, &background, Transformation::IDENTITY, &mut pixmap, &mut mask, clip);
        }
        let kept = engine.shadows.iter().map(|(_, sliced)| sliced.pixels.len()).sum();
        (frame, kept)
    }

    #[test]
    fn a_quad_scaled_off_whole_pixels_is_put_on_them_edge_by_edge() {
        let at = |x, y, width, height| Rectangle { x, y, width, height };
        // At 1 only whole pixels are taken.
        assert_eq!(whole_pixels(at(10.0, 20.0, 30.0, 40.0), 1.0), Some((10, 20, 30, 40)));
        assert_eq!(whole_pixels(at(10.5, 20.0, 30.0, 40.0), 1.0), None);
        // At 1.1, each edge goes to its nearest pixel's, so two quads that
        // meet still meet.
        let left = at(11.0, 22.0, 27.5, 13.2) * Transformation::scale(1.1);
        let right = at(38.5, 22.0, 20.0, 13.2) * Transformation::scale(1.1);
        let (x, _, width, _) = whole_pixels(left, 1.1).expect("whole");
        let (next, _, _, _) = whole_pixels(right, 1.1).expect("whole");
        assert_eq!((x, x + width), (12, 42));
        assert_eq!(next, x + width);
        // Less than a pixel across rounds to nothing.
        assert_eq!(whole_pixels(at(10.1, 0.0, 0.2, 5.0), 1.25), None);
    }

    #[test]
    fn an_outline_of_no_fill_draws_its_edge_and_leaves_what_it_rounds() {
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let size = (120, 80);
        let mut frame = vec![ground; (size.0 * size.1) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size.0, size.1).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
        let quad = Quad {
            bounds: Rectangle { x: 10.0, y: 10.0, width: 100.0, height: 60.0 },
            border: crate::core::Border { color: Color::WHITE, width: 1.0, radius: 8.0.into() },
            ..Quad::default()
        };
        // Clipped to part of it, as a damaged region inside a card is.
        let clip = Rectangle { x: 0.0, y: 20.0, width: 120.0, height: 40.0 };
        adjust_clip_mask(&mut mask, clip);
        engine.draw_quad(&quad, &Background::Color(Color::TRANSPARENT), Transformation::IDENTITY, &mut pixmap, &mut mask, clip);
        let at = |x: u32, y: u32| frame[(y * size.0 + x) as usize];
        assert_eq!(at(60, 40), ground, "inside");
        assert_eq!(at(60, 10), ground, "outside the clip");
        assert_eq!(at(10, 40), 0xffff_ffff, "its left edge");
        assert_eq!(at(109, 40), 0xffff_ffff, "its right edge");
    }

    /// Draws `image` cut to its own clip, (30, 30) 40 by 40, in a layer
    /// clipped to the whole 100 by 100 frame, and says whether any pixel
    /// outside its own clip changed and whether any inside it did.
    #[cfg(any(feature = "image", feature = "svg"))]
    fn spills_and_shows(image: &Image) -> (bool, bool) {
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let size = 100u32;
        let mut frame = vec![ground; (size * size) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size, size).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size, size).expect("mask");
        let layer = Rectangle::new(crate::core::Point::ORIGIN, Size::new(size as f32, size as f32));
        adjust_clip_mask(&mut mask, layer);
        engine.draw_image(image, Transformation::IDENTITY, &mut pixmap, &mut mask, layer);
        let own = |x: u32, y: u32| (30..70).contains(&x) && (30..70).contains(&y);
        let changed = |inside: bool| {
            (0..size).any(|y| (0..size).any(|x| own(x, y) == inside && frame[(y * size + x) as usize] != ground))
        };
        let result = (changed(false), changed(true));
        // The mask is the layer's again for what draws after.
        let mut whole = tiny_skia::Mask::new(size, size).expect("mask");
        whole.data_mut().fill(255);
        assert_eq!(mask.data(), whole.data(), "the layer's clip put back");
        result
    }

    #[cfg(feature = "image")]
    #[test]
    fn a_turned_picture_is_cut_to_its_own_clip() {
        let own = Rectangle::new(crate::core::Point::new(30.0, 30.0), Size::new(40.0, 40.0));
        let bounds = Rectangle::new(crate::core::Point::new(20.0, 20.0), Size::new(60.0, 60.0));
        for rotation in [0.0, 0.3] {
            let image = Image::Raster {
                image: crate::core::image::Image {
                    rotation: crate::core::Radians(rotation),
                    ..crate::core::image::Image::new(crate::core::image::Handle::from_rgba(60, 60, vec![255u8; 60 * 60 * 4]))
                },
                bounds,
                clip_bounds: own,
            };
            assert_eq!(spills_and_shows(&image), (false, true), "turned {rotation}");
        }
    }

    #[cfg(feature = "image")]
    #[test]
    fn a_round_picture_at_twice_the_scale_has_corners_twice_as_round() {
        // Patch 39: 40 by 40 at (10, 10) with 8 px corners, at scale 2: an
        // 80 px box at (20, 20) with 16 px corners, not 32 px ones.
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let size = 120u32;
        let mut frame = vec![ground; (size * size) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size, size).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size, size).expect("mask");
        let layer = Rectangle::new(crate::core::Point::ORIGIN, Size::new(size as f32, size as f32));
        adjust_clip_mask(&mut mask, layer);
        let bounds = Rectangle::new(crate::core::Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let image = Image::Raster {
            image: crate::core::image::Image {
                border_radius: 8.0.into(),
                ..crate::core::image::Image::new(crate::core::image::Handle::from_rgba(80, 80, vec![255u8; 80 * 80 * 4]))
            },
            bounds,
            clip_bounds: bounds,
        };
        engine.draw_image(&image, Transformation::scale(2.0), &mut pixmap, &mut mask, layer);
        let shown = |x: u32, y: u32| frame[(y * size + x) as usize] != ground;
        // 13 px from the 16 px corner's centre: inside it, outside a 32 px one.
        assert!(shown(27, 27), "inside a 16 px corner");
        assert!(!shown(21, 21), "outside any corner");
        assert!(shown(60, 20) && shown(20, 60) && shown(99, 60), "its edges");
        assert!(!shown(100, 60) && !shown(60, 100), "nothing past its 80 px");
    }

    #[cfg(feature = "svg")]
    #[test]
    fn an_svg_is_cut_to_its_own_clip() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60"><rect width="60" height="60" fill="#fff"/></svg>"##;
        let image = Image::Vector {
            svg: crate::core::svg::Svg::new(crate::core::svg::Handle::from_memory(svg.as_slice())),
            bounds: Rectangle::new(crate::core::Point::new(20.0, 20.0), Size::new(60.0, 60.0)),
            clip_bounds: Rectangle::new(crate::core::Point::new(30.0, 30.0), Size::new(40.0, 40.0)),
        };
        assert_eq!(spills_and_shows(&image), (false, true));
    }

    #[test]
    fn a_quad_of_no_area_draws_nothing() {
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let size = (60, 40);
        let mut frame = vec![ground; (size.0 * size.1) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size.0, size.1).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size.0, size.1).expect("mask");
        // Clipped, so it can't take the plain fill: square corners, a
        // border and a shadow, on half pixels (a card's empty banner).
        let clip = Rectangle { x: 0.0, y: 0.0, width: 50.0, height: 30.0 };
        adjust_clip_mask(&mut mask, clip);
        for bounds in [
            Rectangle { x: 10.5, y: 10.5, width: 30.0, height: 0.0 },
            Rectangle { x: 10.5, y: 10.5, width: 0.0, height: 20.0 },
            // Less than none, as a layout squeezed below nothing gives.
            Rectangle { x: 10.5, y: 10.5, width: -2.0, height: 20.0 },
        ] {
            let quad = Quad {
                bounds,
                border: crate::core::Border { color: Color::WHITE, width: 1.0, radius: 0.0.into() },
                shadow: crate::core::Shadow { color: Color::BLACK, offset: Vector::new(2.0, 2.0), blur_radius: 0.0 },
                ..Quad::default()
            };
            // Filled, an outline alone (patch 21), and a gradient off whole
            // pixels, each taking its own path.
            let gradient = Background::Gradient(Gradient::Linear(
                crate::core::gradient::Linear::new(crate::core::Radians(0.0))
                    .add_stop(0.0, Color::WHITE)
                    .add_stop(1.0, Color::BLACK),
            ));
            for background in [Background::Color(Color::WHITE), Background::Color(Color::TRANSPARENT), gradient] {
                engine.draw_quad(&quad, &background, Transformation::IDENTITY, &mut pixmap, &mut mask, clip);
            }
        }
        assert!(frame.iter().all(|&pixel| pixel == ground));
    }

    #[test]
    fn a_large_shadow_drawn_from_its_slices_is_the_shadow_worked_out_whole() {
        let shadow = crate::core::Shadow {
            color: Color { a: 0.5, ..Color::BLACK },
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        };
        // A card on whole pixels, and one between them, with uneven corners.
        for (bounds, radii) in [
            (Rectangle::new(crate::core::Point::new(60.0, 50.0), Size::new(600.0, 400.0)), [12.0; 4]),
            (Rectangle::new(crate::core::Point::new(60.3, 49.6), Size::new(580.5, 390.25)), [16.0, 4.0, 0.0, 8.0]),
        ] {
            let size = (720, 520);
            let (drawn, kept) = drawn_shadow(bounds, radii, shadow, size);
            let whole = whole_shadow(bounds, radii, shadow, size);
            let differing = drawn.iter().zip(&whole).filter(|(a, b)| a != b).count();
            assert_eq!(differing, 0, "{bounds:?}: {differing} pixels differ");
            // Its corners and one row and column across, not its reach.
            assert!(kept < 120 * 120, "{kept} pixels kept");
        }
    }

    #[test]
    fn a_small_shadow_is_kept_whole_and_drawn_the_same() {
        let shadow = crate::core::Shadow {
            color: Color { a: 0.35, ..Color::BLACK },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 10.0,
        };
        let bounds = Rectangle::new(crate::core::Point::new(20.0, 20.0), Size::new(30.0, 20.0));
        let (drawn, _) = drawn_shadow(bounds, [8.0; 4], shadow, (80, 70));
        assert_eq!(drawn, whole_shadow(bounds, [8.0; 4], shadow, (80, 70)));
    }

    #[test]
    fn a_see_through_quad_shows_no_shadow_inside_its_rounded_edge() {
        // Patch 34: the band between the body (inside the 12 px corners)
        // and the outline is under the quad too.
        let shadow = crate::core::Shadow { color: Color { a: 0.5, ..Color::BLACK }, offset: Vector::new(0.0, 0.0), blur_radius: 16.0 };
        let bounds = Rectangle::new(crate::core::Point::new(40.0, 40.0), Size::new(200.0, 120.0));
        let size = (280, 200);
        let (drawn, _) = drawn_shadow(bounds, [12.0; 4], shadow, size);
        let ground = 0xff20_1d1c_u32;
        let at = |x: u32, y: u32| drawn[(y * size.0 + x) as usize];
        for (x, y) in [(41, 100), (51, 100), (140, 41), (238, 100), (140, 158)] {
            assert_eq!(at(x, y), ground, "({x}, {y}), inside the edge");
        }
        // Outside, it's there.
        assert_ne!(at(38, 100), ground);
        assert_eq!(drawn, whole_shadow(bounds, [12.0; 4], shadow, size));
    }

    #[test]
    fn a_hard_shadow_steps_at_its_edge_and_is_drawn_whole() {
        // Patch 35: no blur, as themes' offset shadows are. Patch 37: on
        // square corners its reach is all straight stretch.
        let shadow = crate::core::Shadow { color: Color::BLACK, offset: Vector::new(4.0, 4.0), blur_radius: 0.0 };
        let bounds = Rectangle::new(crate::core::Point::new(20.0, 20.0), Size::new(30.0, 20.0));
        let size = (80, 60);
        let ground = 0xff20_1d1c_u32;
        for radii in [[0.0; 4], [6.0; 4]] {
            let (drawn, _) = drawn_shadow(bounds, radii, shadow, size);
            let at = |x: u32, y: u32| drawn[(y * size.0 + x) as usize];
            // Right of the quad and below it, to the reach's last pixel.
            for (x, y) in [(50, 30), (53, 30), (35, 40), (35, 43)] {
                assert_eq!(at(x, y), 0xff00_0000, "{radii:?}: ({x}, {y}) in the shadow");
            }
            // Past the reach, and under the see-through quad.
            for (x, y) in [(54, 30), (35, 44), (30, 30), (45, 35)] {
                assert_eq!(at(x, y), ground, "{radii:?}: ({x}, {y}) clear");
            }
            assert_eq!(drawn, whole_shadow(bounds, radii, shadow, size), "{radii:?}");
        }
    }

    #[cfg(feature = "image")]
    #[test]
    fn a_turned_picture_stretched_far_lands_on_its_bounds() {
        // Patch 36: a 4 px picture turned half round and stretched to 40
        // px at x = 15. Stock placed it at 15 / 10 cut to a whole picture
        // pixel, 1, times 10: at x = 10.
        let mut engine = Engine::new();
        let ground = 0xff20_1d1c_u32;
        let size = 80u32;
        let mut frame = vec![ground; (size * size) as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), size, size).expect("pixmap");
        let mut mask = tiny_skia::Mask::new(size, size).expect("mask");
        let layer = Rectangle::new(crate::core::Point::ORIGIN, Size::new(size as f32, size as f32));
        adjust_clip_mask(&mut mask, layer);
        let image = Image::Raster {
            image: crate::core::image::Image {
                rotation: crate::core::Radians(std::f32::consts::PI),
                ..crate::core::image::Image::new(crate::core::image::Handle::from_rgba(4, 4, vec![255u8; 4 * 4 * 4]))
            },
            bounds: Rectangle::new(crate::core::Point::new(15.0, 20.0), Size::new(40.0, 40.0)),
            clip_bounds: layer,
        };
        engine.draw_image(&image, Transformation::IDENTITY, &mut pixmap, &mut mask, layer);
        let at = |x: u32, y: u32| frame[(y * size + x) as usize];
        for x in [16, 35, 53] {
            assert_eq!(at(x, 40), 0xffff_ffff, "({x}, 40) in the picture");
        }
        for x in [11, 13, 57] {
            assert_eq!(at(x, 40), ground, "({x}, 40) outside it");
        }
    }
}
