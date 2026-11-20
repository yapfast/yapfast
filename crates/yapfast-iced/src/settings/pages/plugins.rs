//! Settings → Plugins: the plugins installed and what each is doing, the
//! marketplace they come from, a plugin's own page with its permissions, log
//! and settings, and the installer's review.
//!
//! The page draws a copy of what the runtime knows
//! (`Application::plugins_page`), and what the person does goes back as a
//! `page::Request`, which the runtime answers. Nothing is guessed here: a
//! switch that was flipped shows the state the runtime settles it in, carried
//! back as an effect and synced into the next view.
//!
//! A plugin's form is kept here while its page is open (`forms` and `trees`),
//! because the widgets the form is drawn from borrow from it and a drawing
//! outlives the copy the runtime handed over.

use std::collections::BTreeMap;
#[cfg(feature = "plugins")]
use std::sync::Arc;

#[cfg(not(feature = "plugins"))]
use iced::Element;
#[cfg(feature = "plugins")]
use iced::{
    Alignment, Color, Element, Length, Task,
    widget::{Space, button, column, container, row, scrollable, text},
};
#[cfg(feature = "plugins")]
use yapfast_core::plugins::page::{
    self, Advanced, Catalog, FetchError, Find, Helper, Installed, Listing, Origin, Permission,
    Request, Step, Tone,
};
#[cfg(feature = "plugins")]
use yapfast_core::plugins::settings::{
    self, Edit, Form, Input, Kind as SettingKind, Refused, Value, address,
};

#[cfg(feature = "plugins")]
use super::Event;
use super::{Page, Settings};
use crate::app::Message;
use crate::ctx::Cx;
use crate::form;
#[cfg(feature = "plugins")]
use crate::{
    controls::{self, Kind as ActionKind},
    fonts::BOLD,
    plugin_tree,
    theme::Theme,
};

/// Which of the page's four tabs shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Installed,
    Marketplace,
    FromUrl,
    Advanced,
}

impl Tab {
    #[cfg_attr(
        not(feature = "plugins"),
        allow(dead_code, reason = "a build without the plugin system draws no tab")
    )]
    const ALL: [Tab; 4] = [
        Tab::Installed,
        Tab::Marketplace,
        Tab::FromUrl,
        Tab::Advanced,
    ];

    #[cfg_attr(
        not(feature = "plugins"),
        allow(dead_code, reason = "a build without the plugin system draws no tab")
    )]
    fn title(self) -> &'static str {
        match self {
            Tab::Installed => "Installed",
            Tab::Marketplace => "Marketplace",
            Tab::FromUrl => "From a URL",
            Tab::Advanced => "Advanced",
        }
    }
}

/// The devtools' views of one plugin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevTab {
    /// Its log, as it comes.
    #[default]
    Console,
    /// What each slot shows of it.
    Slots,
    /// The host calls it made and what it was handed, with timing and size.
    Calls,
}

/// What the page keeps while Settings is open. A build without the plugin
/// system keeps it for the field in `Settings` and draws none of it.
#[derive(Default)]
#[cfg_attr(
    not(feature = "plugins"),
    allow(
        dead_code,
        reason = "a build without the plugin system draws no plugin page"
    )
)]
pub struct PluginPage {
    pub tab: Tab,
    /// The marketplace's search field and the query it filters by.
    pub search: String,
    pub query: String,
    /// The chosen category, 0 being every one.
    pub category: usize,
    /// The URL being installed from.
    pub url: String,
    /// The installed plugin whose page is open, by id.
    pub open: Option<String>,
    /// Which dropdown is open, by what it sets.
    pub dropdown: Option<String>,
    /// Text a field holds before it is submitted, by setting key or
    /// `key/row/field`.
    pub drafts: BTreeMap<String, String>,
    /// What the window keeps of the plugin's drawings on its page (a widget
    /// setting's, its own settings page): typed text, open tabs.
    #[cfg(feature = "plugins")]
    pub local: crate::plugin_tree::Local,
    /// The open plugin's form, and the trees its widget settings draw. They
    /// are kept here so a drawing can borrow them.
    #[cfg(feature = "plugins")]
    pub form: Option<Form>,
    #[cfg(feature = "plugins")]
    pub trees: BTreeMap<String, Arc<yapfast_core::plugins::ui::Tree>>,
    /// Whether the marketplace was asked for since the page opened.
    pub asked: bool,
    /// The plugin whose Uninstall was pressed, while the page asks whether to
    /// keep its data.
    pub uninstalling: Option<String>,
    /// The open plugin's own settings page shows, rather than its details.
    pub custom: bool,
    /// Reset to defaults was pressed, and the page asks first.
    pub resetting: bool,
    /// Which of the devtools' views shows on a plugin's page.
    pub dev_tab: DevTab,
    /// The red install dialog's box, ticked: a plugin asking for a powerful
    /// permission installs only once the person said they understand.
    pub powerful_confirmed: bool,
    /// What the runtime last said the page shows, and when: drawing reads it
    /// rather than asking the runtime each frame. It is forgotten when a
    /// plugin's status or the page's own state moves, and after a second, so
    /// the helper's memory still updates while the page is open.
    #[cfg(feature = "plugins")]
    view: std::cell::RefCell<Option<(std::time::Instant, Arc<page::View>)>>,
}

#[cfg(feature = "plugins")]
impl PluginPage {
    /// How long a drawn copy of the page is kept with nothing moving.
    const KEPT: std::time::Duration = std::time::Duration::from_secs(1);

    /// What the page shows, from the copy kept since the last change.
    pub fn view(&self, app: &yapfast_core::application::Application) -> Arc<page::View> {
        let mut kept = self.view.borrow_mut();
        if let Some((at, view)) = kept.as_ref()
            && at.elapsed() < Self::KEPT
        {
            return Arc::clone(view);
        }
        let view = Arc::new(app.plugins_page());
        *kept = Some((std::time::Instant::now(), Arc::clone(&view)));
        view
    }

    /// Drops the kept copy, so the next draw asks the runtime again.
    pub fn forget(&self) {
        self.view.borrow_mut().take();
    }
}

impl std::fmt::Debug for PluginPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginPage")
            .field("tab", &self.tab)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

/// What the page sends.
#[derive(Clone, Debug)]
pub enum PluginEvent {
    /// The switch in Advanced that lets plugins use powerful permissions.
    SetPowerful(bool),
    /// The red install dialog's "I understand" box.
    ConfirmPowerful(bool),
    /// Copies a plugin's local endpoint token.
    CopyToken(String),
    /// Runs a plugin or stops it.
    SetEnabled {
        id: String,
        enabled: bool,
    },
    /// The page opened or closed, so the runtime samples the helper's memory
    /// while it's up and drops the catalog when it isn't.
    Opened(bool),
    Tab(Tab),
    Search(String),
    /// The marketplace's category chip chosen, 0 being every one.
    Category(usize),
    Url(String),
    /// Opens an installed plugin's own page.
    Open(String),
    /// Leaves it, for the list.
    Back,
    /// Opens or closes a dropdown, by what it sets.
    Dropdown(Option<String>),
    /// A field took a keystroke, before it is submitted.
    Draft {
        at: String,
        text: String,
    },
    /// A field is submitted: its text becomes the setting's value.
    Submit(String),
    /// A switch, a select or a slider settled on a value.
    Set {
        key: String,
        value: String,
    },
    /// A list's row field settled on a value.
    SetField {
        key: String,
        row: usize,
        field: String,
        value: String,
    },
    AddRow(String),
    RemoveRow {
        key: String,
        row: usize,
    },
    MoveRow {
        key: String,
        row: usize,
        to: i32,
    },
    ResetSettings,
    /// A cell of a plugin's drawing on its page was used: a widget
    /// setting's, by its key, or its own settings page's, with none.
    #[cfg(feature = "plugins")]
    Act {
        key: Option<String>,
        act: crate::plugin_tree::Act,
    },
    /// Opens an installed plugin's own settings page.
    OpenSettings(String),
    /// Starts a plugin again: after it ran out of memory, or the devtools'
    /// Reload.
    Restart(String),
    /// Turns the devtools on or off (Advanced).
    SetDevtools(bool),
    /// A second passed while the devtools show: the page is read again.
    Tick,
    DevTab(DevTab),
    /// Applies a plugin's changes to Yapfast's own interface, or not.
    SetLooks {
        id: String,
        on: bool,
    },
    /// Reset to defaults pressed (`true`), or its question answered No.
    AskReset(bool),
    Install(String),
    Find,
    Confirm,
    Cancel,
    /// Opens the system picker for a plugin's `.wasm` file.
    ChooseWasm,
    /// Opens the system picker for a local plugin repository.
    ChooseFolder,
    InstallWasm(std::path::PathBuf),
    LoadFolder(std::path::PathBuf),
    Uninstall {
        id: String,
        delete_data: bool,
    },
    /// Uninstall was pressed on a plugin's page: the page asks whether to keep
    /// its data; `None` cancels.
    AskUninstall(Option<String>),
    Update(String),
    RollBack(String),
    #[cfg(feature = "plugins")]
    Revoke {
        id: String,
        permission: Permission,
    },
    SetMemoryBudget(u32),
    SetIdleMinutes(u32),
}

#[cfg(feature = "plugins")]
fn plugin_message(event: PluginEvent) -> Message {
    Message::Settings(Event::Plugin(event))
}

