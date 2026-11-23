//! Theme options: switches, choices and sliders a theme offers the person,
//! each changing the theme's own fields. See `docs/themes.md`.
//!
//! An option is data like the rest of `theme.json`: it names fields of the
//! theme by their path (`art.overlays.grain`, `layout.messages`) and the
//! values they take while it's on, chosen or slid, or a named set of such
//! changes from the theme's `sets`. Applying it rewrites the theme's JSON
//! before [`super::Package::resolve_with`] reads it, so every value an option
//! sets is checked against its range exactly as if the theme had written it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::package::Error;

/// The person's choices for one theme, by the option's id: `true`/`false`
/// for a toggle, the choice's `value` for a choice, a number for a slider.
/// An option it doesn't name is at its default.
pub type Choices = BTreeMap<String, Value>;

/// Changes to the theme's fields, in order: each a path and the value it
/// takes, `null` taking the field away.
pub type Overrides = Vec<(String, Value)>;

/// The most options a theme offers.
pub const MAX_OPTIONS: usize = 12;
/// The most choices one choice option has.
pub const MAX_CHOICES: usize = 8;
/// The most steps a slider has.
pub const MAX_STEPS: f64 = 1000.0;

/// The id of the option a theme with ransom names offers without declaring
/// it, and that one declared under the same id replaces.
pub const RANSOM: &str = "ransom-names";

/// One option, as Settings shows it under the theme.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeOption {
    pub id: String,
    pub label: String,
    /// A sentence under the label; empty for none.
    pub description: String,
    pub kind: Kind,
    /// Another option's id and the value it has to have for this one to
    /// apply and show, such as a hue only for a "custom" preset.
    pub when: Option<(String, Value)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A switch: `on` applies while it's on, `off` while it's off.
    Toggle {
        default: bool,
        on: Overrides,
        off: Overrides,
    },
    /// One of several, each with what it changes.
    Choice {
        default: usize,
        choices: Vec<Choice>,
    },
    /// A number between `min` and `max`, by `step`, written to every path.
    /// Yapfast's own [`SOUNDS`] has no paths: the app applies it.
    Slider {
        default: f64,
        min: f64,
        max: f64,
        step: f64,
        paths: Vec<String>,
    },
    /// A hue, 0 to 360 degrees, that colours are made from: each keeps its
    /// own lightness and chroma (OKLCH), so its contrast holds whatever the
    /// hue.
    Hue {
        default: f64,
        colours: Vec<HueColour>,
    },
}

/// One colour a hue option makes: the field it's written to, and the
/// lightness (0 to 1), chroma (0 to 0.37) and opacity it's made with.
#[derive(Clone, Debug, PartialEq)]
pub struct HueColour {
    pub path: String,
    pub lightness: f64,
    pub chroma: f64,
    pub alpha: f64,
}

/// The id of the slider Yapfast adds to a theme that ships sounds: how loud
/// they play, 0 (off) to 100%, beside Yapfast's own volumes.
pub const SOUNDS: &str = "theme-sounds";
/// The id of the choice Yapfast adds to a theme that ships both a dark and
/// a light variant: follow the system, or pin one.
pub const MODE: &str = "mode";
/// What a hue option's slider steps by while `theme check` tries it.
pub const HUE_CHECK_STEP: f64 = 30.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub set: Overrides,
}

impl ThemeOption {
    /// What the person has for it: their choice when it's one the option
    /// has, its default otherwise.
    pub fn value(&self, choices: &Choices) -> Value {
        let chosen = choices.get(&self.id);
        match &self.kind {
            Kind::Toggle { default, .. } => {
                Value::Bool(chosen.and_then(Value::as_bool).unwrap_or(*default))
            }
            Kind::Choice { default, choices } => {
                let value = chosen
                    .and_then(Value::as_str)
                    .filter(|value| choices.iter().any(|choice| choice.value == *value))
                    .unwrap_or(&choices[*default].value);
                Value::String(value.to_owned())
            }
            Kind::Slider {
                default,
                min,
                max,
                step,
                ..
            } => {
                let value = chosen.and_then(Value::as_f64).unwrap_or(*default);
                number(snap(value, *min, *max, *step))
            }
            Kind::Hue { default, .. } => {
                let value = chosen.and_then(Value::as_f64).unwrap_or(*default);
                number(snap(value, 0.0, 360.0, 1.0))
            }
        }
    }

    /// Whether it applies with `choices`: always, unless its `when` names
    /// another option's value that isn't chosen.
    pub fn active(&self, options: &[ThemeOption], choices: &Choices) -> bool {
        let Some((other, wanted)) = &self.when else {
            return true;
        };
        options
            .iter()
            .find(|option| option.id == *other)
            .is_some_and(|option| option.value(choices) == *wanted)
    }

    /// Its default, as [`Self::value`] gives it.
    pub fn default_value(&self) -> Value {
        self.value(&Choices::new())
    }

