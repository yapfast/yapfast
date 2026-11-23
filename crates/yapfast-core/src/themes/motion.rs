//! What a declared animation shows at a moment, and what a text effect shows.
//!
//! Both are pure functions of time, so the interface keeps one clock and
//! asks; nothing here holds state. The interface ticks only while something
//! is moving, at the rate [`frame_rate`] gives, and stops when the last
//! animation or effect is done.

use super::Performance;
use super::package::{Animation, Curve, Effect, Property, Repeat};

/// A value an animation gives: a number, or an ARGB colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Colour(u32),
}

/// Whether an animation runs at this setting, given that the person asked
/// for less motion or not. Light and reduced motion run none; Balanced leaves
/// heavy ones still.
pub fn runs(animation: &Animation, performance: Performance, reduced: bool) -> bool {
    !reduced
        && match performance {
            Performance::Full => true,
            Performance::Balanced => !animation.heavy,
            Performance::Light => false,
        }
}

/// Whether text effects play.
pub fn effects_play(performance: Performance, reduced: bool) -> bool {
    !reduced && performance != Performance::Light
}

/// How many times a second an animation needs a new frame: its step rate when
/// it steps, otherwise smooth at the setting's cap.
pub fn frame_rate(animation: &Animation, performance: Performance) -> u32 {
    let cap = match performance {
        Performance::Full => 60,
        Performance::Balanced => 30,
        Performance::Light => 0,
    };
    match animation.steps {
        Some(steps) => {
            let rate = (steps as f32 * 1000.0 / animation.period).ceil() as u32;
            rate.clamp(1, cap.max(1))
        }
        None => cap,
    }
}

/// Whether an animation that plays once has finished at `elapsed`
/// milliseconds after it started.
pub fn finished(animation: &Animation, elapsed: f32) -> bool {
    animation.repeat == Repeat::Once && elapsed >= animation.delay + animation.period
}

/// The value at `elapsed` milliseconds after the animation started. Before
/// its delay, and after a single pass, it rests on its first and last value.
pub fn value_at(animation: &Animation, elapsed: f32) -> Value {
    let count = if animation.property.is_colour() {
        animation.colours.len()
    } else {
        animation.values.len()
    };
    if count == 0 {
        return Value::Number(rest(animation.property));
    }
    let time = (elapsed - animation.delay).max(0.0);
    let pass = time / animation.period;
    let progress = match animation.repeat {
        Repeat::Once => pass.min(1.0),
        Repeat::Loop => pass.fract(),
        Repeat::Alternate => {
            let whole = pass.rem_euclid(2.0);
            if whole > 1.0 { 2.0 - whole } else { whole }
        }
    };
    // Steps jump between the values; otherwise the curve eases across each
    // span between two of them.
    let (index, within) = match animation.steps {
        Some(steps) => {
            let step = ((progress * steps as f32).floor() as usize).min(steps as usize - 1);
            let at = if steps <= 1 {
                0.0
            } else {
                step as f32 / (steps - 1) as f32
            };
            let position = at * (count - 1) as f32;
            (position.round() as usize, 0.0)
        }
        None => {
            let position = progress * (count - 1) as f32;
            let index = (position.floor() as usize).min(count.saturating_sub(2));
            let within = position - index as f32;
            let eased = match animation.bezier {
                Some([x1, y1, x2, y2]) => cubic_bezier(x1, y1, x2, y2, within.clamp(0.0, 1.0)),
                None => ease(animation.easing, within),
            };
            (index, eased)
        }
    };
    let next = (index + 1).min(count - 1);
    if animation.property.is_colour() {
        Value::Colour(mix(
            animation.colours[index],
            animation.colours[next],
            within,
        ))
    } else {
        let (from, to) = (animation.values[index], animation.values[next]);
        Value::Number(from + (to - from) * within)
    }
}

