//! Checks on a theme's art: that text stays readable where it sits over
//! pictures and glass, and what the pictures cost once decoded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Files, READABLE, base_colour, contrast, follows};
use crate::themes::package::Resolved;
use crate::themes::package::art::{ArtSurface, Blend, Layer, Source};

/// The largest a surface's picture is decoded: about a large window. A
/// picture is never decoded larger than it is.
pub const DISPLAY: (u32, u32) = (2560, 1600);
/// A glass pane's blurred copy of the window's art is kept at this fraction
/// of its size on each side: the blur hides the difference.
pub const GLASS_SHRINK: u32 = 4;
/// The largest a card's picture is decoded when the theme gives it no size.
pub const CARD_ART: (u32, u32) = (960, 480);
/// The largest a picture with a size of its own is decoded: twice that size,
/// for sharp edges on a high-density screen, no larger than this.
pub const SIZED_ART: (u32, u32) = (2048, 1024);

/// The most a picture shown at `size` logical pixels is decoded at.
pub fn sized_cap(size: (f32, f32)) -> (u32, u32) {
    (
        ((size.0 * 2.0).ceil() as u32).clamp(8, SIZED_ART.0),
        ((size.1 * 2.0).ceil() as u32).clamp(8, SIZED_ART.1),
    )
}
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

/// Each picture's 16 × 16 sample, decoded once however many layers name it.
type Sampled = HashMap<PathBuf, Option<Vec<u32>>>;

/// A 16 × 16 sample of a layer, as ARGB, or none when it can't be read.
fn samples(layer: &Layer, files: &Files, sampled: &mut Sampled) -> Option<Vec<u32>> {
    match &layer.source {
        Source::Image(path) => sampled
            .entry(path.clone())
            .or_insert_with(|| sample(files.files.get(path)?))
            .clone(),
        // A texture covers about half its ground with its colour.
        Source::Texture(_) => {
            let alpha = ((layer.colour >> 24) as f32 * 0.5) as u32;
            Some(vec![(alpha << 24) | (layer.colour & 0xff_ffff)])
        }
    }
}

