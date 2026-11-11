//! Settings → Plugins, from the application's side: what the page shows, and
//! what its requests do.
//!
//! The page reaches the runtime through [`crate::plugins::page::Source`], which
//! the interface's half (`src/ui/plugin_settings.rs`) implements over
//! [`Application::plugins_page`]; a request comes back here as an [`Action`], so
//! the runtime stays the only writer of a plugin's state.
//!
//! [`Application::plugins_page`] is a copy of what the runtime knows, taken when
//! the page draws itself: the page holds no lock and nothing is kept while it's
//! closed. With no plugin enabled there is no runtime either, and the page's one
//! question is whether plugins could run at all.

use crate::plugins::installing;
use crate::plugins::marketplace::{self, Catalog as Registry};
use crate::plugins::page::{self, Catalog, FetchError, Find, Helper, Request, Step};
use crate::plugins::spawn;
use std::sync::Arc;
use yapfast_plugin_api::PluginId;

use super::plugins::Action as PluginAction;
use super::{Action as AppAction, Application, Effect as AppEffect};

/// What Settings → Plugins is in the middle of, which the runtime's view doesn't
/// hold: the fetch, its review, and the marketplace behind the catalog tab.
#[derive(Default)]
pub struct PageState {
    pub find: Find,
    /// What a review shows, kept until Install asks the store to write it.
    pub fetched: Option<installing::Fetched>,
    /// The marketplace tab's state, and the registry whose entries it lists.
    /// Both exist only while the page is open: `Request::Close` drops them, and
    /// the icons' bytes with them.
    pub catalog: Catalog,
    pub registry: Option<Arc<Registry>>,
    /// What a fetch the marketplace started is for, so its answer is checked
    /// against the entry that named it.
    pub ask: Option<Ask>,
    /// Whether the registry was asked for since the page opened, so that
    /// showing the tab again doesn't fetch it twice.
    pub asked: bool,
}

/// What a fetch the page started is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Install { id: String },
    Update { id: String },
}

impl Ask {
    fn id(&self) -> &str {
        match self {
            Ask::Install { id } | Ask::Update { id } => id,
        }
    }

    fn is_update(&self) -> bool {
        matches!(self, Ask::Update { .. })
    }
}

impl Application {
    /// What Settings → Plugins shows: a row per installed plugin, what each is
    /// doing, and what the helper is doing.
    pub fn plugins_page(&self) -> page::View {
        let mut view = match self.state.plugins.as_deref() {
            Some(plugins) => plugins.page(),
            None => page::View {
                helper: if spawn::program().is_ok() {
                    Helper::Stopped
                } else {
                    Helper::Missing
                },
                ..page::View::default()
            },
        };
        // The fetch and its review are the page's own: the runtime's view can't
        // hold a plugin that isn't installed yet.
        view.find = self.plugin_page.find.clone();
        view.catalog = self.plugin_page.catalog.clone();
        // What the registry lists is what makes a plugin's origin The
        // marketplace, and what says whether a newer commit is listed. It is
        // read here rather than kept in the row, so a page with no catalog
        // says nothing about a marketplace it hasn't read.
        if let Some(registry) = &self.plugin_page.registry {
            registry.describe(&mut view.installed);
        }
        view
    }

