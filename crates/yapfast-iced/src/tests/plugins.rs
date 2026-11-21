//! Plugins in the window: an enabled plugin's buttons and slots draw, a
//! plugin's request for a site is asked about and answered, and a plugin's
//! link waits in the leaving dialog. Each runs the way iced does, with the
//! background runtime entered.

use std::path::Path;

use yapfast_core::application::{Effect, plugins::ConsentAsk};
use yapfast_core::plugins::{page::Request, slots::Slot, ui::Tree};

use super::{interact, local, present, preview, shows};
use crate::app::App;

/// A plugin in a folder of its own whose manifest adds a header button; its
/// module is a test fixture that is never asked for anything here.
fn folder_plugin() -> tempfile::TempDir {
    folder_plugin_with("")
}

/// The same, its manifest given `more` fields too (`"ui": {...}, `).
fn folder_plugin_with(more: &str) -> tempfile::TempDir {
    let source = tempfile::tempdir_in(local()).expect("plugin source");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/echo.wasm"),
        source.path().join("plugin.wasm"),
    )
    .expect("a module");
    std::fs::write(
        source.path().join("yapfast-plugin.json"),
        format!(
            r#"{{
            "api": 1, "id": "local.window-test", "name": "Window Test",
            "version": "0.1.0", "wasm": "plugin.wasm", {more}
            "header_buttons": [{{ "label": "Wave hello", "action": "wave" }}]
        }}"#
        ),
    )
    .expect("a manifest");
    source
}

/// The id the folder plugin has.
const PLUGIN: &str = "local.window-test";

/// Draws `json` as the folder plugin's tree in `slot`, as the runtime would,
/// checked as that slot checks it.
fn draw(app: &mut App, slot: Slot, json: &str) {
    let tree = Tree::parse_within(json, slot.limits()).expect("a tree");
    let slots = app
        .client
        .app
        .state
        .plugin_slots
        .as_mut()
        .expect("the plugin's slots");
    assert!(slots.set(PLUGIN, slot, tree));
    let _ = app.sync();
}

/// Loads `folder` as a development plugin from Settings, as a click inside
/// iced's `update` does, and syncs the window after it.
fn load(app: &mut App, folder: &Path) {
    let handle = app.client.runtime().handle();
    let _entered = handle.enter();
    app.client
        .plugin_request(Request::LoadFolder(folder.to_owned()));
    let _ = app.sync();
}

