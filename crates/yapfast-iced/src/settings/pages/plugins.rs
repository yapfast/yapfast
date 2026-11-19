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
    Alignment, Color, Element, Length,
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

/// Which of the page's three lists shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Installed,
    Marketplace,
    FromUrl,
}

impl Tab {
    #[cfg_attr(
        not(feature = "plugins"),
        allow(dead_code, reason = "a build without the plugin system draws no tab")
    )]
    const ALL: [Tab; 3] = [Tab::Installed, Tab::Marketplace, Tab::FromUrl];

    #[cfg_attr(
        not(feature = "plugins"),
        allow(dead_code, reason = "a build without the plugin system draws no tab")
    )]
    fn title(self) -> &'static str {
        match self {
            Tab::Installed => "Installed",
            Tab::Marketplace => "Marketplace",
            Tab::FromUrl => "From a URL",
        }
    }
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
    /// What a widget setting's input cells hold, by cell id.
    pub cells: BTreeMap<String, String>,
    /// The open plugin's form, and the trees its widget settings draw. They
    /// are kept here so a drawing can borrow them.
    #[cfg(feature = "plugins")]
    pub form: Option<Form>,
    #[cfg(feature = "plugins")]
    pub trees: BTreeMap<String, Arc<yapfast_core::plugins::ui::Tree>>,
    /// Whether the marketplace was asked for since the page opened.
    pub asked: bool,
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
    /// A widget setting's cell was used.
    SettingWidget {
        key: String,
        id: String,
        value: String,
    },
    Install(String),
    Find,
    Confirm,
    Cancel,
    Uninstall {
        id: String,
        delete_data: bool,
    },
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
                    self.plugins.open = Some(id);
                    self.plugins.drafts.clear();
                    self.plugins.cells.clear();
                    self.plugins.dropdown = None;
                }
                PluginEvent::Back => {
                    self.plugins.open = None;
                    self.plugins.drafts.clear();
                    self.plugins.cells.clear();
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
                    self.edit(cx, selected.as_deref(), Some(Edit::Reset));
                }
                PluginEvent::SettingWidget { key, id, value } => {
                    if let Some(plugin) = selected {
                        cx.plugin_request(Request::SettingWidget {
                            plugin,
                            key,
                            id,
                            value,
                        });
                    }
                }
                PluginEvent::Install(plugin) => cx.plugin_request(Request::Install { plugin }),
                PluginEvent::Find => {
                    let url = self.plugins.url.trim().to_owned();
                    if !url.is_empty() {
                        cx.plugin_request(Request::Find { url });
                    }
                }
                PluginEvent::Confirm => {
                    cx.plugin_request(Request::Confirm);
                    if matches!(cx.client.app.plugins_page().find, Find::Idle) {
                        self.plugins.url.clear();
                        self.plugins.tab = Tab::Installed;
                    }
                }
                PluginEvent::Cancel => cx.plugin_request(Request::Cancel),
                PluginEvent::Uninstall { id, delete_data } => {
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
        let Some(id) = self.plugins.open.clone() else {
            self.plugins.form = None;
            self.plugins.trees.clear();
            return;
        };
        let view = cx.client.app.plugins_page();
        let Some(plugin) = view.plugin(&id) else {
            self.plugins.form = None;
            self.plugins.trees.clear();
            return;
        };
        self.plugins.form = Some(plugin.settings.clone());
        self.plugins.trees = plugin.settings.widgets.clone();
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
                form::heading(theme, "Plugins"),
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
            return self.plugin_page(&id);
        }
        form::page(self.plugins_list())
    }

    #[cfg(feature = "plugins")]
    fn plugins_list(self) -> Vec<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let view = self.cx.app.plugins_page();
        let mut parts = vec![form::heading(theme, "Plugins")];
        if let Some(line) = helper_line(theme, &view.helper) {
            parts.push(line);
        }
        parts.push(self.tabs());
        match self.s.plugins.tab {
            Tab::Installed => parts.extend(self.installed_tab(theme, &view)),
            Tab::Marketplace => parts.extend(self.marketplace_tab(theme, &view)),
            Tab::FromUrl => parts.extend(self.url_tab(theme, &view.find)),
        }
        parts.extend(self.advanced(theme, &view.advanced));
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
        let rows: Vec<Element<'a, Message>> = view
            .installed
            .iter()
            .map(|plugin| installed_row(theme, plugin))
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
        parts.extend(find_status(theme, find));
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
        let mut parts = vec![form::heading(theme, "Advanced")];
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
        let view = self.cx.app.plugins_page();
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
            let lines = page::permission_lines(&plugin.capabilities, &[])
                .into_iter()
                .map(|line| permission_row(theme, plugin, line))
                .collect::<Vec<_>>();
            parts.push(form::card(theme, form::group(lines)));
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
        parts.extend(self.settings_form(theme));
        form::page(parts)
    }

    #[cfg(feature = "plugins")]
    fn plugin_actions(self, theme: &Theme, plugin: &Installed) -> Element<'a, Message> {
        let id = plugin.id.clone();
        let mut buttons: Vec<Element<'a, Message>> = Vec::new();
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
        buttons.push(
            controls::action(
                theme,
                "Uninstall",
                ActionKind::Danger,
                Some(plugin_message(PluginEvent::Uninstall {
                    id,
                    delete_data: false,
                })),
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
        let reset: Element<'a, Message> = controls::action(
            theme,
            "Reset to defaults",
            ActionKind::Ghost,
            Some(plugin_message(PluginEvent::ResetSettings)),
        )
        .small()
        .into();
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
                    Some(tree) => plugin_tree::draw(theme, tree, &self.s.plugins.cells, {
                        let key = key.clone();
                        move |act| {
                            let (id, value) = match act {
                                plugin_tree::Act::Set { id, value }
                                | plugin_tree::Act::Edit { id, value } => (id, value),
                            };
                            plugin_message(PluginEvent::SettingWidget {
                                key: key.clone(),
                                id,
                                value,
                            })
                        }
                    }),
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
        Helper::Stopped => {
            "The plugin helper isn't running: it starts with the first plugin that runs.".to_owned()
        }
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

/// One installed plugin: what it is, what it's doing, and the switch that
/// runs it.
#[cfg(feature = "plugins")]
fn installed_row<'a>(theme: &Theme, plugin: &Installed) -> Element<'a, Message> {
    let mut lines = column![
        row![
            text(plugin.name.clone())
                .size(15)
                .font(BOLD)
                .color(theme.text),
            text(format!(" {}", plugin.version))
                .size(12)
                .color(theme.muted),
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    if !plugin.description.is_empty() {
        lines = lines.push(text(plugin.description.clone()).size(13).color(theme.muted));
    }
    let status = plugin.status.text();
    if !status.is_empty() {
        lines = lines.push(
            text(status)
                .size(12)
                .color(tone_colour(theme, plugin.status.tone())),
        );
    }
    if let Some(update) = &plugin.update {
        lines = lines.push(
            text(format!("Version {} is available.", update.version))
                .size(12)
                .color(theme.accent),
        );
    }
    let switch = form::toggle(
        theme,
        plugin.enabled,
        Some(plugin_message(PluginEvent::SetEnabled {
            id: plugin.id.clone(),
            enabled: !plugin.enabled,
        })),
    );
    row![
        container(lines).width(Length::Fill).clip(true),
        Space::new().width(form::GAP),
        controls::action(
            theme,
            "Settings",
            ActionKind::Plain,
            Some(plugin_message(PluginEvent::Open(plugin.id.clone()))),
        )
        .small(),
        Space::new().width(8.0),
        switch,
    ]
    .align_y(Alignment::Center)
    .into()
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
        controls::action(theme, "Installed", ActionKind::Plain, None).small()
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
fn find_status<'a>(theme: &Theme, find: &Find) -> Vec<Element<'a, Message>> {
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
            if let Some(replaces) = &review.replaces {
                parts.push(form::note(theme, format!("Replaces version {replaces}.")));
            }
            let lines = page::permission_lines(&review.capabilities, &review.network)
                .into_iter()
                .map(|line| {
                    text(line.text)
                        .size(13)
                        .color(tone_colour(theme, line.tone))
                        .into()
                })
                .collect::<Vec<_>>();
            parts.push(form::card(theme, form::group(lines)));
            parts.push(
                row![
                    controls::action(
                        theme,
                        "Install",
                        ActionKind::Primary,
                        Some(plugin_message(PluginEvent::Confirm)),
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
}
