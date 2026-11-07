//! Native plugins, in-process, behind strict capability checks.
//!
//! A plugin is a bundled extension identified by a [`Manifest`]: an id, a
//! version and the capabilities it asks for. Plugins run in the same process
//! as the application, so there is no operating-system sandbox; the host's
//! capability checks are the enforcement point. A plugin without a
//! capability cannot use it, and privileged capabilities (credentials,
//! files, network, processes, the Discord API) are rejected at load time
//! while the host policy disallows them, which is the default. See
//! `docs/plugins.md` for the design and `docs/plugins-wasm.md` for the
//! WebAssembly follow-up that would sandbox untrusted plugins.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::chat::Alert;

/// The host API version plugins are built against. A manifest asking for a
/// newer one is rejected rather than run against an older host.
pub const HOST_API_VERSION: u32 = 1;

/// What a plugin may do. Every capability is denied unless the manifest
/// asks for it and the host grants it; privileged ones additionally need
/// the host policy to allow them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Contribute a colour theme to Appearance.
    UiTheme,
    /// Switch the window to a denser layout.
    UiDensity,
    /// Add a section to Settings.
    UiSettings,
    /// Decide whether a desktop notification shows.
    NotificationsFilter,
    /// Read stored sign-in credentials.
    Credentials,
    /// Read or write files outside the plugin's own settings.
    Filesystem,
    /// Make network requests.
    Network,
    /// Start processes.
    Process,
    /// Call the Discord API directly.
    DiscordApi,
}

impl Capability {
    /// Whether this capability is privileged: denied by default, even when
    /// a manifest asks for it.
    pub fn is_privileged(self) -> bool {
        matches!(
            self,
            Capability::Credentials
                | Capability::Filesystem
                | Capability::Network
                | Capability::Process
                | Capability::DiscordApi
        )
    }

    /// The capability's name, as Settings shows it.
    pub fn label(self) -> &'static str {
        match self {
            Capability::UiTheme => "Themes",
            Capability::UiDensity => "Compact layout",
            Capability::UiSettings => "Settings page",
            Capability::NotificationsFilter => "Notification filter",
            Capability::Credentials => "Credentials",
            Capability::Filesystem => "Files",
            Capability::Network => "Network",
            Capability::Process => "Processes",
            Capability::DiscordApi => "Discord API",
        }
    }
}

/// A plugin's identity and what it asks for. Serialized in `layout.json`
/// only through the manager's enabled state and config; the manifest itself
/// ships with the plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub api_version: u32,
    pub capabilities: Vec<Capability>,
    pub bundled: bool,
    pub enabled_by_default: bool,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            description: String::new(),
            api_version: HOST_API_VERSION,
            capabilities: Vec::new(),
            bundled: false,
            enabled_by_default: false,
        }
    }
}

impl Manifest {
    /// Parses a manifest from JSON, as a plugin package would carry it.
    pub fn from_json(text: &str) -> Result<Self, PluginError> {
        serde_json::from_str(text).map_err(|error| PluginError::BadManifest(error.to_string()))
    }