/// The core's `Effect::Interface` reaches the window: the plugin's header
/// button draws, and its slots are known, so what it draws in them shows,
/// under its name.
#[test]
fn an_enabled_plugin_draws_its_buttons_and_slots() {
    let (mut app, _data) = preview();
    let source = folder_plugin();
    assert!(!shows(&app, "Wave hello"));

    load(&mut app, source.path());

    assert!(app.client.state().plugins.is_some(), "the system opened");
    assert!(app.plugin_ui.is_some(), "the overrides were built");
    assert!(shows(&app, "Wave hello"), "the header button draws");
    let slots = app
        .client
        .app
        .state
        .plugin_slots
        .as_mut()
        .expect("the plugin's slots");
    let toast =
        Tree::parse(r#"{ "rows": [{ "cells": [{ "text": "Toasted" }] }] }"#).expect("a tree");
    assert!(slots.set("local.window-test", Slot::Toast, toast));
    assert!(shows(&app, "Toasted"), "the toast draws");
    assert!(
        shows(&app, "Window Test plugin"),
        "the toast says whose it is"
    );

    // Turned off from Settings, its button goes with the click.
    {
        let handle = app.client.runtime().handle();
        let _entered = handle.enter();
        app.client.plugin_request(Request::SetEnabled {
            plugin: "local.window-test".into(),
            enabled: false,
        });
    }
    let _ = app.sync();
    assert!(
        !shows(&app, "Wave hello"),
        "a plugin turned off draws nothing"
    );
}

/// A plugin asking for a site nobody allowed is asked about over the window,
/// and Always allow writes the grant and takes the question away.
#[test]
fn the_site_question_asks_and_answers() {
    let (mut app, data) = preview();
    {
        let handle = app.client.runtime().handle();
        let _entered = handle.enter();
        app.client.open_plugins();
    }
    let _ = data;
    let plugin = yapfast_plugin_api::PluginId::parse("com.example.weather").expect("an id");
    app.client.app.state.plugin_consent = Some(ConsentAsk {
        plugin: plugin.clone(),
        name: "Weather".into(),
        host: "api.example.com".into(),
    });
    assert!(shows(&app, "Allow a plugin to reach a site?"));
    assert!(shows(&app, "api.example.com"));

    interact(&mut app, |ui| {
        ui.click("Always allow").expect("Always allow");
    });

    assert!(app.client.state().plugin_consent.is_none(), "answered");
    assert!(!shows(&app, "Allow a plugin to reach a site?"));
    let grants = app
        .client
        .state()
        .plugins
        .as_ref()
        .expect("the plugin system")
        .grants()
        .clone();
    let host =
        yapfast_plugin_api::capability::HostPattern::parse("api.example.com").expect("a host");
    assert!(
        grants.allowed(&plugin).contains(&host),
        "Always allow is kept for the plugin"
    );
}

/// Escape denies the site this time, and nothing is kept.
#[test]
fn escape_denies_the_site_question() {
    let (mut app, _data) = preview();
    {
        let handle = app.client.runtime().handle();
        let _entered = handle.enter();
        app.client.open_plugins();
    }
    app.client.app.state.plugin_consent = Some(ConsentAsk {
        plugin: yapfast_plugin_api::PluginId::parse("com.example.weather").expect("an id"),
        name: "Weather".into(),
        host: "api.example.com".into(),
    });
    let _ = app.key(
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::empty(),
    );
    assert!(app.client.state().plugin_consent.is_none());
}

/// A plugin's `open.url` doesn't open anything: it waits in the leaving
/// dialog, which names the plugin, until the person chooses.
#[test]
fn a_plugin_link_waits_in_the_leaving_dialog() {
    let (mut app, _data) = preview();
    app.client.execute(vec![Effect::ConfirmOpen {
        url: "https://example.com/page".into(),
        plugin: "Weather (com.example.weather)".into(),
    }]);
    let _ = app.sync();
    assert_eq!(app.leaving.as_deref(), Some("https://example.com/page"));
    assert!(shows(
        &app,
        "The plugin Weather (com.example.weather) wants to open a link that goes to"
    ));
    interact(&mut app, |ui| {
        ui.click("Go back").expect("Go back");
    });
    assert!(app.leaving.is_none());
    assert!(app.leaving_from.is_none());
}

/// A plugin's own page lists the hosts the person allowed in the popup, each
/// with its Revoke, and Uninstall asks whether to keep the plugin's data.
#[test]
fn the_plugin_page_shows_allowed_hosts_and_asks_before_uninstalling() {
    use crate::app::Message;
    use crate::settings::{Event, PluginEvent, Section};

    let (mut app, _data) = preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    let plugin = yapfast_plugin_api::PluginId::parse("local.window-test").expect("an id");
    let host =
        yapfast_plugin_api::capability::HostPattern::parse("api.example.com").expect("a host");
    app.client
        .state()
        .plugins
        .as_ref()
        .expect("the plugin system")
        .answer_consent(
            &plugin,
            &host,
            true,
            yapfast_core::plugins::grants::Lasts::Stored,
        )
        .expect("the grant is kept");
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let _ = app.update(Message::Settings(Event::Open(Section::Plugins)));
    let _ = app.update(Message::Settings(Event::Plugin(PluginEvent::Open(
        "local.window-test".into(),
    ))));
    assert!(
        shows(&app, "Connects to api.example.com, as you allowed"),
        "an allowed host is listed on the plugin's page"
    );

    let _ = app.update(Message::Settings(Event::Plugin(PluginEvent::AskUninstall(
        Some("local.window-test".into()),
    ))));
    assert!(shows(&app, "Uninstall and delete its data"));
    let _ = app.update(Message::Settings(Event::Plugin(PluginEvent::AskUninstall(
        None,
    ))));
    assert!(!shows(&app, "Uninstall and delete its data"));
}

/// A plugin's banner keeps its name and Dismiss in view, however wide the
/// widgets it draws: the tree is the sample plugin's own.
#[test]
fn a_wide_banner_keeps_its_dismiss() {
    let (mut app, _data) = preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    let trees: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/widgets/slots.json"),
        )
        .expect("the sample's slots"),
    )
    .expect("JSON");
    let banner = Tree::parse(&trees["banner"].to_string()).expect("a tree");
    let slots = app.client.app.state.plugin_slots.as_mut().expect("slots");
    assert!(slots.set("local.window-test", Slot::Banner, banner));
    for width in [1120.0, 933.0, 820.0] {
        app.shell.frame.width = width;
        crate::window_bar::fit_window(&mut app);
        assert!(shows(&app, "Daily digest"), "the banner draws at {width}");
        assert!(shows(&app, "Dismiss"), "its Dismiss shows at {width}");
    }
}