fn sample(bytes: &[u8]) -> Option<Vec<u32>> {
    {
        {
            let picture = decode(bytes)?;
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

/// Whether a surface's ground lets what's under it show: a member list
/// over the conversation's ground and art.
pub fn see_through(variant: &Resolved, token: &str) -> bool {
    token == "members" && shown(variant, token).is_some_and(|colour| colour >> 24 < 0xff)
}

/// What text on a surface sits on: its ground, the window's art under it if
/// it's glass, and its own layers, sampled.
fn composite(
    variant: &Resolved,
    files: &Files,
    surface: ArtSurface,
    sampled: &mut Sampled,
) -> Option<Vec<u32>> {
    let art = &variant.art;
    let ground = shown(variant, surface.ground())?;
    // A see-through member list: the conversation as it shows, with the
    // list's ground laid over it at its alpha.
    if surface == ArtSurface::Members
        && see_through(variant, "members")
        && !art
            .glass
            .as_ref()
            .is_some_and(|glass| glass.surfaces.contains(&surface))
    {
        let beneath = composite(variant, files, ArtSurface::Chat, sampled)
            .or_else(|| shown(variant, "chat").map(|chat| vec![chat]))?;
        let alpha = (ground >> 24) as f32 / 255.0;
        let tint = channels(ground);
        let mut colours: Vec<u32> = beneath
            .iter()
            .map(|under| {
                let under = channels(*under);
                argb(std::array::from_fn(|channel| {
                    under[channel] * (1.0 - alpha) + tint[channel] * alpha
                }))
            })
            .collect();
        for layer in art.surfaces.get(&surface).into_iter().flatten() {
            let over = samples(layer, files, sampled)?;
            colours = lay(&colours, &spread(layer), &over, ground);
        }
        return Some(colours);
    }
    let glass = art
        .glass
        .as_ref()
        .filter(|glass| glass.surfaces.contains(&surface));
    let mut colours = match glass {
        Some(glass) => {
            let window = composite(variant, files, ArtSurface::Window, sampled)?;
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
        let over = samples(layer, files, sampled)?;
        colours = lay(&colours, &spread(layer), &over, ground);
    }
    Some(colours)
}

/// A layer as text meets it across its surface: one of a fixed size covers
/// only its share of a pane of about 800 × 600, and weighs that much.
fn spread(layer: &Layer) -> Layer {
    let mut spread = layer.clone();
    if let Some((width, height)) = layer.size {
        spread.opacity *= (width * height / (800.0 * 600.0)).min(1.0);
    }
    spread
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
        ArtSurface::Sidebar
        | ArtSurface::Chat
        | ArtSurface::Members
        | ArtSurface::Account
        | ArtSurface::ProfilePane
        | ArtSurface::Settings
        | ArtSurface::Embed => &["text", "secondary", "muted"],
        ArtSurface::Card
        | ArtSurface::Dialog
        | ArtSurface::Menu
        | ArtSurface::Composer
        | ArtSurface::Actions
        | ArtSurface::Code
        | ArtSurface::Header => &["text"],
        ArtSurface::RowSelected => &["row-selected-text"],
        // Over the card's content, it's the theme's to keep faint; a label's
        // words are checked on its own ground (`shape.label`), and the member
        // list's foot holds no text.
        ArtSurface::Window
        | ArtSurface::Rail
        | ArtSurface::CardOver
        | ArtSurface::Label
        | ArtSurface::Footer => &[],
    }
}

/// What secondary and muted text need over art.
pub const HINT: f64 = 3.0;

/// Text over art: refused when it reads under 4.5:1 (secondary and muted
/// text 3:1) against the art's average under it, and a warning when a tenth of the art reads under 3:1.
pub fn readable(variant: &Resolved, files: &Files) -> (Vec<String>, Vec<String>) {
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    let mut sampled = Sampled::new();
    for surface in ArtSurface::ALL.iter().copied() {
        let tokens = texts(surface);
        if tokens.is_empty() {
            continue;
        }
        let Some(colours) = composite(variant, files, surface, &mut sampled) else {
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
            let own = if see_through(variant, surface.ground()) {
                bar
            } else {
                shown(variant, surface.ground())
                    .map_or(bar, |ground| contrast(text, ground).min(bar))
            };
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
    bubbles(variant, files, &mut sampled, &mut errors, &mut warnings);
    (errors, warnings)
}

/// Text on bubbles with pictures on them: your own bubbles' text, and
/// everyone else's, over each ground colour with the pictures laid on it.
fn bubbles(
    variant: &Resolved,
    files: &Files,
    sampled: &mut Sampled,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let bubbles = &variant.layout.bubbles;
    let own_ink = match (bubbles.own_text, &bubbles.own) {
        (Some(colour), _) => Some(colour),
        (None, Some(_)) => shown(variant, "text"),
        (None, None) => shown(variant, "on-accent"),
    };
    let sides = [
        (
            "own",
            &bubbles.own_art,
            bubbles.own.as_ref(),
            "accent",
            own_ink,
        ),
        (
            "other",
            &bubbles.other_art,
            bubbles.other.as_ref(),
            "raised",
            shown(variant, "text"),
        ),
    ];
    for (side, layers, paint, token, ink) in sides {
        if layers.is_empty() {
            continue;
        }
        let (Some(ink), Some(grounds)) = (
            ink,
            paint
                .map(crate::themes::package::Paint::colours)
                .or_else(|| shown(variant, token).map(|colour| vec![colour])),
        ) else {
            continue;
        };
        let mut colours = Vec::new();
        for ground in grounds {
            let mut laid = vec![ground];
            for layer in layers {
                let Some(over) = samples(layer, files, sampled) else {
                    continue;
                };
                laid = lay(&laid, layer, &over, ground);
            }
            colours.extend(laid);
        }
        let average = mean(&colours);
        let on_average = contrast(ink, average);
        if on_average < READABLE - 0.005 {
            errors.push(format!(
                "text on layout.bubbles.{side}'s art contrasts {on_average:.1}:1 on average, under {READABLE:.1}:1; dim the art or change the colours"
            ));
            continue;
        }
        let mut each: Vec<f64> = colours.iter().map(|back| contrast(ink, *back)).collect();
        each.sort_by(f64::total_cmp);
        let tenth = each[each.len() / 10];
        if tenth < 3.0 {
            warnings.push(format!(
                "text on layout.bubbles.{side}'s art reads under 3:1 over a tenth of it ({tenth:.1}:1)"
            ));
        }
    }
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
    // A picture shows once in memory however many surfaces lay it, unless
    // they bake it differently: blurred, dimmed, scaled or blended with
    // their own grounds. Opacity is drawn, not baked.
    let mut layers: Vec<(&Layer, Option<&str>)> = art
        .surfaces
        .iter()
        .flat_map(|(surface, layers)| layers.iter().map(|layer| (layer, Some(surface.ground()))))
        .collect();
    layers.extend(
        art.overlays
            .iter()
            .filter_map(|overlay| overlay.layer.as_ref())
            .map(|layer| (layer, Some(ArtSurface::Window.ground()))),
    );
    let bubbles = &variant.layout.bubbles;
    layers.extend(bubbles.own_art.iter().map(|layer| (layer, Some("accent"))));
    layers.extend(
        bubbles
            .other_art
            .iter()
            .map(|layer| (layer, Some("raised"))),
    );
    let mut seen = Vec::new();
    for (layer, ground) in layers {
        let key = bake_key(layer, ground);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        total += match &layer.source {
            Source::Image(path) => size(path, layer.size.map_or(DISPLAY, sized_cap)),
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
    // A card's pictures are decoded once, shared by every card.
    for card in &variant.layout.cards.art {
        let cap = card.size.map_or(CARD_ART, sized_cap);
        total += match &card.layer.source {
            Source::Image(path) => size(path, cap),
            Source::Texture(_) => u64::from(TEXTURE_SIDE * TEXTURE_SIDE * 4),
        };
        total += card.choices.iter().map(|path| size(path, cap)).sum::<u64>();
    }
    total
}

/// What tells two layers' pictures apart once baked: their source and how
/// it's decoded (a sized layer at its own cap), scaled, blurred, dimmed and blended, with the ground only
/// when a blend mixes with it. Opacity isn't baked: it's drawn.
pub fn bake_key(layer: &Layer, ground: Option<&str>) -> String {
    let source = match &layer.source {
        Source::Image(path) => format!("image:{}", path.display()),
        Source::Texture(texture) => format!("texture:{}:{:08x}", texture.name(), layer.colour),
    };
    let ground = if layer.blend == Blend::Normal {
        ""
    } else {
        ground.unwrap_or("")
    };
    format!(
        "{source}:{}:{:?}:{}:{}:{}:{}:{ground}",
        layer.fit == crate::themes::package::art::Fit::Tile,
        layer.size,
        layer.scale,
        layer.blur,
        layer.dim,
        layer.blend.name(),
    )
}