/// The system pickers Advanced opens. Their answers come back as ordinary
/// plugin events, so choosing and cancelling need no state in the page.
#[cfg(feature = "plugins")]
pub(in crate::settings) fn plugin_dialog(event: &PluginEvent) -> Option<Task<Message>> {
    match event {
        PluginEvent::ChooseWasm => Some(Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Install a plugin")
                    .add_filter("WebAssembly plugins", &["wasm"])
                    .pick_file()
                    .await
                    .map(|file| file.path().to_owned())
            },
            |path| match path {
                Some(path) => plugin_message(PluginEvent::InstallWasm(path)),
                None => Message::Nothing,
            },
        )),
        PluginEvent::ChooseFolder => Some(Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose a local plugin repository")
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_owned())
            },
            |path| match path {
                Some(path) => plugin_message(PluginEvent::LoadFolder(path)),
                None => Message::Nothing,
            },
        )),
        PluginEvent::CopyToken(token) => Some(iced::clipboard::write(token.clone())),
        _ => None,
    }
}

/// The powerful capabilities among a plugin's, by their names.
#[cfg(feature = "plugins")]
fn powerful_of(capabilities: &[String]) -> Vec<yapfast_plugin_api::capability::Capability> {
    capabilities
        .iter()
        .filter_map(|name| {
            serde_json::from_value::<yapfast_plugin_api::capability::Capability>(
                serde_json::Value::String(name.clone()),
            )
            .ok()
        })
        .filter(|capability| capability.is_powerful())
        .collect()
}

impl Settings {
    #[cfg_attr(
        not(feature = "plugins"),
        allow(
            unused_variables,
            clippy::needless_pass_by_ref_mut,
            reason = "a build without the plugin system has nothing to ask, and no page to keep"
        )
    )]
    pub(in crate::settings) fn plugin(&mut self, event: PluginEvent, cx: &mut Cx<'_>) {
        #[cfg(not(feature = "plugins"))]
        {
            let _ = (event, cx);
        }
        #[cfg(feature = "plugins")]
        {
            let selected = self.plugins.open.clone();
            match event {
                PluginEvent::SetEnabled { id, enabled } => {
                    cx.plugin_request(Request::SetEnabled {
                        plugin: id,
                        enabled,
                    });
                }
                PluginEvent::Opened(open) => {
                    cx.plugin_request(if open { Request::Open } else { Request::Close });
                }
                PluginEvent::Tab(tab) => {
                    self.plugins.tab = tab;
                    if tab == Tab::Marketplace && !self.plugins.asked {
                        self.plugins.asked = true;
                        cx.plugin_request(Request::LoadCatalog);
                    }
                }
                PluginEvent::Search(text) => {
                    self.plugins.search = text.clone();
                    self.plugins.query = text;
                }
                PluginEvent::Category(index) => self.plugins.category = index,
                PluginEvent::Url(text) => self.plugins.url = text,
                PluginEvent::Open(id) => {
                    self.plugins.custom = false;
                    self.plugins.resetting = false;
                    self.plugins.open = Some(id);
                    self.plugins.drafts.clear();
                    self.plugins.local = Default::default();
                    self.plugins.dropdown = None;
                }
                PluginEvent::Back => {
                    self.plugins.custom = false;
                    self.plugins.resetting = false;
                    self.plugins.open = None;
                    self.plugins.uninstalling = None;
                    self.plugins.drafts.clear();
                    self.plugins.local = Default::default();
                    self.plugins.dropdown = None;
                    self.plugins.form = None;
                    self.plugins.trees.clear();
                }
                PluginEvent::Dropdown(which) => self.plugins.dropdown = which,
                PluginEvent::Draft { at, text } => {
                    self.plugins.drafts.insert(at, text);
                }
                PluginEvent::Submit(at) => {
                    // The typed text becomes the value; the draft goes so the
                    // field shows what the runtime settles on.
                    if let Some(text) = self.plugins.drafts.remove(&at) {
                        self.edit(cx, selected.as_deref(), submit_edit(&at, text));
                    }
                }
                PluginEvent::Set { key, value } => {
                    if let Some(kind) = self.kind_of(&key) {
                        let input = input_of(&kind, &value);
                        self.plugins.drafts.remove(&key);
                        self.edit(
                            cx,
                            selected.as_deref(),
                            Some(Edit::Set { key, value: input }),
                        );
                    }
                }
                PluginEvent::SetField {
                    key,
                    row,
                    field,
                    value,
                } => {
                    if let Some(kind) = self.field_kind_of(&key, &field) {
                        let input = input_of(&kind, &value);
                        let at = address(&key, Some((row, &field)));
                        self.plugins.drafts.remove(&at);
                        self.edit(
                            cx,
                            selected.as_deref(),
                            Some(Edit::SetField {
                                key,
                                row,
                                field,
                                value: input,
                            }),
                        );
                    }
                }
                PluginEvent::AddRow(key) => {
                    self.edit(cx, selected.as_deref(), Some(Edit::AddRow { key }));
                }
                PluginEvent::RemoveRow { key, row } => {
                    self.edit(cx, selected.as_deref(), Some(Edit::RemoveRow { key, row }));
                }
                PluginEvent::MoveRow { key, row, to } => {
                    if to >= 0 {
                        self.edit(
                            cx,
                            selected.as_deref(),
                            Some(Edit::MoveRow {
                                key,
                                row,
                                to: to as usize,
                            }),
                        );
                    }
                }
                PluginEvent::ResetSettings => {
                    self.plugins.drafts.clear();
                    self.plugins.local.drafts.clear();
                    self.plugins.resetting = false;
                    self.edit(cx, selected.as_deref(), Some(Edit::Reset));
                }
                PluginEvent::AskReset(asking) => self.plugins.resetting = asking,
                PluginEvent::OpenSettings(id) => {
                    self.plugins.open = Some(id.clone());
                    self.plugins.custom = true;
                    self.plugins.resetting = false;
                    self.plugins.drafts.clear();
                    self.plugins.local = Default::default();
                    self.plugins.dropdown = None;
                    // Opening its page wakes the plugin, which draws it.
                    cx.dispatch(yapfast_core::application::Action::PluginInput(
                        yapfast_core::plugins::Input::Widget {
                            plugin: id,
                            slot: yapfast_core::plugins::slots::Slot::SettingsPage,
                            id: OPENED.to_owned(),
                            value: String::new(),
                            subject: String::new(),
                        },
                    ));
                }
                PluginEvent::Restart(id) => cx.plugin_request(Request::Restart { plugin: id }),
                PluginEvent::SetDevtools(on) => {
                    cx.layout.plugin_devtools = on;
                    cx.keep("plugin_devtools", on);
                    yapfast_core::plugins::devtools::set_on(on);
                }
                PluginEvent::DevTab(tab) => self.plugins.dev_tab = tab,
                PluginEvent::Tick => cx.touch(),
                PluginEvent::SetLooks { id, on } => {
                    if on {
                        cx.layout.plugin_looks_off.remove(&id);
                    } else {
                        cx.layout.plugin_looks_off.insert(id);
                    }
                    cx.keep("plugin_looks_off", &cx.layout.plugin_looks_off);
                    cx.request(crate::ctx::Request::RefreshPlugins);
                }
                PluginEvent::Act { key, act } => self.plugin_act(cx, selected, key, act),
                PluginEvent::Install(plugin) => cx.plugin_request(Request::Install { plugin }),
                PluginEvent::Find => {
                    let url = self.plugins.url.trim().to_owned();
                    if !url.is_empty() {
                        cx.plugin_request(Request::Find { url });
                    }
                }
                PluginEvent::Confirm => {
                    self.plugins.powerful_confirmed = false;
                    cx.plugin_request(Request::Confirm);
                    if matches!(cx.client.app.plugins_page().find, Find::Idle) {
                        self.plugins.url.clear();
                        self.plugins.tab = Tab::Installed;
                    }
                }
                PluginEvent::Cancel => {
                    self.plugins.powerful_confirmed = false;
                    cx.plugin_request(Request::Cancel);
                }
                PluginEvent::ConfirmPowerful(ticked) => self.plugins.powerful_confirmed = ticked,
                PluginEvent::SetPowerful(enabled) => {
                    cx.client
                        .dispatch(yapfast_core::application::Action::Plugin(
                            yapfast_core::application::plugins::Action::SetPowerful(enabled),
                        ));
                    self.plugins.forget();
                }
                PluginEvent::CopyToken(_) => {}
                // Settings::update opens these dialogs before dispatching here.
                PluginEvent::ChooseWasm | PluginEvent::ChooseFolder => {}
                PluginEvent::InstallWasm(path) => {
                    cx.plugin_request(Request::InstallWasm(path));
                    self.plugins.tab = Tab::Installed;
                }
                PluginEvent::LoadFolder(path) => {
                    cx.plugin_request(Request::LoadFolder(path));
                    self.plugins.tab = Tab::Installed;
                }
                PluginEvent::AskUninstall(id) => self.plugins.uninstalling = id,
                PluginEvent::Uninstall { id, delete_data } => {
                    self.plugins.uninstalling = None;
                    if self.plugins.open.as_deref() == Some(id.as_str()) {
                        self.plugins.open = None;
                        self.plugins.form = None;
                    }
                    cx.plugin_request(Request::Uninstall {
                        plugin: id,
                        delete_data,
                    });
                }
                PluginEvent::Update(id) => cx.plugin_request(Request::Update { plugin: id }),
                PluginEvent::RollBack(id) => cx.plugin_request(Request::RollBack { plugin: id }),
                PluginEvent::Revoke { id, permission } => {
                    cx.plugin_request(Request::Revoke {
                        plugin: id,
                        permission,
                    });
                }
                PluginEvent::SetMemoryBudget(mib) => {
                    cx.plugin_request(Request::SetMemoryBudget(mib));
                }
                PluginEvent::SetIdleMinutes(minutes) => {
                    cx.plugin_request(Request::SetIdleMinutes(minutes));
                }
            }
            // The form borrows from what is kept here, so it is taken again
            // after every change the runtime may have answered.
            self.keep_form(cx);
        }
    }

    /// Hands an edit to the runtime, for the open plugin.
    #[cfg(feature = "plugins")]
    fn edit(&self, cx: &mut Cx<'_>, plugin: Option<&str>, edit: Option<Edit>) {
        if let (Some(plugin), Some(edit)) = (plugin, edit) {
            cx.plugin_request(Request::Setting {
                plugin: plugin.to_owned(),
                edit,
            });
        }
    }

    /// Takes the open plugin's form and widget trees into the page, where a
    /// drawing can borrow them.
    #[cfg(feature = "plugins")]
    fn keep_form(&mut self, cx: &Cx<'_>) {
        // Whatever was asked may have changed what the page shows.
        self.plugins.forget();
        let Some(id) = self.plugins.open.clone() else {
            self.plugins.form = None;
            self.plugins.trees.clear();
            return;
        };
        let view = self.plugins.view(&cx.client.app);
        let Some(plugin) = view.plugin(&id) else {
            self.plugins.form = None;
            self.plugins.trees.clear();
            return;
        };
        self.plugins.form = Some(plugin.settings.clone());
        self.plugins.trees = plugin.settings.widgets.clone();
        // The settings page's cells named by a setting's key show its value.
        self.plugins.local.bound = plugin
            .settings
            .values
            .0
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value_text(value)?)))
            .collect();
    }

    /// A cell of a plugin's drawing on its page was used. A widget setting's
    /// goes to the plugin; on its own settings page, a cell whose id is one
    /// of its settings' keys sets that setting (kept, and told to the plugin
    /// as a change), and any other goes to the plugin as a slot's would.
    #[cfg(feature = "plugins")]
    fn plugin_act(
        &mut self,
        cx: &mut Cx<'_>,
        plugin: Option<String>,
        key: Option<String>,
        act: crate::plugin_tree::Act,
    ) {
        let Some(plugin) = plugin else {
            return;
        };
        if !self.plugins.local.apply(&act) {
            return;
        }
        let (crate::plugin_tree::Act::Set { id, value }
        | crate::plugin_tree::Act::Edit { id, value }) = act
        else {
            return;
        };
        if let Some(key) = key {
            cx.plugin_request(Request::SettingWidget {
                plugin,
                key,
                id,
                value,
            });
            return;
        }
        if let Some(kind) = self.kind_of(&id) {
            let input = match kind {
                SettingKind::Bool { .. } => Input::Bool(value == "true"),
                _ => Input::Text(value),
            };
            self.edit(
                cx,
                Some(&plugin),
                Some(Edit::Set {
                    key: id,
                    value: input,
                }),
            );
            return;
        }
        cx.dispatch(yapfast_core::application::Action::PluginInput(
            yapfast_core::plugins::Input::Widget {
                plugin,
                slot: yapfast_core::plugins::slots::Slot::SettingsPage,
                id,
                value,
                subject: String::new(),
            },
        ));
    }

    #[cfg(feature = "plugins")]
    fn kind_of(&self, key: &str) -> Option<SettingKind> {
        let form = self.plugins.form.as_ref()?;
        Some(form.schema.setting(key)?.kind.clone())
    }

    #[cfg(feature = "plugins")]
    fn field_kind_of(&self, key: &str, field: &str) -> Option<SettingKind> {
        let form = self.plugins.form.as_ref()?;
        let SettingKind::List { fields, .. } = &form.schema.setting(key)?.kind else {
            return None;
        };
        fields
            .iter()
            .find(|spec| spec.key == field)
            .map(|spec| spec.kind.clone())
    }
}

