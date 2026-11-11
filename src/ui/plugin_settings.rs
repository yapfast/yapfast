//! Settings → Plugins: shows the plugin runtime's [`View`] in the
//! `PluginPage` global while the page is open, and passes what's done on it
//! to the runtime as a [`Request`]. A closed page holds no rows: its models
//! are emptied, and the runtime drops the marketplace's catalog.
//!
//! The runtime plugs in with [`attach`], and calls [`changed`] whenever its
//! view moves. Until it exists, the page shows [`Fixture`] in `--preview`,
//! and says plugins can't run anywhere else. See `docs/plugins/settings.md`.

use std::{cell::RefCell, collections::HashMap, rc::Weak, sync::Arc};

use yapfast_core::plugins::{
    page::{
        self, Catalog, Find, Fixture, Helper, IDLE_MINUTES, Installed, Line, Listing,
        MEMORY_BUDGETS, Origin, Permission, Request, Review, Source, Tone, Unavailable, View,
    },
    settings::{self, Edit, Input, Kind, Value},
    ui::Tree,
};

use yapfast_core::application::Action;

use super::*;

/// The page: its source, and what only the page knows.
struct Page {
    source: Box<dyn Source>,
    open: bool,
    /// The plugin whose own page shows.
    plugin: Option<String>,
    query: String,
    /// The category chosen, 0 for all.
    category: usize,
    tab: i32,
    /// 0 a vertical list, 1 cards.
    view: i32,
    /// The open plugin's settings form, while its page shows.
    form: Option<FormModel>,
}

/// A plugin's settings form, in one model whose rows Rust changes in
/// place, so a field keeps its focus and its text while the form changes.
struct FormModel {
    plugin: String,
    rows: Rc<VecModel<PluginFormRow>>,
    /// Each row's fields, kept while the row is there for the same reason.
    cells: Vec<Rc<VecModel<PluginControl>>>,
    /// A select's options, which don't change, by the setting's or field's
    /// key.
    options: HashMap<String, ModelRc<SharedString>>,
    /// What each widget setting draws, while its tree is the same.
    widgets: HashMap<String, (Arc<Tree>, PluginPart)>,
}

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
    /// Decoded plugin icons, by plugin and by the length of the bytes: reading
    /// and decoding a picture on every redraw would spend the page's time on
    /// something that changes only when a plugin is installed.
    static ICONS: RefCell<HashMap<(String, usize), slint::Image>> = RefCell::new(HashMap::new());
}

/// A plugin's icon, or the empty image that draws its initial instead.
fn icon_image(plugin: &Installed) -> slint::Image {
    let Some(bytes) = plugin.icon.as_deref() else {
        return slint::Image::default();
    };
    let key = (plugin.id.clone(), bytes.len());
    if let Some(image) = ICONS.with_borrow(|icons| icons.get(&key).cloned()) {
        return image;
    }
    let Ok(picture) = image::load_from_memory(bytes) else {
        tracing::warn!(plugin = %plugin.id, "its icon isn't a picture this build can read");
        return slint::Image::default();
    };
    let picture = picture.into_rgba8();
    let (width, height) = picture.dimensions();
    let image = slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
        picture.as_raw(),
        width,
        height,
    ));
    ICONS.with_borrow_mut(|icons| {
        icons.insert(key, image.clone());
    });
    image
}

/// Gives the page the plugin runtime to show: the one place it plugs in.
///
/// Called once the window is built, from [`bind`]. `--preview` keeps the sample
/// plugins instead, which is how a run without a runtime still shows what the
/// page can do.
fn attach(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    if window.get_preview() {
        return;
    }
    let view = match adapter.try_borrow() {
        Ok(adapter) => adapter.app.plugins_page(),
        // The app is borrowed while the window is built: the page opens later,
        // and reads the view again then.
        Err(_) => View::default(),
    };
    let source = Box::new(Runtime {
        adapter: Rc::downgrade(adapter),
        view,
    });
    PAGE.with_borrow_mut(|page| match page {
        Some(page) => page.source = source,
        None => *page = Some(Page::new(source)),
    });
}

/// The runtime's view moved: the page draws it again if it's open.
pub(super) fn set_view(window: &YapfastWindow, view: View) {
    PAGE.with_borrow_mut(|page| {
        let Some(page) = page.as_mut() else { return };
        page.source.set_view(view);
        if page.open {
            show(window, page);
        }
    });
}

/// The plugin runtime, as the page reaches it: the copy of its view the page
/// draws, and one action per request.
struct Runtime {
    adapter: Weak<RefCell<Adapter>>,
    view: View,
}

impl Source for Runtime {
    fn view(&self) -> &View {
        &self.view
    }

    fn request(&mut self, request: Request) {
        let Some(adapter) = self.adapter.upgrade() else {
            return;
        };
        // A request comes from a callback that holds the page, not the adapter,
        // so the adapter is free; `try_borrow_mut` keeps one that ever holds
        // both from panicking.
        let Ok(mut adapter) = adapter.try_borrow_mut() else {
            tracing::warn!(?request, "the plugin page asked while the window was busy");
            return;
        };
        let effects = adapter.app.dispatch(Action::PluginsPage(request));
        // What the request changed is drawn at once, rather than a frame
        // later; a status that arrives from the helper comes back as
        // `Effect::Plugin` and redraws the row then.
        self.view = adapter.app.plugins_page();
        adapter.defer(effects);
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
    }
}

/// The stand-in until a runtime attaches: the sample plugins in the
/// preview, and otherwise a page that says plugins can't run.
fn source_for(preview: bool) -> Box<dyn Source> {
    if preview {
        Box::new(Fixture::new())
    } else {
        Box::new(Unavailable::default())
    }
}