    /// Whether the manifest is well-formed and asks for nothing the host
    /// cannot grant. `allow_privileged` is the host policy: false unless
    /// the person explicitly opts into privileged plugins.
    pub fn validate(&self, allow_privileged: bool) -> Result<(), PluginError> {
        if !valid_id(&self.id) {
            return Err(PluginError::InvalidManifest(format!(
                "invalid plugin id {:?}: use lowercase letters, digits, hyphens and dots, 1 to 64 characters",
                self.id
            )));
        }
        if self.name.trim().is_empty() || self.name.len() > 64 {
            return Err(PluginError::InvalidManifest(
                "plugin name must be 1 to 64 characters".to_owned(),
            ));
        }
        if !valid_version(&self.version) {
            return Err(PluginError::InvalidManifest(format!(
                "invalid version {:?}: use major.minor.patch",
                self.version
            )));
        }
        if self.description.len() > 280 {
            return Err(PluginError::InvalidManifest(
                "plugin description must be at most 280 characters".to_owned(),
            ));
        }
        if self.api_version == 0 || self.api_version > HOST_API_VERSION {
            return Err(PluginError::UnsupportedApi {
                plugin: self.id.clone(),
                wanted: self.api_version,
                host: HOST_API_VERSION,
            });
        }
        let mut seen = HashSet::new();
        for capability in &self.capabilities {
            if !seen.insert(*capability) {
                return Err(PluginError::InvalidManifest(format!(
                    "duplicate capability {}",
                    capability.label()
                )));
            }
            if capability.is_privileged() && !allow_privileged {
                return Err(PluginError::PrivilegedDenied {
                    plugin: self.id.clone(),
                    capability: *capability,
                });
            }
        }
        Ok(())
    }

    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        })
        && !id.starts_with(['-', '.'])
        && !id.ends_with(['-', '.'])
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Why a plugin was rejected or an operation denied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginError {
    BadManifest(String),
    InvalidManifest(String),
    UnsupportedApi {
        plugin: String,
        wanted: u32,
        host: u32,
    },
    PrivilegedDenied {
        plugin: String,
        capability: Capability,
    },
    DuplicateId(String),
    UnknownPlugin(String),
    CapabilityDenied {
        plugin: String,
        capability: Capability,
    },
    /// The capability is granted and policy allows it, but the in-process
    /// host has no implementation: see `docs/plugins-wasm.md`. Privileged
    /// native plugins stay off until the sandbox lands.
    UnavailableInV1 {
        plugin: String,
        capability: Capability,
    },
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::BadManifest(error) => {
                write!(f, "could not parse plugin manifest: {error}")
            }
            PluginError::InvalidManifest(error) => write!(f, "invalid plugin manifest: {error}"),
            PluginError::UnsupportedApi {
                plugin,
                wanted,
                host,
            } => write!(
                f,
                "plugin {plugin} needs host API {wanted}, this host has {host}"
            ),
            PluginError::PrivilegedDenied { plugin, capability } => write!(
                f,
                "plugin {plugin} asks for privileged capability {}: denied by host policy",
                capability.label()
            ),
            PluginError::DuplicateId(id) => write!(f, "a plugin with id {id} is already installed"),
            PluginError::UnknownPlugin(id) => write!(f, "no plugin with id {id}"),
            PluginError::CapabilityDenied { plugin, capability } => write!(
                f,
                "plugin {plugin} does not have the {} capability",
                capability.label()
            ),
            PluginError::UnavailableInV1 { plugin, capability } => write!(
                f,
                "plugin {plugin} has the {} capability, but privileged native plugins are unavailable until the WASM sandbox lands",
                capability.label()
            ),
        }
    }
}

impl std::error::Error for PluginError {}

/// A plugin's settings, as plain strings so any host can store them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginConfig {
    pub values: BTreeMap<String, String>,
}

impl PluginConfig {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        if value.is_empty() {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_owned(), value.to_owned());
        }
    }
}

/// A theme a plugin contributes: a name and the colours it overrides,
/// keyed by their name in `ui/theme.slint`, as `#rgb` or `#rrggbb` text.
/// Unnamed colours stand as the base theme's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeContribution {
    pub id: String,
    pub name: String,
    pub base: &'static str,
    pub dark: bool,
    pub colours: BTreeMap<String, String>,
}

/// Whether a notification shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterDecision {
    Allow,
    Suppress,
}

/// The host a plugin acts through. Each method checks the plugin's
/// manifest first, so a plugin cannot reach an operation its manifest
/// does not declare.
pub struct Host {
    manifest: Manifest,
    allow_privileged: bool,
}