/// The id of the input a plugin is told its settings page opened with: not a
/// cell's, which are plain names.
#[cfg(feature = "plugins")]
pub const OPENED: &str = "#opened";

/// A setting's value as a cell of the plugin's page shows it.
#[cfg(feature = "plugins")]
fn value_text(value: &Value) -> Option<String> {
    Some(match value {
        Value::Bool(on) => on.to_string(),
        Value::Text(text) => text.clone(),
        Value::Number(number) => trim(*number),
        Value::Integer(number) => number.to_string(),
        Value::List(_) => return None,
    })
}

/// The edit a submitted text field makes, by its address.
#[cfg(feature = "plugins")]
fn submit_edit(at: &str, text: String) -> Option<Edit> {
    let (key, rest) = at.split_once('/')?;
    match rest.split_once('/') {
        Some((row, field)) => Some(Edit::SetField {
            key: key.to_owned(),
            row: row.parse().ok()?,
            field: field.to_owned(),
            value: Input::Text(text),
        }),
        None => Some(Edit::Set {
            key: key.to_owned(),
            value: Input::Text(text),
        }),
    }
}

/// What a control gave, as the setting takes it: a select's index as its
/// option's value, a slider's position as its number.
#[cfg(feature = "plugins")]
pub fn input_of(kind: &SettingKind, value: &str) -> Input {
    match kind {
        SettingKind::Bool { .. } => Input::Bool(value == "true"),
        SettingKind::Select { options, .. } => {
            let chosen = value
                .parse::<usize>()
                .ok()
                .and_then(|index| options.get(index))
                .map_or_else(|| value.to_owned(), |option| option.value.clone());
            Input::Text(chosen)
        }
        _ => Input::Text(value.to_owned()),
    }
}

