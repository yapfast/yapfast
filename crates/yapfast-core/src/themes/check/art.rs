//! Checks on a theme's art: that text stays readable where it sits over
//! pictures and glass, and what the pictures cost once decoded.

use std::path::Path;

use super::{Files, READABLE, base_colour, contrast, follows};
use crate::themes::package::Resolved;
use crate::themes::package::art::{ArtSurface, Blend, Layer, Source};

/// The largest a surface's picture is decoded: about a large window. A
/// picture is never decoded larger than it is.
pub const DISPLAY: (u32, u32) = (2560, 1600);
/// A glass pane's blurred copy of the window's art is kept at this fraction
/// of its size on each side: the blur hides the difference.
pub const GLASS_SHRINK: u32 = 4;
/// A texture Yapfast makes: square, this many pixels a side.
pub const TEXTURE_SIDE: u32 = 256;

/// What a colour token shows in a variant: its own, its base's, or what it
/// follows.
pub fn shown(variant: &Resolved, token: &str) -> Option<u32> {
    variant
        .paint(token)
        .map(|paint| paint.solid())
        .or_else(|| base_colour(&variant.base, token))
        .or_else(|| follows(token).and_then(|follow| shown(variant, follow)))
}

fn decode(bytes: &[u8]) -> Option<image::DynamicImage> {
    let format = image::guess_format(bytes).ok()?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(super::MAX_IMAGE_SIDE);
    limits.max_image_height = Some(super::MAX_IMAGE_SIDE);
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    reader.decode().ok()
}

/// A picture's size on each side, from its header.
fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let format = image::guess_format(bytes).ok()?;
    image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        .into_dimensions()
        .ok()
}

/// Mixes one channel as a blend mode does, both from 0 to 1.
pub fn blend_channel(blend: Blend, ground: f32, art: f32) -> f32 {
    match blend {
        Blend::Normal => art,
        Blend::Multiply => ground * art,
        Blend::Screen => 1.0 - (1.0 - ground) * (1.0 - art),
        Blend::Overlay => {
            if ground < 0.5 {
                2.0 * ground * art
            } else {
                1.0 - 2.0 * (1.0 - ground) * (1.0 - art)
            }
        }
    }
}

fn channels(colour: u32) -> [f32; 3] {
    [16, 8, 0].map(|shift| ((colour >> shift) & 0xff) as f32 / 255.0)
}

fn argb([red, green, blue]: [f32; 3]) -> u32 {
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    0xff00_0000 | (byte(red) << 16) | (byte(green) << 8) | byte(blue)
}

/// A 16 × 16 sample of a layer, as ARGB, or none when it can't be read.
fn samples(layer: &Layer, files: &Files) -> Option<Vec<u32>> {
    match &layer.source {
        Source::Image(path) => {
            let picture = decode(files.files.get(path)?)?;
            let small = picture.thumbnail_exact(16, 16).to_rgba8();
            Some(
                small
                    .pixels()
                    .map(|pixel| {
                        let [red, green, blue, alpha] = pixel.0;
                        (u32::from(alpha) << 24)
                            | (u32::from(red) << 16)
                            | (u32::from(green) << 8)
                            | u32::from(blue)
                    })
                    .collect(),
            )
        }
        // A texture covers about half its ground with its colour.
        Source::Texture(_) => {
            let alpha = ((layer.colour >> 24) as f32 * 0.5) as u32;
            Some(vec![(alpha << 24) | (layer.colour & 0xff_ffff)])
        }
    }
}

/// Lays a layer over what's beneath, sample by sample: its blend against
/// the surface's ground, its dim, and its opacity.
fn lay(beneath: &[u32], layer: &Layer, over: &[u32], ground: u32) -> Vec<u32> {
    let count = beneath.len().max(over.len());
    (0..count)
        .map(|index| {
            let under = beneath[index % beneath.len()];
            let art = over[index % over.len()];
            let alpha = (art >> 24) as f32 / 255.0 * layer.opacity;
            let ground = channels(ground);
            let art = channels(art);
            let under = channels(under);
            let mixed: [f32; 3] = std::array::from_fn(|channel| {
                let blended =
                    blend_channel(layer.blend, ground[channel], art[channel]) * (1.0 - layer.dim);
                under[channel] * (1.0 - alpha) + blended * alpha
            });
            argb(mixed)
        })
        .collect()
}

