//! What Settings → Plugins shows, and what it asks for: the seam between the
//! page and the plugin runtime.
//!
//! The runtime keeps a [`View`] current and answers each [`Request`], through
//! [`Source`]. The page reads the view only while it's open, and builds its
//! rows from it then, so a closed page holds nothing. Until the runtime
//! exists, [`Fixture`] stands in for it in `--preview` and in tests, and
//! [`Unavailable`] everywhere else.

use std::{path::PathBuf, sync::Arc};

use super::settings::{Edit, Form};

/// The default memory cap of one plugin instance, in 64 KiB WebAssembly
/// pages: 16 MiB.
pub const DEFAULT_PAGES: u32 = 256;

/// Everything the page shows.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub helper: Helper,
    /// The installed plugins, in the order they're listed.
    pub installed: Vec<Installed>,
    pub catalog: Catalog,
    /// Installing from a git URL, or from a marketplace entry.
    pub find: Find,
    pub advanced: Advanced,
}

impl View {
    pub fn plugin(&self, id: &str) -> Option<&Installed> {
        self.installed.iter().find(|plugin| plugin.id == id)
    }
}

/// The `yapfast-plugins` helper process, which runs while any plugin
/// instance lives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Helper {
    /// No instance lives, so no process does.
    #[default]
    Stopped,
    /// Its memory in bytes, sampled only while the page is open, where this
    /// system reports it.
    Running { memory: Option<u64> },
    /// The helper binary isn't beside Yapfast's. Everything else works.
    Missing,
    /// It crashed three times in a minute, so plugins stay off until
    /// Yapfast starts again.
    Crashed,
}

/// An installed plugin.
#[derive(Clone, Debug, Default)]
pub struct Installed {
    /// Reverse-DNS, stable forever, as `com.example.word-filter`.
    pub id: String,
    pub name: String,
    pub description: String,
    pub authors: Vec<String>,
    pub version: String,
    /// The icon's own bytes, when its manifest named one and the install kept
    /// it. The page draws its initial instead when this is `None`, so a plugin
    /// without one still has a face in a list that expects one.
    pub icon: Option<Vec<u8>>,
    pub enabled: bool,
    pub status: Status,
    /// A newer version on its ref; see [`Update`].
    pub update: Option<Update>,
    /// The version kept in `store/` to roll back to.
    pub previous: Option<String>,
    pub origin: Origin,
    pub source: String,
    /// The folder in that repository, empty when its manifest is at the root.
    pub path: String,
    /// The full commit it's pinned to.
    pub commit: String,
    pub license: String,
    /// Capabilities its manifest declares and the user accepted, as
    /// `messages.read`.
    pub capabilities: Vec<String>,
    /// Hosts it may connect to: those its manifest declares, and those the
    /// user answered in the popup.
    pub grants: Vec<Grant>,
    /// Its memory cap, in 64 KiB pages.
    pub memory_pages: u32,
    /// How many of its requests for undeclared hosts were denied without
    /// asking, after three denials this session.
    pub quiet_denials: u32,
    /// Its own settings, from its manifest's `settings`: the form its page
    /// draws, with the values chosen.
    pub settings: Form,
    /// Its recent log lines, oldest first.
    pub log: Vec<LogLine>,
}

/// What a plugin is doing; see the states in `docs/plugins/interface.md`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Installed, not enabled.
    #[default]
    Off,
    /// Enabled, with no instance: it starts when something it waits for
    /// happens.
    Idle,
    Running,
    /// Being replaced by a new version, or reloaded from its folder.
    Updating,
    /// It failed once, and stays on: the runtime tries it again on its next
    /// trigger. A load that timed out on a busy machine lands here, which is not
    /// the same as being suspended and is what a person retries by using it.
    Faulted(String),
    /// Stopped for this session, after three failures in a minute.
    Suspended(Suspension),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Suspension {
    /// It went past its memory cap.
    OutOfMemory,
    /// It trapped.
    Crashed,
    /// It ran past its deadline.
    TooSlow,
    /// A plugin it depends on, by name, is off or missing.
    Needs(String),
    /// It failed for a reason only the helper knows, in its own words: an
    /// activation that returned a status, a module that didn't instantiate, an
    /// answer the ABI refused. The sentence comes from the runtime, and the
    /// plugin's log has it too.
    Failed(String),
}

impl Status {
    /// The status as the page words it.
    pub fn text(&self) -> String {
        match self {
            Status::Off => "Off".into(),
            Status::Idle => "Idle".into(),
            Status::Running => "Running".into(),
            Status::Updating => "Updating".into(),
            Status::Faulted(why) => format!("Faulted: {why}"),
            Status::Suspended(Suspension::OutOfMemory) => "Suspended: ran out of memory".into(),
            Status::Suspended(Suspension::Crashed) => "Suspended: kept crashing".into(),
            Status::Suspended(Suspension::TooSlow) => "Suspended: too slow to answer".into(),
            Status::Suspended(Suspension::Needs(name)) => format!("Suspended: needs {name}"),
            Status::Suspended(Suspension::Failed(why)) => format!("Suspended: {why}"),
        }
    }

    pub fn tone(&self) -> Tone {
        match self {
            Status::Running => Tone::Success,
            Status::Faulted(_) | Status::Suspended(_) => Tone::Danger,
            Status::Off | Status::Idle | Status::Updating => Tone::Muted,
        }
    }
}

/// A newer version of an installed plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    /// Capabilities it adds, which need the user's consent again: updating
    /// then shows the review, and is otherwise one click.
    pub adds: Vec<String>,
}