impl Page {
    fn new(source: Box<dyn Source>) -> Self {
        Self {
            source,
            open: false,
            plugin: None,
            query: String::new(),
            category: 0,
            tab: 1,
            view: 0,
            form: None,
        }
    }
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    bind_page(window);
    attach(window, adapter);
    // Installing a `.wasm` file asks for it in the system's dialog, which the
    // adapter runs: the file is what a person downloads, and the manifest beside
    // it, when there is one, comes along.
    let state = adapter.clone();
    window.global::<PluginPage>().on_install_wasm(move || {
        let weak = Rc::downgrade(&state);
        state.borrow_mut().dialog(async move {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Install a plugin")
                .add_filter(
                    "A plugin's module or its yapfast-plugin.json",
                    &["wasm", "json"],
                )
                .pick_file()
                .await;
            let (Some(picked), Some(adapter)) = (picked, weak.upgrade()) else {
                return;
            };
            let Some(window) = adapter.borrow().window.upgrade() else {
                return;
            };
            PAGE.with_borrow_mut(|page| {
                if let Some(page) = page.as_mut() {
                    page.source
                        .request(Request::InstallWasm(picked.path().to_owned()));
                    // The file becomes an installed plugin, so the list that
                    // holds it is the one to show.
                    page.tab = 1;
                    if page.open {
                        show(&window, page);
                    }
                }
            });
        });
    });
}

/// Everything on the page but the file picker.
fn bind_page(window: &YapfastWindow) {
    let page = window.global::<PluginPage>();
    // Copy puts the open plugin's report on the clipboard: its details and every
    // log line it has, which is more than the pane shows and what a person
    // pastes when asking why it broke.
    {
        let weak = window.as_weak();
        page.on_copy_log(move || {
            let Some(_window) = weak.upgrade() else {
                return;
            };
            let report = PAGE.with_borrow(|page| {
                let page = page.as_ref()?;
                let id = page.plugin.as_deref()?;
                page.source.view().plugin(id).map(page::Installed::report)
            });
            if let Some(report) = report {
                crate::copy::text(report);
            }
        });
    }
    page.set_available(true);
    page.set_budgets(strings(MEMORY_BUDGETS.iter().map(|&mib| budget_text(mib))));
    page.set_idle_times(strings(IDLE_MINUTES.iter().map(|&minutes| match minutes {
        60 => "1 hour".to_owned(),
        1 => "1 minute".to_owned(),
        minutes => format!("{minutes} minutes"),
    })));

    // Each callback changes the page or asks the runtime, then shows the
    // view again.
    macro_rules! on {
        ($callback:ident($($arg:ident),*) => |$page:ident| $body:expr) => {{
            let weak = window.as_weak();
            page.$callback(move |$($arg),*| {
                let Some(window) = weak.upgrade() else { return };
                PAGE.with_borrow_mut(|page| {
                    if let Some($page) = page.as_mut() {
                        $body;
                        if $page.open {
                            show(&window, $page);
                        }
                    }
                });
            });
        }};
    }

    {
        let weak = window.as_weak();
        page.on_opened(move || {
            let Some(window) = weak.upgrade() else { return };
            PAGE.with_borrow_mut(|page| {
                let page = page.get_or_insert_with(|| Page::new(source_for(window.get_preview())));
                if page.open {
                    return;
                }
                page.open = true;
                page.source.request(Request::Open);
                if page.tab == 0 {
                    load_catalog(page);
                }
                show(&window, page);
            });
        });
    }
    {
        let weak = window.as_weak();
        page.on_closed(move || {
            let Some(window) = weak.upgrade() else { return };
            PAGE.with_borrow_mut(|page| {
                if let Some(page) = page.as_mut().filter(|page| page.open) {
                    page.open = false;
                    page.plugin = None;
                    page.form = None;
                    page.source.request(Request::Close);
                    clear(&window);
                }
            });
        });
    }
    // How the Installed tab draws plugins: a list to read down, cards to scan
    // across. The page keeps the choice for as long as it is open.
    on!(on_view_chosen(index) => |page| {
        page.view = index;
    });
    on!(on_tab_chosen(tab) => |page| {
        page.tab = tab;
        if tab == 0 {
            load_catalog(page);
        }
    });
    on!(on_search(query) => |page| page.query = query.to_string());
    on!(on_category_chosen(index) => |page| page.category = usize::try_from(index).unwrap_or(0));
    on!(on_set_enabled(plugin, enabled) => |page| page.source.request(Request::SetEnabled {
        plugin: plugin.into(),
        enabled,
    }));
    on!(on_open_plugin(plugin) => |page| page.plugin = Some(plugin.into()));
    on!(on_close_plugin() => |page| page.plugin = None);
    on!(on_install(plugin) => |page| page.source.request(Request::Install { plugin: plugin.into() }));
    on!(on_find(url) => |page| page.source.request(Request::Find { url: url.trim().to_owned() }));
    {
        let weak = window.as_weak();
        page.on_confirm(move || {
            let Some(window) = weak.upgrade() else { return };
            PAGE.with_borrow_mut(|page| {
                let Some(page) = page.as_mut() else { return };
                page.source.request(Request::Confirm);
                // Installed from its URL: show it among the others.
                if page.tab == 2 && matches!(page.source.view().find, Find::Idle) {
                    window.global::<PluginPage>().set_url(SharedString::new());
                    page.tab = 1;
                }
                show(&window, page);
            });
        });
    }
    on!(on_cancel() => |page| page.source.request(Request::Cancel));
    on!(on_uninstall(plugin, delete_data) => |page| {
        if page.plugin.as_deref() == Some(plugin.as_str()) {
            page.plugin = None;
        }
        page.source.request(Request::Uninstall { plugin: plugin.into(), delete_data });
    });
    on!(on_revoke(plugin, permission) => |page| {
        if let Some(permission) = permission_of(&permission) {
            page.source.request(Request::Revoke { plugin: plugin.into(), permission });
        }
    });
    on!(on_update(plugin) => |page| page.source.request(Request::Update { plugin: plugin.into() }));
    on!(on_roll_back(plugin) => |page| page.source.request(Request::RollBack { plugin: plugin.into() }));
    on!(on_setting(key, value) => |page| {
        let edit = schema_of(page).and_then(|schema| {
            let kind = &schema.setting(&key)?.kind;
            Some(Edit::Set { key: key.into(), value: input(kind, &value) })
        });
        if let Some(edit) = edit {
            request_edit(page, edit);
        }
    });
    on!(on_field(key, row, field, value) => |page| {
        let edit = schema_of(page).and_then(|schema| {
            let Kind::List { fields, .. } = &schema.setting(&key)?.kind else { return None };
            let kind = &fields.iter().find(|spec| spec.key == field.as_str())?.kind;
            Some(Edit::SetField {
                key: key.into(),
                row: usize::try_from(row).ok()?,
                field: field.into(),
                value: input(kind, &value),
            })
        });
        if let Some(edit) = edit {
            request_edit(page, edit);
        }
    });
    on!(on_add_row(key) => |page| request_edit(page, Edit::AddRow { key: key.into() }));
    on!(on_remove_row(key, row) => |page| {
        if let Ok(row) = usize::try_from(row) {
            request_edit(page, Edit::RemoveRow { key: key.into(), row });
        }
    });
    on!(on_move_row(key, row, to) => |page| {
        if let (Ok(row), Ok(to)) = (usize::try_from(row), usize::try_from(to)) {
            request_edit(page, Edit::MoveRow { key: key.into(), row, to });
        }
    });
    on!(on_reset_settings() => |page| request_edit(page, Edit::Reset));
    on!(on_setting_widget(key, id, value) => |page| {
        if let Some(plugin) = page.plugin.clone() {
            page.source.request(Request::SettingWidget {
                plugin,
                key: key.into(),
                id: id.into(),
                value: value.into(),
            });
        }
    });
    on!(on_budget_chosen(index) => |page| {
        if let Some(&mib) = usize::try_from(index).ok().and_then(|index| MEMORY_BUDGETS.get(index)) {
            page.source.request(Request::SetMemoryBudget(mib));
        }
    });
    on!(on_idle_chosen(index) => |page| {
        if let Some(&minutes) = usize::try_from(index).ok().and_then(|index| IDLE_MINUTES.get(index)) {
            page.source.request(Request::SetIdleMinutes(minutes));
        }
    });
}