/// Each place a plugin draws shows what it drew under its name: rows in a
/// message's and a person's menu, a section on a profile, a tab in the
/// picker, and buttons in the voice panel and the call.
#[test]
fn every_slot_draws_under_its_plugins_name() {
    use crate::picker::{Event as PickerEvent, Tab, Target};

    let (mut app, _data) = super::preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    let button = |id: &str, label: &str| {
        format!(
            r#"{{ "rows": [{{ "cells": [{{ "kind": "button", "id": "{id}", "text": "{label}" }}] }}] }}"#
        )
    };
    // The menus' rows, each under its plugin's name, at most four a plugin.
    draw(
        &mut app,
        Slot::MenuMessage,
        &button("translate", "Translate"),
    );
    draw(
        &mut app,
        Slot::MenuUser,
        &button("note", "Add a quick note"),
    );
    for (slot, label) in [
        (Slot::MenuMessage, "Translate"),
        (Slot::MenuUser, "Add a quick note"),
    ] {
        let rows = crate::plugin_slots::menu_rows(app.client.state(), slot, "subject");
        assert_eq!(rows.len(), 1, "{slot:?}");
        assert_eq!(rows[0].label.as_ref(), label);
        assert_eq!(rows[0].subtitle.as_deref(), Some("Window Test"));
        assert!(rows[0].separated, "under a line, after Yapfast's own");
    }
    // A section on a person's card.
    draw(
        &mut app,
        Slot::ProfileSection,
        r#"{ "rows": [{ "cells": [{ "text": "Pronouns checked" }] }] }"#,
    );
    let user = app
        .client
        .state()
        .members
        .iter()
        .flatten()
        .flat_map(|members| members.iter())
        .find(|member| member.name == "Riley")
        .map(|member| member.id.clone())
        .expect("Riley in the preview");
    app.client
        .dispatch(yapfast_core::application::Action::ShowProfile { user, large: true });
    let _ = app.sync();
    assert!(shows(&app, "Pronouns checked"), "the profile's section");
    assert!(shows(&app, "WINDOW TEST"), "headed by its plugin");
    // A tab in the picker, named by its plugin, showing its drawing.
    let _ = app.key(
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        iced::keyboard::Modifiers::empty(),
    );
    draw(
        &mut app,
        Slot::PickerTab,
        r#"{ "rows": [{ "cells": [{ "text": "Kaomoji go here" }] }] }"#,
    );
    super::change(&mut app, |state| {
        state.selected = Some("discord:10:11".into());
        state.room_ready = true;
    });
    let _ = app.update(crate::app::Message::OpenPicker(
        Target::Composer,
        Tab::Emoji,
    ));
    // The preview picks no GIFs, so its picker has no tabs: as a Discord
    // room's composer has them.
    app.win.picker.get_mut().expect("the picker").tabs = true;
    assert!(shows(&app, "Window Test"), "the plugin's tab");
    assert!(!shows(&app, "Kaomoji go here"));
    let _ = app.update(crate::app::Message::Picker(PickerEvent::PluginTab(
        PLUGIN.into(),
    )));
    assert!(shows(&app, "Kaomoji go here"), "its tab chosen");
}

/// In a call, a plugin's buttons show in the voice panel and in the call's
/// own controls, apart from Yapfast's.
#[test]
fn a_plugins_buttons_show_in_the_voice_panel_and_the_call() {
    let (mut app, _data) = super::preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    let room = "lounge".to_owned();
    app.client
        .dispatch(yapfast_core::application::Action::SelectRoom(room.clone()));
    let _ = app.sync();
    app.client
        .dispatch(yapfast_core::application::Action::SelectRoom(room));
    let _ = app.sync();
    assert!(app.client.state().voice.active(), "in the call");
    draw(
        &mut app,
        Slot::VoicePanel,
        r#"{ "rows": [{ "cells": [{ "kind": "button", "id": "clip", "text": "Clip it" }] }] }"#,
    );
    draw(
        &mut app,
        Slot::CallActions,
        r#"{ "rows": [{ "cells": [{ "kind": "button", "id": "rec", "text": "Notes" }] }] }"#,
    );
    assert!(shows(&app, "Clip it"), "the voice panel's");
    assert!(shows(&app, "Notes"), "the call's");
}