/// Where a plugin was installed from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    Marketplace,
    #[default]
    Url,
    /// Loaded from a folder, as Advanced → Load folder does: it reloads when
    /// its files change.
    Folder,
}

/// A host a plugin may, or may not, connect to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The host, or a glob as `*.cdn.example.com`.
    pub host: String,
    /// Granted by accepting the install, as its manifest declares it.
    pub declared: bool,
    /// Allowed, or denied, in the popup.
    pub allowed: bool,
    /// Stored in `grants.json`, rather than kept for this session.
    pub remembered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// When, as `14:02:11`.
    pub at: String,
    pub text: String,
    pub error: bool,
}

/// The marketplace's catalog. It's fetched only while the page is open, and
/// dropped when it closes.
#[derive(Clone, Debug, Default)]
pub enum Catalog {
    /// Not asked for since the page opened.
    #[default]
    Unloaded,
    Loading,
    Failed(String),
    /// The listings, held in one `Arc` so that the copy the page draws from
    /// costs a pointer rather than every row.
    Ready(Arc<[Listing]>),
}

/// A plugin in the marketplace.
#[derive(Clone, Debug, Default)]
pub struct Listing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub category: String,
    pub tags: Vec<String>,
    /// The address of its icon in the registry repository.
    pub icon: Option<String>,
    /// A maintainer read its source and permissions before listing it.
    pub reviewed: bool,
    pub capabilities: Vec<String>,
    pub network: Vec<String>,
}

impl Listing {
    /// What it reaches, in short, as `messages, 1 host`.
    pub fn reach(&self) -> String {
        reach(&self.capabilities, &self.network)
    }

    /// Whether `query` is in its name, description, author or tags,
    /// ignoring case.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || [&self.name, &self.description, &self.author]
                .into_iter()
                .chain(&self.tags)
                .any(|text| text.to_lowercase().contains(&query))
    }
}

/// Installing a plugin: fetching it, then the review, which asks for
/// consent before anything is stored or run.
#[derive(Clone, Debug, Default)]
pub enum Find {
    #[default]
    Idle,
    Fetching {
        url: String,
        step: Step,
    },
    Failed {
        url: String,
        error: FetchError,
    },
    Review(Box<Review>),
    Installing {
        name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Fetching,
    Checking,
    Compiling,
}

impl Step {
    pub fn text(self) -> &'static str {
        match self {
            Step::Fetching => "Fetching the repository...",
            Step::Checking => "Checking the plugin...",
            Step::Compiling => "Compiling the plugin...",
        }
    }
}

/// Why a URL gave no plugin, each said plainly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// `http`, `ext::` or another scheme than `https`.
    NotHttps,
    Ssh,
    NoManifest,
    /// The module, at this path, is a Git LFS pointer.
    LfsPointer(String),
    /// The manifest isn't one this Yapfast can use, and why.
    BadManifest(String),
    /// Over the 64 MiB fetch cap.
    TooLarge,
    /// Over the 120 s fetch cap.
    TimedOut,
    /// Git's answer, when the host or the repository can't be reached.
    Unreachable(String),
    /// What the marketplace listed and what its repository holds disagree, in
    /// its own words: the commit or the hash of the module changed under the
    /// pin a maintainer reviewed, so nothing was installed.
    Mismatch(String),
}

impl FetchError {
    pub fn text(&self) -> String {
        match self {
            FetchError::NotHttps => "Only https:// addresses can be installed.".into(),
            FetchError::Ssh => {
                "SSH addresses can't be installed. Use the repository's https:// address.".into()
            }
            FetchError::NoManifest => "That repository has no yapfast-plugin.json.".into(),
            FetchError::LfsPointer(path) => {
                format!("{path} is a Git LFS pointer; check in the file itself.")
            }
            FetchError::BadManifest(why) => {
                format!("Its yapfast-plugin.json can't be used: {why}.")
            }
            FetchError::TooLarge => "That repository is over 64 MiB.".into(),
            FetchError::TimedOut => "Fetching that repository took over 2 minutes.".into(),
            FetchError::Unreachable(why) => format!("Couldn't fetch that repository: {why}."),
            FetchError::Mismatch(why) => why.clone(),
        }
    }
}

/// What a plugin would be, shown before it's installed. Nothing of it has
/// run.
#[derive(Clone, Debug, Default)]
pub struct Review {
    pub id: String,
    pub name: String,
    pub version: String,
    pub icon: Option<String>,
    pub source: String,
    /// The folder in that repository, empty when the manifest is at its root:
    /// a repository that holds several plugins names one here.
    pub path: String,
    pub commit: String,
    /// It comes from the marketplace, which reviewed it. A URL install is
    /// an *Unreviewed source*.
    pub reviewed: bool,
    pub capabilities: Vec<String>,
    pub network: Vec<String>,
    pub memory_pages: u32,
    pub dependencies: Vec<Dependency>,
    /// The installed version this replaces, when it's an update.
    pub replaces: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    /// The versions it accepts, as `^1.2`.
    pub version: String,
    pub installed: bool,
}

/// Settings in Advanced, at the page's foot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advanced {
    /// The sum of live instances' caps above which the least recently used
    /// idle one is evicted, in MiB.
    pub memory_budget: u32,
    /// Minutes without calls before an instance is evicted.
    pub idle_minutes: u32,
    /// The helper can't apply the OS sandbox here, and runs under
    /// wasmtime's alone.
    pub sandbox_unavailable: bool,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            memory_budget: 256,
            idle_minutes: 5,
            sandbox_unavailable: false,
        }
    }
}

