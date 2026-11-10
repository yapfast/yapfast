//! A plugin's manifest: what it is, what it needs, and what it adds.
//!
//! The manifest is strict JSON, read with the `serde_json` Yapfast already has.
//! Everything the interface shows before a plugin runs — its
//! commands, menu rows, settings — is here, so a plugin that has never been
//! activated still appears.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::API_VERSION;
use crate::capability::{Capability, Permissions};
use crate::ids::PluginId;
use crate::limits;

/// A version as a manifest writes it: `1.4.0`, or `1.4` for `1.4.0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// The leading number: two versions with different majors aren't
    /// compatible under a `^` requirement.
    pub const fn major(self) -> u64 {
        self.major
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        // A pre-release or build suffix, as in `1.0.0-beta.2`, is compared by
        // its numbers: the plugin system pins commits, not version tags.
        let core = match text.find(['-', '+']) {
            Some(cut) => &text[..cut],
            None => text,
        };
        let mut parts = core.split('.');
        let mut numbers = [0u64; 3];
        let mut seen = 0;
        for (index, part) in parts.by_ref().enumerate() {
            if index >= 3 {
                return Err(VersionError);
            }
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(VersionError);
            }
            numbers[index] = part.parse().map_err(|_| VersionError)?;
            seen = index + 1;
        }
        if seen == 0 {
            return Err(VersionError);
        }
        Ok(Version::new(numbers[0], numbers[1], numbers[2]))
    }
}

/// Why a string isn't a version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionError;

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a version is `major.minor[.patch]`, as in 1.4.0")
    }
}

impl std::error::Error for VersionError {}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A dependency requirement, as `^1.2`, `~1.4.0`, `1.4.0` or `*`.
///
/// The plugin system pins commits in the lock file, so requirements are read
/// once, when the graph is built, and compared against the version the
/// dependency's manifest declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionReq {
    pub op: Requirement,
    pub version: Version,
}

/// Which versions a requirement admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requirement {
    /// `*`: any version at all.
    Any,
    /// `^1.2.3`: the same major version, at least this one.
    Compatible,
    /// `~1.2.3`: the same major and minor, at least this one.
    Patch,
    /// `=1.2.3`: exactly this one.
    Exact,
}

impl VersionReq {
    /// Any version.
    pub const ANY: VersionReq = VersionReq {
        op: Requirement::Any,
        version: Version::new(0, 0, 0),
    };

    /// Whether `candidate` satisfies this requirement.
    pub fn matches(&self, candidate: Version) -> bool {
        let at_least = candidate >= self.version;
        match self.op {
            Requirement::Any => true,
            Requirement::Exact => candidate == self.version,
            Requirement::Patch => {
                at_least
                    && candidate.major == self.version.major
                    && candidate.minor == self.version.minor
            }
            // A major version of zero is its own compatibility boundary, so
            // `^0.2` admits `0.2.x` alone; the same rule cargo uses.
            Requirement::Compatible if self.version.major == 0 => {
                at_least && candidate.major == 0 && candidate.minor == self.version.minor
            }
            Requirement::Compatible => at_least && candidate.major == self.version.major,
        }
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self.op {
            Requirement::Any => return f.write_str("*"),
            Requirement::Compatible => "^",
            Requirement::Patch => "~",
            Requirement::Exact => "=",
        };
        write!(f, "{op}{}", self.version)
    }
}

impl FromStr for VersionReq {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text == "*" {
            return Ok(VersionReq::ANY);
        }
        let (op, rest) = match text.as_bytes().first() {
            Some(b'^') => (Requirement::Compatible, &text[1..]),
            Some(b'~') => (Requirement::Patch, &text[1..]),
            Some(b'=') => (Requirement::Exact, &text[1..]),
            _ => (Requirement::Compatible, text),
        };
        Ok(VersionReq {
            op,
            version: rest.parse()?,
        })
    }
}

impl Serialize for VersionReq {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for VersionReq {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// What wakes a plugin, from its manifest's `activation`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activation {
    /// Triggers such as `command`, `menu`, `startup` or `event:message_create`.
    #[serde(default)]
    pub on: Vec<String>,
}

impl Activation {
    /// The plugin asks to run at startup.
    pub fn at_startup(&self) -> bool {
        self.on.iter().any(|trigger| trigger == "startup")
    }

    /// The event names this activation subscribes to, without the prefix.
    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.on
            .iter()
            .filter_map(|trigger| trigger.strip_prefix("event:"))
    }
}