impl<'a> Page<'a> {
    #[cfg(not(feature = "plugins"))]
    pub(in crate::settings) fn plugins_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        form::page([form::card(
            theme,
            form::group([
                form::title(theme, "Plugins"),
                form::note(
                    theme,
                    "This build carries no plugin system: no plugin can be \
                     installed or run.",
                ),
            ]),
        )])
    }

    #[cfg(feature = "plugins")]
    pub(in crate::settings) fn plugins_page(self) -> Element<'a, Message> {
        if let Some(id) = self.s.plugins.open.clone() {
            if self.s.plugins.custom {
                return self.custom_page(&id);
            }
            return self.plugin_page(&id);
        }
        form::page(self.plugins_list())
    }

    #[cfg(feature = "plugins")]
    fn plugins_list(self) -> Vec<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let view = self.s.plugins.view(self.cx.app);
        let mut parts = vec![form::title(theme, "Plugins")];
        if let Some(line) = helper_line(theme, &view.helper) {
            parts.push(line);
        }
        parts.push(self.tabs());
        // An install or update asked from the marketplace or a plugin's row
        // shows its progress and review here, over whichever tab is open: the
        // person pressed a button on this tab and the answer is on it.
        if self.s.plugins.tab != Tab::FromUrl && !matches!(view.find, Find::Idle) {
            parts.push(form::card(
                theme,
                form::group(find_status(
                    theme,
                    &view.find,
                    self.s.plugins.powerful_confirmed,
                )),
            ));
        }
        match self.s.plugins.tab {
            Tab::Installed => parts.extend(self.installed_tab(theme, &view)),
            Tab::Marketplace => parts.extend(self.marketplace_tab(theme, &view)),
            Tab::FromUrl => parts.extend(self.url_tab(theme, &view.find)),
            Tab::Advanced => parts.extend(self.advanced(theme, &view.advanced)),
        }
        parts
    }

    #[cfg(feature = "plugins")]
    fn tabs(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let mut row = row![].spacing(8).align_y(Alignment::Center);
        for tab in Tab::ALL {
            let chosen = self.s.plugins.tab == tab;
            let ink = if chosen { theme.text } else { theme.muted };
            row = row.push(
                button(
                    container(text(tab.title()).size(14).font(BOLD).color(ink))
                        .padding([6, 10])
                        .style(move |_| {
                            if chosen {
                                crate::style::rounded(theme.raised, 6)
                            } else {
                                container::Style::default()
                            }
                        }),
                )
                .padding(0)
                .on_press(plugin_message(PluginEvent::Tab(tab)))
                .style(controls::ink_style(Color::TRANSPARENT, Color::TRANSPARENT)),
            );
        }
        row.into()
    }

    #[cfg(feature = "plugins")]
    fn installed_tab(self, theme: &Theme, view: &page::View) -> Vec<Element<'a, Message>> {
        if view.installed.is_empty() {
            return vec![form::note(
                theme,
                "No plugins are installed. A plugin adds to Yapfast's interface and \
                 to what it does, and installing one is asked about first.",
            )];
        }
        let look = self.cx.look;
        let slots = self.cx.state.plugin_slots.as_deref();
        let rows: Vec<Element<'a, Message>> = view
            .installed
            .iter()
            .map(|plugin| {
                let page = !plugin.settings.schema.settings.is_empty()
                    || slots.is_some_and(|slots| {
                        slots
                            .drawn_by(&plugin.id, yapfast_core::plugins::slots::Slot::SettingsPage)
                            .is_some()
                    });
                installed_row(look, plugin, page)
            })
            .collect();
        vec![form::card(theme, form::group(rows))]
    }

    #[cfg(feature = "plugins")]
    fn marketplace_tab(self, theme: &'a Theme, view: &page::View) -> Vec<Element<'a, Message>> {
        let mut parts = vec![
            form::field(
                theme,
                "plugin-market-search",
                "Search the marketplace",
                &self.s.plugins.search,
            )
            .on_input(|text| plugin_message(PluginEvent::Search(text)))
            .into(),
        ];
        match &view.catalog {
            Catalog::Unloaded | Catalog::Loading => {
                parts.push(form::note(theme, "Reading the marketplace..."));
            }
            Catalog::Failed(error) => {
                parts.push(form::note(
                    theme,
                    format!("The marketplace couldn't be read: {error}"),
                ));
            }
            Catalog::Ready(listings) => {
                let names = categories(listings);
                if !names.is_empty() {
                    parts.push(category_chips(theme, &names, self.s.plugins.category));
                }
                // The chip chosen is a place in the same list, so a catalog
                // read again can't point past it: a category the marketplace
                // no longer lists shows every one.
                let wanted = self
                    .s
                    .plugins
                    .category
                    .checked_sub(1)
                    .and_then(|index| names.get(index).copied());
                let query = &self.s.plugins.query;
                let matching: Vec<&Listing> = listings
                    .iter()
                    .filter(|listing| listing.kind == page::Kind::Plugin)
                    .filter(|listing| wanted.is_none_or(|name| listing.category == name))
                    .filter(|listing| listing.matches(query))
                    .collect();
                if matching.is_empty() {
                    parts.push(form::note(theme, "No plugin in the marketplace matches."));
                } else {
                    let rows: Vec<Element<'a, Message>> = matching
                        .into_iter()
                        .map(|listing| listing_row(theme, listing, view))
                        .collect();
                    parts.push(form::card(theme, form::group(rows)));
                }
            }
        }
        parts
    }

    #[cfg(feature = "plugins")]
    fn url_tab(self, theme: &Theme, find: &Find) -> Vec<Element<'a, Message>> {
        let mut parts = vec![
            form::note(
                theme,
                "Paste a git URL. Yapfast fetches it, checks what it asks for and \
                 shows you before anything is stored or run.",
            ),
            row![
                form::field(theme, "plugin-url", "https://…", &self.s.plugins.url)
                    .on_input(|text| plugin_message(PluginEvent::Url(text)))
                    .on_submit(plugin_message(PluginEvent::Find)),
                controls::action(
                    theme,
                    "Fetch",
                    ActionKind::Primary,
                    (!self.s.plugins.url.trim().is_empty())
                        .then(|| plugin_message(PluginEvent::Find)),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ];
        parts.extend(find_status(theme, find, self.s.plugins.powerful_confirmed));
        parts
    }

    #[cfg(feature = "plugins")]
    fn advanced(self, theme: &Theme, advanced: &Advanced) -> Vec<Element<'a, Message>> {
        let look = self.cx.look;
        let budgets = page::MEMORY_BUDGETS;
        let budget = advanced.memory_budget;
        let idle_minutes = advanced.idle_minutes;
        let current = budgets.iter().position(|&mib| mib == budget);
        let chosen = self.s.plugins.dropdown.as_deref() == Some("budget");
        let mut parts = vec![
            form::heading(theme, "LOCAL PLUGINS"),
            form::note(
                theme,
                "Install a plugin from its .wasm file, or choose a plugin repository \
                 for development. A plugin installed from a file starts turned off; \
                 a development folder runs and reloads when its manifest or plugin \
                 changes.",
            ),
            form::buttons([
                controls::action(
                    theme,
                    "Install a plugin",
                    ActionKind::Primary,
                    Some(plugin_message(PluginEvent::ChooseWasm)),
                )
                .into(),
                controls::action(
                    theme,
                    "Load development folder",
                    ActionKind::Plain,
                    Some(plugin_message(PluginEvent::ChooseFolder)),
                )
                .into(),
            ]),
            form::heading(theme, "POWERFUL PLUGINS"),
            form::beside(
                column![
                    text("Allow powerful plugins").size(14).color(theme.text),
                    text(
                        "Lets a plugin you allowed read a folder or your files, reach any \
                         website, serve apps on this computer, or send as you without a \
                         keystroke. Each still asks you the first time.",
                    )
                    .size(12)
                    .color(theme.danger),
                ]
                .spacing(2)
                .width(Length::Fill),
                form::toggle(
                    theme,
                    advanced.powerful,
                    Some(plugin_message(PluginEvent::SetPowerful(!advanced.powerful))),
                ),
            ),
            form::heading(theme, "DEVELOPER"),
            form::switch(
                "Plugin devtools",
                self.cx.layout.plugin_devtools,
                Some(plugin_message(PluginEvent::SetDevtools(
                    !self.cx.layout.plugin_devtools,
                ))),
            )
            .description(
                "Each plugin's page shows its console, the slots it draws in, the \
                 calls it makes with their timing and size, its processor time \
                 and Reload. Nothing is recorded while this is off.",
            )
            .view(look),
            form::heading(theme, "RUNTIME"),
        ];
        if advanced.sandbox_unavailable {
            parts.push(form::note(
                theme,
                "The plugin helper can't apply the operating system's sandbox here, \
                 so plugins run under wasmtime's alone.",
            ));
        }
        parts.push(
            form::beside(
                column![
                    text("Memory budget").size(14).color(theme.text),
                    text("The most plugins may hold together; the least recently used idle one goes first.")
                        .size(12)
                        .color(theme.muted),
                ]
                .spacing(2),
                form::dropdown(
                    budgets.iter().map(|mib| format!("{mib} MiB")).collect(),
                    current,
                    chosen,
                    plugin_message(PluginEvent::Dropdown(if chosen {
                        None
                    } else {
                        Some("budget".to_owned())
                    })),
                    move |index| {
                        plugin_message(PluginEvent::SetMemoryBudget(
                            budgets.get(index).copied().unwrap_or(budget),
                        ))
                    },
                )
                .width(Length::Fixed(140.0))
                .view(look),
            ),
        );
        let idle = page::IDLE_MINUTES;
        let current = idle.iter().position(|&minutes| minutes == idle_minutes);
        let chosen = self.s.plugins.dropdown.as_deref() == Some("idle");
        parts.push(form::beside(
            column![
                text("Idle eviction").size(14).color(theme.text),
                text("Minutes without a call before an idle plugin is evicted.")
                    .size(12)
                    .color(theme.muted),
            ]
            .spacing(2),
            form::dropdown(
                idle.iter()
                    .map(|minutes| format!("{minutes} min"))
                    .collect(),
                current,
                chosen,
                plugin_message(PluginEvent::Dropdown(if chosen {
                    None
                } else {
                    Some("idle".to_owned())
                })),
                move |index| {
                    plugin_message(PluginEvent::SetIdleMinutes(
                        idle.get(index).copied().unwrap_or(idle_minutes),
                    ))
                },
            )
            .width(Length::Fixed(140.0))
            .view(look),
        ));
        parts
    }

    /// One plugin's own page: what it is, what it may do, what it did, and
    /// its settings.
    #[cfg(feature = "plugins")]
    fn plugin_page(self, id: &str) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let view = self.s.plugins.view(self.cx.app);
        let Some(plugin) = view.plugin(id) else {
            return form::page([form::note(theme, "That plugin isn't installed.")]);
        };
        let mut parts: Vec<Element<'a, Message>> = Vec::new();
        parts.push(
            row![
                controls::action(
                    theme,
                    "Back",
                    ActionKind::Ghost,
                    Some(plugin_message(PluginEvent::Back))
                )
                .small(),
                text(plugin.name.clone())
                    .size(18)
                    .font(BOLD)
                    .color(theme.text),
                text(plugin.version.clone()).size(13).color(theme.muted),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
        );
        parts.push(form::note(
            theme,
            format!("From {}.", origin_text(plugin.origin)),
        ));
        if !plugin.description.is_empty() {
            parts.push(form::note(theme, plugin.description.clone()));
        }
        parts.push(self.plugin_actions(theme, plugin));
        if !plugin.capabilities.is_empty() || !plugin.grants.is_empty() {
            parts.push(form::heading(theme, "Permissions"));
            // The hosts it declared and those allowed in the popup are here
            // too, each with its Revoke, so "Don't ask again" can be taken back.
            let lines = plugin
                .permission_lines()
                .into_iter()
                .map(|line| permission_row(theme, plugin, line))
                .collect::<Vec<_>>();
            parts.push(form::card(theme, form::group(lines)));
        }
        // A local endpoint shows where it listens and the token an app sends,
        // which is what a person pastes into Claude or another local app.
        if let Some(endpoint) = yapfast_plugin_api::PluginId::parse(id)
            .ok()
            .and_then(|plugin_id| {
                self.cx
                    .app
                    .state
                    .plugins
                    .as_ref()
                    .and_then(|plugins| plugins.endpoint(&plugin_id))
            })
        {
            parts.push(form::heading(theme, "Local endpoint"));
            parts.push(form::card(
                theme,
                form::group([
                    text(format!(
                        "Listening on http://127.0.0.1:{}/ for apps on this computer only.",
                        endpoint.port
                    ))
                    .size(13)
                    .color(theme.text)
                    .into(),
                    text("Apps send the token as Authorization: Bearer <token>.")
                        .size(12)
                        .color(theme.muted)
                        .into(),
                    form::buttons([controls::action(
                        theme,
                        "Copy token",
                        ActionKind::Plain,
                        Some(plugin_message(PluginEvent::CopyToken(endpoint.token))),
                    )
                    .into()]),
                ]),
            ));
        }
        if !plugin.log.is_empty() {
            parts.push(form::heading(theme, "Recent log"));
            let lines: Vec<Element<'a, Message>> = plugin
                .log
                .iter()
                .rev()
                .take(20)
                .map(|line| {
                    let ink = if line.error {
                        theme.danger
                    } else {
                        theme.muted
                    };
                    text(format!("{} {}", line.at, line.text))
                        .size(12)
                        .color(ink)
                        .width(Length::Fill)
                        .into()
                })
                .collect();
            parts.push(form::card(
                theme,
                scrollable(form::group(lines)).height(Length::Fixed(200.0)),
            ));
        }
        parts.extend(self.looks(theme, plugin));
        if self.cx.layout.plugin_devtools {
            parts.extend(self.devtools(theme, plugin));
        }
        parts.extend(self.settings_form(theme));
        form::page(parts)
    }

    /// What the plugin changes in Yapfast's own interface, each change said
    /// in words, those refused with why, and the switch that turns them off
    /// while it keeps running.
    #[cfg(feature = "plugins")]
    fn looks(self, theme: &Theme, plugin: &Installed) -> Vec<Element<'a, Message>> {
        let Some(manifest) = self
            .cx
            .app
            .plugin_interfaces()
            .into_iter()
            .find(|manifest| manifest.id == plugin.id)
        else {
            return Vec::new();
        };
        let checked = yapfast_core::plugins::ui::check(&manifest);
        if checked.changes.is_empty() && checked.refused.is_empty() {
            return Vec::new();
        }
        let on = !self.cx.layout.plugin_looks_off.contains(&plugin.id);
        let mut lines: Vec<Element<'a, Message>> = vec![
            form::switch(
                "Apply its changes to Yapfast's look",
                on,
                Some(plugin_message(PluginEvent::SetLooks {
                    id: plugin.id.clone(),
                    on: !on,
                })),
            )
            .description(
                "Colours, sizes and hidden parts. Sign-in, security questions, \
                 dialogs that ask for your consent and this page always show \
                 Yapfast's own look.",
            )
            .view(self.cx.look),
        ];
        for change in &checked.changes {
            lines.push(
                text(change.words())
                    .size(13)
                    .color(if on { theme.text } else { theme.muted })
                    .into(),
            );
        }
        for refused in &checked.refused {
            lines.push(
                text(format!(
                    "Not applied: {}, as {}.",
                    refused.what, refused.why
                ))
                .size(13)
                .color(theme.muted)
                .into(),
            );
        }
        vec![
            form::heading(theme, "Changes to Yapfast's look"),
            form::card(theme, form::group(lines)),
        ]
    }

    /// The devtools on a plugin's page, while they are on: its console, the
    /// slots it draws in, the calls it made with their timing and size, its
    /// processor time and memory, and Reload.
    #[cfg(feature = "plugins")]
    fn devtools(self, theme: &'a Theme, plugin: &Installed) -> Vec<Element<'a, Message>> {
        let kept = yapfast_core::plugins::devtools::snapshot(&plugin.id).unwrap_or_default();
        let cpu = kept.cpu.map_or_else(
            || "no processor time reported yet".to_owned(),
            |cpu| format!("{:.1} ms of processor time", cpu.as_secs_f64() * 1000.0),
        );
        let memory = yapfast_core::plugins::page::memory(plugin.memory_pages).0;
        // The memory figure is the instance's ceiling, said as one: Extism
        // keeps an instance's store to itself, so what it uses right now
        // can't be read (docs/plugins/host.md).
        let usage = format!(
            "{cpu} \u{b7} {} host calls \u{b7} {} (its ceiling; what it uses now isn't reported)",
            kept.calls,
            memory.to_lowercase()
        );
        let mut parts = vec![
            form::heading(theme, "Devtools"),
            row![
                text(usage)
                    .size(13)
                    .color(theme.secondary)
                    .width(Length::Fill),
                controls::action(
                    theme,
                    "Reload",
                    ActionKind::Plain,
                    Some(plugin_message(PluginEvent::Restart(plugin.id.clone()))),
                )
                .small(),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ];
        let mut tabs = row![].spacing(4);
        for (tab, label) in [
            (DevTab::Console, "Console"),
            (DevTab::Slots, "Slots"),
            (DevTab::Calls, "Calls"),
        ] {
            let chosen = self.s.plugins.dev_tab == tab;
            let ink = if chosen { theme.text } else { theme.muted };
            tabs = tabs.push(
                button(
                    container(text(label).size(13).font(BOLD).color(ink))
                        .padding([5, 10])
                        .style(move |_| {
                            if chosen {
                                crate::style::rounded(theme.raised, 6)
                            } else {
                                container::Style::default()
                            }
                        }),
                )
                .padding(0)
                .on_press(plugin_message(PluginEvent::DevTab(tab)))
                .style(controls::ink_style(Color::TRANSPARENT, Color::TRANSPARENT)),
            );
        }
        parts.push(tabs.into());
        let mono = |words: String, ink: Color| -> Element<'a, Message> {
            text(words)
                .size(12)
                .font(iced::Font::MONOSPACE)
                .color(ink)
                .width(Length::Fill)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        };
        let lines: Vec<Element<'a, Message>> = match self.s.plugins.dev_tab {
            DevTab::Console => {
                if plugin.log.is_empty() {
                    vec![form::note(theme, "It hasn't logged anything.")]
                } else {
                    plugin
                        .log
                        .iter()
                        .map(|line| {
                            mono(
                                format!("{} {}", line.at, line.text),
                                if line.error { theme.danger } else { theme.text },
                            )
                        })
                        .collect()
                }
            }
            DevTab::Slots => {
                let mut lines = Vec::new();
                if let Some(slots) = self.cx.state.plugin_slots.as_deref() {
                    for drawn in slots.all_drawn() {
                        let Some(name) = slots.names().get(drawn.plugin) else {
                            continue;
                        };
                        if name.id != plugin.id {
                            continue;
                        }
                        let shown = slots
                            .drawn(drawn.slot)
                            .any(|shown| shown.plugin == drawn.plugin);
                        let cells = drawn.tree.cells();
                        let kinds: Vec<String> = cells
                            .iter()
                            .take(12)
                            .map(|cell| format!("{:?}", cell.kind).to_lowercase())
                            .collect();
                        lines.push(mono(
                            format!(
                                "{}  {} cells{}{}  {}",
                                drawn.slot.id(),
                                cells.len(),
                                drawn
                                    .tree
                                    .part
                                    .as_deref()
                                    .map(|part| format!(", part {part}"))
                                    .unwrap_or_default(),
                                if shown {
                                    ""
                                } else {
                                    ", waiting: the slot is full"
                                },
                                kinds.join(" "),
                            ),
                            theme.text,
                        ));
                    }
                }
                if lines.is_empty() {
                    lines.push(form::note(theme, "It draws in no slot now."));
                }
                lines
            }
            DevTab::Calls => {
                if kept.records.is_empty() {
                    vec![form::note(
                        theme,
                        "No calls since the devtools were turned on.",
                    )]
                } else {
                    kept.records
                        .iter()
                        .rev()
                        .map(|record| {
                            let at =
                                yapfast_core::clock::time_with(record.at_ms / 1000, false, true);
                            let took = record.took.map_or_else(
                                || "…".to_owned(),
                                |took| format!("{:.2} ms", took.as_secs_f64() * 1000.0),
                            );
                            let words = match record.way {
                                yapfast_core::plugins::devtools::Way::Call => format!(
                                    "{at}  \u{2192} {}  {} B sent, {} B back, {took}, {}",
                                    record.what,
                                    record.sent,
                                    record.answered.unwrap_or(0),
                                    record.outcome
                                ),
                                yapfast_core::plugins::devtools::Way::Handed => {
                                    format!("{at}  \u{2190} {}  {} B", record.what, record.sent)
                                }
                            };
                            let ink = match record.outcome {
                                "denied" | "error" => theme.danger,
                                "" => theme.muted,
                                _ => theme.text,
                            };
                            mono(words, ink)
                        })
                        .collect()
                }
            }
        };
        parts.push(form::card(
            theme,
            scrollable(container(form::group(lines)).padding([0, 8]))
                .anchor_bottom()
                .height(Length::Fixed(240.0)),
        ));
        parts
    }

    /// A plugin's own settings page: what it draws on it, its cells named by
    /// its settings' keys showing and saving their values, or its settings'
    /// form while it hasn't drawn one; and Reset to defaults.
    #[cfg(feature = "plugins")]
    fn custom_page(self, id: &str) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let view = self.s.plugins.view(self.cx.app);
        let Some(plugin) = view.plugin(id) else {
            return form::page([form::note(theme, "That plugin isn't installed.")]);
        };
        let mut parts: Vec<Element<'a, Message>> = vec![
            row![
                controls::action(
                    theme,
                    "Back",
                    ActionKind::Ghost,
                    Some(plugin_message(PluginEvent::Back))
                )
                .small(),
                text(format!("{} settings", plugin.name))
                    .size(18)
                    .font(BOLD)
                    .color(theme.text),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            // Whose page it is, so nothing on it passes for Yapfast's own.
            form::note(
                theme,
                format!("Drawn by the plugin {} ({}).", plugin.name, plugin.id),
            ),
        ];
        let drawn =
            self.cx.state.plugin_slots.as_deref().and_then(|slots| {
                slots.drawn_by(id, yapfast_core::plugins::slots::Slot::SettingsPage)
            });
        match drawn {
            Some(drawn) => {
                let page = plugin_tree::draw(
                    plugin_tree::Paint {
                        look: self.cx.look,
                        local: &self.s.plugins.local,
                        images: &self.cx.plugin_kept.images,
                        plugin: self.s.plugins.open.as_deref().unwrap_or_default(),
                    },
                    &drawn.tree,
                    |act| plugin_message(PluginEvent::Act { key: None, act }),
                );
                parts.push(form::card(theme, page));
                if let Some(form_state) = self.s.plugins.form.as_ref() {
                    for refused in form_state.refused.values() {
                        parts.push(form::error(theme, refused.message.clone()).into());
                    }
                }
            }
            None => {
                parts.push(form::note(
                    theme,
                    if plugin.enabled {
                        "The plugin draws its page while it runs. Until it does, \
                         these are its settings."
                    } else {
                        "Turn the plugin on for its own page. These are its settings."
                    },
                ));
                parts.extend(self.settings_form(theme));
                return form::page(parts);
            }
        }
        parts.push(self.reset_line(theme));
        form::page(parts)
    }

    /// Reset to defaults, which asks first: a list's rows go with it.
    #[cfg(feature = "plugins")]
    fn reset_line(self, theme: &Theme) -> Element<'a, Message> {
        if self.s.plugins.resetting {
            return form::group([
                form::note(
                    theme,
                    "Put every setting of this plugin back to its default? Lists lose their rows.",
                ),
                form::buttons([
                    controls::action(
                        theme,
                        "Reset",
                        ActionKind::Danger,
                        Some(plugin_message(PluginEvent::ResetSettings)),
                    )
                    .into(),
                    controls::action(
                        theme,
                        "Cancel",
                        ActionKind::Ghost,
                        Some(plugin_message(PluginEvent::AskReset(false))),
                    )
                    .into(),
                ]),
            ]);
        }
        controls::action(
            theme,
            "Reset to defaults",
            ActionKind::Ghost,
            Some(plugin_message(PluginEvent::AskReset(true))),
        )
        .small()
        .into()
    }

    #[cfg(feature = "plugins")]
    fn plugin_actions(self, theme: &Theme, plugin: &Installed) -> Element<'a, Message> {
        let id = plugin.id.clone();
        let mut buttons: Vec<Element<'a, Message>> = Vec::new();
        if suspended(plugin) {
            buttons.push(
                controls::action(
                    theme,
                    "Restart",
                    ActionKind::Primary,
                    Some(plugin_message(PluginEvent::Restart(id.clone()))),
                )
                .into(),
            );
        }
        if let Some(update) = &plugin.update {
            buttons.push(
                controls::action(
                    theme,
                    format!("Update to {}", update.version),
                    ActionKind::Primary,
                    Some(plugin_message(PluginEvent::Update(id.clone()))),
                )
                .into(),
            );
        }
        if plugin.previous.is_some() {
            buttons.push(
                controls::action(
                    theme,
                    "Roll back",
                    ActionKind::Plain,
                    Some(plugin_message(PluginEvent::RollBack(id.clone()))),
                )
                .into(),
            );
        }
        if self.s.plugins.uninstalling.as_deref() == Some(id.as_str()) {
            // Removing it is asked once more, with the choice of what it
            // stored: settings and storage stay for a reinstall unless the
            // person says otherwise.
            return form::group([
                form::note(
                    theme,
                    format!(
                        "Uninstall {}? Its settings and what it stored can stay, \
                         for if you install it again.",
                        plugin.name
                    ),
                ),
                form::buttons([
                    controls::action(
                        theme,
                        "Uninstall",
                        ActionKind::Danger,
                        Some(plugin_message(PluginEvent::Uninstall {
                            id: id.clone(),
                            delete_data: false,
                        })),
                    )
                    .into(),
                    controls::action(
                        theme,
                        "Uninstall and delete its data",
                        ActionKind::Danger,
                        Some(plugin_message(PluginEvent::Uninstall {
                            id,
                            delete_data: true,
                        })),
                    )
                    .into(),
                    controls::action(
                        theme,
                        "Cancel",
                        ActionKind::Ghost,
                        Some(plugin_message(PluginEvent::AskUninstall(None))),
                    )
                    .into(),
                ]),
            ]);
        }
        buttons.push(
            controls::action(
                theme,
                "Uninstall",
                ActionKind::Danger,
                Some(plugin_message(PluginEvent::AskUninstall(Some(id)))),
            )
            .into(),
        );
        form::buttons(buttons)
    }

    /// The open plugin's form: a row per setting it declares, the ones its
    /// own `show` rules hide left out.
    #[cfg(feature = "plugins")]
    fn settings_form(self, theme: &Theme) -> Vec<Element<'a, Message>> {
        let Some(form_state) = self.s.plugins.form.as_ref() else {
            return Vec::new();
        };
        let schema = &form_state.schema;
        if schema.settings.is_empty() {
            return Vec::new();
        }
        let states = schema.states(&form_state.values);
        let mut rows: Vec<Element<'a, Message>> = Vec::new();
        for (setting, state) in schema.settings.iter().zip(states) {
            if !state.shown {
                continue;
            }
            let value = form_state.values.get(&setting.key);
            let refused = form_state
                .refused
                .get(&address(&setting.key, None))
                .map(|refused: &Refused| refused.message.clone());
            rows.push(self.setting_row(theme, setting, value, state.enabled, refused, form_state));
        }
        let mut parts = vec![form::heading(theme, "Settings")];
        let reset = self.reset_line(theme);
        parts.push(form::card(theme, form::group([form::group(rows), reset])));
        parts
    }

    #[cfg(feature = "plugins")]
    fn setting_row(
        self,
        theme: &Theme,
        setting: &'a settings::Setting,
        value: Option<&'a Value>,
        enabled: bool,
        refused: Option<String>,
        form_state: &'a Form,
    ) -> Element<'a, Message> {
        let key = setting.key.clone();
        let look = self.cx.look;
        let mut rows = column![text(setting.label.clone()).size(14).color(theme.text),].spacing(4);
        if !setting.description.is_empty() {
            rows = rows.push(
                text(setting.description.clone())
                    .size(12)
                    .color(theme.muted),
            );
        }
        let control: Element<'a, Message> = match &setting.kind {
            SettingKind::Bool { .. } => {
                let on = matches!(value, Some(Value::Bool(true)));
                let next = if on { "false" } else { "true" }.to_owned();
                form::toggle(
                    theme,
                    on,
                    enabled.then(|| {
                        plugin_message(PluginEvent::Set {
                            key: key.clone(),
                            value: next,
                        })
                    }),
                )
            }
            SettingKind::String(spec) => {
                let text_value = self.draft_or(&key, value);
                form::field(
                    theme,
                    format!("plugin-setting-{key}"),
                    &spec.placeholder,
                    text_value,
                )
                .on_input({
                    let at = key.clone();
                    move |text| {
                        plugin_message(PluginEvent::Draft {
                            at: at.clone(),
                            text,
                        })
                    }
                })
                .on_submit(plugin_message(PluginEvent::Submit(key.clone())))
                .into()
            }
            SettingKind::Select { options, .. } => {
                let current = match value {
                    Some(Value::Text(chosen)) => options.iter().position(|o| &o.value == chosen),
                    _ => None,
                };
                let labelled: Vec<String> = options.iter().map(|o| o.label.clone()).collect();
                let at = format!("select:{key}");
                let open = self.s.plugins.dropdown.as_deref() == Some(at.as_str());
                form::dropdown(
                    labelled,
                    current,
                    open,
                    plugin_message(PluginEvent::Dropdown(if open { None } else { Some(at) })),
                    {
                        let key = key.clone();
                        move |index| {
                            plugin_message(PluginEvent::Set {
                                key: key.clone(),
                                value: index.to_string(),
                            })
                        }
                    },
                )
                .enabled(enabled)
                .view(look)
            }
            SettingKind::Slider(range) | SettingKind::Number(range) => {
                let number = match value {
                    Some(Value::Number(number)) => *number,
                    _ => range.default,
                };
                let (min, max) = (range.min.unwrap_or(0.0), range.max.unwrap_or(100.0));
                if matches!(setting.kind, SettingKind::Slider(_)) {
                    let span = (max - min).max(f64::EPSILON);
                    let position = ((number - min) / span).clamp(0.0, 1.0);
                    const STEPS: u32 = 1000;
                    let current = (position * f64::from(STEPS)).round() as u32;
                    controls::slider(setting.label.clone(), current, STEPS, {
                        let key = key.clone();
                        move |chosen| {
                            let value = min + span * f64::from(chosen) / f64::from(STEPS);
                            plugin_message(PluginEvent::Set {
                                key: key.clone(),
                                value: trim(value),
                            })
                        }
                    })
                    .view(theme)
                } else {
                    let at = key.clone();
                    let shown = self.draft_or(&key, value);
                    form::field(theme, format!("plugin-setting-{key}"), "", shown)
                        .on_input({
                            let at = at.clone();
                            move |text| {
                                plugin_message(PluginEvent::Draft {
                                    at: at.clone(),
                                    text,
                                })
                            }
                        })
                        .on_submit(plugin_message(PluginEvent::Submit(at)))
                        .into()
                }
            }
            SettingKind::Integer { .. } => {
                let at = key.clone();
                let shown = self.draft_or(&key, value);
                form::field(theme, format!("plugin-setting-{key}"), "", shown)
                    .on_input({
                        let at = at.clone();
                        move |text| {
                            plugin_message(PluginEvent::Draft {
                                at: at.clone(),
                                text,
                            })
                        }
                    })
                    .on_submit(plugin_message(PluginEvent::Submit(at)))
                    .into()
            }
            SettingKind::List {
                fields, max_rows, ..
            } => {
                let records = match value {
                    Some(Value::List(records)) => records,
                    _ => return form::card(theme, rows),
                };
                let mut lines: Vec<Element<'a, Message>> = Vec::new();
                for (index, record) in records.iter().enumerate() {
                    let mut line = row![].spacing(8).align_y(Alignment::Center);
                    for field in fields {
                        let cell = record.get(&field.key);
                        line = line.push(self.list_field(theme, &key, index, field, cell, enabled));
                    }
                    line = line.push(Space::new().width(Length::Fill));
                    let remove: Element<'a, Message> = controls::action(
                        theme,
                        "Remove",
                        ActionKind::Ghost,
                        enabled.then(|| {
                            plugin_message(PluginEvent::RemoveRow {
                                key: key.clone(),
                                row: index,
                            })
                        }),
                    )
                    .small()
                    .into();
                    line = line.push(remove);
                    if index > 0 {
                        let up: Element<'a, Message> = controls::action(
                            theme,
                            "Up",
                            ActionKind::Ghost,
                            enabled.then(|| {
                                plugin_message(PluginEvent::MoveRow {
                                    key: key.clone(),
                                    row: index,
                                    to: index as i32 - 1,
                                })
                            }),
                        )
                        .small()
                        .into();
                        line = line.push(up);
                    }
                    if index + 1 < records.len() {
                        let down: Element<'a, Message> = controls::action(
                            theme,
                            "Down",
                            ActionKind::Ghost,
                            enabled.then(|| {
                                plugin_message(PluginEvent::MoveRow {
                                    key: key.clone(),
                                    row: index,
                                    to: index as i32 + 1,
                                })
                            }),
                        )
                        .small()
                        .into();
                        line = line.push(down);
                    }
                    lines.push(line.into());
                }
                if enabled && records.len() < *max_rows {
                    let add: Element<'a, Message> = controls::action(
                        theme,
                        "Add",
                        ActionKind::Plain,
                        Some(plugin_message(PluginEvent::AddRow(key.clone()))),
                    )
                    .small()
                    .into();
                    lines.push(add);
                }
                rows = rows.push(column(lines).spacing(6));
                return form::card(theme, rows);
            }
            SettingKind::Widget => {
                let tree = form_state
                    .widgets
                    .get(&key)
                    .or_else(|| self.s.plugins.trees.get(&key));
                match tree {
                    Some(tree) => plugin_tree::draw(
                        plugin_tree::Paint {
                            look: self.cx.look,
                            local: &self.s.plugins.local,
                            images: &self.cx.plugin_kept.images,
                            plugin: self.s.plugins.open.as_deref().unwrap_or_default(),
                        },
                        tree,
                        {
                            let key = key.clone();
                            move |act| {
                                plugin_message(PluginEvent::Act {
                                    key: Some(key.clone()),
                                    act,
                                })
                            }
                        },
                    ),
                    None => form::note(theme, "The plugin hasn't drawn this yet."),
                }
            }
        };
        rows = rows.push(container(control).width(Length::Fill));
        if let Some(refused) = refused {
            rows = rows.push(text(refused).size(12).color(theme.danger));
        }
        form::group([rows.into()])
    }

    /// One field of a list's row.
    #[cfg(feature = "plugins")]
    fn list_field(
        self,
        theme: &Theme,
        key: &str,
        row: usize,
        field: &'a settings::Field,
        value: Option<&'a Value>,
        enabled: bool,
    ) -> Element<'a, Message> {
        let at = address(key, Some((row, &field.key)));
        let look = self.cx.look;
        let label = field.label.clone();
        let control: Element<'a, Message> = match &field.kind {
            SettingKind::Bool { .. } => {
                let on = matches!(value, Some(Value::Bool(true)));
                let key = key.to_owned();
                let field_key = field.key.clone();
                form::switch(
                    label,
                    on,
                    enabled.then(|| {
                        plugin_message(PluginEvent::SetField {
                            key,
                            row,
                            field: field_key,
                            value: if on { "false" } else { "true" }.to_owned(),
                        })
                    }),
                )
                .view(look)
            }
            SettingKind::Select { options, .. } => {
                let current = match value {
                    Some(Value::Text(chosen)) => options.iter().position(|o| &o.value == chosen),
                    _ => None,
                };
                let labelled: Vec<String> = options.iter().map(|o| o.label.clone()).collect();
                let open_at = format!("select:{at}");
                let open = self.s.plugins.dropdown.as_deref() == Some(open_at.as_str());
                let (key, field_key) = (key.to_owned(), field.key.clone());
                form::dropdown(
                    labelled,
                    current,
                    open,
                    plugin_message(PluginEvent::Dropdown(if open {
                        None
                    } else {
                        Some(open_at)
                    })),
                    move |index| {
                        plugin_message(PluginEvent::SetField {
                            key: key.clone(),
                            row,
                            field: field_key.clone(),
                            value: index.to_string(),
                        })
                    },
                )
                .width(Length::Fixed(160.0))
                .enabled(enabled)
                .view(look)
            }
            _ => {
                let shown = self.draft_or(&at, value);
                let submitted = at.clone();
                form::field(theme, format!("plugin-list-{at}"), &field.label, shown)
                    .on_input(move |text| {
                        plugin_message(PluginEvent::Draft {
                            at: at.clone(),
                            text,
                        })
                    })
                    .on_submit(plugin_message(PluginEvent::Submit(submitted)))
                    .width(Length::Fixed(160.0))
                    .into()
            }
        };
        container(control).into()
    }

    /// What a field shows: what the person has typed, or the value itself.
    #[cfg(feature = "plugins")]
    fn draft_or(&self, at: &str, value: Option<&'a Value>) -> &'a str {
        if let Some(text) = self.s.plugins.drafts.get(at) {
            return text.as_str();
        }
        match value {
            Some(Value::Text(text)) => text.as_str(),
            Some(Value::Number(number)) => {
                // A borrowed rendering would need a place to live; the draft
                // is taken instead when the field is first used.
                let _ = number;
                ""
            }
            Some(Value::Integer(number)) => {
                let _ = number;
                ""
            }
            Some(Value::Bool(on)) => {
                if *on {
                    "true"
                } else {
                    "false"
                }
            }
            _ => "",
        }
    }
}

