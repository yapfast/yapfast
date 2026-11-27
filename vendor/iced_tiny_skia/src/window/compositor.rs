use crate::core::{Color, Rectangle, Size};
use crate::graphics::compositor::{self, Information};
use crate::graphics::damage;
use crate::graphics::error::{self, Error};
use crate::graphics::{self, Shell, Viewport};
use crate::{Layer, Renderer, Settings};

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::time::Instant;

pub struct Compositor {
    context: softbuffer::Context<Box<dyn compositor::Display>>,
    settings: Settings,
}

pub struct Surface {
    window: softbuffer::Surface<
        Box<dyn compositor::Display>,
        Box<dyn compositor::Window>,
    >,
    clip_mask: tiny_skia::Mask,
    /// Yapfast port (14): each past frame's layers with the background it
    /// was drawn on. Stock kept one background for all of them, so after
    /// the background changed, a buffer two frames old was diffed as if it
    /// had the new one, and kept the old one where nothing else changed.
    layer_stack: VecDeque<(Vec<Layer>, Color)>,
    max_age: u8,
    /// Yapfast port (patch 27): the scale its last frame was drawn at.
    scale_factor: f32,
}

impl crate::graphics::Compositor for Compositor {
    type Renderer = Renderer;
    type Surface = Surface;

    async fn with_backend(
        settings: graphics::Settings,
        display: impl compositor::Display,
        _compatible_window: impl compositor::Window,
        _shell: Shell,
        backend: Option<&str>,
    ) -> Result<Self, Error> {
        // Yapfast port (patch 26): off while the app draws with the GPU.
        if !crate::enabled::enabled() {
            return Err(Error::GraphicsAdapterNotFound {
                backend: "tiny-skia",
                reason: error::Reason::DidNotMatch {
                    preferred_backend: "wgpu".to_owned(),
                },
            });
        }
        match backend {
            None | Some("tiny-skia") | Some("tiny_skia") => {
                Ok(new(settings.into(), display))
            }
            Some(backend) => Err(Error::GraphicsAdapterNotFound {
                backend: "tiny-skia",
                reason: error::Reason::DidNotMatch {
                    preferred_backend: backend.to_owned(),
                },
            }),
        }
    }

    fn create_renderer(&self) -> Self::Renderer {
        Renderer::new(
            self.settings.default_font,
            self.settings.default_text_size,
        )
    }

    fn create_surface<W: compositor::Window + Clone>(
        &mut self,
        window: W,
        width: u32,
        height: u32,
    ) -> Self::Surface {
        let window = softbuffer::Surface::new(
            &self.context,
            Box::new(window.clone()) as _,
        )
        .expect("Create softbuffer surface for window");

        let mut surface = Surface {
            window,
            clip_mask: tiny_skia::Mask::new(1, 1).expect("Create clip mask"),
            layer_stack: VecDeque::new(),
            max_age: 0,
            scale_factor: 0.0,
        };

        if width > 0 && height > 0 {
            self.configure_surface(&mut surface, width, height);
        }

        surface
    }

    fn configure_surface(
        &mut self,
        surface: &mut Self::Surface,
        width: u32,
        height: u32,
    ) {
        surface
            .window
            .resize(
                NonZeroU32::new(width).expect("Non-zero width"),
                NonZeroU32::new(height).expect("Non-zero height"),
            )
            .expect("Resize surface");

        surface.clip_mask =
            tiny_skia::Mask::new(width, height).expect("Create clip mask");
        surface.layer_stack.clear();
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

pub fn new(
    settings: Settings,
    display: impl compositor::Display,
) -> Compositor {
    #[allow(unsafe_code)]
    let context = softbuffer::Context::new(Box::new(display) as _)
        .expect("Create softbuffer context");

    Compositor { context, settings }
}

pub fn present(
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background_color: Color,
    on_pre_present: impl FnOnce(),
) -> Result<(), compositor::SurfaceError> {
    let physical_size = viewport.physical_size();
    // Yapfast port (patch 22): the frame's timings, while a hook asks.
    let hook = crate::timing::hook();
    let start = hook.map(|_| Instant::now());

    let mut buffer = surface
        .window
        .buffer_mut()
        .map_err(|_| compositor::SurfaceError::Lost)?;
    let waited = start.map(|_| Instant::now());

    // Yapfast port (patch 27): a new scale (the app's zoom, which doesn't
    // reconfigure the surface) redraws the window whole. Past frames'
    // layers are in logical pixels, so diffed across it they'd damage the
    // old scale's places, leaving the new one's stale. The renderer's
    // backdrop lets go of what it kept on its own (`layer_changes`).
    if surface.scale_factor != viewport.scale_factor() {
        surface.scale_factor = viewport.scale_factor();
        surface.layer_stack.clear();
    }

    let last_layers = {
        let age = buffer.age();

        surface.max_age = surface.max_age.max(age);
        surface.layer_stack.truncate(surface.max_age as usize);

        if age > 0 {
            surface.layer_stack.get(age as usize - 1)
        } else {
            None
        }
    };

    // Yapfast port (patch 32): what each layer changed since the frame
    // presented last, for the renderer's backdrop: a layer that's new
    // changed what it draws (patch 27's extent).
    let front = surface
        .layer_stack
        .front()
        .filter(|(_, previous_background)| *previous_background == background_color)
        .map(|(previous, _)| previous);
    let changes = {
        let current = renderer.layers();
        match front {
            // Yapfast port (patch 41): paired by place where they lie, the
            // layers that came or went partway between.
            Some(previous) => crate::layer::changes(previous, current),
            None => crate::layer::Changes {
                layers: current.iter().map(|layer| vec![layer.bounds]).collect(),
                gone: Vec::new(),
                shifted: None,
            },
        }
    };

    // Yapfast port (patch 23): the compositor is told what differs from
    // the frame it shows, the one presented last, not the whole window.
    // That is `changes` and what the layers that went drew (patch 32
    // already worked it out). The draw's `damage` is against the buffer's
    // own older frame.
    let bounds = Rectangle::with_size(viewport.logical_size());
    let shown = front.map(|_| changes.all().collect::<Vec<_>>());
    renderer.layer_changes(
        changes.layers,
        changes.shifted,
        viewport.scale_factor(),
        physical_size,
    );

    // Yapfast port (patch 27): a layer that came or went damages what it
    // drew (`Layer::extent`), not its bounds; (patch 41) paired as above.
    let damage = last_layers
        .and_then(|(last_layers, last_background)| {
            (*last_background == background_color).then(|| {
                crate::layer::changes(last_layers, renderer.layers())
                    .all()
                    .collect()
            })
        })
        .unwrap_or_else(|| vec![Rectangle::with_size(viewport.logical_size())]);
    let shown = if buffer.age() == 1 {
        Some(damage.clone())
    } else {
        shown
    }
    .map(|shown| physical(&merged(damage::group(shown, bounds)), viewport));

    let diffed = start.map(|_| Instant::now());
    let (mut regions, mut area) = (0, 0.0);

    if damage.is_empty() {
        if let Some(last_layers) = last_layers {
            surface.layer_stack.push_front(last_layers.clone());
        }
    } else {
        surface
            .layer_stack
            .push_front((renderer.layers().to_vec(), background_color));

        let damage = merged(damage::group(
            damage,
            Rectangle::with_size(viewport.logical_size()),
        ));

        let mut pixels = tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut buffer),
            physical_size.width,
            physical_size.height,
        )
        .expect("Create pixel map");

        if hook.is_some() {
            regions = damage.len();
            area = damage.iter().map(Rectangle::area).sum();
        }

        renderer.draw(
            &mut pixels,
            &mut surface.clip_mask,
            viewport,
            &damage,
            background_color,
        );
    }

