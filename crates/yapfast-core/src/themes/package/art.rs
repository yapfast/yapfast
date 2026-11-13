//! A theme's art: pictures and textures on surfaces, overlays above the
//! content, glass panes, text shadows and outlines, particles and the event
//! effects. Everything here is data from closed lists, as the rest of
//! `theme.json`; `src/ui/theme_art.rs` draws it. See `docs/themes.md`, *Art*.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::{Error, Shadow, colour_at, named, number, object, read_shadow, relative};

/// The most layers one surface stacks.
pub const MAX_LAYERS: usize = 4;
/// The most overlays a variant lays over the window.
pub const MAX_OVERLAYS: usize = 6;
/// The most particle sets a variant declares.
pub const MAX_PARTICLES: usize = 4;

macro_rules! closed {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub fn name(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            pub fn named(text: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|item| item.name() == text)
            }
            pub fn index(self) -> usize {
                Self::ALL.iter().position(|item| *item == self).unwrap_or(0)
            }
        }
    };
}

closed! {
    /// Where art is drawn: under each surface's content, over its ground.
    ArtSurface {
        Window = "window",
        Rail = "rail",
        Sidebar = "sidebar",
        Chat = "chat",
        Members = "members",
        Account = "account",
        Card = "card",
        Dialog = "dialog",
        Composer = "composer",
        Menu = "menu",
        Actions = "actions",
        Header = "header",
        Label = "label",
        Footer = "footer",
    }
}

impl ArtSurface {
    /// The surface a theme names, where `panel`, the account panel's first
    /// name, is still read as `account`.
    pub fn read(text: &str) -> Option<Self> {
        Self::named(if text == "panel" { "account" } else { text })
    }

    /// The colour token of the ground the art lies on, which a blend mixes
    /// with and contrast is checked over.
    pub fn ground(self) -> &'static str {
        match self {
            Self::Window | Self::Chat => "chat",
            Self::Rail => "rail",
            Self::Sidebar => "sidebar",
            Self::Members => "members",
            Self::Account => "account",
            Self::Card | Self::Dialog => "raised",
            Self::Composer => "composer",
            Self::Menu => "menu",
            Self::Actions => "actions",
            Self::Header => "chat",
            Self::Label => "sidebar",
            Self::Footer => "members",
        }
    }
}

closed! {
    /// How a picture fills its surface.
    Fit { Cover = "cover", Contain = "contain", Tile = "tile", Stretch = "stretch" }
}

closed! {
    /// How a picture mixes with the ground under it. Yapfast mixes it with
    /// the surface's own colour once, as it decodes the picture, so a blend
    /// costs nothing to draw; over a gradient the first stop stands in.
    Blend { Normal = "normal", Multiply = "multiply", Screen = "screen", Overlay = "overlay" }
}

closed! {
    /// Textures Yapfast makes itself, tiled, in the layer's colour.
    Texture { Grain = "grain", Paper = "paper", Kraft = "kraft", Noise = "noise", Halftone = "halftone" }
}

/// Where a layer's picture comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A PNG, JPEG or WebP in the theme's folder.
    Image(PathBuf),
    Texture(Texture),
}

/// A sprite sheet: frames in rows of `columns`, shown in turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub count: u32,
    pub columns: u32,
    /// Milliseconds for the whole sheet.
    pub period: f32,
}

/// One picture or texture on a surface, or over the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub source: Source,
    pub fit: Fit,
    /// Where it sits when it doesn't fill the surface, 0 to 1 across and down.
    pub position: (f32, f32),
    pub opacity: f32,
    pub blend: Blend,
    /// Pixels of blur, baked in when the picture is decoded.
    pub blur: f32,
    /// Darkens the picture: 0 none, 1 black.
    pub dim: f32,
    /// A tile's size against the picture's own.
    pub scale: f32,
    /// A texture's colour, ARGB.
    pub colour: u32,
    /// Logical pixels a second it slides, looping: clouds, a river.
    pub drift: (f32, f32),
    pub frames: Option<Frames>,
    /// Stays still in Balanced. Drift and sprite sheets are light enough
    /// to run there unless the theme says so.
    pub heavy: bool,
}

impl Layer {
    /// Whether it moves at all.
    pub fn moves(&self) -> bool {
        self.drift != (0.0, 0.0) || self.frames.is_some_and(|frames| frames.count > 1)
    }
}