/// The budgets Advanced offers, in MiB.
pub const MEMORY_BUDGETS: [u32; 4] = [128, 256, 512, 1024];
/// The idle times Advanced offers, in minutes.
pub const IDLE_MINUTES: [u32; 4] = [1, 5, 15, 60];

/// What the page asks of the runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// The page opened: start sampling the helper's memory, and check for
    /// updates if a day has passed.
    Open,
    /// The page closed: stop sampling, and drop the catalog.
    Close,
    /// Fetch the marketplace's catalog, if its `HEAD` moved.
    LoadCatalog,
    /// Takes effect at once.
    SetEnabled {
        plugin: String,
        enabled: bool,
    },
    /// Fetch a marketplace entry, and show its review.
    Install {
        plugin: String,
    },
    /// Fetch a git URL, and show its review.
    Find {
        url: String,
    },
    /// Install what the review shows.
    Confirm,
    /// Close the review, or stop a fetch; nothing is kept.
    Cancel,
    Uninstall {
        plugin: String,
        /// Also remove `data/<id>/`, its storage and settings.
        delete_data: bool,
    },
    /// Takes effect at once.
    Revoke {
        plugin: String,
        permission: Permission,
    },
    /// Update to [`Installed::update`]. One that adds capabilities shows its
    /// review first.
    Update {
        plugin: String,
    },
    /// Go back to [`Installed::previous`].
    RollBack {
        plugin: String,
    },
    /// Changes one of its settings, which takes effect at once: the
    /// runtime applies it with [`Form::edit`], stores what changed, and
    /// tells the plugin through `settings.on_change`.
    Setting {
        plugin: String,
        edit: Edit,
    },
    /// The person used a widget a widget setting draws: for the plugin,
    /// as `Input::Widget` is for a slot's.
    SettingWidget {
        plugin: String,
        key: String,
        id: String,
        value: String,
    },
    /// Install a `.wasm` file the person picked, with the manifest beside it
    /// when there is one and a manifest made from its name when there isn't. It
    /// shows up installed and off: enabling it is the person's next choice.
    InstallWasm(PathBuf),
    /// Load a plugin from a folder, and reload it when its files change. The
    /// development loop, for writing one; the page offers `InstallWasm` instead.
    LoadFolder(PathBuf),
    SetMemoryBudget(u32),
    SetIdleMinutes(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    Capability(String),
    Host(String),
}

/// The plugin runtime, as the page reaches it.
///
/// The runtime implements it. After answering a request it changes the view
/// at once, or later, from its own events; then it calls
/// `plugin_settings::changed` in the Slint adapter, which shows the view
/// again if the page is open.
pub trait Source {
    fn view(&self) -> &View;
    fn request(&mut self, request: Request);
    /// Hands over the view the runtime built, which is how it says what moved
    /// since the last one. The fixture and the page that says plugins can't run
    /// build their own, and ignore this.
    fn set_view(&mut self, _view: View) {}
}

/// The source of a build whose plugin runtime isn't there: the page says so,
/// and asks for nothing.
#[derive(Debug)]
pub struct Unavailable(View);

impl Default for Unavailable {
    fn default() -> Self {
        Self(View {
            helper: Helper::Missing,
            ..View::default()
        })
    }
}

impl Source for Unavailable {
    fn view(&self) -> &View {
        &self.0
    }

    fn request(&mut self, _: Request) {}
}

/// How a line of what a plugin can do is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    /// Bold: it reaches something people care about.
    Strong,
    Muted,
    Success,
    Warning,
    Danger,
}

/// A line of what a plugin can do, in plain words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub tone: Tone,
    /// What revoking it takes away; `None` for a line that only explains.
    pub permission: Option<Permission>,
}

/// The sentence `messages.send.background` must show, in the danger tone,
/// at install and whenever an update adds it.
pub const BACKGROUND_SEND: &str = "Sends messages automatically, without you doing anything. Automated sending can get your Discord account banned.";

/// A capability in plain words, as `docs/plugins/permissions.md` words it, and whether it's
/// shown in bold or in the danger tone.
pub fn capability(name: &str) -> (String, Tone) {
    let (text, tone) = match name {
        "storage" => ("Keeps its own data", Tone::Normal),
        "messages.read" => ("Reads your messages", Tone::Strong),
        "messages.intercept" => ("Can change what you send", Tone::Strong),
        "messages.send" => ("Can send messages as you", Tone::Strong),
        "messages.send.background" => (BACKGROUND_SEND, Tone::Danger),
        "messages.retain" => ("Keeps messages after they're deleted", Tone::Strong),
        "messages.filter" => ("Can hide messages from you", Tone::Normal),
        "people.read" => ("Reads people's profiles", Tone::Normal),
        "servers.read" => ("Reads server details", Tone::Normal),
        "servers.manage" => ("Can change servers you manage", Tone::Strong),
        "presence.write" => ("Changes your status", Tone::Normal),
        "notifications.manage" => ("Can mute and mark things read", Tone::Normal),
        "typing.intercept" => ("Can hide that you're typing", Tone::Normal),
        "uploads.intercept" => ("Can change files you upload", Tone::Strong),
        "media.read" => ("Reads attachments and pictures", Tone::Normal),
        "files.save" => ("Saves files where you choose", Tone::Normal),
        "voice.read" => ("Sees your calls", Tone::Normal),
        "audio.play" | "audio.speak" => ("Plays sounds and speech", Tone::Normal),
        "microphone" => ("Records your microphone", Tone::Strong),
        "notifications" => ("Shows notifications", Tone::Normal),
        "clipboard.write" => ("Writes to your clipboard", Tone::Normal),
        "ui.modify" => ("Can change Yapfast's interface", Tone::Strong),
        "open.app" => ("Opens other apps", Tone::Normal),
        _ => {
            if let Some(port) = name.strip_prefix("network.local:") {
                return (
                    format!("Connects to an app on this computer, on port {port}"),
                    Tone::Normal,
                );
            }
            return (format!("Uses {name}"), Tone::Normal);
        }
    };
    (text.into(), tone)
}

