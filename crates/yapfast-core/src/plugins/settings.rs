//! A plugin's own settings: the `settings` array of its manifest, checked
//! into a [`Schema`], the values a person chose, kept in
//! `<data>/plugins/data/<id>/settings.json` by a [`Store`], and the form
//! Settings → Plugins draws from them ([`Form`]).
//!
//! A schema is strict: a field a setting doesn't take, a type Yapfast
//! doesn't know, a default its own rules refuse or a cap passed refuses the
//! whole manifest, so a mistake fails loudly where the plugin's author sees
//! it. Values are forgiving: a stored value the schema refuses is dropped and
//! logged, and its default takes its place. See `docs/plugins/settings.md`.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value as Json;

use super::ui;

/// The most settings one plugin declares.
pub const SETTINGS: usize = 64;
/// The most bytes in a setting's or a field's key.
pub const KEY_BYTES: usize = 64;
/// The most characters in a description.
pub const DESCRIPTION_CHARS: usize = 256;
/// The most characters a string setting can hold, and its cap when the
/// manifest names none.
pub const TEXT_CHARS: usize = 4096;
pub const DEFAULT_TEXT_CHARS: usize = 256;
/// The most options in a select.
pub const OPTIONS: usize = 32;
/// The most fields in a list's record.
pub const FIELDS: usize = 8;
/// The most rows a list can hold, and its cap when the manifest names none.
pub const ROWS: usize = 256;
pub const DEFAULT_ROWS: usize = 64;
/// The most bytes in a `pattern`.
pub const PATTERN_BYTES: usize = 256;
/// The most positions a slider has.
pub const SLIDER_STEPS: f64 = 1000.0;
/// The most tests in one `show` or `enable`.
pub const TESTS: usize = 8;
/// The largest `settings.json` read; a larger one is left alone and the
/// defaults apply.
pub const FILE_BYTES: u64 = 1024 * 1024;
/// What each plugin's values are kept in, in its folder under `data/`.
pub const FILE: &str = "settings.json";

/// A plugin's settings, in the order its manifest declares them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(try_from = "Vec<RawSetting>")]
pub struct Schema {
    pub settings: Vec<Setting>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: Kind,
    /// Shown only while every test holds, and the settings they name show.
    pub show: Vec<Test>,
    /// Can be changed only while every test holds, and the settings they
    /// name can be.
    pub enable: Vec<Test>,
}

/// A setting's type, with its rules and default.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool {
        default: bool,
    },
    String(Text),
    Select {
        options: Vec<Choice>,
        default: String,
    },
    /// A number chosen on a track, from `min` to `max` in `step`s.
    Slider(Range),
    /// A number typed in, within the range given.
    Number(Range),
    /// A whole number typed in, within the range given.
    Integer {
        min: Option<i64>,
        max: Option<i64>,
        step: Option<i64>,
        default: i64,
    },
    /// Rows of records the person adds, removes and reorders.
    List {
        fields: Vec<Field>,
        max_rows: usize,
        default: Vec<Record>,
    },
    /// Drawn by the plugin as a widget tree, which the runtime supplies; it
    /// has no value here.
    Widget,
}

#[derive(Clone, Debug)]
pub struct Text {
    pub default: String,
    pub max_chars: usize,
    pub placeholder: String,
    /// What a typed value must match. An empty value always may be.
    pub pattern: Option<regex::Regex>,
    /// What's said when a value doesn't match.
    pub pattern_message: String,
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.default == other.default
            && self.max_chars == other.max_chars
            && self.placeholder == other.placeholder
            && self.pattern.as_ref().map(regex::Regex::as_str)
                == other.pattern.as_ref().map(regex::Regex::as_str)
            && self.pattern_message == other.pattern_message
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub default: f64,
}

/// A field of a list's records: `bool`, `string`, `select`, `number` or
/// `integer`, without `when`.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub kind: Kind,
}

/// A test on another setting, declared before this one.
#[derive(Clone, Debug, PartialEq)]
pub struct Test {
    pub setting: String,
    pub check: Check,
}

/// The closed set of tests.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    Is(Value),
    Not(Value),
    In(Vec<Value>),
    /// A string without text, or a list without rows; `false` for the
    /// opposite.
    Empty(bool),
}

/// A setting's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    /// A string's text, or a select's chosen option.
    Text(String),
    Number(f64),
    Integer(i64),
    List(Vec<Record>),
}

/// A list's row: each field's value, by key.
pub type Record = BTreeMap<String, Value>;

/// A plugin's values, by key. A widget setting has none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Values(pub BTreeMap<String, Value>);

impl Values {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// The values as `settings.json` holds them, and `settings.get` gives
    /// them: an object by key, a list as an array of objects.
    pub fn to_json(&self) -> Json {
        Json::Object(
            self.0
                .iter()
                .map(|(key, value)| (key.clone(), value.to_json()))
                .collect(),
        )
    }
}

