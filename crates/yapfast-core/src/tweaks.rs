//! Setting points: behaviours of Yapfast a plugin switches by naming them in
//! its manifest's `tweaks`, with no code of its own, so it needs no
//! WebAssembly. Each point is an ordinary field of [`Tweaks`], with a type,
//! a default and a doc line, read where the behaviour applies.
//!
//! [`derive`] lays the enabled plugins' points over the defaults, in order.
//! It gives `None` while no enabled plugin sets a point, so Yapfast tests one
//! `Option` where a point could apply and holds nothing otherwise. Two
//! plugins setting one point differently are a [`Conflict`], which names
//! both. See `docs/plugins.md`.
//!
//! Points are added one at a time, as a behaviour is worth exposing; none is
//! listed yet, so every manifest's `tweaks` is refused point by point as
//! unknown, and `derive` always gives `None`.

use std::{collections::BTreeMap, sync::Arc};

use serde_json::Value as Json;

use crate::plugins::Manifest;

/// The most points one plugin sets.
pub const TWEAKS: usize = 64;

/// A point as the registry lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    /// Its name in a manifest, as `messages.attachments.mosaic`.
    pub name: &'static str,
    pub doc: &'static str,
    pub kind: Kind,
}

/// The types a point can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Integer { min: i64, max: i64 },
}

/// A value a plugin sets a point to, checked against its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Integer(i64),
}

impl Kind {
    fn read(self, json: &Json) -> Result<Value, String> {
        match (self, json) {
            (Kind::Bool, Json::Bool(on)) => Ok(Value::Bool(*on)),
            (Kind::Integer { min, max }, Json::Number(number)) => match number.as_i64() {
                Some(number) if (min..=max).contains(&number) => Ok(Value::Integer(number)),
                _ => Err(format!("it takes a whole number from {min} to {max}")),
            },
            (Kind::Bool, _) => Err("it takes true or false".into()),
            (Kind::Integer { min, max }, _) => {
                Err(format!("it takes a whole number from {min} to {max}"))
            }
        }
    }
}

/// A field's type, as a point's value sets it.
pub trait Field: Sized {
    fn from_value(value: Value) -> Option<Self>;
}

impl Field for bool {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Bool(on) => Some(on),
            Value::Integer(_) => None,
        }
    }
}

impl Field for i64 {
    fn from_value(value: Value) -> Option<Self> {
        match value {
            Value::Integer(number) => Some(number),
            Value::Bool(_) => None,
        }
    }
}

/// A set of points: [`Tweaks`], or one a test makes with [`points!`].
pub trait Registry: Default {
    const POINTS: &'static [Point];
    /// Sets the point named, which [`Registry::POINTS`] lists with the
    /// value's kind.
    fn set(&mut self, point: &str, value: Value);
}

/// Declares a registry: a struct with one field per point, its defaults,
/// and the list of points with their names, kinds and doc lines.
macro_rules! points {
    ($(#[$meta:meta])* $vis:vis struct $name:ident {
        $( #[doc = $doc:literal] $point:literal => $field:ident: $type:ty = $default:expr, $kind:expr; )*
    }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        $vis struct $name {
            $( #[doc = $doc] pub $field: $type, )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        impl Registry for $name {
            const POINTS: &'static [Point] = &[
                $( Point { name: $point, doc: $doc.trim_ascii(), kind: $kind }, )*
            ];

            fn set(&mut self, point: &str, value: Value) {
                $(
                    if point == $point
                        && let Some(value) = <$type as Field>::from_value(value)
                    {
                        self.$field = value;
                    }
                )*
                let _ = (point, value);
            }
        }
    };
}

points! {
    /// Yapfast's setting points, at their defaults unless a plugin sets them.
    pub struct Tweaks {}
}

/// A plugin's points after checking: those that apply, and those refused,
/// each with why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Checked {
    pub id: String,
    pub name: String,
    pub set: Vec<(&'static str, Value)>,
    pub refused: Vec<(String, String)>,
}

/// A manifest's `tweaks`, checked against the points of `R`: a point
/// Yapfast doesn't list, perhaps one a newer Yapfast has, or a value of the
/// wrong type is refused, and the rest apply.
pub fn check<R: Registry>(manifest: &Manifest) -> Checked {
    let mut checked = Checked {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        ..Checked::default()
    };
    for (index, (name, json)) in manifest.tweaks.iter().enumerate() {
        if index == TWEAKS {
            checked.refused.push((
                name.clone(),
                format!("a plugin sets at most {TWEAKS} points"),
            ));
            break;
        }
        let Some(point) = R::POINTS.iter().find(|point| point.name == name) else {
            checked
                .refused
                .push((name.clone(), "this Yapfast has no such point".into()));
            continue;
        };
        match point.kind.read(json) {
            Ok(value) => checked.set.push((point.name, value)),
            Err(why) => checked.refused.push((name.clone(), why)),
        }
    }
    checked
}

/// Two plugins setting one point differently: `kept`'s value applies.
/// Both are named by their plugin's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub point: &'static str,
    pub kept: String,
    pub lost: String,
}