/// What a plugin would be able to do, a line each, as the review shows it:
/// its capabilities, the hosts it connects to, and the sentence for a risky
/// combination.
pub fn permission_lines(capabilities: &[String], hosts: &[String]) -> Vec<Line> {
    let mut lines = capability_lines(capabilities);
    lines.extend(hosts.iter().map(|host| Line {
        text: format!("Connects to {host}"),
        tone: Tone::Normal,
        permission: Some(Permission::Host(host.clone())),
    }));
    lines.extend(risk(capabilities, hosts));
    lines
}

impl Installed {
    /// What Copy puts on the clipboard: who the plugin is, what it is doing and
    /// why, where it came from, what it may reach, and every line it logged —
    /// more than the page's "recent log", which is only this session's ring and
    /// only what was on screen. It is written to be pasted into a bug report.
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("{} {}", self.name, self.version));
        if !self.name.is_empty() && !self.version.is_empty() {
            out.push_str(" — ");
        }
        out.push_str(&format!("{}\n", self.id));
        out.push_str(&format!("Status: {}\n", self.status.text()));
        out.push_str(&format!(
            "Enabled: {}\n",
            if self.enabled { "yes" } else { "no" }
        ));
        out.push_str(&format!(
            "Origin: {} {}{}{}\n",
            match self.origin {
                Origin::Marketplace => "marketplace",
                Origin::Url => "git URL",
                Origin::Folder => "folder",
            },
            self.source,
            if self.path.is_empty() {
                String::new()
            } else {
                format!("#path={}", self.path)
            },
            self.commit
                .is_empty()
                .then(String::new)
                .unwrap_or_else(|| format!(" @ {}", short_commit(&self.commit))),
        ));
        if !self.license.is_empty() {
            out.push_str(&format!("License: {}\n", self.license));
        }
        out.push_str(&format!("Memory cap: {}\n", memory(self.memory_pages).0));
        if self.capabilities.is_empty() {
            out.push_str("Capabilities: none\n");
        } else {
            out.push_str(&format!("Capabilities: {}\n", self.capabilities.join(", ")));
        }
        for grant in &self.grants {
            out.push_str(&format!(
                "Host: {} ({}{}{})\n",
                grant.host,
                if grant.declared {
                    "declared"
                } else {
                    "not declared"
                },
                if grant.allowed {
                    ", allowed"
                } else {
                    ", denied"
                },
                if grant.remembered {
                    ", remembered"
                } else {
                    ", this session"
                },
            ));
        }
        if self.quiet_denials > 0 {
            out.push_str(&format!("Denied without asking: {}\n", self.quiet_denials));
        }
        out.push_str("Log:\n");
        if self.log.is_empty() {
            out.push_str("  (nothing this session)\n");
        }
        for line in &self.log {
            out.push_str(&format!("  {} {}\n", line.at, line.text));
        }
        out
    }

    /// What it can do, a line each, as its page shows it: its capabilities,
    /// the hosts it declared and those answered in the popup, and the
    /// sentence for a risky combination.
    pub fn permission_lines(&self) -> Vec<Line> {
        let mut lines = capability_lines(&self.capabilities);
        lines.extend(grant_lines(&self.grants));
        let allowed: Vec<String> = self
            .grants
            .iter()
            .filter(|grant| grant.allowed)
            .map(|grant| grant.host.clone())
            .collect();
        lines.extend(risk(&self.capabilities, &allowed));
        lines
    }
}

/// A line per capability; `audio.play` and `audio.speak` share one.
fn capability_lines(capabilities: &[String]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for name in capabilities {
        let (text, tone) = capability(name);
        if lines.iter().any(|line| line.text == text) {
            continue;
        }
        lines.push(Line {
            text,
            tone,
            permission: Some(Permission::Capability(name.clone())),
        });
    }
    lines
}

/// Reading messages with any host to send them to is the real threat, and
/// two unrelated lines would hide it, so it gets a sentence of its own.
fn risk(capabilities: &[String], hosts: &[String]) -> Option<Line> {
    if hosts.is_empty() || !capabilities.iter().any(|name| name == "messages.read") {
        return None;
    }
    Some(Line {
        text: format!(
            "It reads your messages and connects to {}, so it could send your messages to {}.",
            list(hosts),
            if hosts.len() == 1 {
                "that server"
            } else {
                "those servers"
            }
        ),
        tone: Tone::Warning,
        permission: None,
    })
}

/// What a plugin may connect to, and what the user answered, a line each.
fn grant_lines(grants: &[Grant]) -> Vec<Line> {
    grants
        .iter()
        .map(|grant| {
            let text = match (grant.declared, grant.allowed, grant.remembered) {
                (true, _, _) => format!("Connects to {}", grant.host),
                (false, true, true) => format!("Connects to {}, as you allowed", grant.host),
                (false, true, false) => {
                    format!("Connects to {}, as you allowed this session", grant.host)
                }
                (false, false, true) => format!("Never asks again for {}", grant.host),
                (false, false, false) => format!("Denied {} this session", grant.host),
            };
            Line {
                text,
                tone: if grant.allowed {
                    Tone::Normal
                } else {
                    Tone::Muted
                },
                permission: Some(Permission::Host(grant.host.clone())),
            }
        })
        .collect()
}