    /// What it changes at `value`.
    pub fn overrides(&self, value: &Value) -> Overrides {
        match &self.kind {
            Kind::Toggle { default, on, off } => {
                if value.as_bool().unwrap_or(*default) {
                    on.clone()
                } else {
                    off.clone()
                }
            }
            Kind::Choice { default, choices } => value
                .as_str()
                .and_then(|value| choices.iter().find(|choice| choice.value == value))
                .unwrap_or(&choices[*default])
                .set
                .clone(),
            Kind::Slider { paths, .. } => paths
                .iter()
                .map(|path| (path.clone(), value.clone()))
                .collect(),
            Kind::Hue { default, colours } => {
                let hue = value.as_f64().unwrap_or(*default);
                colours
                    .iter()
                    .map(|colour| (colour.path.clone(), Value::String(colour.at(hue))))
                    .collect()
            }
        }
    }

    /// The values other than its default that `theme check` tries it at:
    /// a toggle flipped, every other choice, a slider's two ends.
    pub fn alternatives(&self) -> Vec<Value> {
        let default = self.default_value();
        let mut values = match &self.kind {
            Kind::Toggle { default, .. } => vec![Value::Bool(!default)],
            Kind::Choice { choices, .. } => choices
                .iter()
                .map(|choice| Value::String(choice.value.clone()))
                .collect(),
            Kind::Slider { min, max, .. } => vec![number(*min), number(*max)],
            Kind::Hue { .. } => (0..12)
                .map(|step| number(f64::from(step) * HUE_CHECK_STEP))
                .collect(),
        };
        values.retain(|value| *value != default);
        values.dedup();
        values
    }

    /// `value` as Settings words it: `on`, the choice's label, the number.
    pub fn describe(&self, value: &Value) -> String {
        match &self.kind {
            Kind::Toggle { .. } => {
                if value.as_bool().unwrap_or(false) {
                    "on".into()
                } else {
                    "off".into()
                }
            }
            Kind::Choice { choices, .. } => value
                .as_str()
                .and_then(|value| choices.iter().find(|choice| choice.value == value))
                .map_or_else(|| value.to_string(), |choice| choice.label.clone()),
            // `100`, not `100.0`; `0.5` as it is.
            Kind::Slider { .. } => match value.as_f64() {
                Some(number) if number.fract() == 0.0 && number.abs() < 1e15 => {
                    format!("{}", number as i64)
                }
                Some(number) => format!("{}", (number * 100.0).round() / 100.0),
                None => value.to_string(),
            },
            Kind::Hue { .. } => format!("{}°", value.as_f64().unwrap_or(0.0).round()),
        }
    }
}

impl HueColour {
    /// The colour at `hue` degrees, as `#rrggbb` or `#rrggbbaa`.
    pub fn at(&self, hue: f64) -> String {
        let [r, g, b] = oklch(self.lightness, self.chroma, hue);
        let alpha = (self.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{alpha:02x}")
        }
    }
}

/// OKLCH to sRGB, 8 bits a channel. A colour outside sRGB keeps its
/// lightness and hue and loses chroma until it fits, so a hue never makes
/// text lighter or darker than the theme said.
pub fn oklch(lightness: f64, chroma: f64, hue: f64) -> [u8; 3] {
    fn linear(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
        let (a, b) = (
            chroma * hue.to_radians().cos(),
            chroma * hue.to_radians().sin(),
        );
        let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
        let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
        let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
        [
            4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
        ]
    }
    let fits = |rgb: [f64; 3]| rgb.iter().all(|channel| (-1e-4..=1.0001).contains(channel));
    let lightness = lightness.clamp(0.0, 1.0);
    let mut rgb = linear(lightness, chroma, hue);
    if !fits(rgb) {
        let (mut low, mut high) = (0.0, chroma);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if fits(linear(lightness, middle, hue)) {
                low = middle;
            } else {
                high = middle;
            }
        }
        rgb = linear(lightness, low, hue);
    }
    rgb.map(|channel| {
        let channel = channel.clamp(0.0, 1.0);
        let encoded = if channel <= 0.003_130_8 {
            12.92 * channel
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    })
}

/// The slider Yapfast adds to a theme with sounds; see [`SOUNDS`].
pub fn sounds_option() -> ThemeOption {
    ThemeOption {
        id: SOUNDS.to_owned(),
        label: "Theme sounds".to_owned(),
        description: "How loud the theme's own sounds play; all the way down is off. Yapfast's own sounds keep their volume."
            .to_owned(),
        kind: Kind::Slider {
            default: 100.0,
            min: 0.0,
            max: 100.0,
            step: 5.0,
            paths: Vec::new(),
        },
        when: None,
    }
}