impl Value {
    pub fn to_json(&self) -> Json {
        match self {
            Value::Bool(on) => Json::Bool(*on),
            Value::Text(text) => Json::String(text.clone()),
            Value::Number(number) => serde_json::Number::from_f64(*number)
                .map(Json::Number)
                .unwrap_or(Json::Null),
            Value::Integer(number) => Json::from(*number),
            Value::List(rows) => Json::Array(
                rows.iter()
                    .map(|record| {
                        Json::Object(
                            record
                                .iter()
                                .map(|(key, value)| (key.clone(), value.to_json()))
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

/// A number as the form writes it: `3`, `0.25`.
pub fn number_text(number: f64) -> String {
    number.to_string()
}

// The manifest's shape, before it's checked.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSetting {
    key: String,
    #[serde(rename = "type")]
    kind: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default: Option<Json>,
    #[serde(default)]
    options: Option<Vec<RawChoice>>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    max_length: Option<usize>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    pattern_message: Option<String>,
    #[serde(default)]
    fields: Option<Vec<RawSetting>>,
    #[serde(default)]
    max_rows: Option<usize>,
    #[serde(default)]
    when: Option<RawWhen>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawChoice {
    Value(String),
    Labelled { value: String, label: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWhen {
    #[serde(default)]
    show: Option<RawTests>,
    #[serde(default)]
    enable: Option<RawTests>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTests {
    One(RawTest),
    All(Vec<RawTest>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTest {
    setting: String,
    #[serde(default)]
    is: Option<Json>,
    #[serde(default)]
    not: Option<Json>,
    #[serde(default, rename = "in")]
    any: Option<Vec<Json>>,
    #[serde(default)]
    empty: Option<bool>,
}

impl TryFrom<Vec<RawSetting>> for Schema {
    type Error = String;

    fn try_from(raw: Vec<RawSetting>) -> Result<Self, String> {
        if raw.len() > SETTINGS {
            return Err(format!("a plugin has at most {SETTINGS} settings"));
        }
        let mut settings: Vec<Setting> = Vec::with_capacity(raw.len());
        for raw in raw {
            let key = raw.key.clone();
            let setting =
                check_setting(raw, &settings).map_err(|why| format!("setting {key:?}: {why}"))?;
            settings.push(setting);
        }
        Ok(Self { settings })
    }
}

fn check_key(key: &str) -> Result<(), String> {
    let ok = !key.is_empty()
        && key.len() <= KEY_BYTES
        && key.starts_with(|letter: char| letter.is_ascii_lowercase())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "a key is 1 to {KEY_BYTES} lowercase letters, digits and _, starting with a letter"
        ))
    }
}

fn check_label(label: &str) -> Result<(), String> {
    if label.trim().is_empty()
        || label.chars().count() > ui::LABEL_CHARS
        || label.contains(['\n', '\r'])
    {
        return Err(format!(
            "its label must be one line of 1 to {} characters",
            ui::LABEL_CHARS
        ));
    }
    Ok(())
}

fn check_setting(raw: RawSetting, before: &[Setting]) -> Result<Setting, String> {
    check_key(&raw.key)?;
    if before.iter().any(|setting| setting.key == raw.key) {
        return Err("another setting has this key".into());
    }
    check_label(&raw.label)?;
    let description = raw.description.clone().unwrap_or_default();
    if description.chars().count() > DESCRIPTION_CHARS {
        return Err(format!(
            "its description has over {DESCRIPTION_CHARS} characters"
        ));
    }
    let (show, enable) = match &raw.when {
        None => (Vec::new(), Vec::new()),
        Some(when) => (
            tests(when.show.as_ref(), before)?,
            tests(when.enable.as_ref(), before)?,
        ),
    };
    let key = raw.key.clone();
    let label = raw.label.clone();
    let kind = check_kind(raw, true)?;
    Ok(Setting {
        key,
        label,
        description,
        kind,
        show,
        enable,
    })
}

/// Fields a setting of `kind` may carry beyond `key`, `type`, `label`,
/// `description`, `when` and `default`.
fn allowed(kind: &str) -> &'static [&'static str] {
    match kind {
        "string" => &["max_length", "placeholder", "pattern", "pattern_message"],
        "select" => &["options"],
        "slider" | "number" | "integer" => &["min", "max", "step"],
        "list" => &["fields", "max_rows"],
        _ => &[],
    }
}

fn check_kind(raw: RawSetting, top: bool) -> Result<Kind, String> {
    let present = [
        ("options", raw.options.is_some()),
        ("min", raw.min.is_some()),
        ("max", raw.max.is_some()),
        ("step", raw.step.is_some()),
        ("max_length", raw.max_length.is_some()),
        ("placeholder", raw.placeholder.is_some()),
        ("pattern", raw.pattern.is_some()),
        ("pattern_message", raw.pattern_message.is_some()),
        ("fields", raw.fields.is_some()),
        ("max_rows", raw.max_rows.is_some()),
    ];
    for (name, _) in present.iter().filter(|(_, set)| *set) {
        if !allowed(&raw.kind).contains(name) {
            return Err(format!("{name} doesn't apply to a {} setting", raw.kind));
        }
    }
    if !top && raw.when.is_some() {
        return Err("a list's field can't have when".into());
    }
    let kind = match raw.kind.as_str() {
        "bool" => Kind::Bool { default: false },
        "string" => {
            let max_chars = raw.max_length.unwrap_or(DEFAULT_TEXT_CHARS);
            if max_chars == 0 || max_chars > TEXT_CHARS {
                return Err(format!("max_length must be 1 to {TEXT_CHARS}"));
            }
            let placeholder = raw.placeholder.unwrap_or_default();
            if placeholder.chars().count() > ui::LABEL_CHARS {
                return Err(format!(
                    "placeholder has over {} characters",
                    ui::LABEL_CHARS
                ));
            }
            let pattern = match raw.pattern {
                None => None,
                Some(pattern) if pattern.len() > PATTERN_BYTES => {
                    return Err(format!("pattern has over {PATTERN_BYTES} bytes"));
                }
                // Anchored, so the whole value must match, and capped in
                // size; the regex crate runs in linear time.
                Some(pattern) => Some(
                    regex::RegexBuilder::new(&format!("^(?:{pattern})$"))
                        .size_limit(256 * 1024)
                        .build()
                        .map_err(|_| {
                            format!("pattern {pattern:?} isn't a valid regular expression")
                        })?,
                ),
            };
            let pattern_message = raw.pattern_message.unwrap_or_default();
            if pattern_message.chars().count() > DESCRIPTION_CHARS {
                return Err(format!(
                    "pattern_message has over {DESCRIPTION_CHARS} characters"
                ));
            }
            Kind::String(Text {
                default: String::new(),
                max_chars,
                placeholder,
                pattern,
                pattern_message,
            })
        }
        "select" => {
            let raw_options = raw.options.ok_or("a select needs options")?;
            if raw_options.is_empty() || raw_options.len() > OPTIONS {
                return Err(format!("a select has 1 to {OPTIONS} options"));
            }
            let mut options: Vec<Choice> = Vec::new();
            for option in raw_options {
                let (value, label) = match option {
                    RawChoice::Value(value) => (value.clone(), value),
                    RawChoice::Labelled { value, label } => (value, label),
                };
                if value.is_empty() || value.len() > KEY_BYTES {
                    return Err(format!("an option's value is 1 to {KEY_BYTES} bytes"));
                }
                check_label(&label).map_err(|_| {
                    format!(
                        "option {value:?} needs a one-line label of 1 to {} characters",
                        ui::LABEL_CHARS
                    )
                })?;
                if options.iter().any(|other| other.value == value) {
                    return Err(format!("option {value:?} is there twice"));
                }
                options.push(Choice { value, label });
            }
            let default = options[0].value.clone();
            Kind::Select { options, default }
        }
        "slider" | "number" => {
            let finite = |number: Option<f64>, name: &str| match number {
                Some(number) if !number.is_finite() => Err(format!("{name} must be a number")),
                _ => Ok(number),
            };
            let (min, max, step) = (
                finite(raw.min, "min")?,
                finite(raw.max, "max")?,
                finite(raw.step, "step")?,
            );
            if let (Some(min), Some(max)) = (min, max)
                && min >= max
            {
                return Err("min must be below max".into());
            }
            if step.is_some_and(|step| step <= 0.0) {
                return Err("step must be above 0".into());
            }
            if raw.kind == "slider" {
                let (Some(low), Some(high)) = (min, max) else {
                    return Err("a slider needs min and max".into());
                };
                let step = step.unwrap_or((high - low) / 100.0);
                if (high - low) / step > SLIDER_STEPS {
                    return Err(format!("a slider has at most {SLIDER_STEPS} steps"));
                }
                Kind::Slider(Range {
                    min,
                    max,
                    step: Some(step),
                    default: low,
                })
            } else {
                let default = max.map_or(0.0_f64, |max| max.min(0.0));
                let default = min.map_or(default, |min| default.max(min));
                Kind::Number(Range {
                    min,
                    max,
                    step,
                    default,
                })
            }
        }
        "integer" => {
            let whole = |number: Option<f64>, name: &str| match number {
                Some(number) if number.fract() != 0.0 || number.abs() > 9_007_199_254_740_991.0 => {
                    Err(format!("{name} must be a whole number"))
                }
                #[allow(clippy::cast_possible_truncation)]
                Some(number) => Ok(Some(number as i64)),
                None => Ok(None),
            };
            let (min, max, step) = (
                whole(raw.min, "min")?,
                whole(raw.max, "max")?,
                whole(raw.step, "step")?,
            );
            if let (Some(min), Some(max)) = (min, max)
                && min >= max
            {
                return Err("min must be below max".into());
            }
            if step.is_some_and(|step| step <= 0) {
                return Err("step must be above 0".into());
            }
            let default = max.map_or(0, |max| max.min(0));
            let default = min.map_or(default, |min| default.max(min));
            Kind::Integer {
                min,
                max,
                step,
                default,
            }
        }
        "list" => {
            if !top {
                return Err("a list's field can't be a list".into());
            }
            let raw_fields = raw.fields.ok_or("a list needs fields")?;
            if raw_fields.is_empty() || raw_fields.len() > FIELDS {
                return Err(format!("a list's record has 1 to {FIELDS} fields"));
            }
            let mut fields: Vec<Field> = Vec::new();
            for raw_field in raw_fields {
                let key = raw_field.key.clone();
                let field = check_field(raw_field, &fields)
                    .map_err(|why| format!("field {key:?}: {why}"))?;
                fields.push(field);
            }
            let max_rows = raw.max_rows.unwrap_or(DEFAULT_ROWS);
            if max_rows == 0 || max_rows > ROWS {
                return Err(format!("max_rows must be 1 to {ROWS}"));
            }
            Kind::List {
                fields,
                max_rows,
                default: Vec::new(),
            }
        }
        "widget" => {
            if !top {
                return Err("a list's field can't be a widget".into());
            }
            if raw.default.is_some() {
                return Err("a widget setting has no default".into());
            }
            Kind::Widget
        }
        other => {
            return Err(format!(
                "its type, {other:?}, isn't bool, string, select, slider, number, integer, list or widget"
            ));
        }
    };
    if !top && matches!(kind, Kind::Slider(_)) {
        return Err("a list's field can't be a slider".into());
    }
    let kind = match raw.default {
        None => kind,
        Some(json) => {
            let default = kind
                .read(&json)
                .map_err(|why| format!("its default can't be used: {why}"))?;
            kind.with_default(default)
        }
    };
    // A default left out must suit the rules too, as 0 may not.
    if let Some(default) = kind.default_value() {
        kind.check(default).map_err(|refused| {
            format!("it needs a default its own rules take: {}", refused.message)
        })?;
    }
    Ok(kind)
}

fn check_field(raw: RawSetting, before: &[Field]) -> Result<Field, String> {
    check_key(&raw.key)?;
    if before.iter().any(|field| field.key == raw.key) {
        return Err("another field has this key".into());
    }
    check_label(&raw.label)?;
    if raw.description.is_some() {
        return Err("a list's field has no description".into());
    }
    let key = raw.key.clone();
    let label = raw.label.clone();
    Ok(Field {
        key,
        label,
        kind: check_kind(raw, false)?,
    })
}

fn tests(raw: Option<&RawTests>, before: &[Setting]) -> Result<Vec<Test>, String> {
    let raw: Vec<&RawTest> = match raw {
        None => return Ok(Vec::new()),
        Some(RawTests::One(test)) => vec![test],
        Some(RawTests::All(tests)) => tests.iter().collect(),
    };
    if raw.is_empty() || raw.len() > TESTS {
        return Err(format!("when has 1 to {TESTS} tests"));
    }
    raw.into_iter()
        .map(|test| check_test(test, before))
        .collect()
}

fn check_test(raw: &RawTest, before: &[Setting]) -> Result<Test, String> {
    let Some(target) = before.iter().find(|setting| setting.key == raw.setting) else {
        return Err(format!(
            "when names {:?}, which isn't a setting declared before it",
            raw.setting
        ));
    };
    let given = [
        raw.is.is_some(),
        raw.not.is_some(),
        raw.any.is_some(),
        raw.empty.is_some(),
    ];
    if given.iter().filter(|given| **given).count() != 1 {
        return Err("a test has exactly one of is, not, in and empty".into());
    }
    let compare = |json: &Json| -> Result<Value, String> {
        match target.kind {
            Kind::List { .. } | Kind::Widget => {
                Err(format!("{:?} can only be tested with empty", raw.setting))
            }
            _ => target.kind.read(json).map_err(|why| {
                format!(
                    "when compares {:?} with a value it can't hold: {why}",
                    raw.setting
                )
            }),
        }
    };
    let check = if let Some(json) = &raw.is {
        Check::Is(compare(json)?)
    } else if let Some(json) = &raw.not {
        Check::Not(compare(json)?)
    } else if let Some(values) = &raw.any {
        if values.is_empty() || values.len() > OPTIONS {
            return Err(format!("in has 1 to {OPTIONS} values"));
        }
        Check::In(values.iter().map(compare).collect::<Result<_, _>>()?)
    } else {
        if !matches!(target.kind, Kind::String(_) | Kind::List { .. }) {
            return Err(format!(
                "empty tests a string or a list, and {:?} is neither",
                raw.setting
            ));
        }
        Check::Empty(raw.empty.unwrap_or(true))
    };
    Ok(Test {
        setting: raw.setting.clone(),
        check,
    })
}

impl Kind {
    fn with_default(self, value: Value) -> Kind {
        match (self, value) {
            (Kind::Bool { .. }, Value::Bool(default)) => Kind::Bool { default },
            (Kind::String(text), Value::Text(default)) => Kind::String(Text { default, ..text }),
            (Kind::Select { options, .. }, Value::Text(default)) => {
                Kind::Select { options, default }
            }
            (Kind::Slider(range), Value::Number(default)) => {
                Kind::Slider(Range { default, ..range })
            }
            (Kind::Number(range), Value::Number(default)) => {
                Kind::Number(Range { default, ..range })
            }
            (Kind::Integer { min, max, step, .. }, Value::Integer(default)) => Kind::Integer {
                min,
                max,
                step,
                default,
            },
            (
                Kind::List {
                    fields, max_rows, ..
                },
                Value::List(default),
            ) => Kind::List {
                fields,
                max_rows,
                default,
            },
            (kind, _) => kind,
        }
    }

    /// Its default value; `None` for a widget.
    pub fn default_value(&self) -> Option<Value> {
        Some(match self {
            Kind::Bool { default } => Value::Bool(*default),
            Kind::String(text) => Value::Text(text.default.clone()),
            Kind::Select { default, .. } => Value::Text(default.clone()),
            Kind::Slider(range) | Kind::Number(range) => Value::Number(range.default),
            Kind::Integer { default, .. } => Value::Integer(*default),
            Kind::List { default, .. } => Value::List(default.clone()),
            Kind::Widget => return None,
        })
    }

    /// A value from JSON, as a manifest's default or `settings.json` holds
    /// it, refused with the reason when the setting can't hold it.
    pub fn read(&self, json: &Json) -> Result<Value, String> {
        match (self, json) {
            (Kind::Bool { .. }, Json::Bool(on)) => Ok(Value::Bool(*on)),
            (Kind::String(_) | Kind::Select { .. }, Json::String(text)) => self
                .check(Value::Text(text.clone()))
                .map_err(|refused| refused.message),
            (Kind::Slider(_) | Kind::Number(_), Json::Number(number)) => {
                let number = number.as_f64().ok_or("not a number")?;
                self.check(Value::Number(number))
                    .map_err(|refused| refused.message)
            }
            (Kind::Integer { .. }, Json::Number(number)) => {
                let number = number.as_i64().ok_or("not a whole number")?;
                self.check(Value::Integer(number))
                    .map_err(|refused| refused.message)
            }
            (
                Kind::List {
                    fields, max_rows, ..
                },
                Json::Array(rows),
            ) => {
                if rows.len() > *max_rows {
                    return Err(format!("it has over {max_rows} rows"));
                }
                rows.iter()
                    .map(|row| {
                        let Json::Object(row) = row else {
                            return Err("a row isn't an object".to_owned());
                        };
                        let (record, problems) = read_record(fields, row);
                        problems.into_iter().next().map_or(Ok(record), Err)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::List)
            }
            (Kind::Widget, _) => Err("a widget setting holds no value".into()),
            (kind, _) => Err(format!("it isn't {}", kind.type_words())),
        }
    }

    fn type_words(&self) -> &'static str {
        match self {
            Kind::Bool { .. } => "true or false",
            Kind::String(_) | Kind::Select { .. } => "a string",
            Kind::Slider(_) | Kind::Number(_) => "a number",
            Kind::Integer { .. } => "a whole number",
            Kind::List { .. } => "a list of rows",
            Kind::Widget => "a value",
        }
    }

    /// Whether this setting may hold `value`, refused in plain words a form
    /// can show under the field.
    pub fn check(&self, value: Value) -> Result<Value, Refused> {
        let refuse = |message: String, value: &Value| Refused {
            message,
            input: match value {
                Value::Text(text) => text.clone(),
                Value::Number(number) => number_text(*number),
                Value::Integer(number) => number.to_string(),
                _ => String::new(),
            },
        };
        match (self, &value) {
            (Kind::Bool { .. }, Value::Bool(_)) => Ok(value),
            (Kind::String(text), Value::Text(typed)) => {
                if typed.chars().count() > text.max_chars {
                    Err(refuse(
                        format!("Keep it to {} characters.", text.max_chars),
                        &value,
                    ))
                } else if typed.chars().any(char::is_control) {
                    Err(refuse("Use one line, without tabs.".into(), &value))
                } else if !typed.is_empty()
                    && text
                        .pattern
                        .as_ref()
                        .is_some_and(|pattern| !pattern.is_match(typed))
                {
                    let message = if text.pattern_message.is_empty() {
                        "That isn't in the form this setting takes.".to_owned()
                    } else {
                        text.pattern_message.clone()
                    };
                    Err(refuse(message, &value))
                } else {
                    Ok(value)
                }
            }
            (Kind::Select { options, .. }, Value::Text(chosen)) => {
                if options.iter().any(|option| option.value == *chosen) {
                    Ok(value)
                } else {
                    Err(refuse("Choose one of the options.".into(), &value))
                }
            }
            (Kind::Slider(range) | Kind::Number(range), Value::Number(number)) => {
                let number = *number;
                let words = range_words(range.min, range.max, number_text);
                if !number.is_finite()
                    || range.min.is_some_and(|min| number < min)
                    || range.max.is_some_and(|max| number > max)
                {
                    Err(refuse(format!("Enter a number{words}."), &value))
                } else if let Some(step) = range.step
                    && !on_step(number - range.min.unwrap_or(0.0), step)
                {
                    Err(refuse(
                        format!("Use steps of {}.", number_text(step)),
                        &value,
                    ))
                } else {
                    Ok(value)
                }
            }
            (Kind::Integer { min, max, step, .. }, Value::Integer(number)) => {
                let number = *number;
                let words = range_words(*min, *max, |number: i64| number.to_string());
                if min.is_some_and(|min| number < min) || max.is_some_and(|max| number > max) {
                    Err(refuse(format!("Enter a whole number{words}."), &value))
                } else if let Some(step) = step
                    && (i128::from(number) - i128::from(min.unwrap_or(0))) % i128::from(*step) != 0
                {
                    Err(refuse(format!("Use steps of {step}."), &value))
                } else {
                    Ok(value)
                }
            }
            (Kind::List { max_rows, .. }, Value::List(rows)) => {
                if rows.len() > *max_rows {
                    Err(refuse(format!("It holds up to {max_rows} rows."), &value))
                } else {
                    Ok(value)
                }
            }
            _ => Err(refuse(format!("Enter {}.", self.type_words()), &value)),
        }
    }

    /// What the form typed, as this setting's value: a select's option by
    /// its value, a number's text.
    pub fn parse(&self, input: Input) -> Result<Value, Refused> {
        let refuse = |message: &str, input: &str| Refused {
            message: message.to_owned(),
            input: input.to_owned(),
        };
        let value = match (self, input) {
            (Kind::Bool { .. }, Input::Bool(on)) => Value::Bool(on),
            (Kind::String(_) | Kind::Select { .. }, Input::Text(text)) => Value::Text(text),
            (Kind::Slider(range) | Kind::Number(range), Input::Text(text)) => {
                match text.trim().parse::<f64>() {
                    Ok(number) if number.is_finite() => Value::Number(number),
                    _ => {
                        let words = range_words(range.min, range.max, number_text);
                        return Err(refuse(&format!("Enter a number{words}."), &text));
                    }
                }
            }
            (Kind::Integer { min, max, .. }, Input::Text(text)) => match text.trim().parse::<i64>()
            {
                Ok(number) => Value::Integer(number),
                Err(_) => {
                    let words = range_words(*min, *max, |number: i64| number.to_string());
                    return Err(refuse(&format!("Enter a whole number{words}."), &text));
                }
            },
            (kind, input) => {
                let text = match input {
                    Input::Text(text) => text,
                    Input::Bool(on) => on.to_string(),
                };
                return Err(refuse(&format!("Enter {}.", kind.type_words()), &text));
            }
        };
        self.check(value)
    }
}

fn range_words<T: Copy>(min: Option<T>, max: Option<T>, text: impl Fn(T) -> String) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!(" from {} to {}", text(min), text(max)),
        (Some(min), None) => format!(" of {} or more", text(min)),
        (None, Some(max)) => format!(" of {} or less", text(max)),
        (None, None) => String::new(),
    }
}

fn on_step(offset: f64, step: f64) -> bool {
    let steps = offset / step;
    (steps - steps.round()).abs() <= 1e-9 * steps.abs().max(1.0)
}

/// A row from JSON: each field's value, its default where it's missing or
/// can't be used, and what was wrong.
fn read_record(fields: &[Field], row: &serde_json::Map<String, Json>) -> (Record, Vec<String>) {
    let mut problems: Vec<String> = row
        .keys()
        .filter(|key| !fields.iter().any(|field| field.key == **key))
        .map(|key| format!("a row has {key:?}, which isn't one of its fields"))
        .collect();
    let record = fields
        .iter()
        .filter_map(|field| {
            let value = match row.get(&field.key).map(|json| field.kind.read(json)) {
                Some(Ok(value)) => Some(value),
                Some(Err(why)) => {
                    problems.push(format!("a row's {:?} can't be used: {why}", field.key));
                    field.kind.default_value()
                }
                None => field.kind.default_value(),
            };
            value.map(|value| (field.key.clone(), value))
        })
        .collect();
    (record, problems)
}

/// A value the form can't take, in plain words, with what was typed, which
/// the field keeps showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub message: String,
    pub input: String,
}

/// What the form changes: one edit at a time, from the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Set {
        key: String,
        value: Input,
    },
    /// Sets one field of a list's row.
    SetField {
        key: String,
        row: usize,
        field: String,
        value: Input,
    },
    /// Adds a row of the fields' defaults at the end.
    AddRow {
        key: String,
    },
    RemoveRow {
        key: String,
        row: usize,
    },
    MoveRow {
        key: String,
        row: usize,
        to: usize,
    },
    /// Every setting back to its default.
    Reset,
}

/// What the form gives: a switch's state, or the text of anything else, a
/// select's being its option's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Bool(bool),
    Text(String),
}

/// Whether a setting shows, and whether it can change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub shown: bool,
    pub enabled: bool,
}

impl Schema {
    /// The schema of a manifest's `settings` array, as JSON.
    pub fn parse(json: &Json) -> Result<Self, String> {
        Self::deserialize(json).map_err(|error| error.to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.settings.is_empty()
    }

    pub fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.iter().find(|setting| setting.key == key)
    }

