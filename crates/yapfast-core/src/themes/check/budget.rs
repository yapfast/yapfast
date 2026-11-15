//! What a theme's always-on motion costs while nobody touches the window:
//! an estimate of the processor it keeps busy, in cores, from what it
//! declares, and what would make an expensive piece cheap.
//!
//! The figures are what Yapfast measured with its software renderer drawing
//! a 1600 × 900 window in Full, focused, with the pointer still over the
//! conversation (Hurt's motion, one piece at a time: a full-window overlay
//! redrawn every frame about 0.95 of a core, its flare 0.86, dust 0.41, the
//! window tint's breathe 0.28, the record's spin 0.24 and the unread pulses
//! 0.22; a spinning record alone 0.20). What drives them: art that moves smoothly (a drift, a flare, a
//! band, particles) is redrawn on every tick of one clock, 60 a second in
//! Full; art that only steps (grain's jumps, a sprite sheet's frames) only
//! when it steps; a declared animation each time its value changes. A frame
//! costs about 0.2 of a core at 60 a second before anything is drawn in it,
//! and drawing the whole window about 0.75 more. GPU rendering costs less, a
//! larger window more.
//!
//! Each piece is priced as if it moved alone; together they share frames,
//! so the sum is the most they cost, and past one core the window can't
//! keep up and drops frames instead. Balanced holds heavy pieces still and
//! runs the rest at 30 frames a second; while Yapfast isn't focused, a
//! theme's motion runs at no more than 15 (see `motion.unfocused`), and
//! nothing runs while the window can't be seen.

use crate::themes::Performance;
use crate::themes::cursors::Animated;
use crate::themes::motion;
use crate::themes::package::art::{ArtSurface, Layer, OverlayKind, Trigger};
use crate::themes::package::{Animation, Property, Repeat, Resolved, Slot};

/// More than this, idle, is worth a warning.
pub const WARN_CORES: f32 = 0.15;
/// A frame's own cost, 60 a second, before anything is drawn in it.
const FRAME: f32 = 0.2;
/// Drawing the whole window, 60 times a second, beyond the frame.
const WINDOW: f32 = 0.75;
/// A flare's radial gradient draws slower than a flat picture: Hurt's, 900
/// pixels across, measured 0.86 where its area alone predicts 0.62.
const GRADIENT: f32 = 1.5;
/// A live ambient particle, 60 frames a second: Hurt's dust, about 9 alive,
/// measured 0.41.
const PARTICLE: f32 = 0.045;
/// A small slot's animation (a badge, a name, an avatar) at 60 frames a
/// second: little to draw, but each change is a frame. Measured alone, a
/// spinning record cost 0.20 of a core (0.24 in Hurt); a rotated picture's
/// box redraws larger.
const SLOT: f32 = 0.2;
const SPUN: f32 = 0.24;
/// Foil sweeping by itself over cards and tiles: the art clock's frames
/// and the band's box.
const FOIL: f32 = 0.1;
/// A full-window tint, beside its own changes, is drawn over every frame
/// anything else redraws: Hurt's breathe, about 3 changes a second,
/// measured 0.28.
const VEIL: f32 = 0.24;
/// The window the figures are for.
const SIZE: (f32, f32) = (1600.0, 900.0);
/// A trail is fine below these.
const TRAIL_PER_STEP: u32 = 3;
const TRAIL_COUNT: u32 = 96;

/// One piece of always-on motion: what it is, what it costs, and how to
/// make it cheaper.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub what: String,
    pub cores: f32,
    /// Whether Balanced holds it still.
    pub heavy: bool,
    pub fix: String,
}