impl Host {
    pub fn new(manifest: Manifest, allow_privileged: bool) -> Self {
        Self {
            manifest,
            allow_privileged,
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Whether the plugin may use a capability at all.
    pub fn check(&self, capability: Capability) -> Result<(), PluginError> {
        if self.manifest.has(capability) {
            Ok(())
        } else {
            Err(PluginError::CapabilityDenied {
                plugin: self.manifest.id.clone(),
                capability,
            })
        }
    }

    fn privileged(&self, capability: Capability) -> Result<(), PluginError> {
        self.check(capability)?;
        if !self.allow_privileged {
            return Err(PluginError::PrivilegedDenied {
                plugin: self.manifest.id.clone(),
                capability,
            });
        }
        Err(PluginError::UnavailableInV1 {
            plugin: self.manifest.id.clone(),
            capability,
        })
    }

    /// Switch the window to a denser layout. Requires [`Capability::UiDensity`].
    pub fn set_density(&self, _compact: bool) -> Result<(), PluginError> {
        self.check(Capability::UiDensity)
    }

    /// Contribute a theme. Requires [`Capability::UiTheme`].
    pub fn contribute_theme(&self, _theme: &ThemeContribution) -> Result<(), PluginError> {
        self.check(Capability::UiTheme)
    }

    /// Read stored sign-in credentials. Requires [`Capability::Credentials`].
    pub fn read_credential(&self) -> Result<(), PluginError> {
        self.privileged(Capability::Credentials)
    }

    /// Read a file outside the plugin's settings. Requires [`Capability::Filesystem`].
    pub fn read_file(&self, _path: &str) -> Result<(), PluginError> {
        self.privileged(Capability::Filesystem)
    }

    /// Make a network request. Requires [`Capability::Network`].
    pub fn network_request(&self, _url: &str) -> Result<(), PluginError> {
        self.privileged(Capability::Network)
    }

    /// Start a process. Requires [`Capability::Process`].
    pub fn start_process(&self, _program: &str) -> Result<(), PluginError> {
        self.privileged(Capability::Process)
    }

    /// Call the Discord API. Requires [`Capability::DiscordApi`].
    pub fn discord_request(&self, _path: &str) -> Result<(), PluginError> {
        self.privileged(Capability::DiscordApi)
    }
}

/// A native plugin. Hooks have default no-ops, so a plugin implements only
/// the hooks its capabilities need.
pub trait Plugin {
    fn manifest(&self) -> Manifest;

    fn on_enable(&self, _host: &Host) -> Result<(), PluginError> {
        Ok(())
    }

    fn on_disable(&self, _host: &Host) -> Result<(), PluginError> {
        Ok(())
    }

    /// Whether the window draws denser while this plugin is enabled.
    /// Requires [`Capability::UiDensity`].
    fn compact_density(&self, host: &Host) -> Result<bool, PluginError> {
        host.check(Capability::UiDensity)?;
        Ok(false)
    }

    /// Themes this plugin contributes while enabled.
    /// Requires [`Capability::UiTheme`].
    fn theme_contributions(&self, host: &Host) -> Result<Vec<ThemeContribution>, PluginError> {
        host.check(Capability::UiTheme)?;
        Ok(Vec::new())
    }

    /// Whether a notification shows. Requires
    /// [`Capability::NotificationsFilter`].
    fn filter_notification(
        &self,
        host: &Host,
        _alert: &Alert,
        _config: &PluginConfig,
    ) -> Result<FilterDecision, PluginError> {
        host.check(Capability::NotificationsFilter)?;
        Ok(FilterDecision::Allow)
    }
}

/// The installed plugins, their enabled state and their settings. The
/// enabled state and settings are what `layout.json` keeps.
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    enabled: HashMap<String, bool>,
    config: HashMap<String, PluginConfig>,
    allow_privileged: bool,
}

impl PluginManager {
    /// The manager with the three bundled extensions installed. Bundled
    /// plugins start out as their manifests say.
    pub fn with_builtins() -> Self {
        let mut manager = Self {
            plugins: Vec::new(),
            enabled: HashMap::new(),
            config: HashMap::new(),
            allow_privileged: false,
        };
        for plugin in builtins() {
            // Bundled manifests are validated by tests; a failure here is a
            // build error, not something to run half-installed.
            if let Err(error) = manager.register(plugin) {
                tracing::warn!(%error, "bundled plugin did not install");
            }
        }
        manager
    }