/// What the helper process is doing.
#[cfg(feature = "plugins")]
fn helper_line<'a>(theme: &Theme, helper: &Helper) -> Option<Element<'a, Message>> {
    let words = match helper {
        Helper::Running { memory } => match memory {
            Some(used) => format!(
                "The plugin helper is running, using {}.",
                page::bytes(*used)
            ),
            None => "The plugin helper is running.".to_owned(),
        },
        Helper::Stopped => return None,
        Helper::Missing => {
            "The yapfast-plugins helper isn't beside Yapfast, so no plugin can run here.".to_owned()
        }
        Helper::Crashed => {
            "The helper crashed three times in a minute, so plugins stay off until Yapfast restarts."
                .to_owned()
        }
    };
    Some(form::note(theme, words))
}

/// One installed plugin: the switch that runs it, under its name and what it
/// does; what it's doing; and its Details, its own Settings page when it has
/// one, and Restart after it was suspended.
#[cfg(feature = "plugins")]
fn installed_row<'a>(
    look: crate::look::Look<'a>,
    plugin: &Installed,
    page: bool,
) -> Element<'a, Message> {
    let theme = look.theme;
    let switch = form::switch(
        plugin.name.clone(),
        plugin.enabled,
        Some(plugin_message(PluginEvent::SetEnabled {
            id: plugin.id.clone(),
            enabled: !plugin.enabled,
        })),
    )
    .description(plugin.description.clone())
    .view(look);
    let status = plugin.status.text();
    let mut facts = column![
        text(format!("Version {}", plugin.version))
            .size(12)
            .color(theme.muted)
    ]
    .spacing(2);
    // A plugin with a powerful permission is marked wherever it is listed, in
    // the danger tone, for as long as it is installed.
    if !powerful_of(&plugin.capabilities).is_empty() {
        facts = facts.push(text("POWERFUL").size(11).font(BOLD).color(theme.danger));
    }
    if !status.is_empty() {
        facts = facts.push(
            text(status)
                .size(12)
                .color(tone_colour(theme, plugin.status.tone())),
        );
    }
    if let Some(update) = &plugin.update {
        facts = facts.push(
            text(format!("Version {} is available.", update.version))
                .size(12)
                .color(theme.accent),
        );
    }
    let mut buttons: Vec<Element<'a, Message>> = Vec::new();
    if suspended(plugin) {
        buttons.push(
            controls::action(
                theme,
                "Restart",
                ActionKind::Primary,
                Some(plugin_message(PluginEvent::Restart(plugin.id.clone()))),
            )
            .small()
            .into(),
        );
    }
    if page {
        buttons.push(
            controls::action(
                theme,
                "Settings",
                ActionKind::Plain,
                Some(plugin_message(PluginEvent::OpenSettings(plugin.id.clone()))),
            )
            .small()
            .into(),
        );
    }
    buttons.push(
        controls::action(
            theme,
            "Details",
            ActionKind::Ghost,
            Some(plugin_message(PluginEvent::Open(plugin.id.clone()))),
        )
        .small()
        .into(),
    );
    column![
        switch,
        row![
            container(facts).width(Length::Fill).clip(true),
            row(buttons).spacing(8).align_y(Alignment::Center),
        ]
        .spacing(form::GAP)
        .align_y(Alignment::Center),
    ]
    .spacing(6)
    .into()
}