/// A variant's always-on motion, priced, most expensive first.
pub fn pieces(variant: &Resolved) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let art = &variant.art;
    let clock = 60.0;
    for overlay in &art.overlays {
        if !overlay.moves() {
            continue;
        }
        let area = if overlay.chat {
            pane(ArtSurface::Chat)
        } else {
            1.0
        };
        let (what, cores, fix) = match overlay.kind {
            OverlayKind::Grain => {
                // Grain redraws only when it jumps.
                let rate = (1000.0 / overlay.period).min(clock);
                (
                    format!(
                        "grain over the {} changing every {:.0} ms",
                        if overlay.chat { "conversation" } else { "window" },
                        overlay.period
                    ),
                    drawn(area, rate),
                    format!(
                        "grain period {:.0}ms redraws the {} {:.0} times a second: try ≥600ms, or a still grain (period 0)",
                        overlay.period,
                        if overlay.chat {
                            "conversation"
                        } else {
                            "whole window"
                        },
                        rate
                    ),
                )
            }
            OverlayKind::Flare => {
                let across = overlay.size / SIZE.1;
                let area = (across * across * SIZE.1 / SIZE.0 * GRADIENT).min(area * GRADIENT);
                (
                    format!("a flare {:.0} px across", overlay.size),
                    drawn(area, clock),
                    "a pulsing flare redraws its square every frame: make it smaller, or still (period 0), or mark it heavy so Balanced holds it".into(),
                )
            }
            OverlayKind::Band => (
                format!("a band rolling down every {:.0} ms", overlay.period),
                drawn((2.0 * overlay.size / SIZE.1 + 0.1).min(area), clock),
                "a rolling band redraws its strip every frame: make it thinner, or mark it heavy".into(),
            ),
            _ => (
                format!("a moving {} overlay", overlay.kind.name()),
                drawn(
                    area,
                    overlay.layer.as_ref().map_or(clock, |layer| layer_rate(layer, clock)),
                ),
                "a moving overlay redraws the whole window as it moves: hold it still, slow its frames, or mark it heavy".into(),
            ),
        };
        pieces.push(Piece {
            what,
            cores,
            heavy: overlay.heavy,
            fix,
        });
    }
    for (surface, layers) in &art.surfaces {
        for layer in layers.iter().filter(|layer| layer.moves()) {
            let area = layer_area(*surface, layer);
            pieces.push(Piece {
                what: format!(
                    "a {} picture on {}",
                    if layer.drift != (0.0, 0.0) {
                        "drifting"
                    } else {
                        "sprite-sheet"
                    },
                    surface.name()
                ),
                cores: drawn(area, layer_rate(layer, clock)),
                heavy: layer.heavy,
                fix: format!(
                    "a moving picture redraws {} as it moves: give it a size, fewer frames a second, or mark it heavy so Balanced holds it",
                    if area >= 0.99 {
                        "the whole window".to_owned()
                    } else {
                        format!("the {}", surface.name())
                    }
                ),
            });
        }
    }
    if art.foil.is_some() {
        pieces.push(Piece {
            what: "foil sweeping over cards".into(),
            cores: drawn(FOIL, clock),
            heavy: false,
            fix: "foil sweeps by itself every frame while a card shows: a longer period doesn't help, leave it out or keep it to the pointer".into(),
        });
    }
    for particles in &art.particles {
        if particles.on != Trigger::Ambient {
            continue;
        }
        let alive = alive(particles.count, particles.rate, particles.life);
        pieces.push(Piece {
            what: format!(
                "{} ambient {} ({alive:.0} alive at once)",
                particles.count,
                particles.preset.name()
            ),
            cores: alive * PARTICLE,
            heavy: particles.heavy,
            fix: format!(
                "{alive:.0} particles alive move every frame: fewer (count, rate × life), or mark them heavy"
            ),
        });
    }
    for animation in &variant.animations {
        if let Some(piece) = animation_piece(animation) {
            pieces.push(piece);
        }
    }
    pieces.sort_by(|a, b| b.cores.total_cmp(&a.cores));
    pieces
}

/// What a declared animation costs, if anything.
fn animation_piece(animation: &Animation) -> Option<Piece> {
    // One pass and it rests: not always on.
    if animation.repeat == Repeat::Once {
        return None;
    }
    let changes = changes_a_second(animation);
    if changes <= 0.0 {
        return None;
    }
    let what = format!(
        "the {} {} animation, period {:.0} ms",
        animation.slot.name(),
        animation.property.name(),
        animation.period
    );
    let (cores, fix) = match animation.slot {
        Slot::Window | Slot::Accent => {
            let veil = if animation.property == Property::Opacity || animation.slot == Slot::Window
            {
                VEIL
            } else {
                0.0
            };
            (
                (veil + (FRAME + WINDOW) * changes / 60.0).min(FRAME + WINDOW),
                format!(
                    "{} redraws the whole window {changes:.0} times a second: fewer steps, a longer period, or a smaller change",
                    if animation.slot == Slot::Window {
                        "a window tint"
                    } else {
                        "an accent colour animation"
                    }
                ),
            )
        }
        slot => {
            let each = if animation.property == Property::Rotation {
                SPUN
            } else {
                SLOT
            };
            let many = if slot == Slot::RailIcon { 3.0 } else { 1.0 };
            (
                each * many * changes / 60.0,
                format!("it changes {changes:.0} times a second: use steps, or a longer period"),
            )
        }
    };
    Some(Piece {
        what,
        cores,
        heavy: animation.heavy,
        fix,
    })
}

