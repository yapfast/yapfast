//! A profile's effect and frame, as the Slint client's `ProfileEffect` and
//! `ProfileFrameLayer`: the effect lies over the card, its still at rest
//! and its layers while the pointer is on the card and pictures play
//! ([`pictures::plays`]), each layer from its start for its duration and again
//! after its delay when it loops, until the last that doesn't loop ends;
//! the frame's still layers lie round the card and past its sides
//! ([`Framed`]). Neither costs anything where the profile has none, and
//! the effect asks for no frame while it doesn't play: a tick falls due
//! only when a layer shows or goes, and the layers' own frames are their
//! pictures' ([`crate::animated::Animated`]).

use std::time::{Duration, Instant};

use iced::{
    Element, Length, Rectangle, Size, Task,
    widget::{Space, container, stack},
};
use yapfast_core::{
    application::AppState,
    chat::{EffectLayer, ProfileEffect},
};

use super::{Event, Mode, Profiles, Shown};
use crate::{
    app::Message,
    widgets::{FrameAnchor, FrameKind, FrameLayer, Framed, Pic},
};

/// How far a frame reaches past the card's sides: the card's and the full
/// profile's.
const FRAME_SIDE: f32 = 12.0;
const FRAME_SIDE_LARGE: f32 = 16.0;
/// How much lower the full profile's top staple sits.
const FRAME_TOP_SHIFT: f32 = 8.0;
/// A window shorter than this leaves no room above the full profile for
/// its frame, which then doesn't show, as Discord's.
const FRAME_ROOM: f32 = 785.0;

/// When the effect began to play on a card, by which card; ticks carry the
/// generation they were asked in, so an old one does nothing.
#[derive(Default)]
pub struct EffectClock {
    card: Option<Instant>,
    panel: Option<Instant>,
    generation: u64,
}

/// Whether a layer shows `time` milliseconds after the effect began.
fn layer_shown(layer: &EffectLayer, time: u64) -> bool {
    let Some(since) = time.checked_sub(u64::from(layer.start)) else {
        return false;
    };
    let duration = u64::from(layer.duration);
    let cycle = duration + u64::from(layer.loop_delay);
    (layer.looped || since <= duration)
        && (!layer.looped || layer.loop_delay == 0 || since % cycle.max(1) <= duration)
}

/// When the last layer that doesn't loop ends, and whether any loops.
fn end(effect: &ProfileEffect) -> (u64, bool) {
    let last = effect
        .layers
        .iter()
        .map(|layer| u64::from(layer.start) + u64::from(layer.duration))
        .max()
        .unwrap_or(0);
    (last, effect.layers.iter().any(|layer| layer.looped))
}

/// Whether the effect has played out `time` milliseconds in.
fn finished(effect: &ProfileEffect, time: u64) -> bool {
    let (last, loops) = end(effect);
    !loops && time > last
}

/// The next time after `time` that a layer shows or goes, or the effect
/// ends; none when nothing more changes.
fn next_change(effect: &ProfileEffect, time: u64) -> Option<u64> {
    let mut times = Vec::new();
    for layer in &effect.layers {
        let start = u64::from(layer.start);
        let duration = u64::from(layer.duration);
        times.push(start);
        if !layer.looped {
            times.push(start + duration + 1);
        } else if layer.loop_delay > 0 && time >= start {
            let cycle = duration + u64::from(layer.loop_delay);
            let into = start + (time - start) / cycle * cycle;
            times.extend([into + duration + 1, into + cycle]);
        }
    }
    let (last, loops) = end(effect);
    if !loops {
        times.push(last + 1);
    }
    times.into_iter().filter(|&at| at > time).min()
}

/// The effect a profile wears, when the profile switches show effects and
/// the core has fetched it.
pub(super) fn worn_effect<'a>(state: &'a AppState, shown: &Shown) -> Option<&'a ProfileEffect> {
    let sku = state
        .cosmetics
        .effect(shown.look.profile().effect.as_deref())?;
    state
        .profile_effects
        .iter()
        .find(|effect| effect.sku == sku)
        .map(|effect| &**effect)
}

/// The effect's pictures to ask for: its still, and its layers while the
/// card is pointed at.
pub(super) fn effect_pictures(state: &AppState, shown: &Shown, pointed: bool) -> Vec<String> {
    let Some(effect) = worn_effect(state, shown) else {
        return Vec::new();
    };
    let mut urls: Vec<String> = effect.still.iter().cloned().collect();
    if pointed {
        urls.extend(effect.layers.iter().map(|layer| layer.src.clone()));
    }
    urls
}

/// The frame's pictures to ask for.
pub(super) fn frame_pictures(state: &AppState, shown: &Shown) -> Vec<String> {
    let profile = shown.look.profile();
    state
        .cosmetics
        .frame(Some(&profile.frame))
        .into_iter()
        .flat_map(|frame| frame.iter())
        .map(|layer| layer.src.clone())
        .collect()
}