/// Whether a plugin is stopped for this session after its failures, which
/// Restart undoes.
#[cfg(feature = "plugins")]
fn suspended(plugin: &Installed) -> bool {
    matches!(plugin.status, page::Status::Suspended(_))
}

/// The marketplace's categories, as its chips list them: those its plugin
/// listings name, in the order they first appear, so a chip's index keeps the
/// rows it stands for. A listing that names none shows under All.
#[cfg(feature = "plugins")]
fn categories(listings: &[Listing]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in listings
        .iter()
        .filter(|listing| listing.kind == page::Kind::Plugin)
        .map(|listing| listing.category.trim())
        .filter(|name| !name.is_empty())
    {
        if !names.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
            names.push(name);
        }
    }
    names
}

/// The marketplace's chips: All first, then a category each, the chosen one
/// lit. A choice past the categories shows All, as the list does.
#[cfg(feature = "plugins")]
fn category_chips<'a>(theme: &'a Theme, names: &[&str], chosen: usize) -> Element<'a, Message> {
    let chosen = if chosen > names.len() { 0 } else { chosen };
    let mut chips = row![].spacing(6).align_y(Alignment::Center);
    for index in 0..names.len() + 1 {
        let lit = index == chosen;
        let ink = if lit { theme.text } else { theme.muted };
        let label = index.checked_sub(1).map_or("All", |at| names[at]);
        chips = chips.push(
            button(
                container(text(label.to_owned()).size(13).color(ink))
                    .padding([4, 8])
                    .style(move |_| {
                        if lit {
                            crate::style::rounded(theme.raised, 6)
                        } else {
                            container::Style::default()
                        }
                    }),
            )
            .padding(0)
            .on_press(plugin_message(PluginEvent::Category(index)))
            .style(controls::ink_style(Color::TRANSPARENT, Color::TRANSPARENT)),
        );
    }
    chips.into()
}