    /// Carries out what the page asked for, and hands back what the interface
    /// has to do about it.
    pub fn plugins_page_request(&mut self, request: Request) -> Vec<AppEffect> {
        match request {
            Request::SetEnabled { plugin, enabled } => match PluginId::parse(&plugin) {
                // The state it settles in comes back as `Effect::Plugin` with a
                // status, which redraws the row: nothing is guessed here.
                Ok(plugin) => self.dispatch(AppAction::Plugin(PluginAction::SetEnabled {
                    plugin,
                    enabled,
                })),
                Err(error) => {
                    tracing::warn!(%error, %plugin, "the plugin page asked about a name that isn't a plugin id");
                    Vec::new()
                }
            },
            // The page is open or closed. The view is a copy taken when it
            // draws, so nothing is held or sampled in between either way; the
            // marketplace is the one thing that is: it is read while the page
            // is open and dropped when it closes.
            Request::Open => self.plugins_catalog_open(),
            Request::Close => self.plugins_catalog_close(),
            Request::LoadCatalog => self.plugins_catalog_ask(true),
            Request::Install { plugin } => self.plugins_install_listed(&plugin),
            Request::Update { plugin } => self.plugins_update(&plugin),
            Request::Find { url } => {
                self.plugins_find(url, None);
                vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
            }
            Request::Confirm => {
                // Nothing to install: the page's Install button without a review
                // behind it, which asks for nothing rather than redrawing.
                if self.plugin_page.fetched.is_none() {
                    return Vec::new();
                }
                self.plugins_install();
                vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
            }
            Request::Cancel => {
                self.plugin_page.find = Find::Idle;
                self.plugin_page.fetched = None;
                vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
            }
            Request::Uninstall {
                plugin,
                delete_data,
            } => self.plugins_remove(&plugin, delete_data),
            Request::RollBack { plugin } => self.plugins_roll_back(&plugin),
            Request::InstallWasm(path) => self.plugins_install_wasm(&path),
            // The marketplace, a plugin's own settings form and the memory
            // budgets are the milestones after this one: the page asks, and
            // nothing answers yet.
            other => {
                tracing::info!(
                    ?other,
                    "Settings → Plugins asked for what the runtime can't do yet"
                );
                Vec::new()
            }
        }
    }

    /// Asks the runtime to read a plugin out of its repository. The answer
    /// arrives as a notice on the application's own event path, so the window
    /// never waits on a fetch.
    fn plugins_find(&mut self, url: String, ask: Option<Ask>) {
        self.plugin_page.fetched = None;
        self.plugin_page.ask = ask;
        let Some(plugins) = self.state.plugins.as_mut() else {
            self.plugin_page.find = Find::Failed {
                url,
                error: FetchError::Unreachable("the plugin system isn't running".to_owned()),
            };
            return;
        };
        self.plugin_page.find = Find::Fetching {
            url: url.clone(),
            step: Step::Fetching,
        };
        plugins.fetch(url);
    }

    /// What a fetch found, or why it found nothing: the notice's answer.
    pub(super) fn plugins_fetched(
        &mut self,
        url: String,
        result: Result<installing::Fetched, String>,
    ) {
        let ask = self.plugin_page.ask.take();
        match result {
            Ok(fetched) => {
                let Some(ask) = ask else {
                    // Add from URL: nothing reviewed it, and the dialog says so.
                    self.plugin_page.find = Find::Review(Box::new(fetched.review()));
                    self.plugin_page.fetched = Some(fetched);
                    return;
                };
                let id = ask.id().to_owned();
                let registry = self.plugin_page.registry.clone();
                let entry = registry.as_deref().and_then(|registry| registry.entry(&id));
                // What the registry listed is what runs: a repository that
                // changed under the commit a maintainer reviewed is refused,
                // not installed.
                if let (Some(registry), Some(entry)) = (registry.as_deref(), entry)
                    && let Err(error) = registry.verify(entry, &fetched)
                {
                    self.plugin_page.find = Find::Failed {
                        url,
                        error: FetchError::Mismatch(error),
                    };
                    self.plugin_page.fetched = None;
                    return;
                }
                let mut review = fetched.review();
                review.reviewed = entry.is_some();
                let installed = self
                    .plugins_page()
                    .installed
                    .into_iter()
                    .find(|installed| installed.id == id);
                if let Some(installed) = &installed {
                    review.replaces = Some(installed.version.clone());
                    let adds: Vec<String> = review
                        .capabilities
                        .iter()
                        .filter(|capability| !installed.capabilities.contains(capability))
                        .cloned()
                        .collect();
                    // An update that asks for no capability the person hasn't
                    // already given is one click (`PLAN.md` §8.5); one that
                    // does shows its review first.
                    if ask.is_update() && adds.is_empty() {
                        self.plugin_page.fetched = Some(fetched);
                        self.plugins_install();
                        return;
                    }
                }
                self.plugin_page.find = Find::Review(Box::new(review));
                self.plugin_page.fetched = Some(fetched);
            }
            Err(error) => {
                self.plugin_page.find = Find::Failed {
                    url,
                    error: FetchError::Unreachable(error),
                };
                self.plugin_page.fetched = None;
            }
        }
    }