closed! {
    /// Procedural layers over the whole window, above the content. None
    /// takes input.
    OverlayKind {
        Image = "image",
        Scanlines = "scanlines",
        Grain = "grain",
        Vignette = "vignette",
        Band = "band",
        Flare = "flare",
        Lcd = "lcd",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub kind: OverlayKind,
    /// The picture of an `image` overlay.
    pub layer: Option<Layer>,
    /// ARGB.
    pub colour: u32,
    pub opacity: f32,
    /// Scanlines: pixels from one line to the next. Band: its height. Flare:
    /// its diameter.
    pub size: f32,
    /// Where a flare sits, 0 to 1 across and down.
    pub at: (f32, f32),
    /// Milliseconds a band takes to roll down, or a flare to pulse, or
    /// grain to change (0: still).
    pub period: f32,
    pub heavy: bool,
    /// Over the conversation only (`"on": "chat"`), not the whole window.
    pub chat: bool,
    /// LCD: a brief afterimage of the conversation when the channel changes.
    pub ghost: bool,
}

impl Overlay {
    pub fn moves(&self) -> bool {
        match self.kind {
            OverlayKind::Image => self.layer.as_ref().is_some_and(Layer::moves),
            OverlayKind::Band | OverlayKind::Flare | OverlayKind::Grain => self.period > 0.0,
            _ => false,
        }
    }
}

/// Frosted panes. Slint has no backdrop blur, so a glass pane draws a
/// blurred copy of the window's art beneath it, aligned to where it sits,
/// and its ground over that at `opacity`: what's behind a pane is the
/// window's art, not other panes' content.
#[derive(Clone, Debug, PartialEq)]
pub struct Glass {
    pub surfaces: Vec<ArtSurface>,
    pub blur: f32,
    /// How much of the pane's own ground covers the art, 0 to 1.
    pub opacity: f32,
    /// A 1 px line inside the top and left edges, ARGB.
    pub highlight: Option<u32>,
    /// A shadow inside the edges.
    pub inset: Option<Shadow>,
    /// Logical pixels between the window's edge and its panes, and between
    /// them, where the window's art shows.
    pub gap: f32,
    /// The panes' corners when there's a gap.
    pub radius: f32,
}

closed! {
    /// The text a theme may shadow, outline or light.
    TextRole { Names = "names", Headings = "headings", Labels = "labels", Body = "body" }
}

/// How one role's text is drawn beyond its colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLook {
    pub shadow: Option<Shadow>,
    /// A soft light around the letters: ARGB and its reach in pixels.
    pub glow: Option<(u32, f32)>,
    /// A line around the letters: its width and ARGB.
    pub outline: Option<(f32, u32)>,
    /// A light edge above and a dark one below, as if pressed.
    pub emboss: bool,
}

closed! {
    /// Particle presets.
    Preset { Bubbles = "bubbles", Sparkles = "sparkles", Dust = "dust", Scraps = "scraps" }
}

