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
use crate::plugins::page::{self, FetchError, Find, Helper, Request, Step};
use crate::plugins::spawn;
use yapfast_plugin_api::PluginId;

use super::plugins::Action as PluginAction;
use super::{Action as AppAction, Application, Effect as AppEffect};

/// What Settings → Plugins is in the middle of, which the runtime's view doesn't
/// hold: the fetch, its review, and the plugin a review would install.
#[derive(Default)]
pub struct PageState {
    pub find: Find,
    /// What a review shows, kept until Install asks the store to write it.
    pub fetched: Option<installing::Fetched>,
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
            // draws, so nothing is held or sampled in between either way.
            Request::Open | Request::Close => Vec::new(),
            Request::Find { url } => {
                self.plugins_find(url);
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
            Request::LoadFolder(path) => self.plugins_load_folder(&path),
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
    fn plugins_find(&mut self, url: String) {
        self.plugin_page.fetched = None;
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
        match result {
            Ok(fetched) => {
                self.plugin_page.find = Find::Review(Box::new(fetched.review()));
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

    /// Installs a plugin from a folder and watches it, so a save in that folder
    /// reloads it: the development loop.
    fn plugins_load_folder(&mut self, folder: &std::path::Path) -> Vec<AppEffect> {
        let Some(plugins) = self.state.plugins.as_mut() else {
            self.state.notice =
                Some("The plugin system isn't running, so no folder can be loaded.".to_owned());
            return Vec::new();
        };
        match plugins.load_folder(folder) {
            Ok(plugin) => {
                self.state.notice = Some(format!(
                    "{plugin} is loaded from {} and reloads when it changes.",
                    folder.display()
                ));
                vec![AppEffect::Plugin(crate::application::plugins::Effect::Page)]
            }
            Err(error) => {
                self.state.notice = Some(format!(
                    "Couldn't load {}: {error:#}",
                    folder.display()
                ));
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
            Request::Open,
            Request::Close,
            Request::LoadCatalog,
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
}