    pub fn defaults(&self) -> Values {
        Values(
            self.settings
                .iter()
                .filter_map(|setting| Some((setting.key.clone(), setting.kind.default_value()?)))
                .collect(),
        )
    }

    /// Values from `settings.json`'s JSON: each one the schema can use, and
    /// its default for the rest. What was dropped, and why, comes back to be
    /// logged.
    pub fn read(&self, json: &Json) -> (Values, Vec<String>) {
        let mut values = self.defaults();
        let Json::Object(stored) = json else {
            return (values, vec!["it isn't a JSON object".into()]);
        };
        let mut dropped = Vec::new();
        for (key, json) in stored {
            let Some(setting) = self.setting(key) else {
                dropped.push(format!("{key:?} isn't one of its settings"));
                continue;
            };
            let value = match (&setting.kind, json) {
                // A list keeps the rows it can use, each field it can.
                (
                    Kind::List {
                        fields, max_rows, ..
                    },
                    Json::Array(rows),
                ) => {
                    let mut kept = Vec::new();
                    for row in rows {
                        let Json::Object(row) = row else {
                            dropped.push(format!("a row of {key:?} isn't an object"));
                            continue;
                        };
                        if kept.len() == *max_rows {
                            dropped.push(format!("{key:?} has over {max_rows} rows"));
                            break;
                        }
                        let (record, problems) = read_record(fields, row);
                        dropped.extend(problems.into_iter().map(|why| format!("{key:?}: {why}")));
                        kept.push(record);
                    }
                    Ok(Value::List(kept))
                }
                (kind, json) => kind.read(json),
            };
            match value {
                Ok(value) => {
                    values.0.insert(key.clone(), value);
                }
                Err(why) => dropped.push(format!("{key:?} can't be used: {why}")),
            }
        }
        (values, dropped)
    }