    /// The marketplace was read, or it couldn't be: its listings take the
    /// place of the tab's spinner, and a plugin the registry took down for a
    /// security reason is said where the person is looking.
    pub(super) fn plugins_catalog_read(
        &mut self,
        source: String,
        result: Result<Box<Registry>, String>,
    ) {
        match result {
            Ok(registry) => {
                for refused in &registry.refused {
                    tracing::warn!(%source, %refused, "the marketplace lists an entry Yapfast refused");
                }
                let installed = self.plugins_page().installed;
                if let Some(warning) = registry.warnings(&installed).into_iter().next() {
                    self.state.notice = Some(warning);
                }
                self.plugin_page.catalog = Catalog::Ready(Arc::clone(&registry.listings));
                self.plugin_page.registry = Some(Arc::from(registry));
            }
            Err(error) => {
                self.plugin_page.catalog = Catalog::Failed(error);
                self.plugin_page.registry = None;
            }
        }
    }

    /// Reads the marketplace when the page opens, once per time it is open. A
    /// forced ask is the tab's own: showing it again after a failure tries
    /// again.
    fn plugins_catalog_ask(&mut self, forced: bool) -> Vec<AppEffect> {
        let loaded = matches!(self.plugin_page.catalog, Catalog::Ready(_));
        let busy = matches!(self.plugin_page.catalog, Catalog::Loading);
        if busy || loaded || (!forced && self.plugin_page.asked) {
            return Vec::new();
        }
        self.plugin_page.asked = true;
        match self.state.plugins.as_mut() {
            Some(plugins) => {
                self.plugin_page.catalog = Catalog::Loading;
                plugins.catalog(marketplace::registry());
            }
            None => {
                self.plugin_page.catalog = Catalog::Failed(
                    "The plugin system isn't running, so there's no marketplace to browse.".into(),
                );
            }
        }
        vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
    }

    /// The page opened: the marketplace is read if it isn't already.
    fn plugins_catalog_open(&mut self) -> Vec<AppEffect> {
        self.plugins_catalog_ask(false)
    }

    /// The page closed: the catalog, its entries and its icons' bytes go, and
    /// so do the decoded pictures the rows asked for. Nothing keeps a copy.
    fn plugins_catalog_close(&mut self) -> Vec<AppEffect> {
        let held = !matches!(self.plugin_page.catalog, Catalog::Unloaded)
            || self.plugin_page.registry.is_some();
        self.plugin_page.catalog = Catalog::Unloaded;
        self.plugin_page.registry = None;
        self.plugin_page.ask = None;
        self.plugin_page.asked = false;
        let forgot = self.forget_plugin_icons();
        if held || forgot {
            vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
        } else {
            Vec::new()
        }
    }

    /// Drops the pictures the marketplace's rows had decoded, which were only
    /// ever on screen while the page was open.
    fn forget_plugin_icons(&mut self) -> bool {
        let icons: Vec<String> = {
            let state = &self.state;
            state
                .pictures
                .keys()
                .filter(|url| marketplace::icon_id(url).is_some())
                .cloned()
                .collect()
        };
        for url in &icons {
            self.state.pictures.remove(url);
        }
        if !icons.is_empty() {
            self.state.picture_changes += 1;
        }
        !icons.is_empty()
    }

    /// A marketplace row's Install: the entry it lists pins a commit and a
    /// hash, and the fetch that follows is checked against both.
    fn plugins_install_listed(&mut self, plugin: &str) -> Vec<AppEffect> {
        let Some(entry) = self
            .plugin_page
            .registry
            .as_deref()
            .and_then(|registry| registry.entry(plugin))
            .cloned()
        else {
            self.state.notice = Some(format!(
                "The marketplace doesn't list {plugin}. Reload the page to read it again."
            ));
            return Vec::new();
        };
        self.plugins_find(entry.url(), Some(Ask::Install {
            id: entry.id.clone(),
        }));
        vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
    }

