use crate::core::backend;
use crate::core::renderer;
use crate::core::{Color, Rectangle, Size};
use crate::graphics::compositor::{self, Information};
use crate::graphics::damage;
use crate::graphics::{Shell, Viewport};
use crate::{Layer, Renderer};

use std::collections::VecDeque;
use std::num::NonZeroU32;

pub struct Compositor {
    context: softbuffer::Context<Box<dyn compositor::Display>>,
}

pub struct Surface {
    window: softbuffer::Surface<Box<dyn compositor::Display>, Box<dyn compositor::Window>>,
    clip_mask: tiny_skia::Mask,
    frames: VecDeque<Frame>,
    max_age: u8,
}

#[derive(Clone)]
struct Frame {
    background: Color,
    layers: Vec<Layer>,
}

impl crate::graphics::Compositor for Compositor {
    type Renderer = Renderer;
    type Surface = Surface;

    async fn new(
        settings: backend::Settings,
        display: impl compositor::Display,
        _compatible_window: impl compositor::Window,
        _shell: Shell,
    ) -> Result<Self, backend::Error> {
        if !settings.backend.is_software() && !settings.backend.matches("tiny-skia") {
            return Err(backend::Error::GraphicsAdapterNotFound {
                backend: "tiny-skia",
                reason: backend::Reason::DidNotMatch {
                    preferred_backend: settings.backend,
                },
            });
        }

        Ok(new(display))
    }

    fn create_renderer(&self, settings: renderer::Settings) -> Self::Renderer {
        Renderer::new(settings)
    }

    fn create_surface(&mut self, window: impl compositor::Window + Clone) -> Self::Surface {
        let window = softbuffer::Surface::new(&self.context, Box::new(window.clone()) as _)
            .expect("Create softbuffer surface for window");

        Surface {
            window,
            clip_mask: tiny_skia::Mask::new(1, 1).expect("Create clip mask"),
            frames: VecDeque::new(),
            max_age: 0,
        }
    }

    fn configure_surface(&mut self, surface: &mut Self::Surface, width: u32, height: u32) {
        surface
            .window
            .resize(
                NonZeroU32::new(width).expect("Non-zero width"),
                NonZeroU32::new(height).expect("Non-zero height"),
            )
            .expect("Resize surface");

        surface.clip_mask = tiny_skia::Mask::new(width, height).expect("Create clip mask");
        surface.frames.clear();
    }

    fn information(&self) -> Information {
        Information {
            adapter: String::from("CPU"),
            backend: String::from("tiny-skia"),
        }
    }

    fn present(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &Viewport,
        background_color: Color,
        on_pre_present: impl FnOnce(),
    ) -> Result<(), compositor::SurfaceError> {
        present(
            renderer,
            surface,
            viewport,
            background_color,
            on_pre_present,
        )
    }

    fn screenshot(
        &mut self,
        renderer: &mut Self::Renderer,
        viewport: &Viewport,
        background_color: Color,
    ) -> Vec<u8> {
        screenshot(renderer, viewport, background_color)
    }
}

pub fn new(display: impl compositor::Display) -> Compositor {
    #[allow(unsafe_code)]
    let context =
        softbuffer::Context::new(Box::new(display) as _).expect("Create softbuffer context");

    Compositor { context }
}