    let drawn = start.map(|_| Instant::now());
    let shown_area = hook.and(shown.as_ref()).map(|shown| {
        shown
            .iter()
            .map(|rect| u64::from(rect.width.get()) * u64::from(rect.height.get()))
            .sum()
    });
    on_pre_present();
    let presented = match shown {
        Some(shown) => buffer.present_with_damage(&shown),
        None => buffer.present(),
    }
    .map_err(|_| compositor::SurfaceError::Lost);
    if let (Some(hook), Some(start), Some(waited), Some(diffed), Some(drawn)) =
        (hook, start, waited, diffed, drawn)
    {
        hook(&crate::timing::Frame {
            start,
            wait: waited - start,
            diff: diffed - waited,
            draw: drawn - diffed,
            present: drawn.elapsed(),
            regions,
            area,
            shown: shown_area,
        });
    }
    presented
}

pub fn screenshot(
    renderer: &mut Renderer,
    viewport: &Viewport,
    background_color: Color,
) -> Vec<u8> {
    let size = viewport.physical_size();
    // Yapfast port (patch 32): a screenshot draws whole. Changes told for a
    // frame that had no damage, and so wasn't drawn, are left unused.
    renderer.backdrop.fresh = false;

    let mut offscreen_buffer: Vec<u32> =
        vec![0; size.width as usize * size.height as usize];

    let mut clip_mask = tiny_skia::Mask::new(size.width, size.height)
        .expect("Create clip mask");

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

/// Yapfast port (patch 23): `regions`, in logical pixels, as the buffer's
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

/// Yapfast port: the damaged regions with any two merged whose union adds
/// no more than iced's own threshold (20,000 px) beyond what they cover
/// together. `damage::group` merges each region only into the one before
/// it, in order of distance from the window's corner, so in a scrolled list
/// the regions of neighbouring rows stayed apart and overlapped: each
/// region is drawn whole, background and all, so the overlaps were drawn
/// twice or more (2.5 times the member list's area per frame).
fn merged(mut regions: Vec<Rectangle>) -> Vec<Rectangle> {
    const AREA_THRESHOLD: f32 = 20_000.0;
    let mut index = 0;
    while index < regions.len() {
        let current = regions[index];
        let partner = regions.iter().enumerate().skip(index + 1).find(|(_, other)| {
            let overlap = current.intersection(other).map_or(0.0, |overlap| overlap.area());
            current.union(other).area() - (current.area() + other.area() - overlap)
                <= AREA_THRESHOLD
        });
        if let Some((other, _)) = partner {
            regions[index] = current.union(&regions.swap_remove(other));
            // The grown region may now take ones already passed.
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

    fn rect(rect: &softbuffer::Rect) -> (u32, u32, u32, u32) {
        (rect.x, rect.y, rect.width.get(), rect.height.get())
    }

    #[test]
    fn damage_is_told_in_the_buffers_pixels_within_it() {
        let viewport = Viewport::with_physical_size(Size::new(300, 200), 1.5);
        let regions = [
            // A fractional region takes in every pixel it touches.
            Rectangle::new(crate::core::Point::new(10.2, 20.0), Size::new(10.0, 4.6)),
            // Past the edge, it stops at it.
            Rectangle::new(crate::core::Point::new(-5.0, 120.0), Size::new(50.0, 50.0)),
            // Outside, or of no size, it goes.
            Rectangle::new(crate::core::Point::new(400.0, 10.0), Size::new(10.0, 10.0)),
            Rectangle::new(crate::core::Point::new(10.0, 10.0), Size::new(0.0, 10.0)),
        ];
        let shown: Vec<_> = physical(&regions, &viewport).iter().map(rect).collect();
        assert_eq!(shown, [(15, 30, 16, 7), (0, 180, 68, 20)]);
    }
}