/// The memory and quota limits a manifest may raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    /// Pages of guest memory, 64 KiB each.
    pub memory_pages: u32,
    /// An HTTP response's cap, in bytes.
    pub http_response_bytes: u64,
    /// The plugin's storage, in bytes.
    pub storage_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            memory_pages: limits::DEFAULT_MEMORY_PAGES,
            http_response_bytes: limits::MAX_HTTP_RESPONSE_BYTES,
            storage_bytes: limits::DEFAULT_STORAGE_BYTES,
        }
    }
}

/// One option of a command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandOption {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: OptionKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The values a `choice` option admits; empty for every other kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

/// The type of a command option, as the composer parses and checks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionKind {
    String,
    Integer,
    Number,
    Bool,
    /// One of `choices`.
    Choice,
    /// A person, by id.
    Person,
    /// A channel, by id.
    Channel,
}

/// A command the plugin adds to the composer's `/` list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub options: Vec<CommandOption>,
}

/// What a menu row applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuTarget {
    Message,
    User,
    Channel,
    Server,
    Role,
    Emoji,
    Sticker,
    Image,
    Stream,
    Game,
    Composer,
    Attach,
}

/// A menu row the manifest contributes.
///
/// Menus are data, so a row appears without loading any WebAssembly; the
/// `when` predicates are a closed set of fields, with no expression language to
/// evaluate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuDecl {
    pub target: MenuTarget,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The action the plugin receives when the row is chosen.
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<WhenClause>,
}

/// The closed set of conditions a menu row or setting may carry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhenClause {
    /// `discord` or `matrix`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// Whether the message or person is the account's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dm: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_link: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_bot: Option<bool>,
}

/// The type of a generated setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    String,
    Bool,
    Select,
    Slider,
    Number,
    Integer,
    /// A list of records the person adds to and edits.
    List,
}

/// One setting the plugin's page draws for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDecl {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: SettingKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub default: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// A regular expression a string must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<WhenClause>,
}

/// A plugin, as its `yapfast-plugin.json` declares it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// The guest ABI and interface version this plugin was built for.
    pub api: u32,
    pub id: PluginId,
    pub name: String,
    pub version: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Where the source that was reviewed lives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_yapfast: Option<Version>,

    /// The WebAssembly module, relative to the manifest. Absent for a plugin
    /// that only sets named points or changes the interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    #[serde(default)]
    pub wasi: bool,

    #[serde(default)]
    pub limits: Limits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation: Option<Activation>,
    #[serde(default)]
    pub permissions: Permissions,

    #[serde(default)]
    pub commands: Vec<CommandDecl>,
    #[serde(default)]
    pub menus: Vec<MenuDecl>,
    #[serde(default)]
    pub settings: Vec<SettingDecl>,

    /// Plugins this one needs; it activates after them.
    #[serde(default)]
    pub dependencies: BTreeMap<PluginId, VersionReq>,
    /// Plugins this one uses when they are enabled.
    #[serde(default)]
    pub optional_dependencies: BTreeMap<PluginId, VersionReq>,
    /// Named behaviours this plugin switches.
    #[serde(default)]
    pub tweaks: BTreeMap<String, Value>,
}

impl PluginManifest {
    /// Reads a manifest from its JSON text, refusing one newer than this build.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: PluginManifest = serde_json::from_str(text).map_err(ManifestError::Json)?;
        manifest.problems()?;
        Ok(manifest)
    }

    /// Whether the plugin runs at startup, or must, because it intercepts
    /// outgoing messages before anything else may see them.
    pub fn activates_at_startup(&self) -> bool {
        self.activation.as_ref().is_some_and(Activation::at_startup)
            || self.permissions.allows(Capability::MessagesIntercept)
    }

    /// The capabilities, in the order the review dialog lists them.
    pub fn capabilities(&self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|capability| self.permissions.allows(*capability))
            .collect()
    }

    /// Everything wrong with the manifest, as a message names it.
    pub fn problems(&self) -> Result<(), ManifestError> {
        if self.api != API_VERSION {
            return Err(ManifestError::UnsupportedApi {
                asked: self.api,
                ours: API_VERSION,
            });
        }
        check_relative_path(self.wasm.as_deref(), "wasm")?;
        check_relative_path(self.icon.as_deref(), "icon")?;
        if self.wasm.is_none()
            && self.commands.is_empty()
            && self.menus.is_empty()
            && self.tweaks.is_empty()
            && self.settings.is_empty()
        {
            return Err(ManifestError::DoesNothing);
        }
        if !(limits::MIN_MEMORY_PAGES..=limits::MAX_MEMORY_PAGES)
            .contains(&self.limits.memory_pages)
        {
            return Err(ManifestError::MemoryPages {
                asked: self.limits.memory_pages,
            });
        }
        if self.limits.http_response_bytes > limits::MAX_HTTP_RESPONSE_CONSENTED_BYTES
            || self.limits.storage_bytes > limits::MAX_STORAGE_BYTES
        {
            return Err(ManifestError::Quota);
        }
        if self.permissions.local_port.is_some()
            && !self.permissions.allows(Capability::NetworkLocal)
        {
            return Err(ManifestError::PortWithoutCapability);
        }
        if self
            .commands
            .iter()
            .any(|command| command.name.is_empty() || command.description.as_deref() == Some(""))
        {
            return Err(ManifestError::Command);
        }
        let mut keys: Vec<&str> = self.settings.iter().map(|setting| &*setting.key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        if keys.len() != count {
            return Err(ManifestError::DuplicateSetting);
        }
        if self.dependencies.contains_key(&self.id) {
            return Err(ManifestError::DependsOnItself);
        }
        Ok(())
    }

    /// The file the manifest names, inside the plugin's directory.
    ///
    /// A path that climbs out of it, or is absolute, is a broken manifest or a
    /// hostile one, and either way it's refused before anything is read.
    pub fn resolve(&self, path: &Path) -> Option<PathBuf> {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => out.push(part),
                // `./plugin.wasm` reads the same file; anything else is refused.
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return None;
                }
            }
        }
        (!out.as_os_str().is_empty()).then_some(out)
    }
}