    /// Whether each setting shows and can change, in order, given `values`.
    pub fn states(&self, values: &Values) -> Vec<State> {
        let mut states: Vec<State> = Vec::with_capacity(self.settings.len());
        for setting in &self.settings {
            let state_of = |key: &str, states: &[State]| {
                self.settings
                    .iter()
                    .position(|other| other.key == key)
                    .and_then(|index| states.get(index).copied())
                    .unwrap_or_default()
            };
            let holds = |test: &Test| test.holds(values.get(&test.setting));
            let shown = setting
                .show
                .iter()
                .all(|test| holds(test) && state_of(&test.setting, &states).shown);
            let enabled = shown
                && setting
                    .enable
                    .iter()
                    .all(|test| holds(test) && state_of(&test.setting, &states).enabled)
                && setting
                    .show
                    .iter()
                    .all(|test| state_of(&test.setting, &states).enabled);
            states.push(State { shown, enabled });
        }
        states
    }
}

impl Test {
    fn holds(&self, value: Option<&Value>) -> bool {
        let Some(value) = value else { return false };
        match &self.check {
            Check::Is(wanted) => value == wanted,
            Check::Not(unwanted) => value != unwanted,
            Check::In(wanted) => wanted.contains(value),
            Check::Empty(empty) => {
                let is_empty = match value {
                    Value::Text(text) => text.is_empty(),
                    Value::List(rows) => rows.is_empty(),
                    _ => false,
                };
                is_empty == *empty
            }
        }
    }
}

/// A plugin's settings as its page shows them: the schema, the values, what
/// the form refused, and the trees widget settings draw.
///
/// The runtime keeps one per plugin. For each [`Edit`] it calls
/// [`Form::edit`]; when that names changed keys, it hands the values to
/// [`Store::save`] and gives the plugin `settings.on_change` with those
/// keys, while the plugin runs. Nothing restarts.
#[derive(Clone, Debug, Default)]
pub struct Form {
    pub schema: Arc<Schema>,
    pub values: Values,
    /// What the form refused, by [`address`], until that field changes.
    pub refused: BTreeMap<String, Refused>,
    /// What a widget setting draws, by key, once the runtime supplies it.
    pub widgets: BTreeMap<String, Arc<ui::Tree>>,
}

/// Where a refusal shows: a setting's key, or `key/row/field` for a list's
/// field.
pub fn address(key: &str, cell: Option<(usize, &str)>) -> String {
    match cell {
        None => key.to_owned(),
        Some((row, field)) => format!("{key}/{row}/{field}"),
    }
}

impl Form {
    pub fn new(schema: Arc<Schema>, values: Values) -> Self {
        Self {
            schema,
            values,
            refused: BTreeMap::new(),
            widgets: BTreeMap::new(),
        }
    }