/// What a plugin reaches, in short, for a marketplace row: the areas of its
/// capabilities and how many hosts, as `messages, 1 host`.
pub fn reach(capabilities: &[String], hosts: &[String]) -> String {
    let mut areas: Vec<&str> = Vec::new();
    for name in capabilities {
        let area = name.split(['.', ':']).next().unwrap_or(name);
        if !areas.contains(&area) {
            areas.push(area);
        }
    }
    let hosts = match hosts.len() {
        0 => None,
        1 => Some("1 host".to_owned()),
        count => Some(format!("{count} hosts")),
    };
    let mut parts: Vec<String> = areas.into_iter().map(str::to_owned).collect();
    parts.extend(hosts);
    if parts.is_empty() {
        "no permissions".into()
    } else {
        parts.join(", ")
    }
}

/// A memory cap in words, and whether it asks for more than the default.
pub fn memory(pages: u32) -> (String, Tone) {
    let mib = pages.div_ceil(16);
    if pages > DEFAULT_PAGES {
        (
            format!(
                "Uses up to {mib} MiB of memory, more than the usual {} MiB",
                DEFAULT_PAGES / 16
            ),
            Tone::Warning,
        )
    } else {
        (format!("Uses up to {mib} MiB of memory"), Tone::Normal)
    }
}

/// An amount of memory in bytes, as `5.4 MiB`.
pub fn bytes(amount: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    format!("{:.1} MiB", amount as f64 / MIB)
}