/// A marketplace entry: what it is and what it reaches.
#[cfg(feature = "plugins")]
fn listing_row<'a>(theme: &Theme, listing: &Listing, view: &page::View) -> Element<'a, Message> {
    let installed = view.plugin(&listing.id).is_some();
    let mut lines = column![
        row![
            text(listing.name.clone())
                .size(15)
                .font(BOLD)
                .color(theme.text),
            text(format!(" {}", listing.version))
                .size(12)
                .color(theme.muted),
            text(format!(" by {}", listing.author))
                .size(12)
                .color(theme.muted),
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    if !listing.description.is_empty() {
        lines = lines.push(
            text(listing.description.clone())
                .size(13)
                .color(theme.muted),
        );
    }
    lines = lines.push(text(listing.reach()).size(12).color(theme.muted));
    let action = if installed {
        // An installed listing opens the plugin's own page, where it is
        // updated, rolled back or removed.
        controls::action(
            theme,
            "Installed",
            ActionKind::Plain,
            Some(plugin_message(PluginEvent::Open(listing.id.clone()))),
        )
        .small()
    } else {
        controls::action(
            theme,
            "Install",
            ActionKind::Primary,
            Some(plugin_message(PluginEvent::Install(listing.id.clone()))),
        )
        .small()
    };
    row![
        container(lines).width(Length::Fill).clip(true),
        Space::new().width(form::GAP),
        action,
    ]
    .align_y(Alignment::Center)
    .into()
}

/// What an install is doing, from fetching to its review.
#[cfg(feature = "plugins")]
fn find_status<'a>(theme: &Theme, find: &Find, confirmed: bool) -> Vec<Element<'a, Message>> {
    match find {
        Find::Idle => Vec::new(),
        Find::Fetching { url, step } => {
            vec![form::note(theme, format!("{url}\n{}", step_text(*step)))]
        }
        Find::Failed { url, error } => vec![form::note(
            theme,
            format!("{url}\n{}", fetch_error_text(error)),
        )],
        Find::Installing { name } => vec![form::note(theme, format!("Installing {name}..."))],
        Find::Review(review) => {
            let mut parts = vec![form::heading(theme, review.name.clone())];
            parts.push(form::note(
                theme,
                if review.reviewed {
                    format!("From {}, reviewed by its maintainers.", review.source)
                } else {
                    format!("From {}, an unreviewed source.", review.source)
                },
            ));
            let mut about = vec![format!("Version {}", review.version)];
            if let Some(replaces) = &review.replaces {
                about.push(format!("replacing {replaces}"));
            }
            if !review.commit.is_empty() {
                let short: String = review.commit.chars().take(12).collect();
                about.push(format!("at commit {short}"));
            }
            if !review.path.is_empty() {
                about.push(format!("from its folder {}", review.path));
            }
            let about = format!("{}.", about.join(", "));
            parts.push(form::note(theme, about));
            if !review.dependencies.is_empty() {
                let needs: Vec<String> = review
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        format!(
                            "{} {}{}",
                            dependency.name,
                            dependency.version,
                            if dependency.installed {
                                ""
                            } else {
                                " (not installed)"
                            }
                        )
                    })
                    .collect();
                parts.push(form::note(theme, format!("Needs {}.", needs.join(", "))));
            }
            let mut lines = page::permission_lines(&review.capabilities, &review.network)
                .into_iter()
                .map(|line| {
                    text(line.text)
                        .size(13)
                        .color(tone_colour(theme, line.tone))
                        .into()
                })
                .collect::<Vec<Element<'a, Message>>>();
            // How much memory it may hold, in the warning tone above the usual
            // cap, and that it holds messages back when it can't check them.
            let (memory, tone) = page::memory(review.memory_pages);
            lines.push(text(memory).size(13).color(tone_colour(theme, tone)).into());
            // What its package carries, so a plugin's pictures and sounds are
            // named before they can show or play.
            if let Some(assets) = &review.assets {
                lines.push(
                    text(assets.clone())
                        .size(13)
                        .color(tone_colour(theme, Tone::Normal))
                        .into(),
                );
            }
            if review.blocks {
                lines.push(
                    text(page::HOLDS_MESSAGES)
                        .size(13)
                        .color(tone_colour(theme, Tone::Warning))
                        .into(),
                );
            }
            parts.push(form::card(theme, form::group(lines)));
            // A powerful permission is said in red, a sentence each, and the
            // plugin installs only once the person ticks that they understand.
            let powerful = !review.powerful.is_empty();
            if powerful {
                let danger = theme.danger;
                let mut warning = column![
                    text("This plugin asks for powerful permissions")
                        .size(15)
                        .font(BOLD)
                        .color(danger),
                ]
                .spacing(6);
                for sentence in &review.powerful {
                    warning =
                        warning.push(text(sentence.clone()).size(13).font(BOLD).color(theme.text));
                }
                warning = warning.push(
                    text(
                        "They reach past the sandbox every other plugin runs in. They work \
                         only while powerful plugins are on in Advanced, and each asks you \
                         again the first time it is used. Install it only if you trust whoever \
                         wrote it.",
                    )
                    .size(12)
                    .color(theme.secondary),
                );
                warning = warning.push(
                    row![
                        form::toggle(
                            theme,
                            confirmed,
                            Some(plugin_message(PluginEvent::ConfirmPowerful(!confirmed))),
                        ),
                        text("I understand what this plugin will be able to do")
                            .size(13)
                            .color(theme.text),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
                parts.push(
                    container(warning)
                        .padding(12)
                        .width(Length::Fill)
                        .style(move |_| {
                            iced::widget::container::Style::default()
                                .background(iced::Color { a: 0.12, ..danger })
                                .border(iced::Border {
                                    color: danger,
                                    width: 1.0,
                                    radius: 8.0.into(),
                                })
                        })
                        .into(),
                );
            }
            parts.push(
                row![
                    controls::action(
                        theme,
                        "Install",
                        if powerful {
                            ActionKind::Danger
                        } else {
                            ActionKind::Primary
                        },
                        (!powerful || confirmed).then(|| plugin_message(PluginEvent::Confirm)),
                    ),
                    controls::action(
                        theme,
                        "Cancel",
                        ActionKind::Ghost,
                        Some(plugin_message(PluginEvent::Cancel)),
                    ),
                ]
                .spacing(8)
                .into(),
            );
            parts
        }
    }
}

#[cfg(feature = "plugins")]
fn step_text(step: Step) -> &'static str {
    step.text()
}

#[cfg(feature = "plugins")]
fn fetch_error_text(error: &FetchError) -> String {
    error.text()
}

/// One permission, and what revoking it takes away.
#[cfg(feature = "plugins")]
fn permission_row<'a>(theme: &Theme, plugin: &Installed, line: page::Line) -> Element<'a, Message> {
    let id = plugin.id.clone();
    let mut row = row![
        text(line.text)
            .size(13)
            .color(tone_colour(theme, line.tone))
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if let Some(permission) = line.permission {
        let revoke: Element<'a, Message> = controls::action(
            theme,
            "Revoke",
            ActionKind::Ghost,
            Some(plugin_message(PluginEvent::Revoke { id, permission })),
        )
        .small()
        .into();
        row = row.push(revoke);
    }
    row.into()
}

/// A status tone as the theme's colour.
#[cfg(feature = "plugins")]
fn tone_colour(theme: &Theme, tone: Tone) -> iced::Color {
    match tone {
        Tone::Normal | Tone::Strong => theme.text,
        Tone::Muted => theme.muted,
        Tone::Success => theme.live,
        Tone::Warning => theme.loud,
        Tone::Danger => theme.danger,
    }
}

/// A number as a plugin reads it: no trailing zeros.
#[cfg_attr(
    not(feature = "plugins"),
    allow(
        dead_code,
        reason = "a build without the plugin system draws no plugin setting"
    )
)]
fn trim(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Where a plugin was installed from.
#[cfg(feature = "plugins")]
pub fn origin_text(origin: Origin) -> &'static str {
    match origin {
        Origin::Marketplace => "the marketplace",
        Origin::Url => "a URL",
        Origin::Folder => "a folder",
    }
}