closed! {
    /// When particles fly.
    Trigger {
        Ambient = "ambient",
        MessageSent = "message.sent",
        ReactionAdded = "reaction.added",
        ThemeApplied = "theme.applied",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Particles {
    pub preset: Preset,
    pub on: Trigger,
    /// ARGB, picked in turn.
    pub colours: Vec<u32>,
    /// A picture drawn instead of the preset's shape.
    pub sprite: Option<PathBuf>,
    /// How many fly on an event, or at most at once when ambient.
    pub count: u32,
    /// Ambient: how many start each second.
    pub rate: f32,
    /// Pixels across.
    pub size: f32,
    /// Milliseconds one lives.
    pub life: f32,
    pub heavy: bool,
}

closed! {
    /// Effects on the fixed event slots.
    EventEffect { Ripple = "ripple", Gloss = "gloss", Highlighter = "highlighter", Stamp = "stamp", Decrypt = "decrypt", None = "none" }
}

closed! {
    /// The event slots: a click on a button, the pointer on a button or a
    /// server, a channel chosen, a reaction added.
    EventSlot { Click = "click", Hover = "hover", Select = "select", Reaction = "reaction", Row = "row", NameHover = "name_hover" }
}

impl EventSlot {
    /// The effect each slot plays.
    pub fn effect(self) -> EventEffect {
        match self {
            Self::Click => EventEffect::Ripple,
            Self::Hover => EventEffect::Gloss,
            Self::Select => EventEffect::Highlighter,
            Self::Reaction => EventEffect::Stamp,
            Self::Row => EventEffect::Highlighter,
            Self::NameHover => EventEffect::Decrypt,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// ARGB.
    pub colour: u32,
    pub duration: f32,
}

/// Everything a variant's `art` asks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    pub surfaces: BTreeMap<ArtSurface, Vec<Layer>>,
    pub overlays: Vec<Overlay>,
    pub glass: Option<Glass>,
    pub text: BTreeMap<TextRole, TextLook>,
    pub particles: Vec<Particles>,
    pub events: BTreeMap<EventSlot, Event>,
    pub foil: Option<Foil>,
    pub transition: Option<Transition>,
    /// Pixels of the room the `footer` surface keeps at the member list's
    /// foot, for stickers and blinkies; 64 when it has art and this isn't set.
    pub footer_height: Option<f32>,
}

closed! {
    /// Where foil shines.
    FoilSurface { Card = "card", Tile = "tile" }
}

/// A holographic band over cards that follows the pointer while it's on one,
/// and otherwise sweeps across by itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Foil {
    pub surfaces: Vec<FoilSurface>,
    /// Three colours across the band, ARGB.
    pub colours: [u32; 3],
    pub opacity: f32,
    /// Milliseconds a sweep takes when the pointer isn't on the card.
    pub period: f32,
}

closed! {
    /// How the conversation changes when another channel opens.
    TransitionKind { Fade = "fade", Dissolve = "dissolve" }
}

/// A channel change: the conversation as it was, going over `duration`.
/// Heavy: only Full plays it, and never with reduced motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: f32,
}

impl Art {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every file it names, once each.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut add = |path: &PathBuf| {
            if !files.contains(path) {
                files.push(path.clone());
            }
        };
        for layers in self.surfaces.values() {
            for layer in layers {
                if let Source::Image(path) = &layer.source {
                    add(path);
                }
            }
        }
        for overlay in &self.overlays {
            if let Some(Layer {
                source: Source::Image(path),
                ..
            }) = &overlay.layer
            {
                add(path);
            }
        }
        for particles in &self.particles {
            if let Some(sprite) = &particles.sprite {
                add(sprite);
            }
        }
        files
    }

    /// Layers and overlays that move, and how many of them are heavy.
    pub fn moving(&self) -> (usize, usize) {
        let layers = self
            .surfaces
            .values()
            .flatten()
            .filter(|layer| layer.moves());
        let overlays = self.overlays.iter().filter(|overlay| overlay.moves());
        let (mut count, mut heavy) = (0, 0);
        for heavy_one in layers
            .map(|layer| layer.heavy)
            .chain(overlays.map(|overlay| overlay.heavy))
        {
            count += 1;
            heavy += usize::from(heavy_one);
        }
        if self.foil.is_some() {
            count += 1;
        }
        if self.transition.is_some() {
            count += 1;
            heavy += 1;
        }
        let ambient = self
            .particles
            .iter()
            .filter(|particles| particles.on == Trigger::Ambient);
        for particles in ambient {
            count += 1;
            heavy += usize::from(particles.heavy);
        }
        (count, heavy)
    }
}

const LAYER_FIELDS: [&str; 13] = [
    "image", "texture", "fit", "position", "opacity", "blend", "blur", "dim", "scale", "colour",
    "drift", "frames", "heavy",
];