    /// Applies an edit, and returns the keys whose values changed: none for
    /// one that changed nothing, or was refused, which [`Form::refused`]
    /// then says.
    pub fn edit(&mut self, edit: Edit) -> Vec<String> {
        if edit == Edit::Reset {
            self.refused.clear();
            let defaults = self.schema.defaults();
            let changed = defaults
                .0
                .iter()
                .filter(|(key, value)| self.values.get(key) != Some(value))
                .map(|(key, _)| key.clone())
                .collect();
            self.values = defaults;
            return changed;
        }
        let key = match &edit {
            Edit::Set { key, .. }
            | Edit::SetField { key, .. }
            | Edit::AddRow { key }
            | Edit::RemoveRow { key, .. }
            | Edit::MoveRow { key, .. } => key.clone(),
            Edit::Reset => unreachable!(),
        };
        let schema = Arc::clone(&self.schema);
        let Some(index) = schema
            .settings
            .iter()
            .position(|setting| setting.key == key)
        else {
            return Vec::new();
        };
        let setting = &schema.settings[index];
        // The form offers neither a hidden nor a disabled setting.
        if !schema.states(&self.values)[index].enabled {
            return Vec::new();
        }
        let Some(current) = self.values.get(&key).cloned() else {
            return Vec::new();
        };
        let next = match (edit, &setting.kind, current.clone()) {
            (Edit::Set { value, .. }, kind, _) => {
                let place = address(&key, None);
                match kind.parse(value) {
                    Ok(value) => {
                        self.refused.remove(&place);
                        value
                    }
                    Err(refused) => {
                        self.refused.insert(place, refused);
                        return Vec::new();
                    }
                }
            }
            (
                Edit::SetField {
                    row, field, value, ..
                },
                Kind::List { fields, .. },
                Value::List(mut rows),
            ) => {
                let (Some(spec), Some(record)) = (
                    fields.iter().find(|spec| spec.key == field),
                    rows.get_mut(row),
                ) else {
                    return Vec::new();
                };
                let place = address(&key, Some((row, &field)));
                match spec.kind.parse(value) {
                    Ok(value) => {
                        self.refused.remove(&place);
                        record.insert(field, value);
                    }
                    Err(refused) => {
                        self.refused.insert(place, refused);
                        return Vec::new();
                    }
                }
                Value::List(rows)
            }
            (
                Edit::AddRow { .. },
                Kind::List {
                    fields, max_rows, ..
                },
                Value::List(mut rows),
            ) => {
                if rows.len() >= *max_rows {
                    return Vec::new();
                }
                rows.push(
                    fields
                        .iter()
                        .filter_map(|field| Some((field.key.clone(), field.kind.default_value()?)))
                        .collect(),
                );
                Value::List(rows)
            }
            (Edit::RemoveRow { row, .. }, Kind::List { .. }, Value::List(mut rows)) => {
                if row >= rows.len() {
                    return Vec::new();
                }
                rows.remove(row);
                self.forget_rows(&key);
                Value::List(rows)
            }
            (Edit::MoveRow { row, to, .. }, Kind::List { .. }, Value::List(mut rows)) => {
                if row >= rows.len() || to >= rows.len() {
                    return Vec::new();
                }
                let moved = rows.remove(row);
                rows.insert(to, moved);
                self.forget_rows(&key);
                Value::List(rows)
            }
            _ => return Vec::new(),
        };
        if next == current {
            return Vec::new();
        }
        self.values.0.insert(key.clone(), next);
        vec![key]
    }

    /// Drops what was refused in a list's rows, once they move.
    fn forget_rows(&mut self, key: &str) {
        let prefix = format!("{key}/");
        self.refused.retain(|place, _| !place.starts_with(&prefix));
    }
}

/// Where a plugin's values are kept: `data/<id>/settings.json` in `folder`,
/// the plugins' folder, `<data>/plugins`.
pub fn path(folder: &Path, plugin: &str) -> PathBuf {
    folder.join("data").join(plugin).join(FILE)
}

/// A plugin's stored values, checked against its schema: the defaults for
/// a missing file, and for anything it holds that can't be used, which is
/// logged. It reads a small file; the runtime calls it on its own thread as
/// it loads a plugin, not on the interface's.
pub fn load(folder: &Path, plugin: &str, schema: &Schema) -> Values {
    let path = path(folder, plugin);
    let json = match std::fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return schema.defaults(),
        Ok(metadata) if metadata.len() > FILE_BYTES => {
            tracing::warn!(path = %path.display(), "a plugin's settings are over 1 MiB; using its defaults");
            return schema.defaults();
        }
        _ => std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<Json>(&bytes).map_err(|error| error.to_string())
            }),
    };
    match json {
        Ok(json) => {
            let (values, dropped) = schema.read(&json);
            for why in dropped {
                tracing::warn!(plugin, why, "dropped a stored plugin setting");
            }
            values
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "couldn't read a plugin's settings; using its defaults");
            schema.defaults()
        }
    }
}

/// How long the store waits for edits to stop before writing.
pub const DEBOUNCE: Duration = Duration::from_millis(500);
/// The longest a change waits while edits keep coming.
const LONGEST_WAIT: Duration = Duration::from_secs(3);
/// How long the writer thread waits for more before it ends.
const IDLE: Duration = Duration::from_secs(30);

