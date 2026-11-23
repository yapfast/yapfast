//! A theme's cost in plain words, for Settings → Themes: how heavy it is to
//! run, in a phrase, and its cost line sorted into rows to show on request.
//!
//! It reads the line [`Cost::text`] writes, since that line is what an
//! installed theme keeps (`lock.json`'s `cost`) and what the gallery holds.
//!
//! [`Cost::text`]: super::Cost::text

/// How heavy a theme is to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Weight {
    Light,
    Moderate,
    Heavy,
}

/// A cost line, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Plain {
    pub weight: Weight,
    /// `Light on resources`, or `Moderate: animated, ~19 MB memory`.
    pub summary: String,
    /// The line's parts under what they are: `("Resources", "~19.1 MB
    /// memory · 2 fonts")`, `("Layout", "bubbles")`.
    pub rows: Vec<(String, String)>,
}

/// Memory from which a theme is moderate, and heavy, in megabytes.
const MODERATE_MEMORY: f64 = 8.0;
const HEAVY_MEMORY: f64 = 40.0;
/// The share of a processor core its idle motion keeps busy from which a
/// theme is moderate, and heavy.
const MODERATE_IDLE: f64 = 0.05;
const HEAVY_IDLE: f64 = 0.2;
/// Heavy animations from which a theme is heavy.
const HEAVY_ANIMATIONS: u32 = 4;

/// The number a part starts with, after an optional `~` or `up to`.
fn leading(part: &str) -> Option<f64> {
    let part = part.trim_start_matches('~');
    let part = part.strip_prefix("up to ").unwrap_or(part);
    let end = part
        .find(|character: char| !(character.is_ascii_digit() || character == '.'))
        .unwrap_or(part.len());
    part[..end].parse().ok()
}

/// What a labelled part is called in the rows: `layout: irc` is Layout.
fn heading(label: &str) -> &'static str {
    match label {
        "layout" => "Layout",
        "rail" | "rail tiles" => "Server rail",
        "channel rows" => "Channel rows",
        "member rows" => "Member rows",
        "content (if allowed)" => "People's content, if you allow it",
        "chrome" => "Interface",
        _ => "Also",
    }
}