/// A field in a slot shows what was typed at once, not when the plugin next
/// draws, and the plugin's own text from then on once it draws again.
#[test]
fn a_slots_field_shows_what_was_typed_at_once() {
    use crate::plugin_slots::Scope;
    use crate::plugin_tree::Act;

    let (mut app, _data) = super::preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    draw(
        &mut app,
        Slot::Banner,
        r#"{ "rows": [{ "cells": [{ "kind": "input", "id": "keyword", "text": "Keyword" }] }] }"#,
    );
    let scope = Scope {
        plugin: PLUGIN.into(),
        slot: Slot::Banner,
        subject: String::new(),
    };
    let _ = app.update(crate::app::Message::PluginAct(
        scope,
        Act::Edit {
            id: "keyword".into(),
            value: "lunch".into(),
        },
    ));
    assert!(shows(&app, "lunch"), "the typed text shows at once");
    // The plugin answers with its own text: that shows.
    draw(
        &mut app,
        Slot::Banner,
        r#"{ "rows": [{ "cells": [{ "kind": "input", "id": "keyword", "text": "Keyword", "value": "dinner" }] }] }"#,
    );
    assert!(shows(&app, "dinner"));
    assert!(!shows(&app, "lunch"));
}

/// The richer widgets draw from one tree: tabs, a section, a grid, a list, a
/// table, a number, a colour, a drop-down, a text area, a spinner, an image
/// and a tooltip; a tab chosen shows its own rows only.
#[test]
fn the_richer_widgets_draw() {
    use crate::plugin_slots::Scope;
    use crate::plugin_tree::Act;

    let (mut app, _data) = super::preview();
    let source = folder_plugin();
    load(&mut app, source.path());
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let trees: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/widgets/slots.json"),
        )
        .expect("the sample's slots"),
    )
    .expect("JSON");
    draw(&mut app, Slot::Banner, &trees["dialog"].to_string());
    // The dialog's tree is drawn in the banner here: one place to look.
    for words in [
        "Overview",
        "Advanced",
        "Channels",
        "Messages",
        "Quiet hours",
        "Muted words",
        "Accent",
        "Syncing",
        "#general",
        "Mentions",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Retries"), "the second tab's rows wait");
    let _ = app.update(crate::app::Message::PluginAct(
        Scope {
            plugin: PLUGIN.into(),
            slot: Slot::Banner,
            subject: String::new(),
        },
        Act::Local {
            key: "tab:pages".into(),
            value: 1,
        },
    ));
    assert!(shows(&app, "Retries"), "the second tab shows");
    assert!(!shows(&app, "Channels"), "and the first's rows go");
}

/// A plugin's colours paint the window, except a text colour that would be
/// hard to read and except on the screens no plugin may restyle; the person
/// turns them off on its page and gets Yapfast's own look back.
#[test]
fn a_plugins_colours_apply_except_on_protected_screens() {
    use crate::settings::{Event, PluginEvent, Section};

    let (mut app, _data) = super::preview();
    let stock = app.theme.clone();
    let text = crate::plugins_ui::argb_text(stock.chat);
    let source = folder_plugin_with(&format!(
        r##""permissions": {{ "capabilities": ["ui.modify"] }},
        "ui": {{ "colors": {{ "accent": "#7a1fa2", "text": "{text}" }},
                 "sizes": {{ "sidebar-width": 333 }},
                 "parts": {{ "hide": ["timeline.typing"] }} }},"##
    ));
    load(&mut app, source.path());
    let pink = iced::Color::from_rgb8(0x7a, 0x1f, 0xa2);
    assert_eq!(app.theme.accent, pink, "the plugin's accent");
    assert_eq!(app.theme.text, stock.text, "unreadable text is left out");
    assert_eq!(crate::sidebar::width(&app.layout), 333.0, "its size");
    assert!(crate::plugins_ui::hides(
        app.plugin_ui.as_deref(),
        yapfast_core::plugins::ui::Part::TimelineTyping
    ));
    // Settings → Plugins shows Yapfast's own look.
    let _ = app.update(crate::app::Message::Settings(Event::Open(Section::Plugins)));
    assert_eq!(app.theme.accent, stock.accent, "protected");
    let _ = app.update(crate::app::Message::Settings(Event::Open(Section::Account)));
    assert_eq!(app.theme.accent, pink, "and the plugin's again elsewhere");
    // So does a plugin's question about a site.
    app.client.app.state.plugin_consent = Some(ConsentAsk {
        plugin: yapfast_plugin_api::PluginId::parse(PLUGIN).expect("an id"),
        name: "Window Test".into(),
        host: "api.example.com".into(),
    });
    let _ = app.update(crate::app::Message::Nothing);
    assert_eq!(app.theme.accent, stock.accent, "protected");
    app.client.app.state.plugin_consent = None;
    let _ = app.update(crate::app::Message::Nothing);
    assert_eq!(app.theme.accent, pink);
    // Turned off on its page: Yapfast's own look, and its width goes.
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::SetLooks {
            id: PLUGIN.into(),
            on: false,
        },
    )));
    let _ = app.update(crate::app::Message::Settings(Event::Open(Section::Account)));
    assert_eq!(app.theme.accent, stock.accent, "turned off");
    assert_ne!(crate::sidebar::width(&app.layout), 333.0);
}