/// What a property shows while nothing moves it.
pub fn rest(property: Property) -> f32 {
    match property {
        Property::Opacity | Property::Scale => 1.0,
        _ => 0.0,
    }
}

/// A curve from the fixed set at `t` from 0 to 1: CSS's named easings, and
/// the ease-out back, bounce and elastic curves of the common easing set.
pub fn ease(curve: Curve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Curve::Linear => t,
        Curve::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
        Curve::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
        Curve::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
        Curve::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
        Curve::Spring => {
            let (c1, c3) = (1.701_58, 2.701_58);
            1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
        }
        Curve::Bounce => {
            let (n, d) = (7.5625, 2.75);
            if t < 1.0 / d {
                n * t * t
            } else if t < 2.0 / d {
                let t = t - 1.5 / d;
                n * t * t + 0.75
            } else if t < 2.5 / d {
                let t = t - 2.25 / d;
                n * t * t + 0.9375
            } else {
                let t = t - 2.625 / d;
                n * t * t + 0.984_375
            }
        }
        Curve::Elastic => {
            if t == 0.0 || t == 1.0 {
                t
            } else {
                let c4 = std::f32::consts::TAU / 3.0;
                2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
    }
}

/// CSS's `cubic-bezier()` at `x`, solved for the curve's own parameter.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let curve = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let (mut low, mut high) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if curve(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve(y1, y2, (low + high) / 2.0)
}

/// Two ARGB colours mixed, `amount` of the way from `from` to `to`.
pub fn mix(from: u32, to: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let (a, b) = (
            ((from >> shift) & 0xff) as f32,
            ((to >> shift) & 0xff) as f32,
        );
        ((a + (b - a) * amount).round() as u32) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
}

/// The glyphs a decrypting letter shows before it settles.
const SCRAMBLE: &[char] = &[
    '#', '%', '&', '*', '+', '=', '?', '@', '0', '1', '<', '>', '/', '\\', '$', 'x', 'z', '7',
];