/// Reads `art` into what the variant had, each key it names replacing the
/// one before; surfaces and text roles one by one.
pub(super) fn read_art(
    value: &Value,
    place: &str,
    art: &mut Art,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "surfaces" => {
                for (name, layers) in object(value, &here)? {
                    let at = format!("{here}.{name}");
                    let Some(surface) = ArtSurface::read(name) else {
                        notes.push(format!("\"{at}\" isn't a surface art lies on; ignored"));
                        continue;
                    };
                    let items: Vec<&Value> = match layers {
                        Value::Array(items) => items.iter().collect(),
                        other => vec![other],
                    };
                    if items.len() > MAX_LAYERS {
                        return Err(Error(format!(
                            "\"{at}\" stacks at most {MAX_LAYERS} layers"
                        )));
                    }
                    let mut read = Vec::new();
                    for (index, item) in items.into_iter().enumerate() {
                        read.push(read_layer(item, &format!("{at}[{index}]"), notes)?);
                    }
                    art.surfaces.insert(surface, read);
                }
            }
            "overlays" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| Error(format!("\"{here}\" is a list of overlays")))?;
                if items.len() > MAX_OVERLAYS {
                    return Err(Error(format!(
                        "\"{here}\" lists at most {MAX_OVERLAYS} overlays"
                    )));
                }
                art.overlays = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| read_overlay(item, &format!("{here}[{index}]"), notes))
                    .collect::<Result<_, _>>()?;
            }
            "glass" => art.glass = Some(read_glass(value, &here, notes)?),
            "text" => {
                for (name, look) in object(value, &here)? {
                    let at = format!("{here}.{name}");
                    let Some(role) = TextRole::named(name) else {
                        notes.push(format!("\"{at}\" isn't text a theme can light; ignored"));
                        continue;
                    };
                    art.text.insert(role, read_text(look, &at, notes)?);
                }
            }
            "particles" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| Error(format!("\"{here}\" is a list of particle sets")))?;
                if items.len() > MAX_PARTICLES {
                    return Err(Error(format!(
                        "\"{here}\" lists at most {MAX_PARTICLES} particle sets"
                    )));
                }
                art.particles = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| read_particles(item, &format!("{here}[{index}]"), notes))
                    .collect::<Result<_, _>>()?;
            }
            "events" => {
                for (name, event) in object(value, &here)? {
                    let at = format!("{here}.{name}");
                    let Some(slot) = EventSlot::named(name) else {
                        notes.push(format!("\"{at}\" isn't an event slot; ignored"));
                        continue;
                    };
                    match read_event(slot, event, &at, notes)? {
                        Some(event) => art.events.insert(slot, event),
                        None => art.events.remove(&slot),
                    };
                }
            }
            "foil" => art.foil = Some(read_foil(value, &here, notes)?),
            "footer_height" => art.footer_height = Some(number(value, &here, (16.0, 240.0))?),
            "transition" => {
                let fields = object(value, &here)?;
                let kind = named(
                    fields
                        .get("kind")
                        .ok_or_else(|| Error(format!("\"{here}\" needs a \"kind\"")))?,
                    &format!("{here}.kind"),
                    TransitionKind::ALL,
                    TransitionKind::name,
                )?;
                let duration = fields.get("duration").map_or(Ok(420.0), |value| {
                    number(value, &format!("{here}.duration"), (100.0, 2000.0))
                })?;
                art.transition = Some(Transition { kind, duration });
            }
            _ => notes.push(format!("\"{here}\" isn't something art takes; ignored")),
        }
    }
    Ok(())
}

fn read_foil(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Foil, Error> {
    let mut foil = Foil {
        surfaces: vec![FoilSurface::Card],
        colours: [0xffff_9ad5, 0xff9a_f0ff, 0xffff_f6a8],
        opacity: 0.35,
        period: 6000.0,
    };
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "surfaces" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| Error(format!("\"{here}\" is a list: card, tile")))?;
                foil.surfaces = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        named(
                            item,
                            &format!("{here}[{index}]"),
                            FoilSurface::ALL,
                            FoilSurface::name,
                        )
                    })
                    .collect::<Result<_, _>>()?;
            }
            "colours" => {
                let items = value
                    .as_array()
                    .filter(|items| items.len() == 3)
                    .ok_or_else(|| Error(format!("\"{here}\" is three colours")))?;
                for (index, item) in items.iter().enumerate() {
                    foil.colours[index] = colour_at(item, &format!("{here}[{index}]"))?;
                }
            }
            "opacity" => foil.opacity = number(value, &here, (0.0, 1.0))?,
            "period" => foil.period = number(value, &here, (1000.0, 60_000.0))?,
            _ => notes.push(format!("\"{here}\" isn't something foil takes; ignored")),
        }
    }
    Ok(foil)
}

fn pair(value: &Value, place: &str, range: (f32, f32)) -> Result<(f32, f32), Error> {
    match value {
        Value::Array(items) if items.len() == 2 => Ok((
            number(&items[0], &format!("{place}[0]"), range)?,
            number(&items[1], &format!("{place}[1]"), range)?,
        )),
        Value::Object(fields) => Ok((
            fields
                .get("x")
                .map_or(Ok(0.0), |x| number(x, &format!("{place}.x"), range))?,
            fields
                .get("y")
                .map_or(Ok(0.0), |y| number(y, &format!("{place}.y"), range))?,
        )),
        _ => Err(Error(format!(
            "\"{place}\" is {{ \"x\": …, \"y\": … }} or [x, y]"
        ))),
    }
}

/// `center`, `top`, `bottom left` and the like, or [x, y] from 0 to 1.
fn position(value: &Value, place: &str) -> Result<(f32, f32), Error> {
    let Some(text) = value.as_str() else {
        return pair(value, place, (0.0, 1.0));
    };
    let (mut x, mut y) = (0.5, 0.5);
    for word in text.split_whitespace() {
        match word {
            "left" => x = 0.0,
            "right" => x = 1.0,
            "top" => y = 0.0,
            "bottom" => y = 1.0,
            "center" | "centre" => {}
            _ => {
                return Err(Error(format!(
                    "\"{place}\" is made of top, bottom, left, right and center, not \"{text}\""
                )));
            }
        }
    }
    Ok((x, y))
}