/// Refuses a path that isn't relative and doesn't stay inside the plugin.
fn check_relative_path(path: Option<&Path>, field: &'static str) -> Result<(), ManifestError> {
    let Some(path) = path else {
        return Ok(());
    };
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ManifestError::Path { field });
    }
    if path.as_os_str().is_empty() {
        return Err(ManifestError::Path { field });
    }
    Ok(())
}

/// Why a manifest was refused.
#[derive(Debug)]
pub enum ManifestError {
    /// Not JSON, or not this shape.
    Json(serde_json::Error),
    /// Built for another version of the plugin API.
    UnsupportedApi {
        asked: u32,
        ours: u32,
    },
    /// A path a plugin names that leaves its own directory.
    Path {
        field: &'static str,
    },
    /// No module and nothing it can do without one.
    DoesNothing,
    /// A memory limit outside the range a manifest may ask for.
    MemoryPages {
        asked: u32,
    },
    /// An HTTP or storage request above what consent may raise the cap to.
    Quota,
    PortWithoutCapability,
    /// A command with no name or an empty description.
    Command,
    /// Two settings with one key.
    DuplicateSetting,
    DependsOnItself,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Json(error) => write!(f, "the manifest isn't valid JSON: {error}"),
            ManifestError::UnsupportedApi { asked, ours } => write!(
                f,
                "this plugin is for plugin API {asked}; this Yapfast speaks {ours}"
            ),
            ManifestError::Path { field } => {
                write!(f, "`{field}` must be a path inside the plugin's directory")
            }
            ManifestError::DoesNothing => {
                f.write_str("this plugin has no `wasm` and adds no command, menu, setting or tweak")
            }
            ManifestError::MemoryPages { asked } => write!(
                f,
                "`memory_pages` is {asked}; it must be between {} and {}",
                limits::MIN_MEMORY_PAGES,
                limits::MAX_MEMORY_PAGES
            ),
            ManifestError::Quota => f.write_str("a limit asks for more than consent may allow"),
            ManifestError::PortWithoutCapability => {
                f.write_str("`local_port` needs the `network.local` capability")
            }
            ManifestError::Command => f.write_str("a command needs a name and a description"),
            ManifestError::DuplicateSetting => f.write_str("two settings share a key"),
            ManifestError::DependsOnItself => f.write_str("a plugin can't depend on itself"),
        }
    }
}