/// What text on a surface sits on: its ground, the window's art under it if
/// it's glass, and its own layers, sampled.
fn composite(variant: &Resolved, files: &Files, surface: ArtSurface) -> Option<Vec<u32>> {
    let art = &variant.art;
    let ground = shown(variant, surface.ground())?;
    let glass = art
        .glass
        .as_ref()
        .filter(|glass| glass.surfaces.contains(&surface));
    let mut colours = match glass {
        Some(glass) => {
            let window = composite(variant, files, ArtSurface::Window)?;
            let ground = channels(ground);
            // Blurred, the window's art reads as its average.
            let mean = mean(&window);
            let under = channels(mean);
            vec![argb(std::array::from_fn(|channel| {
                under[channel] * (1.0 - glass.opacity) + ground[channel] * glass.opacity
            }))]
        }
        None => vec![ground],
    };
    let layers = art.surfaces.get(&surface);
    if layers.is_none() && glass.is_none() {
        return None;
    }
    for layer in layers.into_iter().flatten() {
        let over = samples(layer, files)?;
        colours = lay(&colours, layer, &over, ground);
    }
    Some(colours)
}

fn mean(colours: &[u32]) -> u32 {
    let mut sum = [0.0; 3];
    for colour in colours {
        for (total, value) in sum.iter_mut().zip(channels(*colour)) {
            *total += value;
        }
    }
    argb(sum.map(|total| total / colours.len().max(1) as f32))
}

/// The text drawn on each surface art may lie on.
fn texts(surface: ArtSurface) -> &'static [&'static str] {
    match surface {
        ArtSurface::Sidebar | ArtSurface::Chat | ArtSurface::Members | ArtSurface::Account => {
            &["text", "secondary", "muted"]
        }
        ArtSurface::Card | ArtSurface::Dialog | ArtSurface::Menu | ArtSurface::Composer => {
            &["text"]
        }
        ArtSurface::Window | ArtSurface::Rail => &[],
    }
}

/// What secondary and muted text need over art.
pub const HINT: f64 = 3.0;

/// Text over art: refused when it reads under 4.5:1 (secondary and muted
/// text 3:1) against the art's average under it, and a warning when a tenth of the art reads under 3:1.
pub fn readable(variant: &Resolved, files: &Files) -> (Vec<String>, Vec<String>) {
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for surface in ArtSurface::ALL.iter().copied() {
        let tokens = texts(surface);
        if tokens.is_empty() {
            continue;
        }
        let Some(colours) = composite(variant, files, surface) else {
            continue;
        };
        let average = mean(&colours);
        for token in tokens {
            let Some(text) = shown(variant, token) else {
                continue;
            };
            let on_average = contrast(text, average);
            // Body text needs 4.5:1; secondary and muted text, dimmer by
            // design, 3:1. Either keeps a lower bar its own ground gives it.
            let bar = if *token == "text" { READABLE } else { HINT };
            let own = shown(variant, surface.ground())
                .map_or(bar, |ground| contrast(text, ground).min(bar));
            if on_average < own - 0.005 {
                errors.push(format!(
                    "{token} on the {} art contrasts {on_average:.1}:1 on average, under {own:.1}:1; dim the art or change the colours",
                    surface.name()
                ));
                continue;
            }
            let mut each: Vec<f64> = colours.iter().map(|back| contrast(text, *back)).collect();
            each.sort_by(f64::total_cmp);
            let tenth = each[each.len() / 10];
            if *token == "text" && tenth < 3.0 {
                warnings.push(format!(
                    "text on the {} art reads under 3:1 over a tenth of it ({tenth:.1}:1)",
                    surface.name()
                ));
            }
        }
    }
    (errors, warnings)
}

/// Bytes a variant's art holds once decoded: each picture at display size,
/// textures, and the glass panes' blurred copy.
pub fn memory(variant: &Resolved, files: &Files) -> u64 {
    let art = &variant.art;
    let size = |path: &Path, cap: (u32, u32)| {
        files
            .files
            .get(path)
            .and_then(|bytes| dimensions(bytes))
            .map_or(0, |(width, height)| {
                let scale = (cap.0 as f64 / f64::from(width))
                    .min(cap.1 as f64 / f64::from(height))
                    .min(1.0);
                (f64::from(width) * scale).round() as u64
                    * (f64::from(height) * scale).round() as u64
                    * 4
            })
    };
    let mut total = 0;
    let mut layers: Vec<&Layer> = art.surfaces.values().flatten().collect();
    layers.extend(
        art.overlays
            .iter()
            .filter_map(|overlay| overlay.layer.as_ref()),
    );
    for layer in layers {
        total += match &layer.source {
            Source::Image(path) => size(path, DISPLAY),
            Source::Texture(_) => u64::from(TEXTURE_SIDE * TEXTURE_SIDE * 4),
        };
    }
    if art.glass.is_some() {
        total += u64::from(DISPLAY.0 / GLASS_SHRINK) * u64::from(DISPLAY.1 / GLASS_SHRINK) * 4;
    }
    for particles in &art.particles {
        if let Some(sprite) = &particles.sprite {
            total += size(sprite, (256, 256));
        }
    }
    total
}