/// `a`, `a and b`, `a, b and c`.
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The first eight characters of a commit, as the page shows it.
pub fn short_commit(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

/// The page's sample plugins, for `--preview` and tests. It answers
/// requests at once, as a runtime would once its work is done, so each part
/// of the page can be tried. URLs ending in `/no-manifest`, `/lfs`, `/huge`
/// or `/slow`, and any that isn't `https://`, fail as a fetch would.
#[derive(Debug)]
pub struct Fixture {
    view: View,
    /// The marketplace's listings, held while the catalog is unloaded.
    listings: Vec<Listing>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub fn new() -> Self {
        let rev = |seed: &str| format!("{seed:0<40}");
        let strings = |items: &[&str]| items.iter().map(|&item| item.to_owned()).collect();
        let installed = vec![
            Installed {
                id: "com.example.word-filter".into(),
                name: "Word Filter".into(),
                description: "Hides words you'd rather not read.".into(),
                authors: strings(&["Ada"]),
                version: "1.4.0".into(),
                enabled: true,
                status: Status::Running,
                update: Some(Update {
                    version: "1.5.0".into(),
                    adds: Vec::new(),
                }),
                previous: Some("1.3.2".into()),
                origin: Origin::Marketplace,
                source: "https://codeberg.org/ada/word-filter".into(),
                path: String::new(),
                commit: "051b765bb6abe4fe95d5e4f489f03ad7b137edbc".into(),
                license: "MIT".into(),
                capabilities: strings(&["messages.read", "messages.intercept", "storage"]),
                grants: vec![
                    Grant {
                        host: "api.example.com".into(),
                        declared: true,
                        allowed: true,
                        remembered: true,
                    },
                    Grant {
                        host: "cdn.example.net".into(),
                        declared: false,
                        allowed: true,
                        remembered: true,
                    },
                ],
                memory_pages: 128,
                quiet_denials: 0,
                settings: Fixture::word_filter_settings(),
                log: vec![
                    LogLine {
                        at: "14:02:11".into(),
                        text: "Loaded 42 words".into(),
                        error: false,
                    },
                    LogLine {
                        at: "14:07:40".into(),
                        text: "Hid 3 words in #general".into(),
                        error: false,
                    },
                ],
                ..Installed::default()
            },
            Installed {
                id: "org.example.translate".into(),
                name: "Translate".into(),
                description: "Translates messages into your language.".into(),
                authors: strings(&["Bea", "Cy"]),
                version: "0.9.1".into(),
                enabled: true,
                status: Status::Suspended(Suspension::OutOfMemory),
                origin: Origin::Url,
                source: "https://github.com/bea/translate".into(),
                path: "plugins/translate".into(),
                commit: rev("9f2c1d"),
                license: "GPL-3.0-or-later".into(),
                capabilities: strings(&["messages.read"]),
                grants: vec![Grant {
                    host: "translate.example.org".into(),
                    declared: true,
                    allowed: true,
                    remembered: true,
                }],
                memory_pages: DEFAULT_PAGES,
                quiet_denials: 4,
                log: vec![LogLine {
                    at: "13:58:02".into(),
                    text: "Translate ran out of its 16 MiB and was restarted.".into(),
                    error: true,
                }],
                ..Installed::default()
            },
            Installed {
                id: "net.example.quiet-hours".into(),
                name: "Quiet Hours".into(),
                description: "Mutes servers at night.".into(),
                authors: strings(&["Dee"]),
                version: "2.0.0".into(),
                enabled: false,
                status: Status::Off,
                origin: Origin::Marketplace,
                source: "https://gitlab.com/dee/quiet-hours".into(),
                commit: rev("4be07a"),
                license: "Apache-2.0".into(),
                capabilities: strings(&["notifications.manage", "storage"]),
                memory_pages: 64,
                ..Installed::default()
            },
        ];
        let listing = |id: &str,
                       name: &str,
                       description: &str,
                       author: &str,
                       category: &str,
                       capabilities: &[&str],
                       network: &[&str]| Listing {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            author: author.into(),
            version: "1.0.0".into(),
            category: category.into(),
            tags: Vec::new(),
            icon: None,
            reviewed: true,
            capabilities: strings(capabilities),
            network: strings(network),
        };
        let mut listings = vec![
            Listing {
                version: "1.4.0".into(),
                ..listing(
                    "com.example.word-filter",
                    "Word Filter",
                    "Hides words you'd rather not read.",
                    "Ada",
                    "Chat",
                    &["messages.read", "messages.intercept", "storage"],
                    &["api.example.com"],
                )
            },
            listing(
                "net.example.quiet-hours",
                "Quiet Hours",
                "Mutes servers at night.",
                "Dee",
                "Notifications",
                &["notifications.manage", "storage"],
                &[],
            ),
            listing(
                "org.example.pronouns",
                "Pronouns",
                "Shows people's pronouns beside their names.",
                "Eli",
                "People",
                &["people.read"],
                &["pronoundb.example.org"],
            ),
            listing(
                "org.example.timestamps",
                "Local Times",
                "Turns times in messages into your time zone.",
                "Fen",
                "Chat",
                &["messages.read"],
                &[],
            ),
            listing(
                "org.example.read-aloud",
                "Read Aloud",
                "Reads new messages in a channel aloud.",
                "Gus",
                "Accessibility",
                &["messages.read", "audio.speak"],
                &[],
            ),
        ];
        // Enough rows to scroll the list, as a real catalog would.
        for index in 1..=40 {
            listings.push(Listing {
                reviewed: index % 3 != 0,
                ..listing(
                    &format!("org.example.sample-{index}"),
                    &format!("Sample {index}"),
                    "A sample plugin, to fill the list.",
                    "Yapfast",
                    ["Chat", "Appearance", "Utilities"][index % 3],
                    &["storage"],
                    &[],
                )
            });
        }
        Self {
            view: View {
                helper: Helper::Running {
                    memory: Some(5_662_310),
                },
                installed,
                ..View::default()
            },
            listings,
        }
    }

    /// Word Filter's settings, which use every type a setting can have.
    fn word_filter_settings() -> Form {
        let manifest = super::Manifest::parse(include_str!(
            "../../../../testing/plugins/word-filter/yapfast-plugin.json"
        ))
        .expect("the sample's manifest is valid");
        let schema = Arc::clone(&manifest.settings);
        let mut form = Form::new(Arc::clone(&schema), schema.defaults());
        let tree: super::ui::Tree = serde_json::from_str(
            r#"{ "rows": [{ "framed": true, "cells": [
                { "kind": "muted", "text": "Oh h*ck, I forgot the" },
                { "kind": "badge", "text": "d***", "tone": "warning" }
            ] }] }"#,
        )
        .expect("the sample's tree is valid");
        form.widgets.insert("preview".into(), Arc::new(tree));
        // Shows a refusal as a form does once a field takes a bad value.
        form.edit(Edit::Set {
            key: "shortest".into(),
            value: super::settings::Input::Text("0".into()),
        });
        form
    }

    fn plugin_mut(&mut self, id: &str) -> Option<&mut Installed> {
        self.view
            .installed
            .iter_mut()
            .find(|plugin| plugin.id == id)
    }

    fn review_of(listing: &Listing) -> Review {
        Review {
            id: listing.id.clone(),
            name: listing.name.clone(),
            version: listing.version.clone(),
            icon: listing.icon.clone(),
            source: format!("https://codeberg.org/{}", listing.author.to_lowercase()),
            path: String::new(),
            commit: format!("{:0<40}", "c0ffee"),
            reviewed: listing.reviewed,
            capabilities: listing.capabilities.clone(),
            network: listing.network.clone(),
            memory_pages: DEFAULT_PAGES,
            dependencies: Vec::new(),
            replaces: None,
        }
    }

    fn found(url: &str) -> Result<Review, FetchError> {
        let url = url.trim();
        if url.starts_with("git@") || url.starts_with("ssh://") {
            return Err(FetchError::Ssh);
        }
        let Some(rest) = url.strip_prefix("https://") else {
            return Err(FetchError::NotHttps);
        };
        let tail = rest.trim_end_matches('/');
        if tail.ends_with("/no-manifest") {
            return Err(FetchError::NoManifest);
        }
        if tail.ends_with("/lfs") {
            return Err(FetchError::LfsPointer("plugin.wasm".into()));
        }
        if tail.ends_with("/huge") {
            return Err(FetchError::TooLarge);
        }
        if tail.ends_with("/slow") {
            return Err(FetchError::TimedOut);
        }
        let name = tail.rsplit('/').next().unwrap_or(tail);
        let title: String = name
            .split(['-', '_'])
            .filter(|word| !word.is_empty())
            .map(|word| {
                let mut letters = word.chars();
                letters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(letters).collect()
                })
            })
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Review {
            id: format!("com.example.{}", name.to_lowercase()),
            name: if title.is_empty() {
                "Plugin".into()
            } else {
                title
            },
            version: "0.3.0".into(),
            icon: None,
            source: url.to_owned(),
            path: url
                .split_once("#path=")
                .map_or_else(String::new, |(_, path)| path.to_owned()),
            commit: "7d1e4a90c3b2f5e6a7d8c9b0a1f2e3d4c5b6a7f8".into(),
            reviewed: false,
            capabilities: vec![
                "messages.read".into(),
                "messages.send.background".into(),
                "storage".into(),
            ],
            network: vec!["hooks.example.com".into()],
            memory_pages: 512,
            dependencies: vec![Dependency {
                name: "Word Filter".into(),
                version: "^1.4".into(),
                installed: true,
            }],
            replaces: None,
        })
    }
}