impl Profiles {
    /// The pointer came onto a card or left it: its effect starts from the
    /// beginning, or stops. `panel` is the card beside a DM.
    pub(super) fn point_effect(
        &mut self,
        panel: bool,
        pointed: bool,
        state: &AppState,
    ) -> Task<Message> {
        let clock = &mut self.effect;
        let since = if panel {
            &mut clock.panel
        } else {
            &mut clock.card
        };
        if pointed == since.is_some() {
            return Task::none();
        }
        *since = pointed.then(Instant::now);
        clock.generation += 1;
        let generation = clock.generation;
        self.effect_tick(generation, state)
    }

    /// A card opened or grew to the full profile: nothing points at it
    /// yet, so its effect rests until the pointer comes onto it.
    pub(super) fn unpoint_card(&mut self) {
        self.pointed = false;
        self.effect.card = None;
        self.effect.generation += 1;
    }

    /// A layer of a playing effect shows or goes now: the view takes it,
    /// and the next such time is asked for.
    pub(super) fn effect_tick(&self, generation: u64, state: &AppState) -> Task<Message> {
        let clock = &self.effect;
        if generation != clock.generation {
            return Task::none();
        }
        let now = Instant::now();
        let next = [
            (self.shown.as_ref(), clock.card),
            (self.panel.as_ref(), clock.panel),
        ]
        .into_iter()
        .filter_map(|(shown, since)| {
            let effect = worn_effect(state, shown?)?;
            let time = now.duration_since(since?).as_millis() as u64;
            next_change(effect, time).map(|at| at - time)
        })
        .min();
        match next {
            Some(wait) => crate::after(
                Duration::from_millis(wait),
                Message::Profile(Event::EffectTick(generation)),
            ),
            None => Task::none(),
        }
    }
}

/// The card `card`, `width` wide, with its effect over it and its frame
/// round it, as `mode` shows them.
pub(super) fn dressed<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &'a Shown,
    mode: Mode,
    width: f32,
    card: Element<'a, Message>,
) -> Element<'a, Message> {
    let since = match mode {
        Mode::Docked => ctx.profiles.effect.panel,
        Mode::Card | Mode::Side => ctx.profiles.effect.card,
    };
    let card = match effect_view(ctx, shown, width, since) {
        Some(effect) => stack![card, effect].into(),
        None => card,
    };
    let framed = match mode {
        Mode::Card => true,
        Mode::Side => ctx.look.window.height > FRAME_ROOM,
        Mode::Docked => false,
    };
    match framed.then(|| frame_art(ctx, shown, mode, width)) {
        Some(layers) if !layers.is_empty() => {
            let (side, shift) = if mode.large() {
                (FRAME_SIDE_LARGE, FRAME_TOP_SHIFT)
            } else {
                (FRAME_SIDE, 0.0)
            };
            Framed::new(card, layers, side, shift).into()
        }
        _ => card,
    }
}

/// The effect over a card `width` wide: its still at rest, its layers
/// showing `since` it began to play while pictures play and until it
/// has played out.
fn effect_view<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &Shown,
    width: f32,
    since: Option<Instant>,
) -> Option<Element<'a, Message>> {
    let effect = worn_effect(ctx.state, shown).filter(|effect| !effect.layers.is_empty())?;
    let pictures = &ctx.state.pictures;
    let time = since.map(|since| since.elapsed().as_millis() as u64);
    let playing = ctx.plays && time.is_some_and(|time| !finished(effect, time));
    let tall = |url: &str| {
        let picture = pictures.get(url)?;
        (picture.width > 0).then(|| (width * picture.height as f32 / picture.width as f32).round())
    };
    let mut layers = stack![Space::new().width(Length::Fill).height(Length::Fill)];
    if playing {
        let time = time.unwrap_or_default();
        for layer in effect
            .layers
            .iter()
            .filter(|layer| layer_shown(layer, time))
        {
            let Some(height) = tall(&layer.src) else {
                continue;
            };
            let at = Rectangle::new(
                iced::Point::new(layer.x as f32, layer.y as f32),
                Size::new(width, height),
            );
            let size = Size::new(width, (at.y + height).max(1.0));
            if let Some(moving) = ctx.moving(&layer.src, size, at, false) {
                layers = layers.push(moving);
            } else if let Some(still) = ctx.picture(&layer.src, width as u32, height as u32, false)
            {
                layers = layers.push(Pic::new(still, size).at(at));
            }
        }
    } else {
        let url = effect.still.as_deref()?;
        let height = tall(url)?;
        let still = ctx.picture(url, width as u32, height as u32, false)?;
        layers = layers.push(Pic::new(still, Size::new(width, height)));
    }
    Some(
        container(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into(),
    )
}

