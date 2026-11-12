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

    /// The plugin asks to draw under messages on screen, as `yf_decorate`.
    pub fn decorates(&self) -> bool {
        self.on.iter().any(|trigger| trigger == "decorate")
    }

    /// The plugin asks to say how people look, as `yf_identity`: one answer
    /// per person and server, which the parent keeps.
    pub fn identities(&self) -> bool {
        self.on.iter().any(|trigger| trigger == "identity")
    }

    /// The plugin asks to colour code blocks, as `yf_code_block`: one answer per
    /// block, which the parent keeps by the block's content hash.
    pub fn colours_code(&self) -> bool {
        self.on.iter().any(|trigger| trigger == "code")
    }

    /// The plugin asks to badge people in the member list, as
    /// `yf_member_decorate`.
    pub fn decorates_members(&self) -> bool {
        self.on.iter().any(|trigger| trigger == "member.decorate")
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
///
/// A closed list, so a name outside it is refused while the JSON is read. It is
/// the list of menus the interface has, one for one with
/// `plugins::ui::Target` in yapfast-core, and the match that turns one into the
/// other names every variant: a menu added to one and not the other stops the
/// build rather than being a target a plugin may name and no menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuTarget {
    Message,
    User,
    /// Someone's profile.
    UserProfile,
    Channel,
    Thread,
    /// A conversation with several people and no server.
    GroupDm,
    Server,
    /// The dropdown in a server's header.
    ServerHeader,
    Role,
    Emoji,
    Sticker,
    Image,
    Stream,
    Game,
    ForumTag,
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
///
/// Each field given must match and each left out always does, and the fields are
/// the ones the interface tests (`plugins::ui::When` in yapfast-core): a
/// condition nothing tests would be one a person read in the manifest and never
/// saw applied, so a name outside this set is refused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhenClause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkName>,
    /// Whether the message or person is the account's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dm: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_attachment: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_bot: Option<bool>,
    /// A role id the person the menu is about has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// A network as a manifest names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkName {
    Discord,
    Matrix,
}

/// The most characters a label can have, on a menu row, a button or the label a
/// `ui` section puts on a built-in row.
///
/// The interface keeps the same number (`plugins::ui::LABEL_CHARS` in
/// yapfast-core) and cuts nothing: a longer label is refused while the manifest
/// is read, because one that won't show is a mistake worth fixing rather than
/// something to draw short.
pub const LABEL_CHARS: usize = 64;

/// The most menu rows, buttons or changes to built-in UI one plugin may add.
///
/// Again the interface's number (`plugins::ui::CHANGES`): a plugin past it is
/// refused rather than silently keeping the first 64, so a manifest that grew
/// by copy and paste is caught before anyone installs it.
pub const CHANGES: usize = 64;

/// A button a plugin puts where Yapfast already draws a row of its own: a
/// `rail_items` entry at the bottom of the server rail, and a `header_buttons`
/// entry in a conversation's header.
///
/// Both are plain data, drawn from the manifest before the plugin has ever run,
/// so both live here with the rest of what a manifest declares. Choosing one
/// sends the plugin its `action`, as a menu row's does, and needs no capability:
/// it is the plugin's own surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonDecl {
    pub label: String,
    /// What choosing it tells the plugin, as a plain name.
    pub action: String,
}

/// A change to one of Yapfast's built-in menu rows, from a manifest's
/// `ui.menu`.
///
/// `row` is the built-in row's id, and the row is hidden, relabelled or moved
/// `before` or `after` another: not both, which the interface refuses on the
/// plugin's page rather than at the manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiRow {
    pub target: MenuTarget,
    /// The built-in row's id, as the interface names it, such as `copy-id`.
    pub row: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<WhenClause>,
}

/// The named parts of the interface a `ui` section hides or replaces.
///
/// The names are the interface's (`plugins::ui::Part` in yapfast-core), which is
/// also where a name it doesn't have is refused, since only it knows which parts
/// exist this build.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiParts {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hide: Vec<String>,
    /// Replacing one needs the plugin's `wasm` to draw what takes its place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replace: Vec<String>,
}