/// The choice Yapfast adds to a theme with both variants; see [`MODE`].
pub fn mode_option() -> ThemeOption {
    let choice = |value: &str, label: &str| Choice {
        value: value.to_owned(),
        label: label.to_owned(),
        set: Vec::new(),
    };
    ThemeOption {
        id: MODE.to_owned(),
        label: "Light or dark".to_owned(),
        description: "Auto follows your system's light or dark windows.".to_owned(),
        kind: Kind::Choice {
            default: 0,
            choices: vec![
                choice("auto", "Auto"),
                choice("dark", "Dark"),
                choice("light", "Light"),
            ],
        },
        when: None,
    }
}

/// The option a theme with ransom names offers unless it declares its own
/// under [`RANSOM`]: on, names are cut into the theme's letters; off, they
/// show plainly.
pub fn ransom_option() -> ThemeOption {
    ThemeOption {
        id: RANSOM.to_owned(),
        label: "Ransom names".to_owned(),
        description: "People's names cut from the theme's letters. Off, they show as plain text."
            .to_owned(),
        kind: Kind::Toggle {
            default: true,
            on: Vec::new(),
            off: vec![("type.names.style".to_owned(), Value::Null)],
        },
        when: None,
    }
}

/// Every change `choices` makes, option by option in the theme's order.
pub fn overrides(options: &[ThemeOption], choices: &Choices) -> Overrides {
    options
        .iter()
        .filter(|option| option.active(options, choices))
        .flat_map(|option| option.overrides(&option.value(choices)))
        .collect()
}

/// The sets of choices `theme check` tries: the defaults, then each option
/// moved to each of its alternatives with the others at their defaults,
/// each with a phrase naming it.
pub fn combinations(options: &[ThemeOption]) -> Vec<(String, Choices)> {
    let mut combinations = vec![(String::new(), Choices::new())];
    for option in options {
        // An option that changes no field (Yapfast's own) has nothing to try.
        if option.overrides(&option.default_value()).is_empty()
            && option
                .alternatives()
                .iter()
                .all(|value| option.overrides(value).is_empty())
        {
            continue;
        }
        let mut values = option.alternatives();
        if option.when.is_some() {
            // Tried where it applies, its default too.
            values.insert(0, option.default_value());
        }
        for value in values {
            let mut phrase = format!("with \"{}\" {}", option.label, option.describe(&value));
            let mut choices = Choices::new();
            if let Some((other, wanted)) = &option.when {
                if let Some(other) = options.iter().find(|known| known.id == *other) {
                    let _ = write!(
                        phrase,
                        " and \"{}\" {}",
                        other.label,
                        other.describe(wanted)
                    );
                }
                choices.insert(other.clone(), wanted.clone());
            }
            choices.insert(option.id.clone(), value);
            combinations.push((phrase, choices));
        }
    }
    combinations
}