pub fn present(
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background: Color,
    on_pre_present: impl FnOnce(),
) -> Result<(), compositor::SurfaceError> {
    let physical_size = viewport.physical_size();

    let mut buffer = surface
        .window
        .buffer_mut()
        .map_err(|_| compositor::SurfaceError::Lost)?;

    let last_frame = {
        let age = buffer.age();

        surface.max_age = surface.max_age.max(age);
        surface.frames.truncate(surface.max_age as usize);

        if age > 0 {
            surface.frames.get(age as usize - 1)
        } else {
            None
        }
    };

    let damage = last_frame
        .and_then(|last_frame| {
            (last_frame.background == background).then(|| {
                damage::diff(
                    &last_frame.layers,
                    renderer.layers(),
                    |layer| vec![layer.bounds],
                    Layer::damage,
                )
            })
        })
        .unwrap_or_else(|| vec![Rectangle::with_size(viewport.logical_size())]);

    // Anomi port (patch 23): the compositor is told what differs from the
    // frame it shows, the one presented last, not the whole window. `damage`
    // is against the buffer's own older frame, which may be another.
    let bounds = Rectangle::with_size(viewport.logical_size());
    let shown = if buffer.age() == 1 {
        Some(damage.clone())
    } else {
        surface
            .frames
            .front()
            .filter(|previous| previous.background == background)
            .map(|previous| {
                damage::diff(
                    &previous.layers,
                    renderer.layers(),
                    |layer| vec![layer.bounds],
                    Layer::damage,
                )
            })
    }
    .map(|shown| physical(&merged(damage::group(shown, bounds)), viewport));

    if damage.is_empty() {
        if let Some(last_frame) = last_frame {
            surface.frames.push_front(last_frame.clone());
        }
    } else {
        surface.frames.push_front(Frame {
            background,
            layers: renderer.layers().to_vec(),
        });

        // Anomi port (patch 9): neighbouring regions merged; see `merged`.
        let damage = merged(damage::group(damage, bounds));

        let mut pixels = tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut buffer),
            physical_size.width,
            physical_size.height,
        )
        .expect("Create pixel map");

        renderer.draw(
            &mut pixels,
            &mut surface.clip_mask,
            viewport,
            &damage,
            background,
        );
    }

    on_pre_present();
    match shown {
        Some(shown) => buffer.present_with_damage(&shown),
        None => buffer.present(),
    }
    .map_err(|_| compositor::SurfaceError::Lost)
}

pub fn screenshot(
    renderer: &mut Renderer,
    viewport: &Viewport,
    background_color: Color,
) -> Vec<u8> {
    let size = viewport.physical_size();

    let mut offscreen_buffer: Vec<u32> = vec![0; size.width as usize * size.height as usize];

    let mut clip_mask = tiny_skia::Mask::new(size.width, size.height).expect("Create clip mask");

    renderer.draw(
        &mut tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut offscreen_buffer),
            size.width,
            size.height,
        )
        .expect("Create offscreen pixel map"),
        &mut clip_mask,
        viewport,
        &[Rectangle::with_size(Size::new(
            size.width as f32,
            size.height as f32,
        ))],
        background_color,
    );

    offscreen_buffer.iter().fold(
        Vec::with_capacity(offscreen_buffer.len() * 4),
        |mut acc, pixel| {
            const A_MASK: u32 = 0xFF_00_00_00;
            const R_MASK: u32 = 0x00_FF_00_00;
            const G_MASK: u32 = 0x00_00_FF_00;
            const B_MASK: u32 = 0x00_00_00_FF;

            let a = ((A_MASK & pixel) >> 24) as u8;
            let r = ((R_MASK & pixel) >> 16) as u8;
            let g = ((G_MASK & pixel) >> 8) as u8;
            let b = (B_MASK & pixel) as u8;

            acc.extend([r, g, b, a]);
            acc
        },
    )
}

/// Anomi port (patch 23): `regions`, in logical pixels, as the buffer's
/// pixels they touch, within it.
fn physical(regions: &[Rectangle], viewport: &Viewport) -> Vec<softbuffer::Rect> {
    let scale = viewport.scale_factor();
    let size = viewport.physical_size();
    regions
        .iter()
        .filter_map(|region| {
            let x = ((region.x * scale).floor().max(0.0) as u32).min(size.width);
            let y = ((region.y * scale).floor().max(0.0) as u32).min(size.height);
            let right =
                (((region.x + region.width) * scale).ceil().max(0.0) as u32).min(size.width);
            let bottom =
                (((region.y + region.height) * scale).ceil().max(0.0) as u32).min(size.height);
            Some(softbuffer::Rect {
                x,
                y,
                width: NonZeroU32::new(right.saturating_sub(x))?,
                height: NonZeroU32::new(bottom.saturating_sub(y))?,
            })
        })
        .collect()
}