    /// A manager with no plugins, for tests and future external loading.
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
            enabled: HashMap::new(),
            config: HashMap::new(),
            allow_privileged: false,
        }
    }

    pub fn set_allow_privileged(&mut self, allow: bool) {
        self.allow_privileged = allow;
    }

    /// Installs a plugin after validating its manifest against the host
    /// policy. A rejected plugin is not installed and runs nothing.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) -> Result<(), PluginError> {
        let manifest = plugin.manifest();
        manifest.validate(self.allow_privileged)?;
        if self
            .plugins
            .iter()
            .any(|installed| installed.manifest().id == manifest.id)
        {
            return Err(PluginError::DuplicateId(manifest.id));
        }
        let enabled = manifest.enabled_by_default;
        self.enabled.insert(manifest.id.clone(), enabled);
        self.config.entry(manifest.id.clone()).or_default();
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn manifests(&self) -> Vec<Manifest> {
        self.plugins
            .iter()
            .map(|plugin| plugin.manifest())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Manifest> {
        self.plugins
            .iter()
            .map(|plugin| plugin.manifest())
            .find(|manifest| manifest.id == id)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.get(id).copied().unwrap_or(false)
    }

    /// The enabled state, as `layout.json` stores it.
    pub fn enabled_state(&self) -> HashMap<String, bool> {
        self.enabled.clone()
    }

    /// The settings, as `layout.json` stores them.
    pub fn config_state(&self) -> HashMap<String, PluginConfig> {
        self.config.clone()
    }

    /// Restores the enabled state and settings after reading `layout.json`.
    /// Unknown ids are ignored, so a layout from a newer build still loads.
    pub fn apply_state(
        &mut self,
        enabled: &HashMap<String, bool>,
        config: &HashMap<String, PluginConfig>,
    ) {
        for (id, on) in enabled {
            if self.enabled.contains_key(id) {
                self.enabled.insert(id.clone(), *on);
            }
        }
        for (id, settings) in config {
            if self.config.contains_key(id) {
                self.config.insert(id.clone(), settings.clone());
            }
        }
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) -> Result<(), PluginError> {
        let Some(plugin) = self
            .plugins
            .iter()
            .find(|plugin| plugin.manifest().id == id)
        else {
            return Err(PluginError::UnknownPlugin(id.to_owned()));
        };
        let host = Host::new(plugin.manifest(), self.allow_privileged);
        if on {
            plugin.on_enable(&host)?;
        } else {
            plugin.on_disable(&host)?;
        }
        self.enabled.insert(id.to_owned(), on);
        Ok(())
    }

    pub fn config(&self, id: &str) -> PluginConfig {
        self.config.get(id).cloned().unwrap_or_default()
    }

    pub fn set_config(&mut self, id: &str, key: &str, value: &str) -> Result<(), PluginError> {
        if !self.enabled.contains_key(id) {
            return Err(PluginError::UnknownPlugin(id.to_owned()));
        }
        self.config
            .entry(id.to_owned())
            .or_default()
            .set(key, value);
        Ok(())
    }

    fn enabled_plugins(&self) -> impl Iterator<Item = &dyn Plugin> {
        self.plugins
            .iter()
            .filter(|plugin| {
                self.enabled
                    .get(&plugin.manifest().id)
                    .copied()
                    .unwrap_or(false)
            })
            .map(|plugin| plugin.as_ref())
    }

    /// Whether any enabled plugin draws the window denser.
    pub fn compact_mode(&self) -> bool {
        self.enabled_plugins().any(|plugin| {
            let host = Host::new(plugin.manifest(), self.allow_privileged);
            plugin.compact_density(&host).unwrap_or(false)
        })
    }

    /// Themes contributed by enabled plugins, in install order.
    pub fn theme_contributions(&self) -> Vec<ThemeContribution> {
        let mut themes = Vec::new();
        for plugin in self.enabled_plugins() {
            let host = Host::new(plugin.manifest(), self.allow_privileged);
            if let Ok(mut contributed) = plugin.theme_contributions(&host) {
                themes.append(&mut contributed);
            }
        }
        themes
    }

    /// The combined notification decision: any enabled filter plugin may
    /// suppress, and a suppressed notification stays suppressed.
    pub fn filter_notification(&self, alert: &Alert) -> FilterDecision {
        for plugin in self.enabled_plugins() {
            let host = Host::new(plugin.manifest(), self.allow_privileged);
            let config = self.config(plugin.manifest().id.as_str());
            if let Ok(FilterDecision::Suppress) = plugin.filter_notification(&host, alert, &config)
            {
                return FilterDecision::Suppress;
            }
        }
        FilterDecision::Allow
    }
}

fn builtins() -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(CompactModePlugin),
        Box::new(ThemeExtensionPlugin),
        Box::new(NotificationFilterPlugin),
    ]
}

/// Compact mode: denser rows and tighter spacing while enabled.
pub struct CompactModePlugin;