pub(crate) fn read_layer(
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<Layer, Error> {
    let fields = object(value, place)?;
    let source = match (fields.get("image"), fields.get("texture")) {
        (Some(image), None) => Source::Image(relative(
            image
                .as_str()
                .ok_or_else(|| Error(format!("\"{place}.image\" is a file name")))?,
            &format!("{place}.image"),
        )?),
        (None, Some(texture)) => Source::Texture(named(
            texture,
            &format!("{place}.texture"),
            Texture::ALL,
            Texture::name,
        )?),
        _ => {
            return Err(Error(format!(
                "\"{place}\" needs an \"image\" or a \"texture\", not both"
            )));
        }
    };
    let textured = matches!(source, Source::Texture(_));
    let mut layer = Layer {
        source,
        fit: if textured { Fit::Tile } else { Fit::Cover },
        position: (0.5, 0.5),
        opacity: 1.0,
        blend: Blend::Normal,
        blur: 0.0,
        dim: 0.0,
        scale: 1.0,
        colour: 0xff00_0000,
        drift: (0.0, 0.0),
        frames: None,
        heavy: false,
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "image" | "texture" => {}
            "fit" => layer.fit = named(value, &here, Fit::ALL, Fit::name)?,
            "position" => layer.position = position(value, &here)?,
            "opacity" => layer.opacity = number(value, &here, (0.0, 1.0))?,
            "blend" => layer.blend = named(value, &here, Blend::ALL, Blend::name)?,
            "blur" => layer.blur = number(value, &here, (0.0, 64.0))?,
            "dim" => layer.dim = number(value, &here, (0.0, 1.0))?,
            "scale" => layer.scale = number(value, &here, (0.25, 4.0))?,
            "colour" => layer.colour = colour_at(value, &here)?,
            "drift" => layer.drift = pair(value, &here, (-200.0, 200.0))?,
            "frames" => {
                let frames = object(value, &here)?;
                let count = frames.get("count").map_or(Ok(1.0), |count| {
                    number(count, &format!("{here}.count"), (1.0, 64.0))
                })? as u32;
                let columns = frames.get("columns").map_or(Ok(count as f32), |columns| {
                    number(columns, &format!("{here}.columns"), (1.0, 64.0))
                })? as u32;
                let period = frames.get("period").map_or(Ok(1000.0), |period| {
                    number(period, &format!("{here}.period"), (100.0, 60_000.0))
                })?;
                layer.frames = Some(Frames {
                    count,
                    columns: columns.min(count),
                    period,
                });
            }
            "heavy" => layer.heavy = value.as_bool().unwrap_or(false),
            _ => notes.push(format!("\"{here}\" isn't something a layer takes; ignored")),
        }
    }
    if layer.frames.is_some() && layer.fit == Fit::Tile {
        return Err(Error(format!(
            "\"{place}\" can't both tile and play frames"
        )));
    }
    Ok(layer)
}