/// The devtools are off until Advanced turns them on, and then a plugin's
/// page shows its console, slots and calls, and Reload.
#[test]
fn the_devtools_are_off_until_turned_on() {
    use crate::settings::{Event, PluginEvent, Section};

    let (mut app, _data) = super::preview();
    assert!(!app.layout.plugin_devtools, "off by default");
    let source = folder_plugin();
    load(&mut app, source.path());
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let _ = app.update(crate::app::Message::Settings(Event::Open(Section::Plugins)));
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::Open(PLUGIN.into()),
    )));
    assert!(!shows(&app, "DEVTOOLS"));
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::SetDevtools(true),
    )));
    assert!(app.layout.plugin_devtools);
    assert!(shows(&app, "DEVTOOLS"));
    assert!(shows(&app, "Console") && shows(&app, "Slots") && shows(&app, "Calls"));
    assert!(shows(&app, "Reload"));
    draw(
        &mut app,
        Slot::Toast,
        r#"{ "rows": [{ "cells": [{ "text": "Hi" }] }] }"#,
    );
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::DevTab(crate::settings::DevTab::Slots),
    )));
    assert!(present(&app, "toast  1 cells  text"), "the slot inspector");
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::SetDevtools(false),
    )));
    assert!(!shows(&app, "DEVTOOLS"));
}

/// The site question denies itself after a minute without an answer, for
/// this time only, and says so while it waits.
#[test]
fn the_site_question_denies_itself_after_a_minute() {
    let (mut app, _data) = super::preview();
    {
        let handle = app.client.runtime().handle();
        let _entered = handle.enter();
        app.client.open_plugins();
    }
    let plugin = yapfast_plugin_api::PluginId::parse("com.example.weather").expect("an id");
    app.client.app.state.plugin_consent = Some(ConsentAsk {
        plugin: plugin.clone(),
        name: "Weather".into(),
        host: "api.example.com".into(),
    });
    let _ = app.sync();
    assert!(app.consent_since.is_some(), "its clock started");
    let _ = app.update(crate::app::Message::Dialog(
        crate::dialogs::Event::ConsentTick,
    ));
    assert!(app.client.state().plugin_consent.is_some(), "not yet");
    let started = std::time::Instant::now()
        .checked_sub(crate::dialogs::CONSENT_WAIT + std::time::Duration::from_secs(1))
        .expect("an earlier time");
    if let Some((_, since)) = app.consent_since.as_mut() {
        *since = started;
    }
    let _ = app.update(crate::app::Message::Dialog(
        crate::dialogs::Event::ConsentTick,
    ));
    assert!(app.client.state().plugin_consent.is_none(), "denied");
    let grants = app
        .client
        .state()
        .plugins
        .as_ref()
        .expect("the plugin system")
        .grants()
        .clone();
    assert!(grants.allowed(&plugin).is_empty(), "nothing kept");
    assert!(app.consent_since.is_none());
}