/// Anomi port (patch 9): the damaged regions, with any two merged whose
/// union adds no more than iced's own threshold (20,000 px) beyond what they
/// cover together. `damage::group` merges each region only into the one
/// before it, in order of distance from the window's corner, so in a
/// scrolled list the regions of neighbouring rows stayed apart and
/// overlapped; each region is drawn whole, its background and all, so the
/// overlaps were drawn twice or more.
fn merged(mut regions: Vec<Rectangle>) -> Vec<Rectangle> {
    const AREA_THRESHOLD: f32 = 20_000.0;

    let mut index = 0;
    while index < regions.len() {
        let current = regions[index];
        let partner = regions
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, other)| {
                let overlap = current
                    .intersection(other)
                    .map_or(0.0, |overlap| overlap.area());
                current.union(other).area() - (current.area() + other.area() - overlap)
                    <= AREA_THRESHOLD
            })
            .map(|(other, _)| other);

        if let Some(other) = partner {
            regions[index] = current.union(&regions.swap_remove(other));
            // The grown region may now take in ones already passed.
            index = 0;
        } else {
            index += 1;
        }
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Point;

    fn rect(rect: &softbuffer::Rect) -> (u32, u32, u32, u32) {
        (rect.x, rect.y, rect.width.get(), rect.height.get())
    }

    #[test]
    fn damage_is_told_in_the_buffers_pixels_within_it() {
        let scale = renderer::Scale {
            window: 1.5,
            application: 1.0,
        };
        let viewport = Viewport::with_physical_size(Size::new(300, 200), scale);
        let regions = [
            // A fractional region takes in every pixel it touches.
            Rectangle::new(Point::new(10.2, 20.0), Size::new(10.0, 4.6)),
            // Past the edge, it stops at it.
            Rectangle::new(Point::new(-5.0, 120.0), Size::new(50.0, 50.0)),
            // Outside, or of no size, it goes.
            Rectangle::new(Point::new(400.0, 10.0), Size::new(10.0, 10.0)),
            Rectangle::new(Point::new(10.0, 10.0), Size::new(0.0, 10.0)),
        ];
        let shown: Vec<_> = physical(&regions, &viewport).iter().map(rect).collect();
        assert_eq!(shown, [(15, 30, 16, 7), (0, 180, 68, 20)]);
    }

    #[test]
    fn the_rows_of_two_scrolled_lists_merge_into_one_region_each() {
        // Each row of a list scrolled by a few pixels damages where it was
        // and where it is, so neighbouring regions overlap. `damage::group`
        // takes them by distance from the window's corner, and these two
        // lists' rows come in turn, so it keeps them apart.
        let column = |x: f32, top: f32| {
            (0..10).map(move |row| {
                Rectangle::new(
                    Point::new(x, top + row as f32 * 44.0),
                    Size::new(200.0, 48.0),
                )
            })
        };
        let rows = column(0.0, 1000.0).chain(column(1000.0, 0.0)).collect();
        let grouped = damage::group(rows, Rectangle::with_size(Size::new(1400.0, 1500.0)));
        assert!(grouped.len() > 2, "{grouped:?}");

        let mut regions = merged(grouped);
        regions.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(
            regions,
            [
                Rectangle::new(Point::new(0.0, 1000.0), Size::new(200.0, 444.0)),
                Rectangle::new(Point::new(1000.0, 0.0), Size::new(200.0, 444.0)),
            ]
        );
    }

    #[test]
    fn regions_far_apart_stay_apart() {
        let corner = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0));
        let far = Rectangle::new(Point::new(1000.0, 600.0), Size::new(200.0, 200.0));
        assert_eq!(merged(vec![corner, far]), [corner, far]);
    }

    #[test]
    fn a_region_grown_by_a_merge_takes_in_one_it_passed() {
        // `top` merges with neither bottom half alone, only with the two
        // together, which come after it.
        let top = Rectangle::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0));
        let right = Rectangle::new(Point::new(100.0, 200.0), Size::new(100.0, 100.0));
        let left = Rectangle::new(Point::new(0.0, 200.0), Size::new(100.0, 100.0));
        assert_eq!(
            merged(vec![top, right, left]),
            [Rectangle::new(Point::ORIGIN, Size::new(200.0, 300.0))]
        );
    }
}