fn read_overlay(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Overlay, Error> {
    let fields = object(value, place)?;
    let kind = match (fields.get("kind"), fields.get("image")) {
        (Some(kind), _) => named(
            kind,
            &format!("{place}.kind"),
            OverlayKind::ALL,
            OverlayKind::name,
        )?,
        (None, Some(_)) => OverlayKind::Image,
        (None, None) => {
            return Err(Error(format!(
                "\"{place}\" needs a \"kind\" (scanlines, grain, vignette, band, flare) or an \"image\""
            )));
        }
    };
    let (size, period, colour, opacity) = match kind {
        OverlayKind::Scanlines => (3.0, 0.0, 0xff00_0000, 0.15),
        OverlayKind::Grain => (0.0, 0.0, 0xff80_8080, 0.08),
        OverlayKind::Vignette => (0.0, 0.0, 0xff00_0000, 0.45),
        OverlayKind::Band => (120.0, 6000.0, 0xffff_ffff, 0.06),
        OverlayKind::Flare => (480.0, 0.0, 0xffff_ffff, 0.25),
        OverlayKind::Lcd => (3.0, 0.0, 0xff00_0000, 0.18),
        OverlayKind::Image => (0.0, 0.0, 0, 1.0),
    };
    let mut overlay = Overlay {
        kind,
        layer: None,
        colour,
        opacity,
        size,
        at: (0.85, 0.1),
        period,
        heavy: false,
        chat: false,
        ghost: false,
    };
    let mut heavy = None;
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "kind" => {}
            "colour" => overlay.colour = colour_at(value, &here)?,
            "opacity" => overlay.opacity = number(value, &here, (0.0, 1.0))?,
            "size" => overlay.size = number(value, &here, (1.0, 2000.0))?,
            "at" => overlay.at = position(value, &here)?,
            "period" => overlay.period = number(value, &here, (0.0, 60_000.0))?,
            "heavy" => heavy = value.as_bool(),
            "on" => {
                overlay.chat = match value.as_str() {
                    Some("chat") => true,
                    Some("window") => false,
                    _ => return Err(Error(format!("\"{here}\" is window or chat"))),
                }
            }
            "ghost" if kind == OverlayKind::Lcd => overlay.ghost = value.as_bool().unwrap_or(false),
            "image" if kind == OverlayKind::Image => {}
            _ if kind == OverlayKind::Image && LAYER_FIELDS.contains(&key.as_str()) => {}
            _ => notes.push(format!(
                "\"{here}\" isn't something an overlay takes; ignored"
            )),
        }
    }
    if kind == OverlayKind::Image {
        let mut fields = fields.clone();
        fields.remove("kind");
        fields.remove("at");
        fields.remove("size");
        fields.remove("period");
        fields.remove("on");
        let layer = read_layer(&Value::Object(fields), place, notes)?;
        overlay.opacity = layer.opacity;
        overlay.layer = Some(layer);
    }
    if matches!(kind, OverlayKind::Scanlines | OverlayKind::Lcd) && overlay.size < 2.0 {
        return Err(Error(format!(
            "\"{place}.size\" is at least 2 for scanlines"
        )));
    }
    if overlay.period > 0.0 && overlay.period < 100.0 {
        return Err(Error(format!("\"{place}.period\" is 0 or 100 to 60000")));
    }
    // A layer over the whole window that moves is heavy unless it says
    // otherwise.
    overlay.heavy = heavy.unwrap_or(overlay.moves());
    Ok(overlay)
}

fn read_glass(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Glass, Error> {
    let fields = object(value, place)?;
    let mut glass = Glass {
        surfaces: vec![ArtSurface::Sidebar, ArtSurface::Chat],
        blur: 16.0,
        opacity: 0.6,
        highlight: None,
        inset: None,
        gap: 0.0,
        radius: 12.0,
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "surfaces" => {
                let items = value
                    .as_array()
                    .ok_or_else(|| Error(format!("\"{here}\" is a list of surfaces")))?;
                glass.surfaces.clear();
                for (index, item) in items.iter().enumerate() {
                    let account = Value::from("account");
                    let item = if item.as_str() == Some("panel") {
                        &account
                    } else {
                        item
                    };
                    let surface = named(
                        item,
                        &format!("{here}[{index}]"),
                        ArtSurface::ALL,
                        ArtSurface::name,
                    )?;
                    if surface == ArtSurface::Window {
                        return Err(Error(format!(
                            "\"{here}\": the window is what glass shows, not a glass pane"
                        )));
                    }
                    glass.surfaces.push(surface);
                }
            }
            "blur" => glass.blur = number(value, &here, (0.0, 64.0))?,
            "opacity" => glass.opacity = number(value, &here, (0.0, 1.0))?,
            "highlight" => glass.highlight = Some(colour_at(value, &here)?),
            "inset" => glass.inset = Some(read_shadow(value, &here)?),
            "gap" => glass.gap = number(value, &here, (0.0, 24.0))?,
            "radius" => glass.radius = number(value, &here, (0.0, 32.0))?,
            _ => notes.push(format!("\"{here}\" isn't something glass takes; ignored")),
        }
    }
    Ok(glass)
}

fn read_text(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<TextLook, Error> {
    let mut look = TextLook::default();
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "shadow" => look.shadow = Some(read_shadow(value, &here)?),
            "glow" => {
                let fields = object(value, &here)?;
                let colour = colour_at(
                    fields
                        .get("colour")
                        .ok_or_else(|| Error(format!("\"{here}\" needs a \"colour\"")))?,
                    &format!("{here}.colour"),
                )?;
                let size = fields.get("size").map_or(Ok(4.0), |size| {
                    number(size, &format!("{here}.size"), (1.0, 12.0))
                })?;
                look.glow = Some((colour, size));
            }
            "outline" => {
                let fields = object(value, &here)?;
                let width = fields.get("width").map_or(Ok(1.0), |width| {
                    number(width, &format!("{here}.width"), (0.5, 4.0))
                })?;
                let colour = colour_at(
                    fields
                        .get("colour")
                        .ok_or_else(|| Error(format!("\"{here}\" needs a \"colour\"")))?,
                    &format!("{here}.colour"),
                )?;
                look.outline = Some((width, colour));
            }
            "emboss" => look.emboss = value.as_bool().unwrap_or(false),
            _ => notes.push(format!("\"{here}\" isn't something text takes; ignored")),
        }
    }
    Ok(look)
}