/// The editor's tile of the effect worn: its still over a little
/// card, as Discord's tile shows it (a light panel, the name's and
/// bio's bars at the lower left), `size` big.
pub(super) fn effect_tile<'a>(
    ctx: crate::ctx::Ctx<'a>,
    shown: &Shown,
    size: Size,
) -> Option<Element<'a, Message>> {
    let effect = worn_effect(ctx.state, shown)?;
    if effect.still.is_none() && effect.layers.is_empty() {
        return None;
    }
    let bar = |x: f32, y: f32, width: f32, height: f32, radius: f32, colour: u32| {
        crate::controls::placed(
            container(Space::new())
                .width(size.width * width)
                .height(size.height * height)
                .style(move |_| crate::style::rounded(crate::style::hex(colour), radius)),
            size.width * x,
            size.height * y,
        )
    };
    let mut tile = stack![
        Space::new().width(size.width).height(size.height),
        bar(0.2, 0.12, 0.6, 0.38, 2.0, 0x8f8f93),
        bar(0.19, 0.81, 0.28, 0.06, 1.0, 0x4d4e63),
        bar(0.19, 0.9, 0.26, 0.07, 1.0, 0x414255),
    ];
    let still = effect.still.as_deref().and_then(|url| {
        let picture = ctx.state.pictures.get(url)?;
        let height = (size.width * picture.height as f32 / picture.width.max(1) as f32).round();
        let handle = ctx.picture(url, size.width as u32, height as u32, false)?;
        Some(Pic::new(handle, Size::new(size.width, height)))
    });
    if let Some(still) = still {
        tile = tile.push(container(still).height(size.height).clip(true));
    }
    Some(tile.into())
}

/// The frame's layers at the width it's drawn, `side` past each of the
/// card's sides; the full profile leaves out the staple at the foot,
/// as Discord's does.
fn frame_art(ctx: crate::ctx::Ctx<'_>, shown: &Shown, mode: Mode, width: f32) -> Vec<FrameLayer> {
    let state = ctx.state;
    let profile = shown.look.profile();
    let Some(frame) = state.cosmetics.frame(Some(&profile.frame)) else {
        return Vec::new();
    };
    let side = if mode.large() {
        FRAME_SIDE_LARGE
    } else {
        FRAME_SIDE
    };
    let across = width + 2.0 * side;
    frame
        .iter()
        .filter_map(|layer| {
            let kind = match layer.kind.as_str() {
                "staple" => FrameKind::Staple,
                "rail" => FrameKind::Rail,
                "border" => FrameKind::Border,
                _ => return None,
            };
            let anchor = match layer.anchor.as_str() {
                "bottom" => FrameAnchor::Bottom,
                "center" => FrameAnchor::Center,
                _ => FrameAnchor::Top,
            };
            if mode.large() && kind == FrameKind::Staple && anchor == FrameAnchor::Bottom {
                return None;
            }
            let picture = state.pictures.get(&layer.src)?;
            if picture.width == 0 {
                return None;
            }
            let height = (across * picture.height as f32 / picture.width as f32).round();
            let handle = ctx.picture(&layer.src, across as u32, height as u32, false)?;
            Some(FrameLayer {
                picture: handle,
                height,
                kind,
                anchor,
                front: layer.front,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(start: u32, duration: u32, looped: bool, loop_delay: u32) -> EffectLayer {
        EffectLayer {
            src: String::new(),
            x: 0,
            y: 0,
            start,
            duration,
            looped,
            loop_delay,
        }
    }

    #[test]
    fn layers_show_from_their_start_for_their_duration_and_again_when_they_loop() {
        let once = layer(100, 500, false, 0);
        assert!(!layer_shown(&once, 50));
        assert!(layer_shown(&once, 100) && layer_shown(&once, 600));
        assert!(!layer_shown(&once, 601));
        let again = layer(0, 400, true, 600);
        assert!(layer_shown(&again, 300));
        assert!(!layer_shown(&again, 700), "in its delay");
        assert!(layer_shown(&again, 1100), "the next round");
        let always = layer(0, 400, true, 0);
        assert!(layer_shown(&always, 5000));
    }

    #[test]
    fn ticks_fall_only_when_a_layer_shows_or_goes() {
        let effect = ProfileEffect {
            sku: "1".into(),
            still: None,
            layers: vec![layer(100, 500, false, 0), layer(200, 300, false, 0)],
        };
        assert_eq!(next_change(&effect, 0), Some(100));
        assert_eq!(next_change(&effect, 100), Some(200));
        assert_eq!(next_change(&effect, 200), Some(501));
        assert_eq!(next_change(&effect, 501), Some(601));
        assert!(finished(&effect, 601));
        assert_eq!(next_change(&effect, 601), None, "played out: no more ticks");
        // A layer that loops forever with no delay asks for nothing more
        // once it shows.
        let looping = ProfileEffect {
            layers: vec![layer(0, 400, true, 0)],
            ..effect
        };
        assert_eq!(next_change(&looping, 10), None);
        assert!(!finished(&looping, 100_000));
    }
}