/// A manifest's `ui` section: what a plugin changes in the interface Yapfast
/// already draws.
///
/// It is here, with the rest of the manifest, because the interface draws it
/// from the file alone: an install reads this and refuses what it can't draw.
/// Which colours, sizes, parts and built-in rows exist is the interface's own
/// business — it changes with Yapfast, not with the plugin ABI — so a name it
/// doesn't have is refused there, on the plugin's page, with a sentence that
/// says what it knew. Asking for any of it needs `ui.modify`, and a section
/// without that capability changes nothing rather than refusing the plugin.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiDecl {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<UiRow>,
    #[serde(default, skip_serializing_if = "UiParts::is_empty")]
    pub parts: UiParts,
    /// Colour tokens, by name, as `#rgb`, `#rrggbb` or `#rrggbbaa`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Sizes, by name, in logical pixels, each clamped to its range.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sizes: BTreeMap<String, f64>,
}

impl UiParts {
    /// Whether it names nothing, which a manifest may leave out entirely.
    pub fn is_empty(&self) -> bool {
        self.hide.is_empty() && self.replace.is_empty()
    }
}

impl UiDecl {
    /// Whether it changes nothing, which a manifest may leave out entirely.
    pub fn is_empty(&self) -> bool {
        self.changes() == 0
    }

    /// How many things the section changes: a row, a part, a colour or a size
    /// each count one against [`CHANGES`].
    pub fn changes(&self) -> usize {
        self.menu.len()
            + self.parts.hide.len()
            + self.parts.replace.len()
            + self.colors.len()
            + self.sizes.len()
    }
}

/// Whether a `bool` is false, which a manifest may leave out.
fn is_false(value: &bool) -> bool {
    !*value
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
    /// A tree the plugin draws while it runs; it carries no value of its own,
    /// which is why it needs no other field.
    Widget,
}

/// One setting the plugin's page draws for it.
///
/// This mirrors what `crates/yapfast-core/src/plugins/settings.rs` reads, field
/// for field, because two readers of one file must not disagree: the installer
/// validated a narrower shape than the interface drew, so a plugin whose
/// settings were written the way `docs/plugins/settings.md` documents them was
/// refused at install and worked when loaded from a folder. Which kinds accept
/// which fields is the interface's check, and stays there; this is the shape.
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
    /// A select's choices: a value, or a value with a label to show for it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SettingChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The step a slider moves in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// The longest text a string holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// A regular expression a string must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// What to say when a string doesn't match [`SettingDecl::pattern`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_message: Option<String>,
    /// A list's fields: the settings one row holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SettingDecl>,
    /// The most rows a list holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<usize>,
    /// Whether this setting is drawn, and whether it can be changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<SettingWhen>,
}

/// One choice in a `select` setting: a bare value, or a value and the label to
/// show for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingChoice {
    Value(String),
    Labelled { value: String, label: String },
}

/// Whether a setting is drawn, and whether it can be changed.
///
/// A plugin states it as tests over its own settings, so a page can hide what
/// does not apply: `show` decides whether the row is there at all, `enable`
/// whether it is there and refused.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingWhen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<SettingTests>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable: Option<SettingTests>,
}

/// One test, or all of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingTests {
    One(SettingTest),
    All(Vec<SettingTest>),
}

/// One test over another setting's value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingTest {
    /// The key of the setting whose value is tested.
    pub setting: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<Value>,
    /// A value this one must be one of.
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<bool>,
}

/// What a plugin's hook does when the plugin fails or runs late.
///
/// The default is fail open: the message goes as the person wrote it. A plugin
/// that must stop the send instead — one that encrypts what leaves — says
/// `"on_failure": "block"`, which needs `messages.intercept`, the capability
/// the review dialog already shows in its danger tone. Anything else is refused
/// with the manifest, so a plugin can't become one that stops messages by
/// accident or by an update nobody looked at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    /// The message goes unchanged, which is what a hook does by default.
    #[default]
    FailOpen,
    /// The chain may stop the send, with a reason the person is shown.
    Block,
}