fn read_particles(value: &Value, place: &str, notes: &mut Vec<String>) -> Result<Particles, Error> {
    let fields = object(value, place)?;
    let preset = named(
        fields
            .get("preset")
            .ok_or_else(|| Error(format!("\"{place}\" needs a \"preset\"")))?,
        &format!("{place}.preset"),
        Preset::ALL,
        Preset::name,
    )?;
    let on = fields.get("on").map_or(Ok(Trigger::Ambient), |on| {
        named(on, &format!("{place}.on"), Trigger::ALL, Trigger::name)
    })?;
    let (size, life, colour) = match preset {
        Preset::Bubbles => (12.0, 2200.0, 0xccff_ffff),
        Preset::Sparkles => (8.0, 900.0, 0xffff_e9a0),
        Preset::Dust => (3.0, 6000.0, 0x80ff_ffff),
        Preset::Scraps => (10.0, 2600.0, 0xfff2_ead8),
    };
    let mut particles = Particles {
        preset,
        on,
        colours: vec![colour],
        sprite: None,
        count: if on == Trigger::Ambient { 24 } else { 14 },
        rate: 2.0,
        size,
        life,
        // Ambient particles run on every frame for as long as the theme
        // shows: heavy unless the theme says otherwise.
        heavy: on == Trigger::Ambient,
    };
    for (key, value) in fields {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "preset" | "on" => {}
            "colours" | "colour" => {
                particles.colours = match value {
                    Value::Array(items) if !items.is_empty() && items.len() <= 8 => items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| colour_at(item, &format!("{here}[{index}]")))
                        .collect::<Result<_, _>>()?,
                    Value::String(_) => vec![colour_at(value, &here)?],
                    _ => return Err(Error(format!("\"{here}\" is 1 to 8 colours"))),
                }
            }
            "sprite" => {
                particles.sprite = Some(relative(
                    value
                        .as_str()
                        .ok_or_else(|| Error(format!("\"{here}\" is a file name")))?,
                    &here,
                )?);
            }
            "count" => particles.count = number(value, &here, (1.0, 64.0))? as u32,
            "rate" => particles.rate = number(value, &here, (0.1, 20.0))?,
            "size" => particles.size = number(value, &here, (2.0, 48.0))?,
            "life" | "duration" => particles.life = number(value, &here, (300.0, 12_000.0))?,
            "heavy" => particles.heavy = value.as_bool().unwrap_or(particles.heavy),
            _ => notes.push(format!(
                "\"{here}\" isn't something particles take; ignored"
            )),
        }
    }
    Ok(particles)
}