/// Reads a cost line; empty for an empty line, which is a cost not worked
/// out yet.
pub fn plain(line: &str) -> Option<Plain> {
    if line.trim().is_empty() {
        return None;
    }
    let (mut memory, mut animations, mut heavy, mut idle) = (0.0, 0u32, 0u32, 0.0);
    let mut moving = false;
    let mut resources = Vec::new();
    let mut rows: Vec<(String, String)> = Vec::new();
    for part in line.split(" · ") {
        let part = part.trim();
        // A labelled part says what the theme changes; the layout's own
        // sizes have colons of their own after the first.
        let labelled = part.split_once(": ").filter(|(label, _)| {
            !label.starts_with(|character: char| character.is_ascii_digit() || character == '~')
        });
        if let Some((label, text)) = labelled {
            let heading = heading(label);
            match rows.iter_mut().find(|(name, _)| name == heading) {
                Some((_, held)) => {
                    held.push_str(", ");
                    held.push_str(text);
                }
                None => rows.push((heading.to_owned(), text.to_owned())),
            }
            continue;
        }
        if part.ends_with("MB memory") {
            memory = leading(part).unwrap_or(0.0);
        } else if part.contains("animation") {
            animations = leading(part).unwrap_or(0.0) as u32;
            heavy = part
                .split_once('(')
                .and_then(|(_, rest)| leading(rest))
                .unwrap_or(0.0) as u32;
        } else if part.contains(" idle") {
            idle = leading(part).unwrap_or(0.0);
        } else if part.contains("particle set") || part.contains("overlay") {
            moving = true;
        }
        if part.contains(' ') && leading(part).is_none() && part != "no extra memory" {
            // A sentence of its own, as `names decrypt under the pointer`.
            rows.push(("Also".to_owned(), part.to_owned()));
        } else {
            resources.push(part);
        }
    }
    let weight = if memory >= HEAVY_MEMORY || idle >= HEAVY_IDLE || heavy >= HEAVY_ANIMATIONS {
        Weight::Heavy
    } else if memory >= MODERATE_MEMORY || idle >= MODERATE_IDLE || animations > 0 || moving {
        Weight::Moderate
    } else {
        Weight::Light
    };
    let mut reasons = Vec::new();
    if idle >= HEAVY_IDLE {
        reasons.push("always-on motion".to_owned());
    } else if heavy >= HEAVY_ANIMATIONS {
        reasons.push("lots of animation".to_owned());
    } else if animations > 0 || moving {
        reasons.push("animated".to_owned());
    }
    if memory >= MODERATE_MEMORY {
        reasons.push(format!("~{} MB memory", memory.round()));
    }
    let summary = match weight {
        Weight::Light => "Light on resources".to_owned(),
        Weight::Moderate => format!("Moderate: {}", reasons.join(", ")),
        Weight::Heavy => format!("Heavier: {}", reasons.join(", ")),
    };
    if !resources.is_empty() {
        rows.insert(0, ("Resources".to_owned(), resources.join(" · ")));
    }
    Some(Plain {
        weight,
        summary,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::super::Cost;
    use super::*;

    #[test]
    fn a_still_theme_is_light_and_an_empty_line_is_no_cost_yet() {
        assert_eq!(plain(""), None);
        let still = plain(&Cost::default().text()).unwrap();
        assert_eq!(still.weight, Weight::Light);
        assert_eq!(still.summary, "Light on resources");
        assert_eq!(
            still.rows,
            [("Resources".to_owned(), "no extra memory · still".to_owned())]
        );
    }

    #[test]
    fn a_cost_reads_back_from_its_own_line() {
        let cost = Cost {
            memory: 19_100_000,
            animations: 7,
            heavy: 1,
            redraws: 60,
            fonts: 2,
            layers: 18,
            particles: 3,
            idle: 0.01,
            idle_balanced: 0.01,
            looks: vec![
                "layout: bubbles".to_owned(),
                "rail tiles: circle 48×48, label none".to_owned(),
                "content (if allowed): tint pictures".to_owned(),
            ],
            ..Cost::default()
        };
        let read = plain(&cost.text()).unwrap();
        assert_eq!(read.weight, Weight::Moderate);
        assert_eq!(read.summary, "Moderate: animated, ~19 MB memory");
        assert_eq!(read.rows[0].0, "Resources");
        assert!(read.rows[0].1.starts_with("~19.1 MB memory · 2 fonts"));
        assert_eq!(
            read.rows[1..],
            [
                ("Layout".to_owned(), "bubbles".to_owned()),
                (
                    "Server rail".to_owned(),
                    "circle 48×48, label none".to_owned()
                ),
                (
                    "People's content, if you allow it".to_owned(),
                    "tint pictures".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn memory_idle_motion_and_heavy_animations_make_a_theme_heavier() {
        let memory = Cost {
            memory: 55_900_000,
            ..Cost::default()
        };
        let read = plain(&memory.text()).unwrap();
        assert_eq!(read.weight, Weight::Heavy);
        assert_eq!(read.summary, "Heavier: ~56 MB memory");
        let idle = Cost {
            animations: 12,
            heavy: 4,
            idle: 0.26,
            idle_balanced: 0.01,
            ..Cost::default()
        };
        assert_eq!(
            plain(&idle.text()).unwrap().summary,
            "Heavier: always-on motion"
        );
        let heavy = Cost {
            animations: 12,
            heavy: 4,
            ..Cost::default()
        };
        assert_eq!(
            plain(&heavy.text()).unwrap().summary,
            "Heavier: lots of animation"
        );
    }

    #[test]
    fn a_sentence_in_the_line_goes_under_also() {
        let read = plain("no extra memory · still · names decrypt under the pointer").unwrap();
        assert_eq!(
            read.rows,
            [
                ("Resources".to_owned(), "no extra memory · still".to_owned()),
                (
                    "Also".to_owned(),
                    "names decrypt under the pointer".to_owned()
                ),
            ]
        );
    }
}