impl std::error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ManifestError::Json(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD_FILTER: &str = r#"{
        "api": 1,
        "id": "com.example.word-filter",
        "name": "Word Filter",
        "version": "1.4.0",
        "wasm": "plugin.wasm",
        "limits": { "memory_pages": 128 },
        "activation": { "on": ["event:message_create", "command"] },
        "permissions": {
            "capabilities": ["messages.read", "storage"],
            "network": ["api.example.com", "*.cdn.example.com"]
        },
        "commands": [
            { "name": "filter", "description": "Add a word", "options": [
                { "name": "word", "type": "string", "required": true },
                { "name": "mode", "type": "choice", "choices": ["hide", "replace"] } ] }
        ],
        "menus": [
            { "target": "message", "label": "Filter this word", "action": "filter_selected",
              "when": { "network": "discord", "own": false } }
        ],
        "settings": [
            { "key": "replacement", "type": "string", "label": "Replacement text", "default": "***" }
        ]
    }"#;

    #[test]
    fn reads_the_plan_s_example() {
        let manifest = PluginManifest::parse(WORD_FILTER).unwrap();
        assert_eq!(manifest.id.as_str(), "com.example.word-filter");
        assert_eq!(manifest.limits.memory_pages, 128);
        assert_eq!(manifest.commands[0].options[0].kind, OptionKind::String);
        assert!(
            !manifest.activates_at_startup(),
            "activation lists no startup and nothing intercepts"
        );
        assert_eq!(manifest.permissions.network.len(), 2);
        assert!(manifest.permissions.allows(Capability::Storage));
    }

    #[test]
    fn a_choice_option_names_what_it_admits() {
        let manifest = PluginManifest::parse(WORD_FILTER).unwrap();
        let mode = manifest.commands[0]
            .options
            .iter()
            .find(|option| option.name == "mode")
            .expect("the example declares a choice option");
        assert_eq!(mode.kind, OptionKind::Choice);
        assert_eq!(mode.choices, ["hide", "replace"]);
        assert!(
            manifest.commands[0].options[0].choices.is_empty(),
            "a string option admits anything"
        );
    }

    #[test]
    fn an_intercepting_plugin_starts_with_the_app() {
        let text = WORD_FILTER.replace("\"storage\"", "\"messages.intercept\"");
        let manifest = PluginManifest::parse(&text).unwrap();
        assert!(manifest.activates_at_startup());
    }

    #[test]
    fn refuses_a_newer_api() {
        let text = WORD_FILTER.replace("\"api\": 1", "\"api\": 2");
        assert!(matches!(
            PluginManifest::parse(&text),
            Err(ManifestError::UnsupportedApi { asked: 2, .. })
        ));
    }

    #[test]
    fn refuses_a_path_out_of_the_plugin() {
        for wasm in ["../plugin.wasm", "/plugin.wasm", "a/../../plugin.wasm"] {
            let text = WORD_FILTER.replace("\"plugin.wasm\"", &format!("\"{wasm}\""));
            assert!(
                matches!(
                    PluginManifest::parse(&text),
                    Err(ManifestError::Path { field: "wasm" })
                ),
                "{wasm} should be refused"
            );
        }
    }

    #[test]
    fn a_host_needs_no_separate_capability() {
        let manifest = PluginManifest::parse(WORD_FILTER).unwrap();
        assert!(manifest.permissions.reaches_network());
        assert!(!manifest.permissions.allows(Capability::Network));
    }

    #[test]
    fn refuses_memory_outside_the_range() {
        for pages in [16, 1025] {
            let text = WORD_FILTER.replace(
                "\"memory_pages\": 128",
                &format!("\"memory_pages\": {pages}"),
            );
            assert!(matches!(
                PluginManifest::parse(&text),
                Err(ManifestError::MemoryPages { .. })
            ));
        }
    }

    #[test]
    fn reads_a_manifest_only_plugin() {
        let text = r#"{
            "api": 1,
            "id": "com.example.no-mosaic",
            "name": "No Mosaic",
            "version": "1.0.0",
            "tweaks": { "messages.attachments.mosaic": false }
        }"#;
        let manifest = PluginManifest::parse(text).unwrap();
        assert!(manifest.wasm.is_none());
        assert_eq!(manifest.limits.memory_pages, limits::DEFAULT_MEMORY_PAGES);
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!("1.4.0".parse::<Version>().unwrap() < "1.10.0".parse().unwrap());
        assert_eq!("1.4".parse::<Version>().unwrap(), Version::new(1, 4, 0));
        assert!("1.4.0-beta.2".parse::<Version>().is_ok());
    }

    #[test]
    fn requirements_admit_what_they_should() {
        let caret: VersionReq = "^1.2".parse().unwrap();
        assert!(caret.matches(Version::new(1, 2, 0)));
        assert!(caret.matches(Version::new(1, 9, 3)));
        assert!(!caret.matches(Version::new(2, 0, 0)));
        assert!(!caret.matches(Version::new(1, 1, 9)));

        let patch: VersionReq = "~1.4.0".parse().unwrap();
        assert!(patch.matches(Version::new(1, 4, 7)));
        assert!(!patch.matches(Version::new(1, 5, 0)));

        let exact: VersionReq = "=1.4.0".parse().unwrap();
        assert!(exact.matches(Version::new(1, 4, 0)));
        assert!(!exact.matches(Version::new(1, 4, 1)));

        let zero: VersionReq = "^0.2".parse().unwrap();
        assert!(zero.matches(Version::new(0, 2, 5)));
        assert!(!zero.matches(Version::new(0, 3, 0)));

        assert!(
            "*".parse::<VersionReq>()
                .unwrap()
                .matches(Version::new(9, 9, 9))
        );
    }
}