enum Message {
    Save(String, Values),
    Flush(mpsc::Sender<()>),
}

/// Writes plugins' values on a thread of its own, so the interface never
/// waits on a disk: a change is written after [`DEBOUNCE`] without
/// another, only if the file would change, through a temporary file renamed
/// over it. The thread starts with the first change and ends after
/// 30 seconds of none, so a Yapfast whose settings don't change has none.
pub struct Store {
    folder: PathBuf,
    writer: Arc<Mutex<Option<mpsc::Sender<Message>>>>,
}

impl Store {
    /// A store for the plugins' folder, `<data>/plugins`.
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            writer: Arc::new(Mutex::new(None)),
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Keeps `values` as `plugin`'s, soon.
    pub fn save(&self, plugin: &str, values: &Values) {
        self.send(Message::Save(plugin.to_owned(), values.clone()));
    }

    /// Writes what's waiting now, and returns once it's written.
    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        self.send(Message::Flush(done));
        let _ = wait.recv();
    }

    fn send(&self, message: Message) {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let message = match writer.as_ref() {
            Some(sender) => match sender.send(message) {
                Ok(()) => return,
                Err(mpsc::SendError(message)) => message,
            },
            None => message,
        };
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(message);
        *writer = Some(sender);
        let folder = self.folder.clone();
        let slot = Arc::clone(&self.writer);
        let started = thread::Builder::new()
            .name("plugin-settings".into())
            .spawn(move || write_loop(&folder, &receiver, &slot));
        if let Err(error) = started {
            tracing::warn!(%error, "couldn't start writing plugin settings");
            *writer = None;
        }
    }
}

impl Drop for Store {
    /// Writes what's waiting, as Yapfast quits.
    fn drop(&mut self) {
        let running = self
            .writer
            .lock()
            .map(|writer| writer.is_some())
            .unwrap_or(false);
        if running {
            self.flush();
        }
    }
}

fn write_loop(
    folder: &Path,
    receiver: &mpsc::Receiver<Message>,
    slot: &Mutex<Option<mpsc::Sender<Message>>>,
) {
    let mut waiting: BTreeMap<String, Values> = BTreeMap::new();
    let mut since: Option<Instant> = None;
    loop {
        let timeout = match since {
            Some(first) => DEBOUNCE.min(LONGEST_WAIT.saturating_sub(first.elapsed())),
            None => IDLE,
        };
        match receiver.recv_timeout(timeout) {
            Ok(Message::Save(plugin, values)) => {
                waiting.insert(plugin, values);
                since.get_or_insert_with(Instant::now);
            }
            Ok(Message::Flush(done)) => {
                write_all(folder, &mut waiting);
                since = None;
                let _ = done.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) if since.is_some() => {
                write_all(folder, &mut waiting);
                since = None;
            }
            Err(_) => {
                // Idle: end, unless a change came as the thread decided to.
                let mut writer = slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match receiver.try_recv() {
                    Ok(Message::Save(plugin, values)) => {
                        waiting.insert(plugin, values);
                        since.get_or_insert_with(Instant::now);
                    }
                    Ok(Message::Flush(done)) => {
                        write_all(folder, &mut waiting);
                        let _ = done.send(());
                    }
                    Err(_) => {
                        *writer = None;
                        return;
                    }
                }
            }
        }
    }
}

fn write_all(folder: &Path, waiting: &mut BTreeMap<String, Values>) {
    for (plugin, values) in std::mem::take(waiting) {
        if let Err(error) = write(folder, &plugin, &values) {
            tracing::warn!(plugin, %error, "couldn't store a plugin's settings");
        }
    }
}