/// A text effect's text at `elapsed` milliseconds after the text changed, and
/// whether it's still playing. `seed` varies the scramble between texts.
pub fn effect_text(
    effect: Effect,
    text: &str,
    elapsed: f32,
    duration: f32,
    seed: u64,
) -> (String, bool) {
    let letters: Vec<char> = text.chars().collect();
    if letters.is_empty() || elapsed >= duration {
        return (text.to_owned(), false);
    }
    let progress = (elapsed / duration).clamp(0.0, 1.0);
    match effect {
        Effect::Typewriter => {
            let shown = (progress * letters.len() as f32).floor() as usize;
            let mut out: String = letters[..shown.min(letters.len())].iter().collect();
            out.push('▌');
            (out, true)
        }
        Effect::Decrypt => {
            // Each letter settles in turn, the rest flicker between glyphs
            // that change every 50 ms.
            let settled = (progress * letters.len() as f32).floor() as usize;
            let frame = (elapsed / 50.0) as u64;
            let out = letters
                .iter()
                .enumerate()
                .map(|(index, letter)| {
                    if index < settled || letter.is_whitespace() {
                        *letter
                    } else {
                        let mixed = seed
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(
                                (index as u64 + 1).wrapping_mul(1_442_695_040_888_963_407),
                            )
                            .wrapping_add(frame.wrapping_mul(2_862_933_555_777_941_757));
                        SCRAMBLE[((mixed >> 33) % SCRAMBLE.len() as u64) as usize]
                    }
                })
                .collect();
            (out, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::package::{Animation, Slot};
    use super::*;

    fn animation(values: &[f32], steps: Option<u32>, repeat: Repeat) -> Animation {
        Animation {
            slot: Slot::Unread,
            property: Property::Opacity,
            values: values.to_vec(),
            colours: Vec::new(),
            period: 1000.0,
            steps,
            easing: Curve::Linear,
            repeat,
            delay: 0.0,
            heavy: false,
            bezier: None,
            phase: 0.0,
        }
    }

    #[test]
    fn a_cubic_bezier_eases_between_values() {
        let mut fade = animation(&[0.0, 1.0], None, Repeat::Once);
        fade.bezier = Some([0.0, 0.0, 1.0, 1.0]);
        let Value::Number(middle) = value_at(&fade, 500.0) else {
            panic!()
        };
        assert!((middle - 0.5).abs() < 0.01);
        fade.bezier = Some([0.9, 0.0, 1.0, 0.1]);
        let Value::Number(slow) = value_at(&fade, 500.0) else {
            panic!()
        };
        assert!(slow < 0.2);
    }

    #[test]
    fn moves_through_its_values() {
        let fade = animation(&[0.0, 1.0], None, Repeat::Loop);
        assert_eq!(value_at(&fade, 0.0), Value::Number(0.0));
        assert_eq!(value_at(&fade, 500.0), Value::Number(0.5));
        assert_eq!(value_at(&fade, 1250.0), Value::Number(0.25));
        let back = animation(&[0.0, 1.0], None, Repeat::Alternate);
        assert_eq!(value_at(&back, 1500.0), Value::Number(0.5));
        assert_eq!(value_at(&back, 1750.0), Value::Number(0.25));
        let once = animation(&[0.0, 1.0], None, Repeat::Once);
        assert_eq!(value_at(&once, 5000.0), Value::Number(1.0));
        assert!(finished(&once, 1000.0));
    }

    #[test]
    fn steps_jump() {
        let blink = animation(&[1.0, 0.0], Some(2), Repeat::Loop);
        assert_eq!(value_at(&blink, 100.0), Value::Number(1.0));
        assert_eq!(value_at(&blink, 600.0), Value::Number(0.0));
        assert_eq!(frame_rate(&blink, Performance::Full), 2);
    }

    #[test]
    fn three_values_take_two_spans() {
        let pulse = animation(&[0.0, 1.0, 0.0], None, Repeat::Loop);
        assert_eq!(value_at(&pulse, 500.0), Value::Number(1.0));
        assert_eq!(value_at(&pulse, 750.0), Value::Number(0.5));
    }

    #[test]
    fn light_and_reduced_motion_keep_everything_still() {
        let mut flicker = animation(&[0.0, 1.0], None, Repeat::Loop);
        assert!(runs(&flicker, Performance::Balanced, false));
        assert!(!runs(&flicker, Performance::Balanced, true));
        assert!(!runs(&flicker, Performance::Light, false));
        flicker.heavy = true;
        assert!(!runs(&flicker, Performance::Balanced, false));
        assert!(runs(&flicker, Performance::Full, false));
        assert!(!effects_play(Performance::Light, false));
    }

    #[test]
    fn curves_start_and_end_where_they_should() {
        for curve in Curve::ALL {
            assert!(ease(curve, 0.0).abs() < 0.01, "{curve:?} at 0");
            assert!((ease(curve, 1.0) - 1.0).abs() < 0.01, "{curve:?} at 1");
        }
        assert!(ease(Curve::Spring, 0.8) > 1.0, "a spring overshoots");
    }

    #[test]
    fn effects_end_on_the_text() {
        let (typed, playing) = effect_text(Effect::Typewriter, "general", 300.0, 700.0, 1);
        assert_eq!(typed, "gen▌");
        assert!(playing);
        let (scrambled, _) = effect_text(Effect::Decrypt, "general", 100.0, 700.0, 1);
        assert_eq!(scrambled.chars().count(), 7);
        assert!(scrambled.starts_with('g'));
        assert_ne!(scrambled, "general");
        assert_eq!(
            effect_text(Effect::Decrypt, "general", 700.0, 700.0, 1),
            ("general".to_owned(), false)
        );
    }

    #[test]
    fn colours_mix() {
        assert_eq!(mix(0xff00_0000, 0xffff_ffff, 0.5), 0xff80_8080);
    }
}