/// A plugin's own settings page: its settings' form until it draws one, then
/// its drawing, whose cells named by a setting's key show and save that
/// setting.
#[test]
fn a_plugins_settings_page_draws_and_saves() {
    use crate::plugin_tree::Act;
    use crate::settings::{Event, PluginEvent, Section};

    let (mut app, _data) = super::preview();
    let source = folder_plugin_with(
        r#""settings": [
            { "key": "loud", "type": "bool", "label": "Loud mode" },
            { "key": "greeting", "type": "string", "label": "Greeting", "default": "hey" }
        ],"#,
    );
    load(&mut app, source.path());
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(&mut app);
    let _ = app.update(crate::app::Message::Settings(Event::Open(Section::Plugins)));
    assert!(shows(&app, "Settings"), "the row offers its page");
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::OpenSettings(PLUGIN.into()),
    )));
    assert!(shows(&app, "Window Test settings"));
    assert!(shows(&app, "Loud mode"), "its form, until it draws");
    draw(
        &mut app,
        Slot::SettingsPage,
        r#"{ "rows": [{ "cells": [{ "kind": "section", "id": "basics", "text": "Basics", "rows": [
            { "cells": [{ "kind": "toggle", "id": "loud", "text": "Shout everything" }] },
            { "cells": [{ "kind": "input", "id": "greeting", "text": "Greeting" }] }
        ] }] }] }"#,
    );
    assert!(shows(&app, "Basics"), "its own page");
    assert!(shows(&app, "hey"), "a setting's value in its cell");
    assert!(shows(&app, "Reset to defaults"));
    let _ = app.update(crate::app::Message::Settings(Event::Plugin(
        PluginEvent::Act {
            key: None,
            act: Act::Set {
                id: "loud".into(),
                value: "true".into(),
            },
        },
    )));
    // The store writes on a thread of its own, once 500 ms pass quietly.
    let saved = || {
        app.client
            .app
            .plugins_page()
            .plugin(PLUGIN)
            .expect("installed")
            .settings
            .values
            .get("loud")
            .cloned()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while saved() != Some(yapfast_core::plugins::settings::Value::Bool(true))
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(
        saved(),
        Some(yapfast_core::plugins::settings::Value::Bool(true)),
        "saved"
    );
}

/// The interface changes that were checked and listed but drew nothing do now:
/// a built-in row relabelled, moved and hidden, the gap between messages, a
/// channel row's height, and a part drawn by the plugin that replaces it.
#[test]
fn the_replaced_parts_rows_and_the_last_two_sizes_apply() {
    use crate::menus::MenuAct;
    use yapfast_core::application::MenuChoice as C;
    use yapfast_core::plugins::ui::{NetworkName, Subject, Target};

    let (mut app, _data) = preview();
    let source = folder_plugin_with(
        r#""permissions": { "capabilities": ["ui.modify"] },
        "ui": { "menu": [
                  { "target": "channel", "row": "copy-link", "label": "Copy channel link" },
                  { "target": "channel", "row": "mark-read", "after": "copy-id" },
                  { "target": "channel", "row": "invite", "hide": true } ],
                "parts": { "replace": ["timeline.intro"] },
                "sizes": { "message-gap": 6, "channel-row-height": 40 } },"#,
    );
    load(&mut app, source.path());
    assert_eq!(crate::plugins_ui::message_gap(), Some(6.0));
    assert_eq!(crate::plugins_ui::channel_row(), Some(40.0));
    assert_eq!(crate::timeline::row_gap(), 3.0, "half above, half under");
    assert_eq!(crate::sidebar::channel_row(), 40.0);

    let row = |choice: C, label: &'static str| crate::menus::MenuRow {
        act: MenuAct::Place(choice),
        label: label.into(),
        icon: None,
        danger: false,
        separated: false,
        disabled: false,
        subtitle: None,
        check: Default::default(),
        key: String::new(),
        submenu: Vec::new(),
        click: None,
    };
    let mut rows = vec![
        row(C::MarkRead, "Mark As Read"),
        row(C::Invite, "Invite People"),
        row(C::CopyLink, "Copy Link"),
        row(C::CopyId, "Copy Channel ID"),
        row(C::DeleteChannel, "Delete Channel"),
    ];
    let subject = Subject {
        network: NetworkName::Discord,
        own: false,
        dm: false,
        has_attachment: false,
        is_bot: false,
        roles: &[],
    };
    let overrides = app.plugin_ui.clone().expect("the plugin's changes");
    crate::plugins_ui::edit_rows(&overrides, Target::Channel, &subject, &mut rows);
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_ref()).collect();
    assert_eq!(
        labels,
        [
            "Copy channel link",
            "Copy Channel ID",
            "Mark As Read",
            "Delete Channel"
        ],
        "relabelled, moved after Copy ID, Invite hidden, the danger row untouched"
    );
    assert_eq!(
        rows[0].subtitle.as_deref(),
        Some("Renamed by the Window Test plugin"),
        "a renamed row says whose words they are"
    );

    // The welcome is Yapfast's until the plugin draws, then the plugin's,
    // credited.
    draw(
        &mut app,
        Slot::TimelineIntro,
        r#"{ "rows": [{ "cells": [{ "kind": "text", "text": "A welcome of its own" }] }] }"#,
    );
    let welcome = crate::plugin_slots::replaced(app.ctx(), Slot::TimelineIntro, "", true);
    assert!(
        welcome.is_some(),
        "the replacing plugin's tree draws in its place"
    );
}