impl Plugin for CompactModePlugin {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "compact-mode".to_owned(),
            name: "Compact mode".to_owned(),
            version: "1.0.0".to_owned(),
            description:
                "Denser rows and tighter spacing, for fitting more of the conversation on screen."
                    .to_owned(),
            api_version: HOST_API_VERSION,
            capabilities: vec![Capability::UiDensity],
            bundled: true,
            enabled_by_default: false,
        }
    }

    fn compact_density(&self, host: &Host) -> Result<bool, PluginError> {
        host.set_density(true)?;
        Ok(true)
    }
}

/// Theme extension: one extra built-in theme while enabled.
pub struct ThemeExtensionPlugin;

impl Plugin for ThemeExtensionPlugin {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "theme-extension".to_owned(),
            name: "Theme extension".to_owned(),
            version: "1.0.0".to_owned(),
            description: "Adds the Midnight Pine theme to Appearance.".to_owned(),
            api_version: HOST_API_VERSION,
            capabilities: vec![Capability::UiTheme],
            bundled: true,
            enabled_by_default: false,
        }
    }

    fn theme_contributions(&self, host: &Host) -> Result<Vec<ThemeContribution>, PluginError> {
        let mut colours = BTreeMap::new();
        for (name, hex) in [
            ("rail", "#0d1512"),
            ("sidebar", "#101a16"),
            ("panel", "#0d1512"),
            ("chat", "#16211c"),
            ("stage", "#16211c"),
            ("raised", "#1d2b24"),
            ("field", "#213129"),
            ("sunken", "#0a100d"),
            ("hover", "#1a2620"),
            ("active", "#24352c"),
            ("row-hover", "#131d18"),
            ("line", "#24352c"),
            ("edge", "#080d0b"),
            ("text", "#e8f2ec"),
            ("secondary", "#b8cdc0"),
            ("muted", "#8ba898"),
            ("faint", "#5d7a6a"),
            ("accent", "#4fd1a5"),
            ("accent-hover", "#6fe3bc"),
            ("on-accent", "#08120d"),
            ("accent-soft", "#4fd1a526"),
            ("danger", "#f0545e"),
            ("live", "#3ecf7e"),
            ("link", "#7cc4ff"),
        ] {
            colours.insert(name.to_owned(), hex.to_owned());
        }
        let theme = ThemeContribution {
            id: "midnight-pine".to_owned(),
            name: "Midnight Pine".to_owned(),
            base: "yap",
            dark: true,
            colours,
        };
        host.contribute_theme(&theme)?;
        Ok(vec![theme])
    }
}

/// Notification filter: show only mentions, or only messages matching
/// keywords. Calls always show.
pub struct NotificationFilterPlugin;

impl Plugin for NotificationFilterPlugin {
    fn manifest(&self) -> Manifest {
        Manifest {
            id: "notification-filter".to_owned(),
            name: "Notification filter".to_owned(),
            version: "1.0.0".to_owned(),
            description:
                "Show only mentions, or only messages that match your keywords. Calls always show."
                    .to_owned(),
            api_version: HOST_API_VERSION,
            capabilities: vec![Capability::NotificationsFilter],
            bundled: true,
            enabled_by_default: false,
        }
    }

    fn filter_notification(
        &self,
        host: &Host,
        alert: &Alert,
        config: &PluginConfig,
    ) -> Result<FilterDecision, PluginError> {
        host.check(Capability::NotificationsFilter)?;
        if alert.call {
            return Ok(FilterDecision::Allow);
        }
        match config.get("mode").unwrap_or("all") {
            "mentions" => Ok(if alert.mention {
                FilterDecision::Allow
            } else {
                FilterDecision::Suppress
            }),
            "keywords" => {
                let haystack = format!("{} {}", alert.title, alert.body).to_lowercase();
                let matched = config
                    .get("keywords")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|keyword| !keyword.is_empty())
                    .any(|keyword| haystack.contains(&keyword.to_lowercase()));
                Ok(if matched {
                    FilterDecision::Allow
                } else {
                    FilterDecision::Suppress
                })
            }
            _ => Ok(FilterDecision::Allow),
        }
    }
}