    /// Update: the marketplace's entry pins the commit to go to; a plugin from
    /// a URL is fetched again at its own ref, and its own source, since no
    /// registry lists it (`PLAN.md` §8.5: URL installs stay manual).
    fn plugins_update(&mut self, plugin: &str) -> Vec<AppEffect> {
        let asked = self
            .plugin_page
            .registry
            .as_deref()
            .and_then(|registry| registry.entry(plugin))
            .map(|entry| entry.url());
        let url = match asked {
            Some(url) => url,
            None => {
                let Some(installed) = self
                    .plugins_page()
                    .installed
                    .into_iter()
                    .find(|installed| installed.id == plugin)
                else {
                    return Vec::new();
                };
                if installed.source.is_empty() {
                    self.state.notice = Some(format!(
                        "{} comes from a folder, and reloads when its files change, so there is \
                         nothing to update from.",
                        installed.name
                    ));
                    return Vec::new();
                }
                let mut url = installed.source.clone();
                if !installed.path.is_empty() {
                    url.push_str("#path=");
                    url.push_str(&installed.path);
                }
                url
            }
        };
        self.plugins_find(
            url,
            Some(Ask::Update {
                id: plugin.to_owned(),
            }),
        );
        vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
    }

    /// The pictures the marketplace's rows ask for: an icon's address is
    /// decoded from the catalog's own bytes here, so a row off screen costs
    /// nothing and a picture is made only where it shows. Everything else goes
    /// on to the backend, which is what fetches pictures from the network.
    #[cfg(feature = "plugins")]
    pub(super) fn plugin_icons(&mut self, urls: Vec<String>) -> Vec<String> {
        let Some(registry) = self.plugin_page.registry.clone() else {
            return urls;
        };
        let mut rest: Vec<String> = Vec::with_capacity(urls.len());
        for url in urls {
            if marketplace::icon_id(&url).is_none() {
                rest.push(url);
                continue;
            }
            if self.state.pictures.contains_key(&url) {
                continue;
            }
            let Some(bytes) = registry.icon(&url) else {
                continue;
            };
            let Some(picture) = crate::chat::decode_picture(
                &bytes,
                marketplace::ICON_SIDE,
                marketplace::ICON_SIDE,
            ) else {
                tracing::warn!(%url, "a marketplace icon isn't a picture this build can read");
                continue;
            };
            self.keep_picture(url, Arc::new(picture));
        }
        rest
    }

    /// Writes what the review showed into the store, which is what Install
    /// means. The page then lists it, and nothing is running until it is
    /// enabled.
    fn plugins_install(&mut self) {
        let Some(fetched) = self.plugin_page.fetched.clone() else {
            return;
        };
        let Some(plugins) = self.state.plugins.as_mut() else {
            self.plugin_page.find = Find::Failed {
                url: fetched.source.clone(),
                error: FetchError::Unreachable("the plugin system isn't running".to_owned()),
            };
            return;
        };
        self.plugin_page.find = Find::Installing {
            name: fetched.review().name,
        };
        match plugins.install(&fetched.package()) {
            Ok(()) => {
                self.plugin_page.find = Find::Idle;
                self.plugin_page.fetched = None;
            }
            Err(error) => {
                self.plugin_page.find = Find::Failed {
                    url: fetched.source.clone(),
                    error: FetchError::Unreachable(format!("{error:#}")),
                };
            }
        }
    }

    /// Writes a `.wasm` file the person picked into the store.
    ///
    /// The plugin it becomes is installed and off: nothing runs until they
    /// enable it, which is the choice the row offers next.
    fn plugins_install_wasm(&mut self, path: &std::path::Path) -> Vec<AppEffect> {
        let fetched = match installing::from_wasm(path) {
            Ok(fetched) => fetched,
            Err(error) => {
                self.state.notice = Some(error);
                return Vec::new();
            }
        };
        let name = fetched.review().name;
        let Some(plugins) = self.state.plugins.as_mut() else {
            self.state.notice =
                Some("The plugin system isn't running, so nothing can be installed.".to_owned());
            return Vec::new();
        };
        match plugins.install(&fetched.package()) {
            Ok(()) => {
                self.state.notice = Some(format!("{name} is installed. Enable it to run it."));
                vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
            }
            Err(error) => {
                self.state.notice = Some(format!("Couldn't install {name}: {error:#}"));
                Vec::new()
            }
        }
    }