/// The open plugin's settings.
fn schema_of(page: &Page) -> Option<Arc<settings::Schema>> {
    let plugin = page.source.view().plugin(page.plugin.as_deref()?)?;
    Some(Arc::clone(&plugin.settings.schema))
}

fn request_edit(page: &mut Page, edit: Edit) {
    if let Some(plugin) = page.plugin.clone() {
        page.source.request(Request::Setting { plugin, edit });
    }
}

/// What a control gave, as the setting takes it: a select's index as its
/// option's value, a slider's position as its number.
fn input(kind: &Kind, value: &str) -> Input {
    let index = || value.parse::<usize>().ok();
    match kind {
        Kind::Bool { .. } => Input::Bool(value == "true"),
        Kind::Select { options, .. } => Input::Text(
            index()
                .and_then(|index| options.get(index))
                .map(|option| option.value.clone())
                .unwrap_or_default(),
        ),
        Kind::Slider(range) => {
            let low = range.min.unwrap_or(0.0);
            let step = range.step.unwrap_or(1.0);
            let number = index().map_or(low, |index| low + index as f64 * step);
            // Rounded, so 0.1 * 3 is 0.3.
            Input::Text(settings::number_text((number * 1e9).round() / 1e9))
        }
        _ => Input::Text(value.to_owned()),
    }
}

/// Asks for the catalog once the marketplace shows, unless it's there.
fn load_catalog(page: &mut Page) {
    if matches!(page.source.view().catalog, Catalog::Unloaded) {
        page.source.request(Request::LoadCatalog);
    }
}

fn strings(items: impl Iterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items.map(SharedString::from).collect::<Vec<_>>(),
    ))
}

fn budget_text(mib: u32) -> String {
    if mib.is_multiple_of(1024) {
        format!("{} GiB", mib / 1024)
    } else {
        format!("{mib} MiB")
    }
}

/// The tone's number, as `PluginPage.ink` reads it.
fn tone(tone: Tone) -> i32 {
    match tone {
        Tone::Normal => 0,
        Tone::Strong => 1,
        Tone::Muted => 2,
        Tone::Success => 3,
        Tone::Warning => 4,
        Tone::Danger => 5,
    }
}

/// A permission as a Revoke button names it, and back.
fn permission_key(permission: &Permission) -> String {
    match permission {
        Permission::Capability(name) => format!("capability:{name}"),
        Permission::Host(host) => format!("host:{host}"),
    }
}

fn permission_of(key: &str) -> Option<Permission> {
    if let Some(name) = key.strip_prefix("capability:") {
        Some(Permission::Capability(name.to_owned()))
    } else {
        key.strip_prefix("host:")
            .map(|host| Permission::Host(host.to_owned()))
    }
}

fn line(line: &Line) -> PluginLine {
    PluginLine {
        text: line.text.as_str().into(),
        tone: tone(line.tone),
        permission: line
            .permission
            .as_ref()
            .map(permission_key)
            .unwrap_or_default()
            .into(),
    }
}

fn lines(items: &[Line]) -> ModelRc<PluginLine> {
    ModelRc::new(VecModel::from(items.iter().map(line).collect::<Vec<_>>()))
}