impl OnFailure {
    /// Whether this is the default, which a manifest may leave out.
    pub fn is_fail_open(&self) -> bool {
        matches!(self, OnFailure::FailOpen)
    }

    /// Whether a hook's `block` outcome is honoured from this plugin.
    pub fn blocks(&self) -> bool {
        matches!(self, OnFailure::Block)
    }
}

/// A plugin, as its `yapfast-plugin.json` declares it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// The JSON Schema this file was written against, for an editor.
    ///
    /// It means nothing to Yapfast: it is here so an editor checks the file as
    /// it is typed, which is the same `deny_unknown_fields` strictness moved to
    /// where a misspelled key is cheap to find. Left out when written back.
    #[serde(default, rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
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
    /// Rows it adds to the menus of Yapfast's own interface.
    #[serde(default)]
    pub menus: Vec<MenuDecl>,
    /// Buttons it adds to the bottom of the server rail, drawn from the
    /// manifest so they appear without the plugin running.
    #[serde(default)]
    pub rail_items: Vec<ButtonDecl>,
    /// Buttons it adds to a conversation's header, the same way.
    #[serde(default)]
    pub header_buttons: Vec<ButtonDecl>,
    /// What it changes in the interface Yapfast already draws.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiDecl>,
    #[serde(default)]
    pub settings: Vec<SettingDecl>,

    /// What happens when this plugin fails or runs late in a hook.
    #[serde(default, skip_serializing_if = "OnFailure::is_fail_open")]
    pub on_failure: OnFailure,

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
    ///
    /// The dialog's lines come from what the manifest declares, so reading one
    /// drops the capabilities this build doesn't check
    /// ([`Capability::is_implemented`]): a line saying a plugin listens to the
    /// microphone, with nothing behind it, is a promise the review can't keep.
    pub fn capabilities(&self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|capability| {
                capability.is_implemented() && self.permissions.allows(*capability)
            })
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
            && self.rail_items.is_empty()
            && self.header_buttons.is_empty()
            && self.ui.as_ref().is_none_or(UiDecl::is_empty)
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
        // Stopping a send by failing is a use of the capability that stops one
        // on purpose: a manifest may not claim it with nothing to ask the
        // person for.
        if self.on_failure.blocks() && !self.permissions.allows(Capability::MessagesIntercept) {
            return Err(ManifestError::BlockWithoutCapability);
        }
        if self
            .commands
            .iter()
            .any(|command| command.name.is_empty() || command.description.as_deref() == Some(""))
        {
            return Err(ManifestError::Command);
        }
        self.drawn()?;
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

    /// Everything wrong with the sections the interface draws from the file
    /// alone: the rows it adds to menus, its rail items and header buttons, and
    /// its `ui` section.
    ///
    /// What is checked here is what would be drawn wrong — a label that won't
    /// show, an id the interface can't look a row up by, more of them than it
    /// draws — because the interface also reads these sections when a folder is
    /// loaded without an install, and a person writing a plugin should hear
    /// about it once, not watch it be silently left out. Which rows, parts,
    /// colours and sizes exist is checked where they are applied, since only the
    /// interface knows what this build has.
    fn drawn(&self) -> Result<(), ManifestError> {
        if self.menus.len() > CHANGES {
            return Err(ManifestError::TooMany { field: "menu rows" });
        }
        for row in &self.menus {
            if !plain_label(&row.label) {
                return Err(ManifestError::Label {
                    field: "a menu row's label",
                });
            }
            if !plain_id(&row.action) {
                return Err(ManifestError::Name {
                    field: "a menu row's action",
                });
            }
        }
        for (field, items) in [
            ("rail items", &self.rail_items),
            ("header buttons", &self.header_buttons),
        ] {
            if items.len() > CHANGES {
                return Err(ManifestError::TooMany { field });
            }
            for item in items {
                if !plain_label(&item.label) {
                    return Err(ManifestError::Label {
                        field: "a button's label",
                    });
                }
                if !plain_id(&item.action) {
                    return Err(ManifestError::Name {
                        field: "a button's action",
                    });
                }
            }
        }
        let Some(ui) = &self.ui else {
            return Ok(());
        };
        if ui.changes() > CHANGES {
            return Err(ManifestError::TooMany {
                field: "changes to the interface",
            });
        }
        for row in &ui.menu {
            if !plain_id(&row.row) {
                return Err(ManifestError::Name {
                    field: "a `ui` row's `row`",
                });
            }
            if let Some(label) = &row.label
                && !plain_label(label)
            {
                return Err(ManifestError::Label {
                    field: "a `ui` row's label",
                });
            }
            for anchor in row.before.iter().chain(row.after.iter()) {
                if !plain_id(anchor) {
                    return Err(ManifestError::Name {
                        field: "a `ui` row's `before` or `after`",
                    });
                }
            }
        }
        for id in ui.parts.hide.iter().chain(ui.parts.replace.iter()) {
            if !plain_id(id) {
                return Err(ManifestError::Name {
                    field: "a part's name",
                });
            }
        }
        for name in ui.colors.keys().chain(ui.sizes.keys()) {
            if !plain_id(name) {
                return Err(ManifestError::Name {
                    field: "a colour's or size's name",
                });
            }
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

/// A label as it can show: plain text on one line, 1 to [`LABEL_CHARS`]
/// characters.
///
/// The interface trims the label it draws (`plugins::ui::label` in yapfast-core);
/// this only refuses one that couldn't be drawn at all.
fn plain_label(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && text.chars().count() <= LABEL_CHARS && !text.chars().any(char::is_control)
}

/// An action's or a row's id: letters, digits, `-`, `_` and `.`.
///
/// The interface carries a row's action inside `plugin/<index>/<action>`, and
/// looks a built-in row up by its id, so neither can hold anything else.
fn plain_id(id: &str) -> bool {
    (1..=LABEL_CHARS).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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
    /// `on_failure: "block"` without the capability that may stop a message.
    BlockWithoutCapability,
    /// A command with no name or an empty description.
    Command,
    /// A label the interface can't draw: empty, over [`LABEL_CHARS`] characters,
    /// or with a line break in it.
    Label {
        field: &'static str,
    },
    /// An action or a row's id that isn't a plain name.
    Name {
        field: &'static str,
    },
    /// More rows, buttons or interface changes than one plugin may add.
    TooMany {
        field: &'static str,
    },
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
            ManifestError::DoesNothing => f.write_str(
                "this plugin has no `wasm` and adds no command, menu row, button, setting, tweak \
                 or change to the interface",
            ),
            ManifestError::MemoryPages { asked } => write!(
                f,
                "`memory_pages` is {asked}; it must be between {} and {}",
                limits::MIN_MEMORY_PAGES,
                limits::MAX_MEMORY_PAGES
            ),
            ManifestError::Quota => f.write_str("a limit asks for more than consent may allow"),
            ManifestError::PortWithoutCapability => {
                f.write_str("`local-port` needs the `network.local` capability")
            }
            ManifestError::BlockWithoutCapability => {
                f.write_str("`on_failure: \"block\"` needs the `messages.intercept` capability")
            }
            ManifestError::Command => f.write_str("a command needs a name and a description"),
            ManifestError::Label { field } => {
                write!(
                    f,
                    "{field} must be 1 to {LABEL_CHARS} characters on one line"
                )
            }
            ManifestError::Name { field } => write!(
                f,
                "{field} must be a plain name of letters, digits, `-`, `_` and `.`"
            ),
            ManifestError::TooMany { field } => {
                write!(f, "a plugin adds at most {CHANGES} {field}")
            }
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

    /// The plan's example plus the sections the interface draws, which the
    /// install path used to refuse as fields it didn't know.
    #[test]
    fn the_sections_the_interface_draws_are_part_of_the_manifest() {
        let text = WORD_FILTER.replace(
            "\"settings\": [",
            r##""rail_items": [{ "label": "Notes", "action": "notes" }],
        "header_buttons": [{ "label": "Summarise", "action": "summarise" }],
        "ui": {
            "menu": [{ "target": "user", "row": "copy-id", "label": "Copy user ID" }],
            "parts": { "hide": ["timeline.typing"] },
            "colors": { "accent": "#7aa2ff" },
            "sizes": { "message-group-gap": 8 }
        },
        "settings": ["##,
        );
        let manifest = PluginManifest::parse(&text).unwrap();
        let ui = manifest.ui.as_ref().expect("its ui section");
        assert_eq!(ui.changes(), 4, "one row, one part, one colour, one size");
        assert_eq!(ui.menu[0].target, MenuTarget::User);
        assert_eq!(manifest.rail_items[0].action, "notes");
        assert_eq!(manifest.header_buttons[0].label, "Summarise");
    }

    /// A plugin that only changes the interface runs no code and adds no row,
    /// and is still a plugin with something to do.
    #[test]
    fn a_plugin_that_only_changes_the_interface_does_something() {
        let text = r##"{
            "api": 1,
            "id": "com.example.accent",
            "name": "Accent",
            "version": "1.0.0",
            "permissions": { "capabilities": ["ui.modify"] },
            "ui": { "colors": { "accent": "#7aa2ff" } },
            "rail_items": [{ "label": "Notes", "action": "notes" }]
        }"##;
        let manifest = PluginManifest::parse(text).unwrap();
        assert_eq!(
            manifest.ui.as_ref().expect("its ui section").colors.len(),
            1
        );
        assert_eq!(manifest.rail_items.len(), 1);
    }

    /// `when` takes the conditions the interface tests, and only those: a
    /// condition nothing looks at would be one a person read and never saw
    /// applied.
    #[test]
    fn a_menu_row_takes_the_conditions_the_interface_tests() {
        let with_role = WORD_FILTER.replace(r#""own": false"#, r#""own": false, "role": "30""#);
        let manifest = PluginManifest::parse(&with_role).unwrap();
        let when = manifest.menus[0].when.as_ref().expect("a condition");
        assert_eq!(when.role.as_deref(), Some("30"));

        let with_link = WORD_FILTER.replace(r#""own": false"#, r#""has_link": true"#);
        assert!(matches!(
            PluginManifest::parse(&with_link),
            Err(ManifestError::Json(_))
        ));
    }

    /// A hook fails open unless the manifest says otherwise, and saying it needs
    /// the capability that may stop a message: a plugin can't start stopping
    /// sends through an update nobody would look twice at.
    #[test]
    fn blocking_on_failure_needs_the_intercept_capability() {
        let refusing = WORD_FILTER.replace(
            "\"permissions\": {",
            "\"on_failure\": \"block\",\n        \"permissions\": {",
        );
        assert!(matches!(
            PluginManifest::parse(&refusing),
            Err(ManifestError::BlockWithoutCapability)
        ));

        // The same manifest with the capability reads, and starts with the app
        // because intercepting is what it asked for.
        let allowed = refusing.replace("\"storage\"", "\"storage\", \"messages.intercept\"");
        let manifest = PluginManifest::parse(&allowed).unwrap();
        assert!(manifest.on_failure.blocks());
        assert!(
            manifest.activates_at_startup(),
            "a hook is ready before the first send"
        );

        // Left out, it is fail open: the message goes as the person wrote it.
        let manifest = PluginManifest::parse(WORD_FILTER).unwrap();
        assert!(manifest.on_failure.is_fail_open());
        assert_eq!(manifest.on_failure, OnFailure::FailOpen);
        // …and a fail-open manifest writes no field at all.
        let text = serde_json::to_string(&manifest).unwrap();
        assert!(!text.contains("on_failure"), "{text}");
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