/// A link a plugin offers to open in an app shows the leaving dialog with the
/// offer beside Visit site, credited, and the offer's address is what opens.
#[test]
fn a_link_offer_shows_in_the_leaving_dialog_under_its_plugins_name() {
    use yapfast_core::application::plugins::{LinkAsk, LinkOfferShown};

    let (mut app, _data) = preview();
    app.leaving = Some("https://open.spotify.com/track/1".into());
    app.client.app.state.plugin_link = Some(LinkAsk {
        url: "https://open.spotify.com/track/1".into(),
        ids: vec![1],
        offers: vec![LinkOfferShown {
            plugin: yapfast_plugin_api::PluginId::parse("com.example.extras").expect("an id"),
            name: "Extras".into(),
            label: "Open in Spotify".into(),
            uri: "spotify:track:1".into(),
        }],
    });
    let _ = app.sync();
    assert!(shows(&app, "Leaving Yapfast"), "the dialog still asks");
    assert!(shows(&app, "Open in Spotify"));
    assert!(shows(&app, "Offered by the Extras plugin"));
    assert!(
        shows(&app, "Visit site"),
        "and the browser is still a choice"
    );
    interact(&mut app, |ui| {
        ui.click("Open in Spotify").expect("the offer");
    });
    assert!(
        app.leaving.is_none(),
        "the dialog closes once a choice is made"
    );
    assert!(app.client.app.state.plugin_link.is_none());
}

/// The window's title takes a plugin's words after Yapfast's own, and a
/// plugin's sound says whose it is while it plays.
#[test]
fn the_title_and_a_playing_sound_name_their_plugin() {
    use yapfast_core::application::plugins::{PlayingShown, TitleShown};

    let (mut app, _data) = preview();
    let plugin = yapfast_plugin_api::PluginId::parse("com.example.extras").expect("an id");
    app.client.app.state.plugin_title = Some(TitleShown {
        plugin: plugin.clone(),
        name: "Extras".into(),
        suffix: "ready in 1.2 s".into(),
    });
    let title = app.title(iced::window::Id::unique());
    assert!(title.ends_with("Yapfast · ready in 1.2 s"), "{title}");
    app.client.app.state.plugin_playing = Some(PlayingShown {
        plugin,
        name: "Extras".into(),
        at: yapfast_core::clock::now_ms(),
    });
    let _ = app.sync();
    assert!(shows(&app, "The Extras plugin is playing a sound"));
}

/// A plugin's order for the quick switcher's search reorders and filters the
/// list, and the list says whose order it is.
#[test]
fn the_switcher_shows_a_plugins_order_and_says_so() {
    use yapfast_core::application::plugins::SwitcherRanking;

    let (mut app, _data) = preview();
    app.client.app.state.switcher = Some("e".into());
    let _ = app.sync();
    let found: Vec<String> = app
        .quick
        .places
        .iter()
        .map(|place| place.id.clone())
        .collect();
    assert!(found.len() >= 2, "the preview has places: {found:?}");
    let order = vec![found[1].clone(), found[0].clone()];
    app.client.app.state.plugin_switcher = Some(SwitcherRanking {
        plugin: yapfast_plugin_api::PluginId::parse("com.example.extras").expect("an id"),
        name: "Extras".into(),
        query: "e".into(),
        order: order.clone(),
    });
    let _ = app.sync();
    let shown: Vec<String> = app
        .quick
        .places
        .iter()
        .map(|place| place.id.clone())
        .collect();
    assert_eq!(shown, order, "its order, and only the places it kept");
    assert!(shows(&app, "Sorted by the Extras plugin"));
}