/// The points the enabled plugins set, given in order: the first plugin
/// keeps a point both set differently, unless `choices` names the plugin
/// (by id) the person chose for it. `None` when no plugin sets any.
pub fn derive<R: Registry>(
    plugins: &[Checked],
    choices: &BTreeMap<&'static str, String>,
) -> (Option<Arc<R>>, Vec<Conflict>) {
    let mut taken: BTreeMap<&'static str, (usize, Value)> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for (index, plugin) in plugins.iter().enumerate() {
        for &(point, value) in &plugin.set {
            match taken.get(point) {
                Some(&(_, other)) if other == value => {}
                Some(&(holder, _)) => {
                    let wins = choices.get(point) == Some(&plugin.id);
                    let (kept, lost) = if wins {
                        taken.insert(point, (index, value));
                        (index, holder)
                    } else {
                        (holder, index)
                    };
                    conflicts.push(Conflict {
                        point,
                        kept: plugins[kept].name.clone(),
                        lost: plugins[lost].name.clone(),
                    });
                }
                None => {
                    taken.insert(point, (index, value));
                }
            }
        }
    }
    if taken.is_empty() {
        return (None, conflicts);
    }
    let mut tweaks = R::default();
    for (point, (_, value)) in taken {
        tweaks.set(point, value);
    }
    (Some(Arc::new(tweaks)), conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;

    points! {
        struct Sample {
            /// Shows a channel's pictures as a mosaic.
            "messages.attachments.mosaic" => mosaic: bool = true, Kind::Bool;
            /// How many messages a group holds.
            "messages.group.size" => group: i64 = 5, Kind::Integer { min: 1, max: 20 };
        }
    }

    fn plugin(id: &str, tweaks: &str) -> Checked {
        let manifest = Manifest::parse(&format!(
            r#"{{ "api": 1, "id": "com.example.{id}", "name": "{id}", "tweaks": {tweaks} }}"#
        ))
        .unwrap();
        check::<Sample>(&manifest)
    }

    #[test]
    fn a_manifest_without_wasm_sets_points_by_name() {
        assert_eq!(Sample::POINTS[1].doc, "How many messages a group holds.");
        let checked = plugin(
            "a",
            r#"{ "messages.attachments.mosaic": false, "messages.group.size": 50, "messages.nope": 1, "messages.group.size2": true }"#,
        );
        assert_eq!(
            checked.set,
            [("messages.attachments.mosaic", Value::Bool(false))]
        );
        let mut refused: Vec<_> = checked
            .refused
            .iter()
            .map(|(name, why)| format!("{name}: {why}"))
            .collect();
        refused.sort();
        assert_eq!(
            refused,
            [
                "messages.group.size2: this Yapfast has no such point",
                "messages.group.size: it takes a whole number from 1 to 20",
                "messages.nope: this Yapfast has no such point",
            ]
        );
        let (tweaks, conflicts) = derive::<Sample>(&[checked], &BTreeMap::new());
        let tweaks = tweaks.unwrap();
        assert!(!tweaks.mosaic);
        assert_eq!(tweaks.group, 5, "a point no plugin sets keeps its default");
        assert!(conflicts.is_empty());
    }

    #[test]
    fn nothing_is_held_while_no_plugin_sets_a_point() {
        assert_eq!(derive::<Sample>(&[], &BTreeMap::new()), (None, Vec::new()));
        assert_eq!(
            derive::<Sample>(&[plugin("a", "{}")], &BTreeMap::new()).0,
            None
        );
        // Yapfast lists no points yet, so a manifest's are refused.
        let manifest = Manifest::parse(
            r#"{ "api": 1, "id": "com.example.a", "name": "A", "tweaks": { "messages.attachments.mosaic": false } }"#,
        )
        .unwrap();
        let checked = check::<Tweaks>(&manifest);
        assert!(checked.set.is_empty());
        assert_eq!(checked.refused.len(), 1);
        assert_eq!(derive::<Tweaks>(&[checked], &BTreeMap::new()).0, None);
    }

    #[test]
    fn two_plugins_setting_a_point_differently_are_named() {
        let first = plugin("first", r#"{ "messages.group.size": 3 }"#);
        let agrees = plugin("agrees", r#"{ "messages.group.size": 3 }"#);
        let second = plugin(
            "second",
            r#"{ "messages.group.size": 9, "messages.attachments.mosaic": false }"#,
        );
        let plugins = [first, agrees, second];
        let (tweaks, conflicts) = derive::<Sample>(&plugins, &BTreeMap::new());
        let tweaks = tweaks.unwrap();
        assert_eq!(tweaks.group, 3);
        assert!(!tweaks.mosaic, "the loser keeps its other points");
        assert_eq!(
            conflicts,
            [Conflict {
                point: "messages.group.size",
                kept: "first".into(),
                lost: "second".into()
            }]
        );
        let choices = BTreeMap::from([("messages.group.size", "com.example.second".to_owned())]);
        let (tweaks, conflicts) = derive::<Sample>(&plugins, &choices);
        assert_eq!(tweaks.unwrap().group, 9);
        assert_eq!(conflicts[0].kept, "second");
    }
}
