//! Settings → Plugins: shows the plugin runtime's [`View`] in the
//! `PluginPage` global while the page is open, and passes what's done on it
//! to the runtime as a [`Request`]. A closed page holds no rows: its models
//! are emptied, and the runtime drops the marketplace's catalog.
//!
//! The runtime plugs in with [`attach`], and calls [`changed`] whenever its
//! view moves. Until it exists, the page shows [`Fixture`] in `--preview`,
//! and says plugins can't run anywhere else. See `docs/plugin-settings.md`.

use std::cell::RefCell;

use yapfast_core::plugins::page::{
    self, Catalog, Find, Fixture, Helper, IDLE_MINUTES, Installed, Line, Listing, MEMORY_BUDGETS,
    Origin, Permission, Request, Review, SettingValue, Source, Tone, Unavailable, View,
};

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
}

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

/// Gives the page the plugin runtime to show: the one place it plugs in.
/// Until it's called, the page chooses on opening, as [`source_for`] says.
// Nothing calls it, or `changed`, until the runtime exists.
#[allow(dead_code)]
pub(super) fn attach(window: &YapfastWindow, source: Box<dyn Source>) {
    PAGE.with_borrow_mut(|page| match page {
        Some(page) => page.source = source,
        None => *page = Some(Page::new(source)),
    });
    changed(window);
}

/// Shows the view again, if the page is open: the runtime calls it after its
/// view moves.
#[allow(dead_code)]
pub(super) fn changed(window: &YapfastWindow) {
    PAGE.with_borrow(|page| {
        if let Some(page) = page.as_ref().filter(|page| page.open) {
            show(window, page);
        }
    });
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
        }
    }
}

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    bind_page(window);
    // Load folder asks for a folder in the system's dialog, which the
    // adapter runs.
    let state = adapter.clone();
    window.global::<PluginPage>().on_load_folder(move || {
        let weak = Rc::downgrade(&state);
        state.borrow_mut().dialog(async move {
            let folder = rfd::AsyncFileDialog::new()
                .set_title("Load a plugin from a folder")
                .pick_folder()
                .await;
            let (Some(folder), Some(adapter)) = (folder, weak.upgrade()) else {
                return;
            };
            let Some(window) = adapter.borrow().window.upgrade() else {
                return;
            };
            PAGE.with_borrow_mut(|page| {
                if let Some(page) = page.as_mut() {
                    page.source
                        .request(Request::LoadFolder(folder.path().to_owned()));
                    page.tab = 1;
                    if page.open {
                        show(&window, page);
                    }
                }
            });
        });
    });
}

/// Everything on the page but Load folder.
fn bind_page(window: &YapfastWindow) {
    let page = window.global::<PluginPage>();
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
                    page.source.request(Request::Close);
                    clear(&window);
                }
            });
        });
    }
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
    on!(on_setting(plugin, key, value) => |page| {
        let toggle = page
            .source
            .view()
            .plugin(&plugin)
            .and_then(|installed| installed.settings.iter().find(|setting| setting.key == key.as_str()))
            .is_some_and(|setting| matches!(setting.value, SettingValue::Bool(_)));
        let value = if toggle {
            SettingValue::Bool(value == "true")
        } else {
            SettingValue::Text(value.into())
        };
        page.source.request(Request::SetSetting { plugin: plugin.into(), key: key.into(), value });
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

fn row(plugin: &Installed) -> PluginRow {
    PluginRow {
        id: plugin.id.as_str().into(),
        name: plugin.name.as_str().into(),
        initial: initial(&plugin.name),
        icon_url: plugin.icon.clone().unwrap_or_default().into(),
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
    }
}

fn detail(plugin: &Installed) -> PluginDetail {
    let settings: Vec<PluginSettingRow> = plugin
        .settings
        .iter()
        .map(|setting| {
            let (toggle, checked, value) = match &setting.value {
                SettingValue::Bool(on) => (true, *on, String::new()),
                SettingValue::Text(text) => (false, false, text.clone()),
            };
            PluginSettingRow {
                key: setting.key.as_str().into(),
                label: setting.label.as_str().into(),
                toggle,
                checked,
                value: value.into(),
            }
        })
        .collect();
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
        icon_url: plugin.icon.clone().unwrap_or_default().into(),
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
        settings: ModelRc::new(VecModel::from(settings)),
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

fn show(window: &YapfastWindow, page: &Page) {
    let global = window.global::<PluginPage>();
    let view: &View = page.source.view();
    global.set_tab(page.tab);

    let (helper, helper_tone, memory) = match view.helper {
        Helper::Stopped => (
            "",
            Tone::Normal,
            "No plugin is running now, so they use no memory.".to_owned(),
        ),
        Helper::Running { memory } => (
            "",
            Tone::Normal,
            format!("Plugins use {} now.", page::bytes(memory)),
        ),
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

    global.set_installed(ModelRc::new(VecModel::from(
        view.installed.iter().map(row).collect::<Vec<_>>(),
    )));
    let open = page.plugin.as_deref().and_then(|id| view.plugin(id));
    global.set_detail_open(open.is_some());
    if let Some(plugin) = open {
        global.set_detail(detail(plugin));
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
        page.invoke_setting(
            "com.example.word-filter".into(),
            "strict".into(),
            "true".into(),
        );
        let strict = page.get_detail().settings.row_data(1).unwrap();
        assert!(strict.toggle && strict.checked);
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