/// The notification filter's modes, in the order Settings shows them.
pub const FILTER_MODES: [&str; 3] = ["all", "mentions", "keywords"];

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, capabilities: Vec<Capability>) -> Manifest {
        Manifest {
            id: id.to_owned(),
            name: "Test plugin".to_owned(),
            version: "1.0.0".to_owned(),
            description: "A test plugin.".to_owned(),
            api_version: HOST_API_VERSION,
            capabilities,
            bundled: false,
            enabled_by_default: false,
        }
    }

    struct Stub(Manifest);

    impl Plugin for Stub {
        fn manifest(&self) -> Manifest {
            self.0.clone()
        }
    }

    fn alert(mention: bool, call: bool, body: &str) -> Alert {
        Alert {
            room: "room".to_owned(),
            title: "Sam".to_owned(),
            body: body.to_owned(),
            picture: None,
            call,
            mention,
        }
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let text = r#"{"id":"compact-mode","name":"Compact mode","version":"1.0.0","description":"Denser.","api_version":1,"capabilities":["ui_density"],"bundled":true,"enabled_by_default":false}"#;
        let manifest = Manifest::from_json(text).unwrap();
        assert_eq!(manifest.id, "compact-mode");
        assert!(manifest.has(Capability::UiDensity));
        manifest.validate(false).unwrap();
    }

    #[test]
    fn manifest_rejects_unknown_capability() {
        let text = r#"{"id":"x","name":"X","version":"1.0.0","description":"","api_version":1,"capabilities":["teleport"]}"#;
        assert!(Manifest::from_json(text).is_err());
    }

    #[test]
    fn manifest_rejects_bad_id_version_and_api() {
        assert!(manifest("Not Valid", vec![]).validate(false).is_err());
        assert!(manifest("-leading", vec![]).validate(false).is_err());
        let mut bad = manifest("ok-id", vec![]);
        bad.version = "1.0".to_owned();
        assert!(bad.validate(false).is_err());
        bad.version = "1.0.0".to_owned();
        bad.api_version = HOST_API_VERSION + 1;
        assert!(matches!(
            bad.validate(false),
            Err(PluginError::UnsupportedApi { .. })
        ));
    }

    #[test]
    fn privileged_capabilities_are_denied_by_default() {
        for capability in [
            Capability::Credentials,
            Capability::Filesystem,
            Capability::Network,
            Capability::Process,
            Capability::DiscordApi,
        ] {
            assert!(capability.is_privileged());
            let attempt = manifest("needs-privilege", vec![capability]).validate(false);
            assert!(matches!(
                attempt,
                Err(PluginError::PrivilegedDenied { capability: claimed, .. }) if claimed == capability
            ));
            manifest("needs-privilege", vec![capability])
                .validate(true)
                .unwrap();
        }
        assert!(!Capability::UiDensity.is_privileged());
    }

    #[test]
    fn host_denies_operations_without_the_capability() {
        let host = Host::new(manifest("plain", vec![]), false);
        assert!(matches!(
            host.set_density(true),
            Err(PluginError::CapabilityDenied { .. })
        ));
        assert!(matches!(
            host.read_credential(),
            Err(PluginError::CapabilityDenied { .. })
        ));
        let host = Host::new(manifest("dense", vec![Capability::UiDensity]), false);
        host.set_density(true).unwrap();
        assert!(matches!(
            host.read_file("/etc/passwd"),
            Err(PluginError::CapabilityDenied { .. })
        ));
    }

    #[test]
    fn privileged_operations_stay_unavailable_in_v1() {
        // Granted and allowed by policy, still not implemented: the native
        // host must not pretend to sandbox what it cannot.
        let host = Host::new(
            manifest(
                "privileged",
                vec![Capability::Network, Capability::Filesystem],
            ),
            true,
        );
        assert!(matches!(
            host.network_request("https://example.com"),
            Err(PluginError::UnavailableInV1 { .. })
        ));
        assert!(matches!(
            host.read_file("layout.json"),
            Err(PluginError::UnavailableInV1 { .. })
        ));
        // Without the policy opt-in, a granted capability is denied.
        let host = Host::new(manifest("privileged", vec![Capability::Network]), false);
        assert!(matches!(
            host.network_request("https://example.com"),
            Err(PluginError::PrivilegedDenied { .. })
        ));
    }

    #[test]
    fn manager_rejects_duplicates_and_unknown_plugins() {
        let mut manager = PluginManager::empty();
        manager
            .register(Box::new(Stub(manifest("dup", vec![]))))
            .unwrap();
        assert!(matches!(
            manager.register(Box::new(Stub(manifest("dup", vec![])))),
            Err(PluginError::DuplicateId(_))
        ));
        assert!(matches!(
            manager.set_enabled("missing", true),
            Err(PluginError::UnknownPlugin(_))
        ));
        assert!(matches!(
            manager.set_config("missing", "k", "v"),
            Err(PluginError::UnknownPlugin(_))
        ));
    }

    #[test]
    fn manager_rejects_privileged_plugins_under_default_policy() {
        let mut manager = PluginManager::empty();
        let result = manager.register(Box::new(Stub(manifest(
            "sneaky",
            vec![Capability::Credentials],
        ))));
        assert!(matches!(result, Err(PluginError::PrivilegedDenied { .. })));
        assert!(manager.manifests().is_empty());
    }

    #[test]
    fn builtins_install_and_start_disabled() {
        let manager = PluginManager::with_builtins();
        let manifests = manager.manifests();
        let ids: Vec<&str> = manifests
            .iter()
            .map(|manifest| manifest.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["compact-mode", "theme-extension", "notification-filter"]
        );
        for manifest in manager.manifests() {
            assert!(manifest.bundled);
            assert!(!manager.is_enabled(&manifest.id));
            assert!(
                manifest
                    .capabilities
                    .iter()
                    .all(|capability| !capability.is_privileged())
            );
        }
    }

    #[test]
    fn compact_mode_follows_the_plugin() {
        let mut manager = PluginManager::with_builtins();
        assert!(!manager.compact_mode());
        manager.set_enabled("compact-mode", true).unwrap();
        assert!(manager.compact_mode());
        manager.set_enabled("compact-mode", false).unwrap();
        assert!(!manager.compact_mode());
    }

    #[test]
    fn theme_extension_contributes_only_when_enabled() {
        let mut manager = PluginManager::with_builtins();
        assert!(manager.theme_contributions().is_empty());
        manager.set_enabled("theme-extension", true).unwrap();
        let themes = manager.theme_contributions();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].id, "midnight-pine");
        assert_eq!(
            themes[0].colours.get("accent").map(String::as_str),
            Some("#4fd1a5")
        );
        for hex in themes[0].colours.values() {
            assert!(hex.starts_with('#'), "colour {hex} is not hex");
        }
    }

    #[test]
    fn notification_filter_modes() {
        let mut manager = PluginManager::with_builtins();
        let plain = alert(false, false, "hello there");
        assert_eq!(manager.filter_notification(&plain), FilterDecision::Allow);
        manager.set_enabled("notification-filter", true).unwrap();
        // Default mode is "all".
        assert_eq!(manager.filter_notification(&plain), FilterDecision::Allow);
        manager
            .set_config("notification-filter", "mode", "mentions")
            .unwrap();
        assert_eq!(
            manager.filter_notification(&plain),
            FilterDecision::Suppress
        );
        assert_eq!(
            manager.filter_notification(&alert(true, false, "hi")),
            FilterDecision::Allow
        );
        // Calls always show, even in mentions mode.
        assert_eq!(
            manager.filter_notification(&alert(false, true, "ringing")),
            FilterDecision::Allow
        );
        manager
            .set_config("notification-filter", "mode", "keywords")
            .unwrap();
        manager
            .set_config("notification-filter", "keywords", "co-op, Mars")
            .unwrap();
        assert_eq!(
            manager.filter_notification(&alert(false, false, "Co-op after nine?")),
            FilterDecision::Allow
        );
        assert_eq!(
            manager.filter_notification(&alert(false, false, "nothing relevant")),
            FilterDecision::Suppress
        );
    }

    #[test]
    fn enabled_state_and_config_round_trip() {
        let mut manager = PluginManager::with_builtins();
        manager.set_enabled("compact-mode", true).unwrap();
        manager
            .set_config("notification-filter", "mode", "mentions")
            .unwrap();
        let enabled = manager.enabled_state();
        let config = manager.config_state();

        let mut restored = PluginManager::with_builtins();
        restored.apply_state(&enabled, &config);
        assert!(restored.is_enabled("compact-mode"));
        assert!(restored.compact_mode());
        assert_eq!(
            restored.config("notification-filter").get("mode"),
            Some("mentions")
        );
        // Unknown ids from a newer layout are ignored, not fatal.
        let mut stray = HashMap::new();
        stray.insert("from-the-future".to_owned(), true);
        restored.apply_state(&stray, &HashMap::new());
    }
}