/// Writes one plugin's values, unless the file holds them already.
pub fn write(folder: &Path, plugin: &str, values: &Values) -> std::io::Result<()> {
    let path = path(folder, plugin);
    let mut bytes = serde_json::to_vec_pretty(&values.to_json()).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    if std::fs::read(&path).is_ok_and(|stored| stored == bytes) {
        return Ok(());
    }
    let directory = path.parent().expect("the path has a folder");
    std::fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(json: Json) -> Result<Schema, String> {
        Schema::parse(&json)
    }

    fn refused(json: Json) -> String {
        schema(json).unwrap_err()
    }

    fn word_filter() -> Schema {
        schema(json!([
            { "key": "enabled_here", "type": "bool", "label": "Filter in DMs", "default": true },
            { "key": "mode", "type": "select", "label": "What to do", "default": "replace",
              "options": [{ "value": "hide", "label": "Hide the message" }, { "value": "replace", "label": "Replace the word" }] },
            { "key": "replacement", "type": "string", "label": "Replacement", "default": "***",
              "max_length": 8, "pattern": "[*#!]+", "pattern_message": "Use *, # or !.",
              "when": { "show": { "setting": "mode", "is": "replace" } } },
            { "key": "strength", "type": "slider", "label": "Blur", "min": 0, "max": 1, "step": 0.25, "default": 0.5 },
            { "key": "ratio", "type": "number", "label": "Ratio", "min": 0.5, "step": 0.5, "default": 1.5 },
            { "key": "shortest", "type": "integer", "label": "Shortest word", "min": 1, "max": 20, "default": 3,
              "when": { "enable": { "setting": "enabled_here", "is": true } } },
            { "key": "words", "type": "list", "label": "Words", "max_rows": 3,
              "fields": [
                { "key": "word", "type": "string", "label": "Word", "pattern": "\\S+" },
                { "key": "with", "type": "string", "label": "Replace with" },
                { "key": "whole", "type": "bool", "label": "Whole word", "default": true }
              ],
              "default": [{ "word": "heck", "with": "h*ck" }] },
            { "key": "preview", "type": "widget", "label": "Preview",
              "when": { "show": { "setting": "words", "empty": false } } }
        ]))
        .unwrap()
    }

    #[test]
    fn every_type_reads_with_its_default() {
        let schema = word_filter();
        let defaults = schema.defaults();
        assert_eq!(defaults.get("enabled_here"), Some(&Value::Bool(true)));
        assert_eq!(defaults.get("mode"), Some(&Value::Text("replace".into())));
        assert_eq!(defaults.get("strength"), Some(&Value::Number(0.5)));
        assert_eq!(defaults.get("shortest"), Some(&Value::Integer(3)));
        let Some(Value::List(rows)) = defaults.get("words") else {
            panic!()
        };
        // A missing field takes its own default.
        assert_eq!(rows[0].get("whole"), Some(&Value::Bool(true)));
        assert_eq!(defaults.get("preview"), None);
        assert_eq!(defaults.to_json()["words"][0]["word"], "heck");
        // Without a default: the first option, the low end, false, nothing.
        let plain = schema_of(json!([
            { "key": "a", "type": "select", "label": "A", "options": ["x", "y"] },
            { "key": "b", "type": "slider", "label": "B", "min": 2, "max": 4 },
            { "key": "c", "type": "integer", "label": "C", "min": -5, "max": -1 },
            { "key": "d", "type": "number", "label": "D" }
        ]));
        let values = plain.defaults();
        assert_eq!(values.get("a"), Some(&Value::Text("x".into())));
        assert_eq!(values.get("b"), Some(&Value::Number(2.0)));
        assert_eq!(values.get("c"), Some(&Value::Integer(-1)));
        assert_eq!(values.get("d"), Some(&Value::Number(0.0)));
    }

    fn schema_of(json: Json) -> Schema {
        schema(json).unwrap()
    }

    #[test]
    fn a_schema_is_strict_and_says_why() {
        let cases = [
            (
                json!([{ "key": "a", "type": "colour", "label": "A" }]),
                "isn't bool",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "A", "colour": 1 }]),
                "unknown field",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "A", "options": ["x"] }]),
                "options doesn't apply",
            ),
            (
                json!([{ "key": "A", "type": "bool", "label": "A" }]),
                "lowercase",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "" }]),
                "label",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "A" }, { "key": "a", "type": "bool", "label": "B" }]),
                "another setting",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "A", "default": "yes" }]),
                "default can't be used",
            ),
            (
                json!([{ "key": "a", "type": "select", "label": "A" }]),
                "needs options",
            ),
            (
                json!([{ "key": "a", "type": "select", "label": "A", "options": ["x", "x"] }]),
                "twice",
            ),
            (
                json!([{ "key": "a", "type": "select", "label": "A", "options": ["x"], "default": "y" }]),
                "Choose one",
            ),
            (
                json!([{ "key": "a", "type": "slider", "label": "A", "min": 0 }]),
                "needs min and max",
            ),
            (
                json!([{ "key": "a", "type": "slider", "label": "A", "min": 0, "max": 1, "step": 0.0001 }]),
                "steps",
            ),
            (
                json!([{ "key": "a", "type": "number", "label": "A", "min": 2, "max": 1 }]),
                "below max",
            ),
            (
                json!([{ "key": "a", "type": "number", "label": "A", "step": -1 }]),
                "above 0",
            ),
            (
                json!([{ "key": "a", "type": "number", "label": "A", "max": 3, "default": 4 }]),
                "3 or less",
            ),
            (
                json!([{ "key": "a", "type": "integer", "label": "A", "min": 0.5 }]),
                "whole number",
            ),
            (
                json!([{ "key": "a", "type": "integer", "label": "A", "step": 2, "default": 3 }]),
                "steps of 2",
            ),
            (
                json!([{ "key": "a", "type": "string", "label": "A", "pattern": "(" }]),
                "regular expression",
            ),
            (
                json!([{ "key": "a", "type": "string", "label": "A", "max_length": 5000 }]),
                "max_length",
            ),
            (
                json!([{ "key": "a", "type": "string", "label": "A", "pattern": "a+", "default": "b" }]),
                "form this setting takes",
            ),
            (
                json!([{ "key": "a", "type": "list", "label": "A" }]),
                "needs fields",
            ),
            (
                json!([{ "key": "a", "type": "list", "label": "A", "fields": [{ "key": "b", "type": "list", "label": "B", "fields": [] }] }]),
                "can't be a list",
            ),
            (
                json!([{ "key": "a", "type": "list", "label": "A", "fields": [{ "key": "b", "type": "slider", "label": "B", "min": 0, "max": 1 }] }]),
                "can't be a slider",
            ),
            (
                json!([{ "key": "a", "type": "list", "label": "A", "max_rows": 1, "fields": [{ "key": "b", "type": "bool", "label": "B" }], "default": [{}, {}] }]),
                "over 1 rows",
            ),
            (
                json!([{ "key": "a", "type": "list", "label": "A", "fields": [{ "key": "b", "type": "bool", "label": "B" }], "default": [{ "c": true }] }]),
                "isn't one of its fields",
            ),
            (
                json!([{ "key": "a", "type": "widget", "label": "A", "default": 1 }]),
                "no default",
            ),
            (
                json!([{ "key": "a", "type": "bool", "label": "A", "when": { "show": { "setting": "b", "is": true } } }]),
                "declared before",
            ),
            (
                json!([{ "key": "b", "type": "bool", "label": "B" }, { "key": "a", "type": "bool", "label": "A", "when": { "show": { "setting": "b", "is": 1 } } }]),
                "can't hold",
            ),
            (
                json!([{ "key": "b", "type": "bool", "label": "B" }, { "key": "a", "type": "bool", "label": "A", "when": { "show": { "setting": "b", "is": true, "not": false } } }]),
                "exactly one",
            ),
            (
                json!([{ "key": "b", "type": "bool", "label": "B" }, { "key": "a", "type": "bool", "label": "A", "when": { "show": { "setting": "b", "empty": true } } }]),
                "neither",
            ),
            (
                json!([{ "key": "b", "type": "bool", "label": "B" }, { "key": "a", "type": "bool", "label": "A", "when": { "hide": { "setting": "b", "is": true } } }]),
                "unknown field",
            ),
        ];
        for (json, why) in cases {
            let error = refused(json.clone());
            assert!(error.contains(why), "{json} gave {error:?}, not {why:?}");
        }
        let many: Vec<Json> = (0..=SETTINGS)
            .map(|index| json!({ "key": format!("s{index}"), "type": "bool", "label": "S" }))
            .collect();
        assert!(refused(Json::Array(many)).contains("at most 64"));
    }

    #[test]
    fn a_manifest_carries_its_settings() {
        let manifest = super::super::Manifest::parse(
            r#"{ "api": 1, "id": "com.example.a", "name": "A",
                 "settings": [{ "key": "on", "type": "bool", "label": "On" }] }"#,
        )
        .unwrap();
        assert_eq!(manifest.settings.settings.len(), 1);
        let error = super::super::Manifest::parse(
            r#"{ "api": 1, "id": "com.example.a", "name": "A",
                 "settings": [{ "key": "on", "type": "switch", "label": "On" }] }"#,
        )
        .unwrap_err();
        assert!(error.contains("isn't bool"), "{error}");
    }

    #[test]
    fn the_form_parses_what_was_typed_and_refuses_in_plain_words() {
        let mut form = Form::new(Arc::new(word_filter()), word_filter().defaults());
        let set = |key: &str, text: &str| Edit::Set {
            key: key.into(),
            value: Input::Text(text.into()),
        };
        assert_eq!(form.edit(set("shortest", "5")), ["shortest"]);
        assert!(
            form.edit(set("shortest", "5")).is_empty(),
            "the same value changes nothing"
        );
        assert!(form.edit(set("shortest", "five")).is_empty());
        assert_eq!(
            form.refused["shortest"],
            Refused {
                message: "Enter a whole number from 1 to 20.".into(),
                input: "five".into()
            }
        );
        assert!(form.edit(set("shortest", "21")).is_empty());
        assert_eq!(
            form.refused["shortest"].message,
            "Enter a whole number from 1 to 20."
        );
        assert_eq!(form.edit(set("shortest", " 7 ")), ["shortest"]);
        assert!(!form.refused.contains_key("shortest"));

        assert!(form.edit(set("replacement", "abc")).is_empty());
        assert_eq!(form.refused["replacement"].message, "Use *, # or !.");
        assert!(form.edit(set("replacement", "*********")).is_empty());
        assert_eq!(
            form.refused["replacement"].message,
            "Keep it to 8 characters."
        );
        assert!(form.edit(set("replacement", "a\nb")).is_empty());
        assert_eq!(
            form.edit(set("replacement", "")),
            ["replacement"],
            "empty always may be"
        );

        assert!(form.edit(set("ratio", "1.2")).is_empty());
        assert_eq!(form.refused["ratio"].message, "Use steps of 0.5.");
        assert!(form.edit(set("ratio", "0")).is_empty());
        assert_eq!(
            form.refused["ratio"].message,
            "Enter a number of 0.5 or more."
        );
        assert_eq!(form.edit(set("ratio", "2.5")), ["ratio"]);
        assert_eq!(form.edit(set("strength", "0.75")), ["strength"]);

        assert!(form.edit(set("mode", "blur")).is_empty());
        assert_eq!(form.refused["mode"].message, "Choose one of the options.");
        assert_eq!(
            form.edit(Edit::Set {
                key: "enabled_here".into(),
                value: Input::Bool(false)
            }),
            ["enabled_here"]
        );
        assert!(form.edit(set("nothing", "1")).is_empty());
    }

    #[test]
    fn when_shows_and_enables_from_earlier_settings() {
        let schema = word_filter();
        let mut form = Form::new(Arc::new(schema.clone()), schema.defaults());
        let state = |form: &Form, key: &str| {
            let index = form
                .schema
                .settings
                .iter()
                .position(|setting| setting.key == key)
                .unwrap();
            form.schema.states(&form.values)[index]
        };
        assert_eq!(
            state(&form, "replacement"),
            State {
                shown: true,
                enabled: true
            }
        );
        assert_eq!(
            state(&form, "preview"),
            State {
                shown: true,
                enabled: true
            }
        );
        form.edit(Edit::Set {
            key: "mode".into(),
            value: Input::Text("hide".into()),
        });
        assert_eq!(
            state(&form, "replacement"),
            State {
                shown: false,
                enabled: false
            }
        );
        // A hidden setting can't change.
        assert!(
            form.edit(Edit::Set {
                key: "replacement".into(),
                value: Input::Text("#".into())
            })
            .is_empty()
        );

        form.edit(Edit::Set {
            key: "enabled_here".into(),
            value: Input::Bool(false),
        });
        assert_eq!(
            state(&form, "shortest"),
            State {
                shown: true,
                enabled: false
            }
        );
        assert!(
            form.edit(Edit::Set {
                key: "shortest".into(),
                value: Input::Text("4".into())
            })
            .is_empty()
        );

        form.edit(Edit::RemoveRow {
            key: "words".into(),
            row: 0,
        });
        assert!(!state(&form, "preview").shown);

        // A test on a hidden setting hides what depends on it, and `in`
        // and `not` test as they read.
        let chained = schema_of(json!([
            { "key": "a", "type": "bool", "label": "A" },
            { "key": "b", "type": "select", "label": "B", "options": ["x", "y", "z"], "when": { "show": { "setting": "a", "is": true } } },
            { "key": "c", "type": "bool", "label": "C", "when": { "show": [{ "setting": "b", "in": ["x", "y"] }, { "setting": "b", "not": "y" }] } }
        ]));
        let mut values = chained.defaults();
        assert!(!chained.states(&values)[2].shown);
        values.0.insert("a".into(), Value::Bool(true));
        assert!(chained.states(&values)[2].shown);
        values.0.insert("b".into(), Value::Text("y".into()));
        assert!(!chained.states(&values)[2].shown);
    }

    #[test]
    fn the_list_editor_adds_removes_and_reorders_within_its_cap() {
        let schema = word_filter();
        let mut form = Form::new(Arc::new(schema.clone()), schema.defaults());
        let words = |form: &Form| -> Vec<String> {
            let Some(Value::List(rows)) = form.values.get("words") else {
                panic!()
            };
            rows.iter()
                .map(|row| match row.get("word") {
                    Some(Value::Text(word)) => word.clone(),
                    _ => panic!(),
                })
                .collect()
        };
        let add = || Edit::AddRow {
            key: "words".into(),
        };
        let set = |row: usize, text: &str| Edit::SetField {
            key: "words".into(),
            row,
            field: "word".into(),
            value: Input::Text(text.into()),
        };
        assert_eq!(form.edit(add()), ["words"]);
        assert_eq!(form.edit(set(1, "darn")), ["words"]);
        assert!(form.edit(set(1, "two words")).is_empty());
        assert_eq!(
            form.refused["words/1/word"].message,
            "That isn't in the form this setting takes."
        );
        assert!(form.edit(set(9, "x")).is_empty(), "no such row");
        form.edit(add());
        assert!(form.edit(add()).is_empty(), "three rows at most");
        assert_eq!(words(&form), ["heck", "darn", ""]);
        assert_eq!(
            form.edit(Edit::MoveRow {
                key: "words".into(),
                row: 2,
                to: 0
            }),
            ["words"]
        );
        assert_eq!(words(&form), ["", "heck", "darn"]);
        assert!(form.refused.is_empty(), "a refusal goes once its row moves");
        assert_eq!(
            form.edit(Edit::RemoveRow {
                key: "words".into(),
                row: 0
            }),
            ["words"]
        );
        assert_eq!(words(&form), ["heck", "darn"]);
        assert!(
            form.edit(Edit::MoveRow {
                key: "words".into(),
                row: 0,
                to: 5
            })
            .is_empty()
        );

        assert_eq!(form.edit(Edit::Reset), ["words"]);
        assert_eq!(words(&form), ["heck"]);
        assert!(form.edit(Edit::Reset).is_empty());
    }

    #[test]
    fn stored_values_keep_what_the_schema_takes_and_drop_the_rest() {
        let schema = word_filter();
        let (values, dropped) = schema.read(&json!({
            "mode": "hide",
            "shortest": 99,
            "gone": true,
            "preview": 1,
            "strength": 0.3,
            "words": [{ "word": "drat", "extra": 1 }, 7, { "word": "a b" }, { "word": "x" }, { "word": "y" }]
        }));
        assert_eq!(values.get("mode"), Some(&Value::Text("hide".into())));
        assert_eq!(values.get("shortest"), Some(&Value::Integer(3)));
        assert_eq!(
            values.get("strength"),
            Some(&Value::Number(0.5)),
            "off its steps"
        );
        assert_eq!(
            values.get("enabled_here"),
            Some(&Value::Bool(true)),
            "missing: its default"
        );
        let Some(Value::List(rows)) = values.get("words") else {
            panic!()
        };
        let words: Vec<_> = rows.iter().map(|row| row["word"].clone()).collect();
        assert_eq!(
            words,
            [
                Value::Text("drat".into()),
                Value::Text(String::new()),
                Value::Text("x".into())
            ]
        );
        let all = dropped.join("\n");
        for why in [
            "\"shortest\"",
            "\"gone\"",
            "\"preview\"",
            "\"strength\"",
            "\"extra\"",
            "isn't an object",
            "over 3 rows",
            "\"word\"",
        ] {
            assert!(all.contains(why), "{why} in {all}");
        }
        let (values, dropped) = schema.read(&json!([1, 2]));
        assert_eq!(values, schema.defaults());
        assert_eq!(dropped.len(), 1);
    }

    #[test]
    fn values_round_trip_through_the_store_and_a_corrupt_file_gives_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let schema = word_filter();
        let plugin = "com.example.word-filter";
        assert_eq!(load(folder.path(), plugin, &schema), schema.defaults());

        let mut form = Form::new(Arc::new(schema.clone()), schema.defaults());
        form.edit(Edit::Set {
            key: "mode".into(),
            value: Input::Text("hide".into()),
        });
        form.edit(Edit::AddRow {
            key: "words".into(),
        });
        form.edit(Edit::Set {
            key: "ratio".into(),
            value: Input::Text("3".into()),
        });
        let store = Store::new(folder.path().to_owned());
        store.save(plugin, &schema.defaults());
        store.save(plugin, &form.values);
        store.flush();
        let file = path(folder.path(), plugin);
        assert!(file.ends_with("data/com.example.word-filter/settings.json"));
        assert_eq!(load(folder.path(), plugin, &schema), form.values);
        // Nothing left behind but the file itself.
        let entries = std::fs::read_dir(file.parent().unwrap()).unwrap().count();
        assert_eq!(entries, 1);

        // Writing the same values leaves the file alone.
        let modified = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        write(folder.path(), plugin, &form.values).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            modified
        );

        // A change waits for the debounce, then lands without a flush.
        form.edit(Edit::Reset);
        store.save(plugin, &form.values);
        assert_ne!(load(folder.path(), plugin, &schema), form.values);
        std::thread::sleep(DEBOUNCE + Duration::from_millis(400));
        assert_eq!(load(folder.path(), plugin, &schema), form.values);

        std::fs::write(&file, b"{ \"mode\": ").unwrap();
        assert_eq!(load(folder.path(), plugin, &schema), schema.defaults());
        drop(store);
    }

    #[test]
    fn dropping_the_store_writes_what_waits() {
        let folder = tempfile::tempdir().unwrap();
        let schema = word_filter();
        let mut values = schema.defaults();
        values.0.insert("shortest".into(), Value::Integer(9));
        let store = Store::new(folder.path().to_owned());
        store.save("com.example.a", &values);
        drop(store);
        assert_eq!(load(folder.path(), "com.example.a", &schema), values);
    }
}