    /// Removes a plugin from the store, and its data when asked.
    fn plugins_remove(&mut self, plugin: &str, delete_data: bool) -> Vec<AppEffect> {
        let parsed = match PluginId::parse(plugin) {
            Ok(id) => id,
            Err(error) => {
                self.state.notice = Some(format!("Couldn't remove {plugin}: {error}"));
                return Vec::new();
            }
        };
        let Some(plugins) = self.state.plugins.as_mut() else {
            return Vec::new();
        };
        match plugins.uninstall(&parsed, delete_data) {
            Ok(()) => vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)],
            Err(error) => {
                self.state.notice = Some(format!("Couldn't remove {plugin}: {error}"));
                Vec::new()
            }
        }
    }

    /// Goes back to the version an install replaced.
    fn plugins_roll_back(&mut self, plugin: &str) -> Vec<AppEffect> {
        let Some(plugins) = self.state.plugins.as_mut() else {
            return Vec::new();
        };
        let parsed = match PluginId::parse(plugin) {
            Ok(id) => id,
            Err(error) => {
                self.state.notice = Some(format!("Couldn't roll {plugin} back: {error}"));
                return Vec::new();
            }
        };
        match plugins.roll_back(&parsed) {
            Ok(()) => vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)],
            Err(error) => {
                self.state.notice = Some(format!("Couldn't roll {plugin} back: {error}"));
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run with no plugin system open still answers the page: nothing is
    /// installed, and the only question left is whether plugins could run.
    #[test]
    fn the_page_without_a_runtime_has_no_plugins() {
        let app = Application::default();
        let view = app.plugins_page();
        assert!(view.installed.is_empty(), "nothing is installed");
        // Whether the helper sits beside this test binary is the machine's
        // business; either answer is the truth while no runtime is open.
        assert!(matches!(view.helper, Helper::Stopped | Helper::Missing));
    }

    /// A picked `.wasm` becomes a row: installed, not enabled, and drawn again
    /// at once.
    #[test]
    fn a_picked_file_shows_up_installed_and_off() {
        let directory = tempfile::tempdir().expect("a data directory");
        let source = tempfile::tempdir().expect("a folder to pick from");
        let module = source.path().join("echo.wasm");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/plugins/bad_answer.wasm"),
            &module,
        )
        .expect("a module");
        // A plugin is its manifest: the module alone declares no command and
        // nothing to reach.
        std::fs::write(
            source.path().join("yapfast-plugin.json"),
            r#"{
                "api": 1, "id": "local.echo", "name": "Echo", "version": "0.1.0",
                "wasm": "echo.wasm", "limits": { "memory_pages": 64 },
                "permissions": { "capabilities": [], "network": [] }
            }"#,
        )
        .expect("its manifest");

        let mut app = Application::default();
        let (events, _received) = tokio::sync::mpsc::unbounded_channel();
        app.open_plugins(directory.path(), events);

        let effects = app.plugins_page_request(Request::InstallWasm(module));
        assert_eq!(effects.len(), 1, "the page draws itself again");

        let view = app.plugins_page();
        assert_eq!(view.installed.len(), 1, "it is listed");
        assert_eq!(view.installed[0].id, "local.echo");
        assert!(!view.installed[0].enabled, "and off until the person says");
        assert_eq!(view.installed[0].status, crate::plugins::page::Status::Off);
    }

    /// The page's requests that the runtime can't do yet answer nothing rather
    /// than failing, so a page with a marketplace tab and no marketplace still
    /// works.
    ///
    /// Finding a plugin is answered without a runtime too: the page says why
    /// rather than staying on a spinner, which is one redraw and no fetch.
    #[test]
    fn finding_a_plugin_without_a_runtime_says_why() {
        let mut app = Application::default();
        let effects = app.plugins_page_request(Request::Find {
            url: "https://example.org/ada/word-filter".into(),
        });
        assert_eq!(effects.len(), 1, "one redraw");
        assert!(matches!(app.plugin_page.find, Find::Failed { .. }));
    }

    #[test]
    fn requests_the_runtime_cannot_do_answer_nothing() {
        let mut app = Application::default();
        for request in [
            Request::Close,
            Request::Confirm,
            Request::SetEnabled {
                plugin: "not a plugin id".into(),
                enabled: true,
            },
        ] {
            assert!(
                app.plugins_page_request(request).is_empty(),
                "nothing to do without a runtime"
            );
        }
    }

    /// The page opening without a runtime says why the marketplace is empty
    /// rather than staying on a spinner; closing it drops what was there.
    #[test]
    fn the_marketplace_without_a_runtime_says_why_it_is_empty() {
        let mut app = Application::default();
        assert_eq!(app.plugins_page_request(Request::Open).len(), 1, "one redraw");
        assert!(matches!(app.plugin_page.catalog, Catalog::Failed(_)));
        // Nothing to browse, so the tab's state goes back to Unloaded, which
        // is one redraw and nothing held.
        assert_eq!(app.plugins_page_request(Request::Close).len(), 1);
        assert!(matches!(app.plugins_page().catalog, Catalog::Unloaded));
        // An install from a listing nobody read says so rather than hanging.
        assert!(
            app.plugins_page_request(Request::Install {
                plugin: "com.example.word-filter".into()
            })
            .is_empty()
        );
        assert!(app.state.notice.is_some(), "it says the marketplace doesn't list it");
    }

    /// The marketplace's catalog, and the icons it holds, exist only while the
    /// page is open: closing it drops the whole catalog, and nothing keeps a
    /// copy.
    #[test]
    fn closing_the_page_drops_the_catalog_and_its_icons() {
        let mut app = Application::default();
        let listing = page::Listing {
            id: "com.example.word-filter".into(),
            name: "Word Filter".into(),
            ..page::Listing::default()
        };
        let icon = vec![0u8; 512];
        let catalog = crate::plugins::marketplace::Catalog::read(
            "https://codeberg.org/yapfast/plugin-registry",
            &"a".repeat(40),
            r#"{ "api": 1, "plugins": ["com.example.word-filter"], "delisted": [] }"#,
            &[(
                "plugins/com.example.word-filter.json".to_owned(),
                serde_json::json!({
                    "api": 1,
                    "id": "com.example.word-filter",
                    "name": "Word Filter",
                    "version": "1.4.0",
                    "repo": "https://codeberg.org/ada/word-filter",
                    "path": "",
                    "rev": "a".repeat(40),
                    "sha256": "0".repeat(64),
                    "source": "https://codeberg.org/ada/word-filter",
                    "license": "MIT"
                })
                .to_string(),
            )],
            &[(
                "icons/com.example.word-filter.webp".to_owned(),
                icon.clone(),
            )],
        )
        .expect("a registry");
        assert_eq!(catalog.listings.len(), 1);
        assert_eq!(catalog.icon_count(), 0, "a 512-byte zero icon isn't one");

        let mut listing = listing;
        listing.icon = Some(crate::plugins::marketplace::icon_url("com.example.word-filter"));
        let catalog = crate::plugins::marketplace::Catalog::read(
            "https://codeberg.org/yapfast/plugin-registry",
            &"a".repeat(40),
            r#"{ "api": 1, "plugins": ["com.example.word-filter"], "delisted": [] }"#,
            &[(
                "plugins/com.example.word-filter.json".to_owned(),
                serde_json::json!({
                    "api": 1,
                    "id": "com.example.word-filter",
                    "name": "Word Filter",
                    "version": "1.4.0",
                    "repo": "https://codeberg.org/ada/word-filter",
                    "path": "",
                    "rev": "a".repeat(40),
                    "sha256": "0".repeat(64),
                    "source": "https://codeberg.org/ada/word-filter",
                    "license": "MIT",
                    "icon": "icons/com.example.word-filter.webp"
                })
                .to_string(),
            )],
            &[(
                "icons/com.example.word-filter.webp".to_owned(),
                webp_fixture(),
            )],
        )
        .expect("a registry with an icon");
        assert_eq!(catalog.icon_count(), 1, "its icon's bytes are held");

        app.plugin_page.registry = Some(std::sync::Arc::new(catalog));
        app.plugin_page.catalog = page::Catalog::Ready(std::sync::Arc::from(vec![listing]));
        app.plugin_page.asked = true;
        assert!(matches!(app.plugins_page().catalog, page::Catalog::Ready(_)));

        let effects = app.plugins_page_request(Request::Close);
        assert_eq!(effects.len(), 1, "the page draws itself again");
        assert!(
            app.plugin_page.registry.is_none(),
            "the entries and the icons' bytes are gone"
        );
        assert!(matches!(app.plugins_page().catalog, page::Catalog::Unloaded));
    }

    /// A WebP header for a 128 by 128 picture, which is what a listing's icon
    /// is measured by.
    fn webp_fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&22u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&[127, 0, 0]);
        bytes.extend_from_slice(&[127, 0, 0]);
        bytes
    }
}