fn number(value: f64) -> Value {
    serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// `value` on the slider's steps, within its ends.
fn snap(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let steps = ((value.clamp(min, max) - min) / step).round();
    // Rounded to a millionth, so 0.1 steps don't show as 0.30000000000000004.
    (((min + steps * step).min(max)) * 1e6).round() / 1e6
}

/// Reads `options`, with `sets` the named changes they may use. `layers` are
/// the keys a path may start with.
pub fn read_options(
    options: Option<&Value>,
    sets: Option<&Value>,
    layers: &[&str],
) -> Result<Vec<ThemeOption>, Error> {
    let fail = |text: String| Err(Error(text));
    let mut named: BTreeMap<String, Overrides> = BTreeMap::new();
    match sets {
        None => {}
        Some(Value::Object(sets)) => {
            for (name, changes) in sets {
                let at = format!("sets.{name}");
                let Value::Object(changes) = changes else {
                    return fail(format!(
                        "\"{at}\" is an object of fields and the values they take"
                    ));
                };
                named.insert(name.clone(), read_changes(changes, &at, layers)?);
            }
        }
        Some(_) => {
            return fail("\"sets\" is an object of named changes".into());
        }
    }
    let Some(options) = options else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = options else {
        return fail("\"options\" is a list".into());
    };
    if items.len() > MAX_OPTIONS {
        return fail(format!("\"options\" lists at most {MAX_OPTIONS}"));
    }
    let mut read: Vec<ThemeOption> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let at = format!("options[{index}]");
        let Value::Object(fields) = item else {
            return fail(format!("\"{at}\" is an object"));
        };
        let text = |key: &str| fields.get(key).and_then(Value::as_str);
        let id = text("id").ok_or_else(|| Error(format!("\"{at}\" needs an \"id\"")))?;
        if !word(id) {
            return fail(format!(
                "\"{at}.id\" is 1 to 32 lowercase letters, digits and dashes"
            ));
        }
        if read.iter().any(|option| option.id == id) {
            return fail(format!("two options have the id \"{id}\""));
        }
        let at = format!("options.{id}");
        let label = text("label").ok_or_else(|| Error(format!("\"{at}\" needs a \"label\"")))?;
        if label.trim().is_empty() || label.chars().count() > 40 {
            return fail(format!("\"{at}.label\" is 1 to 40 characters"));
        }
        super::package::plain(&format!("{at}.label"), label)?;
        let description = text("description").unwrap_or_default();
        if description.chars().count() > 160 {
            return fail(format!("\"{at}.description\" is at most 160 characters"));
        }
        super::package::plain(&format!("{at}.description"), description)?;
        let changes = |key: &str| -> Result<Overrides, Error> {
            match fields.get(key) {
                None => Ok(Vec::new()),
                Some(value) => resolve_changes(value, &format!("{at}.{key}"), &named, layers),
            }
        };
        let kind = match text("kind") {
            Some("toggle") => {
                let default = match fields.get("default") {
                    None => true,
                    Some(Value::Bool(default)) => *default,
                    Some(_) => return fail(format!("\"{at}.default\" is true or false")),
                };
                let (on, off) = (changes("on")?, changes("off")?);
                if on.is_empty() && off.is_empty() {
                    return fail(format!(
                        "\"{at}\" changes nothing: give it \"on\" or \"off\""
                    ));
                }
                Kind::Toggle { default, on, off }
            }
            Some("choice") => {
                let Some(Value::Array(list)) = fields.get("choices") else {
                    return fail(format!("\"{at}\" needs \"choices\", a list"));
                };
                if list.len() < 2 || list.len() > MAX_CHOICES {
                    return fail(format!("\"{at}.choices\" lists 2 to {MAX_CHOICES}"));
                }
                let mut choices: Vec<Choice> = Vec::new();
                for (number, choice) in list.iter().enumerate() {
                    let here = format!("{at}.choices[{number}]");
                    let Value::Object(choice) = choice else {
                        return fail(format!("\"{here}\" is an object"));
                    };
                    let value = choice
                        .get("value")
                        .and_then(Value::as_str)
                        .filter(|value| word(value))
                        .ok_or_else(|| {
                            Error(format!(
                                "\"{here}.value\" is 1 to 32 lowercase letters, digits and dashes"
                            ))
                        })?;
                    if choices.iter().any(|known| known.value == value) {
                        return fail(format!("\"{at}\" has two choices \"{value}\""));
                    }
                    let label = choice
                        .get("label")
                        .and_then(Value::as_str)
                        .filter(|label| !label.trim().is_empty() && label.chars().count() <= 24)
                        .ok_or_else(|| Error(format!("\"{here}.label\" is 1 to 24 characters")))?;
                    super::package::plain(&format!("{here}.label"), label)?;
                    let set = match choice.get("set") {
                        None => Vec::new(),
                        Some(set) => resolve_changes(set, &format!("{here}.set"), &named, layers)?,
                    };
                    choices.push(Choice {
                        value: value.to_owned(),
                        label: label.to_owned(),
                        set,
                    });
                }
                let default = match fields.get("default") {
                    None => 0,
                    Some(default) => default
                        .as_str()
                        .and_then(|default| {
                            choices.iter().position(|choice| choice.value == default)
                        })
                        .ok_or_else(|| {
                            Error(format!("\"{at}.default\" names one of its choices' values"))
                        })?,
                };
                Kind::Choice { default, choices }
            }
            Some("slider") => {
                let number = |key: &str| {
                    fields
                        .get(key)
                        .and_then(Value::as_f64)
                        .filter(|value| value.is_finite())
                        .ok_or_else(|| Error(format!("\"{at}\" needs \"{key}\", a number")))
                };
                let (min, max) = (number("min")?, number("max")?);
                let step = match fields.get("step") {
                    None => (max - min) / 100.0,
                    Some(_) => number("step")?,
                };
                if min >= max || step <= 0.0 || (max - min) / step > MAX_STEPS {
                    return fail(format!(
                        "\"{at}\" needs \"min\" under \"max\" and at most {MAX_STEPS} steps between"
                    ));
                }
                let default = match fields.get("default") {
                    None => min,
                    Some(_) => number("default")?,
                };
                if default < min || default > max {
                    return fail(format!("\"{at}.default\" is between \"min\" and \"max\""));
                }
                let paths: Vec<String> = match fields.get("paths") {
                    Some(Value::Array(paths)) => paths
                        .iter()
                        .map(|path| path.as_str().map(str::to_owned))
                        .collect::<Option<_>>()
                        .ok_or_else(|| Error(format!("\"{at}.paths\" is a list of fields")))?,
                    Some(Value::String(path)) => vec![path.clone()],
                    _ => return fail(format!("\"{at}\" needs \"paths\", the fields it sets")),
                };
                if paths.is_empty() || paths.len() > 8 {
                    return fail(format!("\"{at}.paths\" lists 1 to 8 fields"));
                }
                for path in &paths {
                    check_path(path, &format!("{at}.paths"), layers)?;
                }
                Kind::Slider {
                    default,
                    min,
                    max,
                    step,
                    paths,
                }
            }
            Some("hue") => {
                let default = match fields.get("default") {
                    None => 0.0,
                    Some(value) => value
                        .as_f64()
                        .filter(|hue| (0.0..=360.0).contains(hue))
                        .ok_or_else(|| Error(format!("\"{at}.default\" is 0 to 360 degrees")))?,
                };
                let Some(Value::Object(list)) = fields.get("colours") else {
                    return fail(format!(
                        "\"{at}\" needs \"colours\": each colour it makes, by name or path"
                    ));
                };
                if list.is_empty() || list.len() > 96 {
                    return fail(format!("\"{at}.colours\" makes 1 to 96 colours"));
                }
                let mut colours = Vec::new();
                for (key, made) in list {
                    let here = format!("{at}.colours.{key}");
                    // A bare name is a colour token; anything else a path.
                    let path = if key.contains('.') {
                        key.clone()
                    } else {
                        format!("colours.{key}")
                    };
                    check_path(&path, &here, layers)?;
                    let number = |name: &str, range: (f64, f64), stock: Option<f64>| match made
                        .get(name)
                        .and_then(Value::as_f64)
                        .or(stock)
                    {
                        Some(value) if (range.0..=range.1).contains(&value) => Ok(value),
                        _ => Err(Error(format!(
                            "\"{here}.{name}\" is a number from {} to {}",
                            range.0, range.1
                        ))),
                    };
                    colours.push(HueColour {
                        path,
                        lightness: number("l", (0.0, 1.0), None)?,
                        chroma: number("c", (0.0, 0.37), None)?,
                        alpha: number("alpha", (0.0, 1.0), Some(1.0))?,
                    });
                }
                Kind::Hue { default, colours }
            }
            _ => {
                return fail(format!(
                    "\"{at}.kind\" is \"toggle\", \"choice\", \"slider\" or \"hue\""
                ));
            }
        };
        let when = match fields.get("when") {
            None => None,
            Some(Value::Object(when)) if when.len() == 1 => {
                let (other, wanted) = when.iter().next().expect("one");
                let Some(known) = read.iter().find(|option| option.id == *other) else {
                    return fail(format!(
                        "\"{at}.when\" names \"{other}\", which has to be an option listed before it"
                    ));
                };
                let fits = match &known.kind {
                    Kind::Toggle { .. } => wanted.is_boolean(),
                    Kind::Choice { choices, .. } => wanted
                        .as_str()
                        .is_some_and(|value| choices.iter().any(|choice| choice.value == value)),
                    _ => false,
                };
                if !fits {
                    return fail(format!(
                        "\"{at}.when\" needs one of \"{other}\"'s choices, or true or false for a switch"
                    ));
                }
                Some((other.clone(), wanted.clone()))
            }
            Some(_) => {
                return fail(format!(
                    "\"{at}.when\" is one option's id and the value it has to have"
                ));
            }
        };
        read.push(ThemeOption {
            id: id.to_owned(),
            label: label.to_owned(),
            description: description.to_owned(),
            kind,
            when,
        });
    }
    Ok(read)
}