fn read_event(
    slot: EventSlot,
    value: &Value,
    place: &str,
    notes: &mut Vec<String>,
) -> Result<Option<Event>, Error> {
    let (effect, fields) = match value {
        Value::String(_) => (value, None),
        Value::Object(fields) => (
            fields
                .get("effect")
                .ok_or_else(|| Error(format!("\"{place}\" needs an \"effect\"")))?,
            Some(fields),
        ),
        _ => {
            return Err(Error(format!(
                "\"{place}\" is an effect's name or an object"
            )));
        }
    };
    let effect = named(
        effect,
        &format!("{place}.effect"),
        EventEffect::ALL,
        EventEffect::name,
    )?;
    if effect == EventEffect::None {
        return Ok(None);
    }
    if effect != slot.effect() {
        return Err(Error(format!(
            "\"{place}\" plays {} or none, not {}",
            slot.effect().name(),
            effect.name()
        )));
    }
    let mut event = Event {
        colour: match slot {
            EventSlot::Click => 0x40ff_ffff,
            EventSlot::Hover => 0x38ff_ffff,
            EventSlot::Select | EventSlot::Row => 0xffff_e45c,
            EventSlot::Reaction | EventSlot::NameHover => 0,
        },
        duration: match slot {
            EventSlot::Click => 450.0,
            EventSlot::Hover => 600.0,
            EventSlot::Select | EventSlot::Row => 260.0,
            EventSlot::Reaction => 320.0,
            EventSlot::NameHover => 450.0,
        },
    };
    for (key, value) in fields.into_iter().flatten() {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "effect" => {}
            "colour" => event.colour = colour_at(value, &here)?,
            "duration" => event.duration = number(value, &here, (80.0, 3000.0))?,
            _ => notes.push(format!(
                "\"{here}\" isn't something an event takes; ignored"
            )),
        }
    }
    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(json: &str) -> Result<Art, Error> {
        let mut art = Art::default();
        let mut notes = Vec::new();
        read_art(
            &serde_json::from_str(json).unwrap(),
            "art",
            &mut art,
            &mut notes,
        )?;
        Ok(art)
    }

    #[test]
    fn surfaces_take_one_layer_or_a_list() {
        let art = art(
            r##"{ "surfaces": {
                "window": [{ "image": "art/sky.jpg" }, { "image": "art/clouds.png", "fit": "tile", "drift": [12, 0] }],
                "chat": { "texture": "paper", "opacity": 0.3, "blend": "multiply", "colour": "#8a6" }
            } }"##,
        )
        .unwrap();
        assert_eq!(art.surfaces[&ArtSurface::Window].len(), 2);
        assert!(art.surfaces[&ArtSurface::Window][1].moves());
        let chat = &art.surfaces[&ArtSurface::Chat][0];
        assert_eq!(chat.fit, Fit::Tile);
        assert_eq!(chat.blend, Blend::Multiply);
        assert_eq!(
            art.files(),
            vec![
                PathBuf::from("art/sky.jpg"),
                PathBuf::from("art/clouds.png")
            ]
        );
        assert_eq!(art.moving(), (1, 0));
    }

    #[test]
    fn the_account_panel_is_still_read_by_its_first_name() {
        let art = art(
            r#"{ "surfaces": { "panel": { "texture": "grain" } }, "glass": { "surfaces": ["panel", "chat"] } }"#,
        )
        .unwrap();
        assert!(art.surfaces.contains_key(&ArtSurface::Account));
        assert_eq!(
            art.glass.unwrap().surfaces,
            vec![ArtSurface::Account, ArtSurface::Chat]
        );
        assert_eq!(ArtSurface::Account.ground(), "account");
    }

    #[test]
    fn urls_and_bad_values_are_refused() {
        assert!(art(r#"{ "surfaces": { "chat": { "image": "https://x.test/a.png" } } }"#).is_err());
        assert!(art(r#"{ "surfaces": { "chat": { "image": "a.png", "opacity": 2 } } }"#).is_err());
        assert!(
            art(r#"{ "surfaces": { "chat": { "image": "a.png", "texture": "paper" } } }"#).is_err()
        );
        assert!(art(r#"{ "glass": { "surfaces": ["window"] } }"#).is_err());
        assert!(art(r#"{ "events": { "click": "stamp" } }"#).is_err());
    }

    #[test]
    fn moving_overlays_and_ambient_particles_are_heavy() {
        let art = art(
            r#"{ "overlays": [{ "kind": "scanlines" }, { "kind": "band" }, { "kind": "grain", "period": 120, "heavy": false }],
                 "particles": [{ "preset": "dust" }, { "preset": "bubbles", "on": "message.sent" }] }"#,
        )
        .unwrap();
        assert!(!art.overlays[0].heavy && art.overlays[1].heavy && !art.overlays[2].heavy);
        assert_eq!(art.moving(), (3, 2));
        assert_eq!(art.particles[1].on, Trigger::MessageSent);
    }

    #[test]
    fn events_and_text() {
        let art = art(
            r##"{ "events": { "click": "ripple", "select": { "effect": "highlighter", "colour": "#ff0" }, "hover": "none" },
                 "text": { "names": { "outline": { "width": 1, "colour": "#000" }, "glow": { "colour": "#f0f" } } } }"##,
        )
        .unwrap();
        assert_eq!(art.events.len(), 2);
        assert_eq!(art.events[&EventSlot::Select].colour, 0xffff_ff00);
        let names = &art.text[&TextRole::Names];
        assert_eq!(names.outline, Some((1.0, 0xff00_0000)));
        assert_eq!(names.glow.map(|glow| glow.1), Some(4.0));
    }

    #[test]
    fn a_name_under_the_pointer_decrypts() {
        let hover =
            art(r#"{ "events": { "name_hover": { "effect": "decrypt", "duration": 600 } } }"#)
                .unwrap();
        assert_eq!(hover.events[&EventSlot::NameHover].duration, 600.0);
        assert!(art(r#"{ "events": { "name_hover": "ripple" } }"#).is_err());
        assert!(art(r#"{ "events": { "click": "decrypt" } }"#).is_err());
    }
}