#[cfg(all(test, feature = "plugins"))]
mod tests {
    use super::*;

    fn listing(kind: page::Kind, category: &str) -> Listing {
        Listing {
            kind,
            category: category.to_owned(),
            ..Listing::default()
        }
    }

    /// A chip stands for a category, so the marketplace's list is one entry
    /// per category named, in the order they first appear, and none for a
    /// listing that names none or is a theme.
    #[test]
    fn the_marketplaces_chips_are_the_plugins_categories_once_each_as_they_first_appear() {
        let listings = [
            listing(page::Kind::Plugin, "Chat"),
            listing(page::Kind::Plugin, ""),
            listing(page::Kind::Theme, "Appearance"),
            listing(page::Kind::Plugin, "Tools"),
            listing(page::Kind::Plugin, " Chat "),
        ];
        assert_eq!(categories(&listings), ["Chat", "Tools"]);
    }

    /// A plugin's row has the kit's switch under its name, Details, its own
    /// Settings when it has a page, and Restart once it was suspended.
    #[test]
    fn a_suspended_plugin_offers_restart_and_its_page() {
        let (app, _data) = crate::tests::preview();
        let plugin = Installed {
            id: "com.example.hog".into(),
            name: "Hog".into(),
            version: "1.0.0".into(),
            enabled: true,
            status: page::Status::Suspended(page::Suspension::OutOfMemory),
            ..Installed::default()
        };
        let finds = |element: Element<'_, Message>, words: &str| {
            let mut ui = iced_test::Simulator::with_size(
                iced::Settings::default(),
                iced::Size::new(700.0, 200.0),
                element,
            );
            ui.find(words).is_ok()
        };
        let row = || installed_row(app.look(), &plugin, true);
        for words in ["Hog", "Restart", "Settings", "Details", "Version 1.0.0"] {
            assert!(finds(row(), words), "{words}");
        }
        let running = Installed {
            status: page::Status::Idle,
            ..plugin.clone()
        };
        let row = installed_row(app.look(), &running, false);
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            iced::Size::new(700.0, 200.0),
            row,
        );
        assert!(ui.find("Restart").is_err(), "only after a suspension");
        assert!(ui.find("Settings").is_err(), "only with a page");
    }
}