/// How many times a second an animation's value changes in Full: a
/// smooth number every frame, a smooth colour as often as one of its 8-bit
/// channels moves, a stepped one at each step whose value differs.
fn changes_a_second(animation: &Animation) -> f32 {
    let passes = 1000.0 / animation.period.max(1.0);
    let cap = motion::frame_rate(animation, Performance::Full) as f32;
    let colour = animation.property.is_colour();
    let count = if colour {
        animation.colours.len()
    } else {
        animation.values.len()
    };
    if count < 2 {
        return 0.0;
    }
    let differ = |a: usize, b: usize| {
        if colour {
            animation.colours[a] != animation.colours[b]
        } else {
            animation.values[a] != animation.values[b]
        }
    };
    match animation.steps {
        Some(steps) if steps > 1 => {
            // The value each step shows, as `motion::value_at` picks it.
            let at = |step: u32| {
                ((step as f32 / (steps - 1) as f32) * (count - 1) as f32).round() as usize
            };
            let changing = (1..steps)
                .filter(|step| differ(at(step - 1), at(*step)))
                .count();
            (changing as f32 * passes).min(cap)
        }
        Some(_) => 0.0,
        None if colour => {
            // Each span moves as far as its farthest channel.
            let mut steps = 0.0;
            for pair in animation.colours.windows(2) {
                steps += (0..4)
                    .map(|byte| {
                        let channel = |colour: u32| ((colour >> (byte * 8)) & 0xff) as f32;
                        (channel(pair[0]) - channel(pair[1])).abs()
                    })
                    .fold(0.0, f32::max);
            }
            (steps * passes).min(cap)
        }
        None => {
            if (1..count).any(|index| differ(index - 1, index)) {
                cap
            } else {
                0.0
            }
        }
    }
}

/// What redrawing `area` of the window `rate` times a second costs.
fn drawn(area: f32, rate: f32) -> f32 {
    (FRAME + WINDOW * area) * rate / 60.0
}

/// How much of a 1600 × 900 window a surface takes, about.
fn pane(surface: ArtSurface) -> f32 {
    match surface {
        ArtSurface::Window => 1.0,
        ArtSurface::Chat => 0.55,
        ArtSurface::Sidebar | ArtSurface::Members | ArtSurface::ProfilePane => 0.17,
        ArtSurface::Rail => 0.05,
        ArtSurface::Composer | ArtSurface::Account => 0.04,
        _ => 0.1,
    }
}

fn layer_area(surface: ArtSurface, layer: &Layer) -> f32 {
    match layer.size {
        Some((width, height)) => ((width * height) / (SIZE.0 * SIZE.1)).min(pane(surface)),
        None => pane(surface),
    }
}

/// How often a moving picture redraws: every frame while it drifts,
/// otherwise as often as its sprite sheet's frame turns.
fn layer_rate(layer: &Layer, clock: f32) -> f32 {
    match layer.frames {
        Some(frames) if layer.drift == (0.0, 0.0) && frames.count > 1 => {
            (frames.count as f32 * 1000.0 / frames.period.max(1.0)).min(clock)
        }
        _ => clock,
    }
}

/// About how many of a set are alive at once.
fn alive(count: u32, rate: f32, life: f32) -> f32 {
    (rate * life / 1000.0).min(count as f32).max(0.0)
}

/// The estimate, in cores, in Full and in Balanced (heavy pieces still,
/// the rest at 30 frames a second).
pub fn idle(pieces: &[Piece]) -> (f32, f32) {
    let full = pieces.iter().map(|piece| piece.cores).sum();
    let balanced = pieces
        .iter()
        .filter(|piece| !piece.heavy)
        .map(|piece| piece.cores / 2.0)
        .sum();
    (full, balanced)
}

/// Warnings for a variant: each expensive piece with its fix, then the
/// whole when it's over [`WARN_CORES`] without one piece being.
pub fn warnings(pieces: &[Piece]) -> Vec<String> {
    let mut warnings: Vec<String> = pieces
        .iter()
        .filter(|piece| piece.cores >= WARN_CORES)
        .map(|piece| {
            format!(
                "idle cost: {} costs about {:.2} of a core while nobody touches the window — {}",
                piece.what, piece.cores, piece.fix
            )
        })
        .collect();
    let (full, _) = idle(pieces);
    if warnings.is_empty() && full >= WARN_CORES {
        let names: Vec<&str> = pieces
            .iter()
            .take(3)
            .map(|piece| piece.what.as_str())
            .collect();
        warnings.push(format!(
            "idle cost: the theme's always-on motion costs about {full:.2} of a core together ({}): hold some still, or mark them heavy",
            names.join(", ")
        ));
    }
    warnings
}

/// Warnings for a cursor trail, which runs while the pointer moves.
pub fn trail_warnings(animated: &Animated) -> Vec<String> {
    let Some(trail) = &animated.trail else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if trail.per_step > TRAIL_PER_STEP {
        warnings.push(format!(
            "the cursor trail lays {} sprites each step: every one redraws while it lives, try at most {TRAIL_PER_STEP}, or a larger \"every\"",
            trail.per_step
        ));
    }
    if trail.count > TRAIL_COUNT {
        warnings.push(format!(
            "the cursor trail keeps up to {} sprites alive: try at most {TRAIL_COUNT}",
            trail.count
        ));
    }
    warnings
}