impl Source for Fixture {
    fn view(&self) -> &View {
        &self.view
    }

    fn request(&mut self, request: Request) {
        match request {
            Request::Open => {}
            Request::Close => self.view.catalog = Catalog::Unloaded,
            Request::LoadCatalog => {
                self.view.catalog = Catalog::Ready(Arc::from(self.listings.clone()));
            }
            Request::SetEnabled { plugin, enabled } => {
                if let Some(plugin) = self.plugin_mut(&plugin) {
                    plugin.enabled = enabled;
                    plugin.status = if enabled { Status::Idle } else { Status::Off };
                }
            }
            Request::Install { plugin } => {
                if let Some(listing) = self.listings.iter().find(|listing| listing.id == plugin) {
                    self.view.find = Find::Review(Box::new(Self::review_of(listing)));
                }
            }
            Request::Find { url } => {
                self.view.find = match Self::found(&url) {
                    Ok(review) => Find::Review(Box::new(review)),
                    Err(error) => Find::Failed { url, error },
                };
            }
            Request::Confirm => {
                let Find::Review(review) = std::mem::take(&mut self.view.find) else {
                    return;
                };
                self.view.installed.retain(|plugin| plugin.id != review.id);
                self.view.installed.push(Installed {
                    id: review.id,
                    name: review.name,
                    version: review.version,
                    // The sample has no bytes for it; the page draws the initial.
                    icon: None,
                    enabled: true,
                    status: Status::Idle,
                    origin: if review.reviewed {
                        Origin::Marketplace
                    } else {
                        Origin::Url
                    },
                    source: review.source,
                    path: review.path,
                    commit: review.commit,
                    license: "MIT".into(),
                    grants: review
                        .network
                        .into_iter()
                        .map(|host| Grant {
                            host,
                            declared: true,
                            allowed: true,
                            remembered: true,
                        })
                        .collect(),
                    capabilities: review.capabilities,
                    memory_pages: review.memory_pages,
                    ..Installed::default()
                });
            }
            Request::Cancel => self.view.find = Find::Idle,
            Request::Uninstall { plugin, .. } => {
                self.view
                    .installed
                    .retain(|installed| installed.id != plugin);
            }
            Request::Revoke { plugin, permission } => {
                if let Some(plugin) = self.plugin_mut(&plugin) {
                    match permission {
                        Permission::Capability(name) => {
                            plugin.capabilities.retain(|held| *held != name);
                        }
                        Permission::Host(host) => plugin.grants.retain(|grant| grant.host != host),
                    }
                }
            }
            Request::Update { plugin } => {
                if let Some(plugin) = self.plugin_mut(&plugin)
                    && let Some(update) = plugin.update.take()
                {
                    plugin.previous = Some(std::mem::replace(&mut plugin.version, update.version));
                    plugin.capabilities.extend(update.adds);
                }
            }
            Request::RollBack { plugin } => {
                if let Some(plugin) = self.plugin_mut(&plugin)
                    && let Some(previous) = plugin.previous.take()
                {
                    let newer = std::mem::replace(&mut plugin.version, previous);
                    plugin.update = Some(Update {
                        version: newer,
                        adds: Vec::new(),
                    });
                }
            }
            Request::Setting { plugin, edit } => {
                if let Some(plugin) = self.plugin_mut(&plugin) {
                    plugin.settings.edit(edit);
                }
            }
            Request::SettingWidget { .. } => {}
            // The fixture has no store to install into: what a picked file
            // becomes is the runtime's to say.
            Request::InstallWasm(_) => {}
            Request::LoadFolder(folder) => {
                let name = folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Folder plugin".into());
                self.view.installed.push(Installed {
                    id: format!("dev.local.{}", name.to_lowercase()),
                    name,
                    version: "0.0.0".into(),
                    enabled: true,
                    status: Status::Idle,
                    origin: Origin::Folder,
                    source: folder.display().to_string(),
                    memory_pages: DEFAULT_PAGES,
                    ..Installed::default()
                });
            }
            Request::SetMemoryBudget(mib) => self.view.advanced.memory_budget = mib,
            Request::SetIdleMinutes(minutes) => self.view.advanced.idle_minutes = minutes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|line| line.text.as_str()).collect()
    }

    #[test]
    fn capabilities_read_in_plain_words_with_the_risky_combination_called_out() {
        let lines = permission_lines(
            &["messages.read".into(), "storage".into()],
            &["api.example.com".into(), "*.cdn.example.com".into()],
        );
        assert_eq!(
            texts(&lines),
            [
                "Reads your messages",
                "Keeps its own data",
                "Connects to api.example.com",
                "Connects to *.cdn.example.com",
                "It reads your messages and connects to api.example.com and *.cdn.example.com, so it could send your messages to those servers.",
            ]
        );
        assert_eq!(lines[0].tone, Tone::Strong);
        assert_eq!(lines[4].tone, Tone::Warning);
        assert_eq!(lines[4].permission, None);
        // Without a host, reading messages is one line, and nothing more.
        assert_eq!(
            texts(&permission_lines(&["messages.read".into()], &[])),
            ["Reads your messages"]
        );
    }