fn initial(name: &str) -> SharedString {
    name.chars()
        .find(|letter| letter.is_alphanumeric())
        .map(|letter| letter.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".into())
        .into()
}

/// Authors as a manifest gives them, `Ada <ada@example.com>`, by name.
fn authors(authors: &[String]) -> String {
    authors
        .iter()
        .map(|author| author.split('<').next().unwrap_or(author).trim())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn byline(author: &str, version: &str) -> String {
    match (author.is_empty(), version.is_empty()) {
        (false, false) => format!("by {author} · v{version}"),
        (false, true) => format!("by {author}"),
        (true, false) => format!("v{version}"),
        (true, true) => String::new(),
    }
}

fn row(plugin: &Installed) -> InstalledRow {
    InstalledRow {
        id: plugin.id.as_str().into(),
        name: plugin.name.as_str().into(),
        initial: initial(&plugin.name),
        picture: icon_image(plugin),
        byline: byline(&authors(&plugin.authors), &plugin.version).into(),
        status: plugin.status.text().into(),
        tone: tone(plugin.status.tone()),
        enabled: plugin.enabled,
        update: plugin
            .update
            .as_ref()
            .map(|update| update.version.as_str())
            .unwrap_or_default()
            .into(),
        // The newest line, with its time, so the row itself says why: a plugin
        // that keeps crashing logs the reason, and opening its page is how the
        // whole ring is read.
        log: match plugin.log.last() {
            Some(line) => format!("{} {}", line.at, line.text).into(),
            None => SharedString::new(),
        },
        log_error: plugin.log.last().is_some_and(|line| line.error),
    }
}

fn detail(plugin: &Installed) -> PluginDetail {
    let log: Vec<PluginLogRow> = plugin
        .log
        .iter()
        .map(|line| PluginLogRow {
            at: line.at.as_str().into(),
            text: line.text.as_str().into(),
            error: line.error,
        })
        .collect();
    PluginDetail {
        id: plugin.id.as_str().into(),
        name: plugin.name.as_str().into(),
        initial: initial(&plugin.name),
        picture: icon_image(plugin),
        byline: byline(&authors(&plugin.authors), &plugin.version).into(),
        description: plugin.description.as_str().into(),
        status: plugin.status.text().into(),
        tone: tone(plugin.status.tone()),
        enabled: plugin.enabled,
        update: plugin
            .update
            .as_ref()
            .map(|update| update.version.as_str())
            .unwrap_or_default()
            .into(),
        previous: plugin.previous.clone().unwrap_or_default().into(),
        source: plugin.source.as_str().into(),
        commit: page::short_commit(&plugin.commit).into(),
        license: plugin.license.as_str().into(),
        origin: match plugin.origin {
            Origin::Marketplace => "The marketplace",
            Origin::Url => "A git URL",
            Origin::Folder => "A folder on this computer, reloaded when you save",
        }
        .into(),
        memory: page::memory(plugin.memory_pages).0.into(),
        quiet: match plugin.quiet_denials {
            0 => String::new(),
            1 => "It asked to connect once more this session, and was denied without asking you."
                .into(),
            count => format!(
                "It asked to connect {count} more times this session, and was denied without asking you."
            ),
        }
        .into(),
        permissions: lines(&plugin.permission_lines()),
        log: ModelRc::new(VecModel::from(log)),
    }
}

fn listing(listing: &Listing, installed: bool) -> ListingRow {
    let mut byline = byline(&listing.author, &listing.version);
    byline.push_str(" · ");
    byline.push_str(&listing.reach());
    ListingRow {
        id: listing.id.as_str().into(),
        name: listing.name.as_str().into(),
        initial: initial(&listing.name),
        icon_url: listing.icon.clone().unwrap_or_default().into(),
        description: listing.description.as_str().into(),
        byline: byline.into(),
        reviewed: listing.reviewed,
        installed,
    }
}

fn review(review: &Review) -> PluginReview {
    let (memory, memory_tone) = page::memory(review.memory_pages);
    let dependencies: Vec<Line> = review
        .dependencies
        .iter()
        .map(|dependency| Line {
            text: if dependency.installed {
                format!("{} {}, installed", dependency.name, dependency.version)
            } else {
                format!(
                    "{} {}, which isn't installed",
                    dependency.name, dependency.version
                )
            },
            tone: if dependency.installed {
                Tone::Normal
            } else {
                Tone::Warning
            },
            permission: None,
        })
        .collect();
    PluginReview {
        shown: true,
        name: review.name.as_str().into(),
        initial: initial(&review.name),
        icon_url: review.icon.clone().unwrap_or_default().into(),
        version: review.version.as_str().into(),
        source: review.source.as_str().into(),
        commit: page::short_commit(&review.commit).into(),
        reviewed: review.reviewed,
        lines: lines(&page::permission_lines(
            &review.capabilities,
            &review.network,
        )),
        memory: line(&Line {
            text: memory,
            tone: memory_tone,
            permission: None,
        }),
        dependencies: lines(&dependencies),
        replaces: review.replaces.clone().unwrap_or_default().into(),
    }
}

/// The categories in the catalog, in order, after "All categories".
fn categories(listings: &[Listing]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for listing in listings {
        if !listing.category.is_empty() && !found.contains(&listing.category) {
            found.push(listing.category.clone());
        }
    }
    found.sort();
    found
}

fn show(window: &YapfastWindow, page: &mut Page) {
    let global = window.global::<PluginPage>();
    let view: &View = page.source.view();
    global.set_tab(page.tab);

    let (helper, helper_tone, memory) = match view.helper {
        Helper::Stopped => (
            "",
            Tone::Normal,
            "No plugin is running now, so they use no memory.".to_owned(),
        ),
        Helper::Running {
            memory: Some(memory),
        } => (
            "",
            Tone::Normal,
            format!("Plugins use {} now.", page::bytes(memory)),
        ),
        // A system that doesn't report a process's memory: it is up, and that
        // is all the page can say.
        Helper::Running { memory: None } => {
            ("", Tone::Normal, "Plugins are running now.".to_owned())
        }
        Helper::Missing => (
            "Plugins can't run: yapfast-plugins, the program that runs them, isn't installed beside Yapfast. Everything else works.",
            Tone::Warning,
            String::new(),
        ),
        Helper::Crashed => (
            "Plugins are off: the program that runs them crashed three times in a minute. They start again when Yapfast does.",
            Tone::Danger,
            String::new(),
        ),
    };
    global.set_helper(helper.into());
    global.set_helper_tone(tone(helper_tone));
    global.set_memory(memory.into());

    let rows: Vec<InstalledRow> = view.installed.iter().map(row).collect();
    let mut cards: Vec<ModelRc<InstalledRow>> = Vec::new();
    for pair in rows.chunks(2) {
        cards.push(ModelRc::new(VecModel::from(pair.to_vec())));
    }
    global.set_installed(ModelRc::new(VecModel::from(rows)));
    global.set_card_rows(ModelRc::new(VecModel::from(cards)));
    global.set_view(page.view);
    let open = page.plugin.as_deref().and_then(|id| view.plugin(id));
    global.set_detail_open(open.is_some());
    if let Some(plugin) = open {
        global.set_detail(detail(plugin));
    }
    match open {
        Some(plugin) => show_form(&global, &mut page.form, plugin),
        None if page.form.take().is_some() => global.set_form(ModelRc::default()),
        None => {}
    }

    let (listings, all_categories, note) = match &view.catalog {
        Catalog::Unloaded | Catalog::Loading => (
            Vec::new(),
            Vec::new(),
            "Loading the marketplace...".to_owned(),
        ),
        Catalog::Failed(why) => (
            Vec::new(),
            Vec::new(),
            format!("Couldn't load the marketplace: {why}"),
        ),
        Catalog::Ready(listings) => {
            let all = categories(listings);
            let category = page
                .category
                .checked_sub(1)
                .and_then(|index| all.get(index));
            let rows: Vec<ListingRow> = listings
                .iter()
                .filter(|item| category.is_none_or(|category| item.category == *category))
                .filter(|item| item.matches(&page.query))
                .map(|item| listing(item, view.plugin(&item.id).is_some()))
                .collect();
            let note = if rows.is_empty() {
                "No plugins match.".to_owned()
            } else {
                String::new()
            };
            (rows, all, note)
        }
    };
    global.set_listings(ModelRc::new(VecModel::from(listings)));
    global.set_categories(strings(
        std::iter::once("All categories".to_owned()).chain(all_categories),
    ));
    global.set_category(i32::try_from(page.category).unwrap_or(0));
    global.set_catalog_note(note.into());

    let (finding, progress, error) = match &view.find {
        Find::Idle | Find::Review(_) => (false, String::new(), String::new()),
        Find::Fetching { step, .. } => (true, step.text().to_owned(), String::new()),
        Find::Installing { name } => (true, format!("Installing {name}..."), String::new()),
        Find::Failed { error, .. } => (false, String::new(), error.text()),
    };
    global.set_finding(finding);
    global.set_find_progress(progress.into());
    global.set_find_error(error.into());
    global.set_review(match &view.find {
        Find::Review(found) => review(found),
        _ => PluginReview::default(),
    });

    let advanced = &view.advanced;
    global.set_budget(nearest(&MEMORY_BUDGETS, advanced.memory_budget));
    global.set_idle(nearest(&IDLE_MINUTES, advanced.idle_minutes));
    global.set_sandbox(
        if advanced.sandbox_unavailable {
            "OS sandbox unavailable: plugins run in WebAssembly's sandbox alone."
        } else {
            ""
        }
        .into(),
    );
}

/// Shows the open plugin's settings form, changing only the rows that
/// changed.
fn show_form(global: &PluginPage, form: &mut Option<FormModel>, plugin: &Installed) {
    if form.as_ref().is_none_or(|form| form.plugin != plugin.id) {
        let model = FormModel {
            plugin: plugin.id.clone(),
            rows: Rc::new(VecModel::default()),
            cells: Vec::new(),
            options: HashMap::new(),
            widgets: HashMap::new(),
        };
        global.set_form(ModelRc::from(Rc::clone(&model.rows)));
        *form = Some(model);
    }
    let Some(form) = form.as_mut() else { return };
    let rows = form_rows(form, plugin);
    let count = rows.len();
    form.cells.truncate(count);
    for (index, (mut row, cells)) in rows.into_iter().enumerate() {
        if !cells.is_empty() {
            if form.cells.len() <= index {
                form.cells
                    .resize_with(index + 1, || Rc::new(VecModel::default()));
            }
            let model = &form.cells[index];
            sync(model, cells);
            row.cells = ModelRc::from(Rc::clone(model));
        }
        if index < form.rows.row_count() {
            if form.rows.row_data(index).as_ref() != Some(&row) {
                form.rows.set_row_data(index, row);
            }
        } else {
            form.rows.push(row);
        }
    }
    while form.rows.row_count() > count {
        form.rows.remove(form.rows.row_count() - 1);
    }
}

/// Puts `rows` in `model`, changing only those that differ.
fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    let len = rows.len();
    for (index, row) in rows.into_iter().enumerate() {
        if index < model.row_count() {
            if model.row_data(index).as_ref() != Some(&row) {
                model.set_row_data(index, row);
            }
        } else {
            model.push(row);
        }
    }
    while model.row_count() > len {
        model.remove(model.row_count() - 1);
    }
}

/// The form's rows: each setting that shows, a list as its heading, its
/// rows and its foot, with the fields of each list's row beside it.
fn form_rows(form: &mut FormModel, plugin: &Installed) -> Vec<(PluginFormRow, Vec<PluginControl>)> {
    let settings = &plugin.settings;
    let states = settings.schema.states(&settings.values);
    let mut rows = Vec::new();
    for (setting, state) in settings.schema.settings.iter().zip(states) {
        if !state.shown {
            continue;
        }
        let base = PluginFormRow {
            key: setting.key.as_str().into(),
            label: setting.label.as_str().into(),
            description: setting.description.as_str().into(),
            enabled: state.enabled,
            ..PluginFormRow::default()
        };
        let value = settings.values.get(&setting.key);
        match &setting.kind {
            Kind::List {
                fields, max_rows, ..
            } => {
                let heads = fields
                    .iter()
                    .map(|field| PluginControl {
                        label: field.label.as_str().into(),
                        kind: control_kind(&field.kind),
                        ..PluginControl::default()
                    })
                    .collect();
                rows.push((
                    PluginFormRow {
                        shape: 1,
                        ..base.clone()
                    },
                    heads,
                ));
                let records = match value {
                    Some(Value::List(records)) => records.as_slice(),
                    _ => &[],
                };
                for (index, record) in records.iter().enumerate() {
                    let mut errors = Vec::new();
                    let cells = fields
                        .iter()
                        .map(|field| {
                            let place = settings::address(&setting.key, Some((index, &field.key)));
                            let refused = settings.refused.get(&place);
                            if let Some(refused) = refused {
                                errors.push(format!("{}: {}", field.label, refused.message));
                            }
                            let options = &mut form.options;
                            control(
                                &field.key,
                                &field.label,
                                &field.kind,
                                record.get(&field.key),
                                refused,
                                state.enabled,
                                options,
                            )
                        })
                        .collect();
                    rows.push((
                        PluginFormRow {
                            shape: 2,
                            row: i32::try_from(index).unwrap_or(0),
                            rows: i32::try_from(records.len()).unwrap_or(0),
                            description: errors.join(" ").into(),
                            ..base.clone()
                        },
                        cells,
                    ));
                }
                let full = records.len() >= *max_rows;
                rows.push((
                    PluginFormRow {
                        shape: 3,
                        can_add: !full,
                        description: if full {
                            format!("It holds up to {max_rows} rows.").into()
                        } else {
                            SharedString::new()
                        },
                        ..base
                    },
                    Vec::new(),
                ));
            }
            Kind::Widget => {
                let widget = match settings.widgets.get(&setting.key) {
                    Some(tree) => match form.widgets.get(&setting.key) {
                        Some((shown, part)) if Arc::ptr_eq(shown, tree) => part.clone(),
                        _ => {
                            let part = super::plugins::tree_part(
                                &plugin.id,
                                &plugin.name,
                                "settings",
                                tree,
                            );
                            form.widgets
                                .insert(setting.key.clone(), (Arc::clone(tree), part.clone()));
                            part
                        }
                    },
                    None => PluginPart::default(),
                };
                rows.push((
                    PluginFormRow {
                        shape: 4,
                        widget,
                        ..base
                    },
                    Vec::new(),
                ));
            }
            kind => {
                let refused = settings.refused.get(&settings::address(&setting.key, None));
                let control = control(
                    &setting.key,
                    &setting.label,
                    kind,
                    value,
                    refused,
                    state.enabled,
                    &mut form.options,
                );
                rows.push((
                    PluginFormRow {
                        shape: 0,
                        control,
                        ..base
                    },
                    Vec::new(),
                ));
            }
        }
    }
    rows
}

/// A control's kind, as `PluginControl.kind` numbers it.
fn control_kind(kind: &Kind) -> i32 {
    match kind {
        Kind::Bool { .. } => 0,
        Kind::String(_) => 1,
        Kind::Select { .. } => 2,
        Kind::Slider(_) => 3,
        Kind::Number(_) => 4,
        Kind::Integer { .. } => 5,
        Kind::List { .. } | Kind::Widget => -1,
    }
}

fn control(
    key: &str,
    label: &str,
    kind: &Kind,
    value: Option<&Value>,
    refused: Option<&settings::Refused>,
    enabled: bool,
    options: &mut HashMap<String, ModelRc<SharedString>>,
) -> PluginControl {
    let mut control = PluginControl {
        key: key.into(),
        label: label.into(),
        kind: control_kind(kind),
        enabled,
        error: refused
            .map(|refused| refused.message.as_str())
            .unwrap_or_default()
            .into(),
        ..PluginControl::default()
    };
    let text = match value {
        Some(Value::Text(text)) => text.clone(),
        Some(Value::Number(number)) => settings::number_text(*number),
        Some(Value::Integer(number)) => number.to_string(),
        _ => String::new(),
    };
    control.text = refused.map_or(text, |refused| refused.input.clone()).into();
    match (kind, value) {
        (Kind::Bool { .. }, Some(Value::Bool(on))) => control.checked = *on,
        (Kind::String(text), _) => control.placeholder = text.placeholder.as_str().into(),
        (
            Kind::Select {
                options: choices, ..
            },
            Some(Value::Text(chosen)),
        ) => {
            control.options = options
                .entry(key.to_owned())
                .or_insert_with(|| strings(choices.iter().map(|choice| choice.label.clone())))
                .clone();
            control.selected = choices
                .iter()
                .position(|choice| choice.value == *chosen)
                .map_or(0, |index| i32::try_from(index).unwrap_or(0));
        }
        (Kind::Slider(range), Some(Value::Number(number))) => {
            let low = range.min.unwrap_or(0.0);
            let high = range.max.unwrap_or(low);
            let step = range.step.unwrap_or(1.0);
            #[allow(clippy::cast_possible_truncation)]
            {
                control.steps = ((high - low) / step).round() as i32;
                control.position = ((number - low) / step).round() as i32;
            }
            #[allow(clippy::cast_possible_truncation)]
            {
                control.minimum = low as f32;
                control.step = step as f32;
            }
        }
        _ => {}
    }
    control
}

/// The index of the choice closest to `value`.
fn nearest(choices: &[u32], value: u32) -> i32 {
    choices
        .iter()
        .enumerate()
        .min_by_key(|(_, choice)| choice.abs_diff(value))
        .map_or(0, |(index, _)| i32::try_from(index).unwrap_or(0))
}

/// Empties the page's models once it closes, so their rows go.
fn clear(window: &YapfastWindow) {
    let global = window.global::<PluginPage>();
    global.set_installed(ModelRc::default());
    global.set_listings(ModelRc::default());
    global.set_categories(ModelRc::default());
    global.set_detail_open(false);
    global.set_detail(PluginDetail::default());
    global.set_form(ModelRc::default());
    global.set_review(PluginReview::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_page(preview: bool) -> YapfastWindow {
        i_slint_backend_testing::init_no_event_loop();
        PAGE.with_borrow_mut(|page| *page = None);
        let window = YapfastWindow::new().unwrap();
        window.set_preview(preview);
        bind_page(&window);
        window.global::<PluginPage>().invoke_opened();
        window
    }

    #[test]
    fn without_a_runtime_the_page_says_plugins_cant_run() {
        let window = open_page(false);
        let page = window.global::<PluginPage>();
        assert!(page.get_available());
        assert!(page.get_helper().contains("isn't installed"));
        assert_eq!(page.get_helper_tone(), 4);
        assert_eq!(page.get_installed().row_count(), 0);
    }

    #[test]
    fn the_preview_lists_its_sample_plugins_with_their_status() {
        let window = open_page(true);
        let page = window.global::<PluginPage>();
        let installed = page.get_installed();
        assert_eq!(installed.row_count(), 3);
        let translate = installed.row_data(1).unwrap();
        assert_eq!(translate.status, "Suspended: ran out of memory");
        assert_eq!(translate.tone, 5);
        assert_eq!(installed.row_data(0).unwrap().update, "1.5.0");
        assert_eq!(page.get_memory(), "Plugins use 5.4 MiB now.");

        page.invoke_set_enabled("net.example.quiet-hours".into(), true);
        assert_eq!(page.get_installed().row_data(2).unwrap().status, "Idle");

        page.invoke_open_plugin("org.example.translate".into());
        assert!(page.get_detail_open());
        let detail = page.get_detail();
        assert_eq!(detail.byline, "by Bea, Cy · v0.9.1");
        assert_eq!(detail.commit, "9f2c1d00");
        let lines: Vec<_> = detail.permissions.iter().map(|line| line.text).collect();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("could send your messages"))
        );
        assert!(detail.quiet.contains("4 more times"));

        page.invoke_revoke(
            "org.example.translate".into(),
            "host:translate.example.org".into(),
        );
        let lines: Vec<_> = page
            .get_detail()
            .permissions
            .iter()
            .map(|line| line.text)
            .collect();
        assert_eq!(lines, ["Reads your messages"]);

        page.invoke_open_plugin("com.example.word-filter".into());
        page.invoke_setting("strict".into(), "true".into());
        let strict = page.get_form().row_data(0).unwrap();
        assert!(strict.control.kind == 0 && strict.control.checked);
        page.invoke_update("com.example.word-filter".into());
        assert_eq!(page.get_detail().update, "");
        assert_eq!(page.get_detail().previous, "1.4.0");

        page.invoke_uninstall("com.example.word-filter".into(), true);
        assert!(!page.get_detail_open());
        assert_eq!(page.get_installed().row_count(), 2);
    }

    #[test]
    fn a_url_shows_its_review_and_installs_only_on_confirm() {
        let window = open_page(true);
        let page = window.global::<PluginPage>();
        page.invoke_tab_chosen(2);
        page.invoke_find("https://codeberg.org/ada/no-manifest".into());
        assert_eq!(
            page.get_find_error(),
            "That repository has no yapfast-plugin.json."
        );
        assert!(!page.get_review().shown);

        page.set_url("https://codeberg.org/ada/auto-reply".into());
        page.invoke_find(page.get_url());
        let review = page.get_review();
        assert!(review.shown);
        assert!(!review.reviewed);
        assert_eq!(page.get_find_error(), "");
        let danger: Vec<_> = review
            .lines
            .iter()
            .filter(|line| line.tone == 5)
            .map(|line| line.text)
            .collect();
        assert_eq!(danger, [page::BACKGROUND_SEND]);
        assert_eq!(review.memory.tone, 4);
        assert_eq!(page.get_installed().row_count(), 3);

        page.invoke_confirm();
        assert!(!page.get_review().shown);
        assert_eq!(page.get_installed().row_count(), 4);
        // It shows among the others, and the field is ready for another.
        assert_eq!(page.get_tab(), 1);
        assert_eq!(page.get_url(), "");
    }

    #[test]
    fn the_marketplace_filters_by_search_and_category_and_is_dropped_on_closing() {
        let window = open_page(true);
        let page = window.global::<PluginPage>();
        assert_eq!(page.get_listings().row_count(), 0);
        page.invoke_tab_chosen(0);
        assert!(page.get_listings().row_count() > 40);
        let first = page.get_listings().row_data(0).unwrap();
        assert_eq!(first.byline, "by Ada · v1.4.0 · messages, storage, 1 host");
        assert!(first.installed && first.reviewed);

        page.invoke_search("pronoun".into());
        assert_eq!(page.get_listings().row_count(), 1);
        page.invoke_search("zzz".into());
        assert_eq!(page.get_catalog_note(), "No plugins match.");

        let categories: Vec<_> = page.get_categories().iter().collect();
        let chat = categories.iter().position(|name| name == "Chat").unwrap();
        page.invoke_search("".into());
        page.invoke_category_chosen(i32::try_from(chat).unwrap());
        assert!(page.get_listings().iter().all(|row| {
            ["Word Filter", "Local Times"].contains(&row.name.as_str())
                || row.name.starts_with("Sample")
        }));

        page.invoke_install("org.example.pronouns".into());
        assert!(page.get_review().shown);
        assert!(page.get_review().reviewed);
        page.invoke_cancel();
        assert!(!page.get_review().shown);

        page.invoke_closed();
        assert_eq!(page.get_listings().row_count(), 0);
        assert_eq!(page.get_installed().row_count(), 0);
        PAGE.with_borrow(|page| {
            assert!(matches!(
                page.as_ref().unwrap().source.view().catalog,
                Catalog::Unloaded
            ));
        });
        // Opening it again on the marketplace fetches the catalog again.
        page.invoke_opened();
        assert!(page.get_listings().row_count() > 0);
    }

    /// The form's rows, as `shape:label`.
    fn shapes(page: &PluginPage) -> Vec<String> {
        page.get_form()
            .iter()
            .map(|row| format!("{}:{}", row.shape, row.label))
            .collect()
    }

    #[test]
    fn the_form_draws_every_type_and_changes_in_place() {
        let window = open_page(true);
        let page = window.global::<PluginPage>();
        assert_eq!(page.get_form().row_count(), 0, "no plugin's page is open");
        page.invoke_open_plugin("com.example.word-filter".into());
        assert_eq!(
            shapes(&page),
            [
                "0:Match inside words",
                "0:When a word matches",
                "0:Replacement text",
                "0:Seconds before a word shows on hover",
                "0:Shortest word to match",
                "1:Words",
                "2:Words",
                "2:Words",
                "3:Words",
                "4:Preview",
            ]
        );
        let form = page.get_form();
        let kinds: Vec<i32> = form.iter().map(|row| row.control.kind).collect();
        assert_eq!(&kinds[..5], [0, 2, 1, 4, 5]);
        let mode = form.row_data(1).unwrap().control;
        assert_eq!(mode.options.row_count(), 3);
        assert_eq!(mode.selected, 1);
        // The sample shows a refusal, with what was typed.
        let shortest = form.row_data(4).unwrap().control;
        assert_eq!(shortest.text, "0");
        assert_eq!(shortest.error, "Enter a whole number from 1 to 20.");
        let row = form.row_data(6).unwrap();
        let cells: Vec<_> = row
            .cells
            .iter()
            .map(|cell| (cell.kind, cell.text))
            .collect();
        assert_eq!(
            cells,
            [(1, "heck".into()), (1, "h*ck".into()), (0, "".into())]
        );
        assert_eq!(form.row_data(9).unwrap().widget.rows.row_count(), 1);

        // Blur shows the slider instead of the replacement, in the same
        // model, and Hide disables the delay.
        page.invoke_setting("mode".into(), "2".into());
        assert!(page.get_form() == form, "the model stays, its rows change");
        let blur = form.row_data(2).unwrap();
        assert_eq!(
            (blur.label.as_str(), blur.control.kind),
            ("Blur strength", 3)
        );
        assert_eq!((blur.control.steps, blur.control.position), (20, 12));
        page.invoke_setting("blur".into(), "13".into());
        assert_eq!(form.row_data(2).unwrap().control.position, 13);
        page.invoke_setting("mode".into(), "0".into());
        assert!(!form.row_data(2).unwrap().enabled);
        page.invoke_setting("mode".into(), "1".into());

        // Typing in a list's row, then moving, adding and removing rows.
        let cells = form.row_data(6).unwrap().cells;
        page.invoke_field("words".into(), 0, "word".into(), "two words".into());
        let row = form.row_data(6).unwrap();
        assert!(row.cells == cells, "a row's fields keep their model");
        assert_eq!(row.cells.row_data(0).unwrap().text, "two words");
        assert_eq!(row.description, "Word: One word, without spaces.");
        page.invoke_field("words".into(), 0, "word".into(), "drat".into());
        page.invoke_field("words".into(), 1, "whole".into(), "true".into());
        page.invoke_move_row("words".into(), 1, 0);
        let first = form.row_data(6).unwrap();
        assert_eq!(first.cells.row_data(0).unwrap().text, "darn");
        assert!(first.cells.row_data(2).unwrap().checked);
        page.invoke_add_row("words".into());
        assert_eq!(form.row_count(), 11);
        page.invoke_remove_row("words".into(), 0);
        page.invoke_remove_row("words".into(), 0);
        page.invoke_remove_row("words".into(), 0);
        assert!(
            !shapes(&page).contains(&"4:Preview".to_owned()),
            "shown only with words"
        );

        page.invoke_reset_settings();
        assert_eq!(shapes(&page).len(), 10);
        assert_eq!(form.row_data(4).unwrap().control.text, "3");
        assert_eq!(form.row_data(4).unwrap().control.error, "");

        page.invoke_close_plugin();
        assert_eq!(page.get_form().row_count(), 0);
    }

    #[test]
    fn controls_give_what_their_setting_takes() {
        let schema = yapfast_core::plugins::settings::Schema::parse(&serde_json::json!([
            { "key": "a", "type": "select", "label": "A", "options": ["x", "y"] },
            { "key": "b", "type": "slider", "label": "B", "min": 0, "max": 1, "step": 0.1 }
        ]))
        .unwrap();
        let kind = |key: &str| &schema.setting(key).unwrap().kind;
        assert_eq!(input(kind("a"), "1"), Input::Text("y".into()));
        assert_eq!(input(kind("b"), "3"), Input::Text("0.3".into()));
    }

    #[test]
    fn revoke_names_what_it_takes_away() {
        for permission in [
            Permission::Capability("messages.read".into()),
            Permission::Host("api.example.com".into()),
        ] {
            assert_eq!(
                permission_of(&permission_key(&permission)),
                Some(permission)
            );
        }
        assert_eq!(permission_of("anything"), None);
        assert_eq!(budget_text(1024), "1 GiB");
        assert_eq!(nearest(&MEMORY_BUDGETS, 300), 1);
    }
}