/// An id or a choice's value: 1 to 32 of `a-z`, `0-9` and `-`.
fn word(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 32
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// An option's `on`, `off` or a choice's `set`: the fields it changes, the
/// name of a set, or a list of either.
fn resolve_changes(
    value: &Value,
    at: &str,
    named: &BTreeMap<String, Overrides>,
    layers: &[&str],
) -> Result<Overrides, Error> {
    match value {
        Value::Object(changes) => read_changes(changes, at, layers),
        Value::String(name) => named.get(name).cloned().ok_or_else(|| {
            Error(format!(
                "\"{at}\" names the set \"{name}\", which \"sets\" doesn't have"
            ))
        }),
        Value::Array(items) => {
            let mut all = Vec::new();
            for (index, item) in items.iter().enumerate() {
                all.extend(resolve_changes(
                    item,
                    &format!("{at}[{index}]"),
                    named,
                    layers,
                )?);
            }
            Ok(all)
        }
        _ => Err(Error(format!(
            "\"{at}\" is an object of fields and values, a set's name, or a list of them"
        ))),
    }
}

fn read_changes(
    changes: &Map<String, Value>,
    at: &str,
    layers: &[&str],
) -> Result<Overrides, Error> {
    let mut read = Vec::new();
    for (path, value) in changes {
        check_path(path, at, layers)?;
        read.push((path.clone(), value.clone()));
    }
    Ok(read)
}

/// A path starts at a layer (`art`, `layout`, …), or at `variants.dark.` or
/// `variants.light.` and then a layer, and has no empty step.
fn check_path(path: &str, at: &str, layers: &[&str]) -> Result<(), Error> {
    let steps: Vec<&str> = path.split('.').collect();
    let layer = match steps.as_slice() {
        ["variants", "dark" | "light", layer, ..] => *layer,
        [layer, ..] => *layer,
        [] => "",
    };
    if steps.iter().any(|step| step.is_empty()) || !layers.contains(&layer) {
        return Err(Error(format!(
            "\"{at}\" changes \"{path}\": a path starts at one of the theme's layers ({})",
            layers.join(", ")
        )));
    }
    Ok(())
}

/// Applies `overrides` to a theme's JSON: `shared` the layers at its top
/// level, `variants` each variant's object. A path under `variants.<mode>.`
/// changes only that variant; any other changes the field wherever the
/// theme has the field's parent, at the top level and in each variant, and
/// is made at the top level when neither has it. `null` takes a field away,
/// or every item of a list it names. Fails naming a path that takes away
/// something the theme doesn't have.
pub fn apply(
    overrides: &Overrides,
    shared: &mut Value,
    variants: &mut [(&str, Option<&mut Value>)],
) -> Result<(), Error> {
    for (path, value) in overrides {
        let steps: Vec<&str> = path.split('.').collect();
        if let ["variants", mode, rest @ ..] = steps.as_slice() {
            // Another variant's change is no business of this one.
            let Some((_, target)) = variants.iter_mut().find(|(name, _)| name == mode) else {
                continue;
            };
            let applied = target
                .as_deref_mut()
                .is_some_and(|target| set(target, rest, value, true));
            if !applied && value.is_null() {
                return Err(missing(path));
            }
            continue;
        }
        let mut applied = set(shared, &steps, value, false);
        for (_, variant) in variants.iter_mut() {
            if let Some(variant) = variant.as_deref_mut() {
                applied |= set(variant, &steps, value, false);
            }
        }
        if !applied {
            if value.is_null() {
                return Err(missing(path));
            }
            set(shared, &steps, value, true);
        }
    }
    Ok(())
}

fn missing(path: &str) -> Error {
    Error(format!(
        "an option takes away \"{path}\", which the theme doesn't have"
    ))
}

/// The keys an item of a list is found by: `art.overlays.grain` is the
/// overlay whose `kind` is `grain`.
const ITEM_KEYS: [&str; 6] = ["id", "kind", "preset", "name", "slot", "effect"];

/// Sets the field at `steps` under `node`, or takes it away for `null`.
/// Without `create`, only where the field's parent is already there. A key
/// with a dot in it, such as `effects.channel.name`'s, is found as written.
fn set(node: &mut Value, steps: &[&str], value: &Value, create: bool) -> bool {
    let Some((first, rest)) = steps.split_first() else {
        return false;
    };
    match node {
        Value::Object(map) => {
            for take in 1..=steps.len() {
                let key = steps[..take].join(".");
                if !map.contains_key(&key) {
                    continue;
                }
                let rest = &steps[take..];
                if rest.is_empty() {
                    if value.is_null() {
                        map.remove(&key);
                    } else {
                        map.insert(key, value.clone());
                    }
                    return true;
                }
                if set(map.get_mut(&key).expect("checked"), rest, value, create) {
                    return true;
                }
            }
            if value.is_null() {
                return false;
            }
            if rest.is_empty() {
                map.insert((*first).to_owned(), value.clone());
                return true;
            }
            if create {
                let mut made = value.clone();
                for step in rest.iter().rev() {
                    let mut object = Map::new();
                    object.insert((*step).to_owned(), made);
                    made = Value::Object(object);
                }
                map.insert((*first).to_owned(), made);
                return true;
            }
            false
        }
        Value::Array(items) => {
            let matches = |index: usize, item: &Value| {
                first.parse::<usize>().is_ok_and(|wanted| wanted == index)
                    || ITEM_KEYS
                        .iter()
                        .any(|key| item.get(key).and_then(Value::as_str) == Some(first))
                    || item.as_str() == Some(first)
            };
            if rest.is_empty() {
                let before = items.len();
                if value.is_null() {
                    let mut index = 0;
                    items.retain(|item| {
                        let keep = !matches(index, item);
                        index += 1;
                        keep
                    });
                    return items.len() != before;
                }
                let mut applied = false;
                for (index, item) in items.iter_mut().enumerate() {
                    if matches(index, item) {
                        *item = value.clone();
                        applied = true;
                    }
                }
                return applied;
            }
            let mut applied = false;
            for (index, item) in items.iter_mut().enumerate() {
                if matches(index, item) {
                    applied |= set(item, rest, value, false);
                }
            }
            applied
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LAYERS: [&str; 4] = ["art", "layout", "type", "colours"];

    fn options(value: Value) -> Result<Vec<ThemeOption>, Error> {
        read_options(value.get("options"), value.get("sets"), &LAYERS)
    }

    #[test]
    fn option_and_choice_labels_refuse_turned_or_hidden_text() {
        let toggle = |label: &str, description: &str| {
            options(json!({ "options": [
                { "id": "grain", "label": label, "description": description, "kind": "toggle",
                  "off": { "art.overlays.grain": null } }
            ]}))
        };
        assert!(toggle("Film grain", "Speckles.").is_ok());
        let error = toggle("Film\u{202e}grain", "").unwrap_err();
        assert!(error.0.contains("options.grain.label"), "{error}");
        let error = toggle("Film grain", "Two\nlines").unwrap_err();
        assert!(error.0.contains("options.grain.description"), "{error}");
        let error = options(json!({ "options": [
            { "id": "tone", "label": "Tone", "kind": "choice", "choices": [
                { "value": "warm", "label": "Warm", "set": { "colours.chat": "#201010" } },
                { "value": "cold", "label": "Co\u{200b}ld", "set": { "colours.chat": "#101020" } }
            ]}
        ]}))
        .unwrap_err();
        assert!(error.0.contains("choices[1].label"), "{error}");
    }

    #[test]
    fn a_toggle_takes_an_overlay_away_by_its_kind() {
        let read = options(json!({ "options": [
            { "id": "grain", "label": "Film grain", "kind": "toggle", "default": true,
              "off": { "art.overlays.grain": null } }
        ]}))
        .unwrap();
        let mut shared = json!({ "art": { "overlays": [
            { "kind": "vignette" }, { "kind": "grain", "opacity": 0.1 }
        ]}});
        let mut choices = Choices::new();
        apply(&overrides(&read, &choices), &mut shared, &mut []).unwrap();
        assert_eq!(shared["art"]["overlays"].as_array().unwrap().len(), 2);
        choices.insert("grain".into(), Value::Bool(false));
        apply(&overrides(&read, &choices), &mut shared, &mut []).unwrap();
        assert_eq!(shared["art"]["overlays"], json!([{ "kind": "vignette" }]));
    }

    #[test]
    fn a_choice_uses_named_sets_and_sets_where_the_parent_is() {
        let read = options(json!({
            "sets": { "cards": { "layout.messages": "cards" } },
            "options": [{ "id": "messages", "label": "Messages", "kind": "choice",
                "default": "bubbles",
                "choices": [{ "value": "bubbles", "label": "Bubbles" },
                            { "value": "cards", "label": "Cards", "set": "cards" }] }]
        }))
        .unwrap();
        let mut shared = json!({ "colours": {} });
        let mut dark = json!({ "base": "yap", "layout": { "rail": "left" } });
        let mut choices = Choices::new();
        choices.insert("messages".into(), json!("cards"));
        apply(
            &overrides(&read, &choices),
            &mut shared,
            &mut [("dark", Some(&mut dark)), ("light", None)],
        )
        .unwrap();
        // The dark variant has a layout, so it changes there, not at the top.
        assert_eq!(dark["layout"]["messages"], "cards");
        assert!(shared.get("layout").is_none());
    }

    #[test]
    fn a_field_neither_has_is_made_at_the_top_level() {
        let mut shared = json!({});
        apply(
            &vec![("layout.rail".to_owned(), json!("right"))],
            &mut shared,
            &mut [],
        )
        .unwrap();
        assert_eq!(shared, json!({ "layout": { "rail": "right" } }));
    }

    #[test]
    fn taking_away_what_isnt_there_fails() {
        let mut shared = json!({ "art": {} });
        let error = apply(
            &vec![("art.overlays.grain".to_owned(), Value::Null)],
            &mut shared,
            &mut [],
        )
        .unwrap_err();
        assert!(error.0.contains("art.overlays.grain"), "{error}");
    }

    #[test]
    fn a_slider_writes_its_number_into_a_listed_item_on_its_steps() {
        let read = options(json!({ "options": [
            { "id": "grain", "label": "Grain", "kind": "slider", "min": 0, "max": 0.2,
              "step": 0.01, "default": 0.08, "paths": ["art.overlays.grain.opacity"] }
        ]}))
        .unwrap();
        let mut choices = Choices::new();
        choices.insert("grain".into(), json!(0.123));
        assert_eq!(read[0].value(&choices), json!(0.12));
        let mut shared = json!({ "art": { "overlays": [{ "kind": "grain", "opacity": 0.08 }] } });
        apply(&overrides(&read, &choices), &mut shared, &mut []).unwrap();
        assert_eq!(shared["art"]["overlays"][0]["opacity"], json!(0.12));
    }

    #[test]
    fn a_dotted_key_is_found_as_written() {
        let mut shared = json!({ "effects": { "channel.name": "decrypt" } });
        apply(
            &vec![("effects.channel.name".to_owned(), Value::Null)],
            &mut shared,
            &mut [],
        )
        .unwrap();
        assert_eq!(shared, json!({ "effects": {} }));
    }

    #[test]
    fn a_variant_path_changes_only_that_variant() {
        let mut shared = json!({ "colours": { "chat": "#000000" } });
        let mut dark = json!({ "base": "yap" });
        apply(
            &vec![("variants.dark.colours.chat".to_owned(), json!("#111111"))],
            &mut shared,
            &mut [("dark", Some(&mut dark))],
        )
        .unwrap();
        assert_eq!(dark["colours"]["chat"], "#111111");
        assert_eq!(shared["colours"]["chat"], "#000000");
    }

    #[test]
    fn bad_options_are_refused_naming_the_place() {
        for (value, says) in [
            (
                json!({ "options": [{ "id": "Grain", "label": "x", "kind": "toggle", "off": {} }] }),
                "id",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "toggle" }] }),
                "changes nothing",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "toggle", "off": { "secret.x": 1 } }] }),
                "layers",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "choice", "choices": [{ "value": "b", "label": "B" }] }] }),
                "2 to",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "toggle", "off": "nope" }] }),
                "set \"nope\"",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "slider", "min": 1, "max": 0, "paths": ["art.x"] }] }),
                "min",
            ),
            (
                json!({ "options": [{ "id": "a", "label": "x", "kind": "dial" }] }),
                "kind",
            ),
        ] {
            let error = options(value).unwrap_err();
            assert!(error.0.contains(says), "{error} should say {says}");
        }
    }

    #[test]
    fn a_hue_keeps_lightness_and_applies_only_when_its_preset_is_chosen() {
        let read = options(json!({ "options": [
            { "id": "phosphor", "label": "Phosphor", "kind": "choice", "default": "p1",
              "choices": [{ "value": "p1", "label": "P1", "set": { "colours.accent": "#33ff66" } },
                          { "value": "custom", "label": "Custom" }] },
            { "id": "hue", "label": "Hue", "kind": "hue", "default": 200,
              "when": { "phosphor": "custom" },
              "colours": { "accent": { "l": 0.8, "c": 0.15 },
                           "art.glow.colour": { "l": 0.7, "c": 0.3, "alpha": 0.5 } } }
        ]}))
        .unwrap();
        let mut choices = Choices::new();
        assert!(!read[1].active(&read, &choices));
        assert_eq!(overrides(&read, &choices).len(), 1, "only P1's set");
        choices.insert("phosphor".into(), json!("custom"));
        let made = overrides(&read, &choices);
        assert_eq!(made[0].0, "colours.accent");
        assert_eq!(made[1].0, "art.glow.colour");
        assert!(made[1].1.as_str().unwrap().ends_with("80"), "{:?}", made[1]);
        // OKLCH white and black, and a colour turned around the wheel
        // keeping about the same luminance.
        assert_eq!(oklch(1.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(oklch(0.0, 0.0, 0.0), [0, 0, 0]);
        let luminance = |[r, g, b]: [u8; 3]| {
            let linear = |channel: u8| (f64::from(channel) / 255.0).powf(2.2);
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        };
        let (red, blue) = (oklch(0.8, 0.12, 30.0), oklch(0.8, 0.12, 250.0));
        assert!(
            (luminance(red) - luminance(blue)).abs() < 0.05,
            "{red:?} {blue:?}"
        );
        // Checked every 30 degrees, with its preset chosen.
        let tried: Vec<String> = combinations(&read)
            .into_iter()
            .map(|(phrase, _)| phrase)
            .collect();
        assert!(
            tried.contains(&"with \"Hue\" 90° and \"Phosphor\" Custom".to_owned()),
            "{tried:?}"
        );
        // The defaults, Custom, then the hue at its default and 12 more.
        assert_eq!(tried.len(), 1 + 1 + 13);
    }

    #[test]
    fn yapfasts_own_options_change_no_field_and_arent_tried() {
        assert!(combinations(&[sounds_option(), mode_option()]).len() == 1);
        let sounds = sounds_option();
        assert_eq!(sounds.default_value(), json!(100.0));
        let mut choices = Choices::new();
        choices.insert(SOUNDS.into(), json!(42));
        assert_eq!(sounds.value(&choices), json!(40.0), "on its 5% steps");
    }

    #[test]
    fn combinations_flip_one_option_at_a_time() {
        let read = options(json!({ "options": [
            { "id": "grain", "label": "Grain", "kind": "toggle", "off": { "art.x": null } },
            { "id": "side", "label": "Side", "kind": "choice", "choices": [
                { "value": "left", "label": "Left" }, { "value": "right", "label": "Right", "set": { "layout.rail": "right" } },
                { "value": "top", "label": "Top", "set": { "layout.rail": "top" } }] }
        ]}))
        .unwrap();
        let phrases: Vec<String> = combinations(&read)
            .into_iter()
            .map(|(phrase, _)| phrase)
            .collect();
        assert_eq!(
            phrases,
            [
                "",
                "with \"Grain\" off",
                "with \"Side\" Right",
                "with \"Side\" Top"
            ]
        );
    }
}