    /// What Copy puts on the clipboard says more than the pane: who the plugin
    /// is, why it is where it is, and every line it logged, in one block.
    #[test]
    fn a_plugins_report_is_what_a_person_pastes_when_asking_why() {
        let plugin = Installed {
            id: "local.ping".into(),
            name: "Ping".into(),
            version: "0.1.0".into(),
            enabled: true,
            status: Status::Suspended(Suspension::Failed(
                "the helper didn't answer its load request within 10 s".into(),
            )),
            origin: Origin::Folder,
            source: "file:///home/someone/plugins".into(),
            commit: "0123456789abcdef".into(),
            capabilities: vec!["messages.read".into()],
            grants: vec![Grant {
                host: "api.example.com".into(),
                declared: true,
                allowed: true,
                remembered: true,
            }],
            memory_pages: 64,
            log: vec![LogLine {
                at: "19:03:43".into(),
                text: "yapfast: the helper didn't answer its load request within 10 s".into(),
                error: true,
            }],
            ..Installed::default()
        };
        let report = plugin.report();
        for wanted in [
            "Ping",
            "local.ping",
            "Suspended: the helper didn't answer its load request within 10 s",
            "Enabled: yes",
            "folder",
            "file:///home/someone/plugins",
            "01234567",
            "Uses up to 4 MiB of memory",
            "messages.read",
            "api.example.com",
            "19:03:43",
        ] {
            assert!(report.contains(wanted), "{wanted} is in:\n{report}");
        }
        // A plugin that logged nothing says so rather than leaving the section
        // out, so the report's shape doesn't depend on what happened.
        let quiet = Installed::default().report();
        assert!(quiet.contains("Log:\n  (nothing this session)"), "{quiet}");
    }

    #[test]
    fn a_plugins_page_words_its_grants_and_warns_only_of_hosts_it_may_reach() {
        let plugin = Installed {
            capabilities: vec!["messages.read".into()],
            grants: vec![
                Grant {
                    host: "api.example.com".into(),
                    declared: true,
                    allowed: true,
                    remembered: true,
                },
                Grant {
                    host: "tracker.example.net".into(),
                    declared: false,
                    allowed: false,
                    remembered: true,
                },
            ],
            ..Installed::default()
        };
        assert_eq!(
            texts(&plugin.permission_lines()),
            [
                "Reads your messages",
                "Connects to api.example.com",
                "Never asks again for tracker.example.net",
                "It reads your messages and connects to api.example.com, so it could send your messages to that server.",
            ]
        );
    }

    #[test]
    fn background_sending_says_it_can_get_an_account_banned_in_the_danger_tone() {
        let (text, tone) = capability("messages.send.background");
        assert_eq!(text, BACKGROUND_SEND);
        assert!(text.contains("can get your Discord account banned"));
        assert_eq!(tone, Tone::Danger);
    }

    #[test]
    fn sound_and_speech_share_a_line() {
        let lines = permission_lines(&["audio.play".into(), "audio.speak".into()], &[]);
        assert_eq!(texts(&lines), ["Plays sounds and speech"]);
    }

    #[test]
    fn marketplace_rows_sum_up_what_a_plugin_reaches() {
        let strings = |items: &[&str]| {
            items
                .iter()
                .map(|&item| item.to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            reach(
                &strings(&["messages.read", "messages.intercept"]),
                &strings(&["api.example.com"])
            ),
            "messages, 1 host"
        );
        assert_eq!(reach(&[], &strings(&["a.example", "b.example"])), "2 hosts");
        assert_eq!(reach(&[], &[]), "no permissions");
    }

    #[test]
    fn statuses_and_errors_are_said_plainly() {
        assert_eq!(
            Status::Suspended(Suspension::OutOfMemory).text(),
            "Suspended: ran out of memory"
        );
        assert_eq!(
            FetchError::NoManifest.text(),
            "That repository has no yapfast-plugin.json."
        );
        assert_eq!(
            FetchError::LfsPointer("plugin.wasm".into()).text(),
            "plugin.wasm is a Git LFS pointer; check in the file itself."
        );
        assert_eq!(
            memory(256),
            ("Uses up to 16 MiB of memory".into(), Tone::Normal)
        );
        assert_eq!(memory(512).1, Tone::Warning);
    }

    #[test]
    fn the_fixture_installs_from_a_url_only_after_the_review() {
        let mut fixture = Fixture::new();
        let before = fixture.view().installed.len();
        fixture.request(Request::Find {
            url: "https://codeberg.org/someone/lfs".into(),
        });
        assert!(matches!(
            fixture.view().find,
            Find::Failed {
                error: FetchError::LfsPointer(_),
                ..
            }
        ));
        fixture.request(Request::Find {
            url: "http://example.com/plugin".into(),
        });
        assert!(matches!(
            fixture.view().find,
            Find::Failed {
                error: FetchError::NotHttps,
                ..
            }
        ));
        fixture.request(Request::Find {
            url: "https://codeberg.org/someone/auto-reply".into(),
        });
        let Find::Review(review) = &fixture.view().find else {
            panic!("no review");
        };
        assert_eq!(review.name, "Auto Reply");
        assert!(!review.reviewed);
        assert_eq!(fixture.view().installed.len(), before);
        fixture.request(Request::Confirm);
        assert!(matches!(fixture.view().find, Find::Idle));
        assert_eq!(fixture.view().installed.len(), before + 1);
        fixture.request(Request::Uninstall {
            plugin: "com.example.auto-reply".into(),
            delete_data: true,
        });
        assert_eq!(fixture.view().installed.len(), before);
    }

    #[test]
    fn the_catalog_lives_only_while_the_page_is_open() {
        let mut fixture = Fixture::new();
        assert!(matches!(fixture.view().catalog, Catalog::Unloaded));
        fixture.request(Request::LoadCatalog);
        assert!(matches!(fixture.view().catalog, Catalog::Ready(_)));
        fixture.request(Request::Close);
        assert!(matches!(fixture.view().catalog, Catalog::Unloaded));
    }
}
