//! Plugins in the window: an enabled plugin's buttons and slots draw, a
//! plugin's request for a site is asked about and answered, and a plugin's
//! link waits in the leaving dialog. Each runs the way iced does, with the
//! background runtime entered.

use std::path::Path;

use yapfast_core::application::{Effect, plugins::ConsentAsk};
use yapfast_core::plugins::{page::Request, slots::Slot, ui::Tree};

use super::{interact, local, preview, shows};
use crate::app::App;

/// A plugin in a folder of its own whose manifest adds a header button; its
/// module is a test fixture that is never asked for anything here.
fn folder_plugin() -> tempfile::TempDir {
    let source = tempfile::tempdir_in(local()).expect("plugin source");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/echo.wasm"),
        source.path().join("plugin.wasm"),
    )
    .expect("a module");
    std::fs::write(
        source.path().join("yapfast-plugin.json"),
        r#"{
            "api": 1, "id": "local.window-test", "name": "Window Test",
            "version": "0.1.0", "wasm": "plugin.wasm",
            "header_buttons": [{ "label": "Wave hello", "action": "wave" }]
        }"#,
    )
    .expect("a manifest");
    source
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
        shows(&app, "Window Test \u{b7} plugin local.window-test"),
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
