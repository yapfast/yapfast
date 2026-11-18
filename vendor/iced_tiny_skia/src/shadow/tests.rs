//! A shadow laid from its slices against stock's, worked out whole.

use super::*;
use crate::core::{Color, Point, Size};

/// Stock's shadow of a quad at `bounds` (logical), as iced's master drew it:
/// every pixel of its reach worked out into a pixmap, laid with
/// `draw_pixmap` through a mask of `clip`.
fn stock(
    frame: &mut [u32],
    (width, height): (u32, u32),
    bounds: Rectangle,
    radii: [f32; 4],
    shadow: Shadow,
    transformation: Transformation,
    clip: Rectangle,
) {
    let physical_bounds = bounds * transformation;
    let shadow_bounds = reach(bounds, shadow) * transformation;
    let radii: Vec<f32> = radii
        .into_iter()
        .map(|radius| radius * transformation.scale_factor())
        .collect();
    let (x, y, w, h) = (
        shadow_bounds.x as u32,
        shadow_bounds.y as u32,
        shadow_bounds.width as u32,
        shadow_bounds.height as u32,
    );
    let half_width = physical_bounds.width / 2.0;
    let half_height = physical_bounds.height / 2.0;
    let scale = transformation.scale_factor();
    let colors: Vec<tiny_skia::PremultipliedColorU8> = (y..y + h)
        .flat_map(|y| (x..x + w).map(move |x| (x as f32, y as f32)))
        .filter_map(|(x, y)| {
            tiny_skia::Size::from_wh(half_width, half_height).map(|size| {
                let distance = rounded_box_sdf(
                    Vector::new(
                        x - physical_bounds.position().x - (shadow.offset.x * scale) - half_width,
                        y - physical_bounds.position().y - (shadow.offset.y * scale) - half_height,
                    ),
                    size,
                    &radii,
                )
                .max(0.0);
                let alpha = 1.0
                    - smoothstep(
                        -shadow.blur_radius * scale,
                        shadow.blur_radius * scale,
                        distance,
                    );
                let mut color = into_color(shadow.color);
                color.apply_opacity(alpha);
                color.to_color_u8().premultiply()
            })
        })
        .collect();
    let mut mask = tiny_skia::Mask::new(width, height).expect("mask");
    crate::clip::Clip::default().set(&mut mask, clip);
    if let Some(pixmap) = tiny_skia::IntSize::from_wh(w, h)
        .and_then(|size| tiny_skia::Pixmap::from_vec(bytemuck::cast_vec(colors), size))
    {
        tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(frame), width, height)
            .expect("frame")
            .draw_pixmap(
                x as i32,
                y as i32,
                pixmap.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::default(),
                Some(&mask),
            );
    }
}

fn ground(width: u32, height: u32) -> Vec<u32> {
    (0..width * height)
        .map(|at| 0xff00_0000 | at.wrapping_mul(2_654_435_761) >> 8)
        .collect()
}

#[test]
fn a_shadow_comes_out_as_stock_drew_it() {
    let (width, height) = (160, 120);
    let frame = ground(width, height);
    let shadows = [
        (8.0, 24.0, 0.0, [0.0; 4]),
        (16.0, 4.0, 2.0, [6.0; 4]),
        (12.5, 10.0, -3.0, [12.0, 0.0, 4.0, 20.0]),
        (3.0, 0.5, 1.5, [100.0; 4]),
    ];
    let bounds = [
        Rectangle::new(Point::new(30.0, 20.0), Size::new(70.0, 50.0)),
        Rectangle::new(Point::new(20.25, 13.5), Size::new(41.3, 37.7)),
        Rectangle::new(Point::new(-10.0, 60.0), Size::new(200.0, 30.0)),
    ];
    let clips = [
        Rectangle::new(Point::ORIGIN, Size::new(160.0, 120.0)),
        Rectangle::new(Point::new(25.5, 10.2), Size::new(60.0, 41.6)),
    ];
    for (blur, offset_y, offset_x, radii) in shadows {
        let shadow = Shadow {
            color: Color::from_rgba(0.1, 0.0, 0.2, 0.6),
            offset: Vector::new(offset_x, offset_y),
            blur_radius: blur,
        };
        for bounds in bounds {
            let radii =
                radii.map(|radius: f32| radius.min(bounds.width / 2.0).min(bounds.height / 2.0));
            for scale in [1.0, 2.0] {
                let transformation = Transformation::scale(scale);
                for clip in clips {
                    let clip = clip * Transformation::scale(scale / 2.0);
                    let mut ours = frame.clone();
                    let mut theirs = frame.clone();
                    // Twice, the second from what's kept.
                    let mut shadows = Shadows::default();
                    for _ in 0..2 {
                        ours.clone_from(&frame);
                        shadows.draw(
                            bounds,
                            radii.map(|radius| radius * scale),
                            shadow,
                            transformation,
                            clip,
                            None,
                            &mut tiny_skia::PixmapMut::from_bytes(
                                bytemuck::cast_slice_mut(&mut ours),
                                width,
                                height,
                            )
                            .expect("frame"),
                        );
                    }
                    stock(
                        &mut theirs,
                        (width, height),
                        bounds,
                        radii,
                        shadow,
                        transformation,
                        clip,
                    );
                    assert!(
                        ours == theirs,
                        "blur {blur}, offset {offset_x},{offset_y}, {radii:?}, {bounds:?} x{scale}, {clip:?}"
                    );
                    assert_eq!(shadows.kept.len(), 1, "worked out once");
                }
            }
        }
    }
}

#[test]
fn a_long_shadow_keeps_one_pixel_of_each_straight_stretch() {
    let mut shadows = Shadows::default();
    let (width, height) = (1600, 900);
    let mut frame = vec![0xff00_0000_u32; (width * height) as usize];
    let bounds = Rectangle::new(Point::new(100.0, 100.0), Size::new(1400.0, 700.0));
    let shadow = Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
        offset: Vector::new(0.0, 8.0),
        blur_radius: 24.0,
    };
    shadows.draw(
        bounds,
        [8.0; 4],
        shadow,
        Transformation::IDENTITY,
        Rectangle::new(Point::ORIGIN, Size::new(width as f32, height as f32)),
        Some((108, 108, 1492, 792)),
        &mut tiny_skia::PixmapMut::from_bytes(bytemuck::cast_slice_mut(&mut frame), width, height)
            .expect("frame"),
    );
    let sliced = &shadows.kept[0].1;
    assert!(
        sliced.pixels.len() < 120 * 120,
        "{} kept",
        sliced.pixels.len()
    );
    // Under the body, nothing was laid.
    assert_eq!(frame[(400 * width + 800) as usize], 0xff00_0000_u32);
}
