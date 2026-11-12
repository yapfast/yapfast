//! What a manifest may say, read the way an install reads it.
//!
//! `testing/plugins/quiet/` is the manifest a person would write: it carries the
//! documented `ui` section, and nothing else in the tree hands it to the
//! installer, which is how the install-time parser and the one the interface
//! reads came apart unnoticed. A plugin whose manifest named `ui` was refused as
//! a typo at install and drawn whole from a folder. These tests hold the two
//! reads to one file: what the interface draws, an install keeps.
//!
//! ```sh
//! cargo test --profile fast -p yapfast-core --features plugins --test plugins_manifest
//! ```

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use yapfast_core::plugins::registry::Registry;
use yapfast_core::plugins::{installing, store::Store};
use yapfast_plugin_api::capability::Capability;
use yapfast_plugin_api::manifest::ManifestError;
use yapfast_plugin_api::{PluginId, PluginManifest};

/// The parts of a plugin's folder this test reads, and the section it and the
/// install share.
fn quiet() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/plugins/quiet")
}

/// The fixture's manifest, exactly as the file holds it.
fn quiet_manifest() -> Value {
    let path = quiet().join("yapfast-plugin.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The fixture's manifest with a change made to it, written back as JSON.
fn changed(edit: impl FnOnce(&mut serde_json::Map<String, Value>)) -> String {
    let mut manifest = quiet_manifest();
    edit(manifest.as_object_mut().expect("a manifest is an object"));
    serde_json::to_string_pretty(&manifest).expect("a manifest writes as JSON")
}

/// The fixture with the two button sections it doesn't have, which is the
/// manifest both reads must take.
fn with_buttons() -> String {
    changed(|manifest| {
        manifest.insert(
            "rail_items".to_owned(),
            json!([{ "label": "Notes", "action": "notes" }]),
        );
        manifest.insert(
            "header_buttons".to_owned(),
            json!([{ "label": "Summarise", "action": "summarise" }]),
        );
    })
}

/// The `ui` section the fixture names, which is what the interface draws.
#[test]
fn a_manifest_with_ui_rail_items_and_header_buttons_reads() {
    let text = with_buttons();
    let manifest = PluginManifest::parse(&text).expect("a manifest the interface draws");
    let ui = manifest.ui.as_ref().expect("its ui section");
    assert_eq!(ui.menu.len(), 3, "three built-in rows change");
    assert_eq!(
        ui.parts.hide,
        ["timeline.typing", "message.actions.react"],
        "two parts are hidden"
    );
    assert_eq!(ui.colors.len(), 3);
    assert_eq!(ui.sizes.get("message-group-gap"), Some(&22.0));
    assert_eq!(manifest.menus.len(), 2);
    assert_eq!(manifest.rail_items[0].action, "notes");
    assert_eq!(manifest.header_buttons[0].label, "Summarise");

    // The other read of the same file draws the same sections: the folder's
    // read keeps what `ui.rs` applies, not a second copy of the shape.
    let drawn =
        yapfast_core::plugins::Manifest::parse(&text).expect("the manifest a folder is read as");
    assert_eq!(drawn.ui.expect("its ui section").menu.len(), 3);
    assert_eq!(drawn.rail_items.len(), 1);
    assert_eq!(drawn.header_buttons.len(), 1);
    assert_eq!(drawn.menus.len(), 2);
}

/// A menu row's target is the closed list the interface draws menus for, and a
/// thread's menu is one of them: the install path knew fewer than the interface.
#[test]
fn a_menu_target_the_interface_draws_reads() {
    let text = changed(|manifest| {
        let menus = manifest
            .get_mut("menus")
            .and_then(Value::as_array_mut)
            .expect("the fixture adds menu rows");
        menus[1]["target"] = json!("thread");
    });
    let manifest = PluginManifest::parse(&text).expect("a thread's menu is drawn");
    assert_eq!(manifest.menus.len(), 2);
}

/// Strictness stays: a field nobody knows is a refused install, in the manifest
/// and in its `ui` section alike.
#[test]
fn a_field_nobody_knows_is_still_refused() {
    let outside = changed(|manifest| {
        manifest.insert("width".to_owned(), json!(3));
    });
    let error = PluginManifest::parse(&outside).expect_err("`width` is no field of a manifest");
    assert!(matches!(&error, ManifestError::Json(_)), "{error}");
    assert!(error.to_string().contains("width"), "{error}");

    let inside = changed(|manifest| {
        let ui = manifest
            .get_mut("ui")
            .and_then(Value::as_object_mut)
            .expect("the fixture has a ui section");
        ui.insert("menu_rows".to_owned(), json!([]));
    });
    let error = PluginManifest::parse(&inside).expect_err("a `ui` field nobody knows");
    assert!(error.to_string().contains("menu_rows"), "{error}");
}

/// A target the interface has no menu for is refused where the file is read,
/// with the target's own name in the sentence.
#[test]
fn a_ui_section_naming_an_unknown_target_is_refused() {
    let text = changed(|manifest| {
        let ui = manifest
            .get_mut("ui")
            .and_then(Value::as_object_mut)
            .expect("the fixture has a ui section");
        let rows = ui
            .get_mut("menu")
            .and_then(Value::as_array_mut)
            .expect("the fixture changes rows");
        rows[2]["target"] = json!("toast");
    });
    let error = PluginManifest::parse(&text).expect_err("no menu is about a toast");
    assert!(error.to_string().contains("toast"), "{error}");
}

/// A button is drawn by its label, so a label that can't show is refused rather
/// than drawn blank.
#[test]
fn a_button_with_no_label_is_refused() {
    let text = changed(|manifest| {
        manifest.insert(
            "rail_items".to_owned(),
            json!([{ "label": "  ", "action": "notes" }]),
        );
    });
    assert!(
        matches!(
            PluginManifest::parse(&text),
            Err(ManifestError::Label { .. })
        ),
        "an empty label is a fault in the manifest, not something to draw"
    );
}

/// No more of them than the interface draws for one plugin.
#[test]
fn more_buttons_than_the_interface_draws_is_refused() {
    let text = changed(|manifest| {
        let items: Vec<Value> = (0..=yapfast_plugin_api::manifest::CHANGES)
            .map(|index| json!({ "label": format!("Button {index}"), "action": "notes" }))
            .collect();
        manifest.insert("rail_items".to_owned(), json!(items));
    });
    assert!(
        matches!(
            PluginManifest::parse(&text),
            Err(ManifestError::TooMany { .. })
        ),
        "the interface draws 64, so a manifest may ask for 64"
    );
}

/// The whole way in: the manifest the interface draws is fetched, reviewed,
/// installed, and read again out of the store with its sections whole.
#[test]
fn an_install_keeps_the_sections_the_interface_draws() {
    let folder = tempfile::tempdir().expect("a folder");
    std::fs::write(folder.path().join("yapfast-plugin.json"), with_buttons())
        .expect("its manifest");
    // The fixture names no module, so the file the person picks is the module it
    // means: the manifest is the plugin, as it is for one that only draws.
    let module = folder.path().join("plugin.wasm");
    std::fs::copy(quiet().join("../echo.wasm"), &module).expect("a module");

    let fetched = installing::from_wasm(&module).expect("the install's read of it");
    assert_eq!(fetched.id.as_str(), "com.example.quiet");
    assert_eq!(
        fetched.review().capabilities,
        ["ui.modify"],
        "the review lists what the manifest declares"
    );

    let data = tempfile::tempdir().expect("a data directory");
    let mut store = Store::open(data.path()).expect("a store");
    store.install(&fetched.package()).expect("it installs");

    let id = PluginId::parse("com.example.quiet").expect("its id");
    let registry = Registry::load(&store);
    let entry = registry.entry(&id).expect("it is installed");
    assert!(!entry.enabled, "installing doesn't enable");
    let ui = entry
        .manifest
        .ui
        .as_ref()
        .expect("the store keeps the ui section it was given");
    assert_eq!(ui.menu.len(), 3);
    assert_eq!(entry.manifest.rail_items.len(), 1);
    assert_eq!(entry.manifest.header_buttons.len(), 1);
}

/// The review dialog lists what a manifest declares, so a capability nothing in
/// the build looks at is left out of what is read: a line promising a recording
/// with nothing behind it is worse than a short list.
#[test]
fn the_review_leaves_out_a_capability_nothing_checks() {
    let text = changed(|manifest| {
        manifest.insert(
            "permissions".to_owned(),
            json!({
                "capabilities": ["ui.modify", "files.save", "microphone", "typing.intercept"]
            }),
        );
    });
    let manifest = PluginManifest::parse(&text).expect("a plugin may list what it would use");
    assert_eq!(
        manifest.permissions.capabilities,
        [Capability::UiModify, Capability::TypingIntercept],
        "the typing hook is drawn from this capability; the save dialog and the microphone aren't"
    );
    assert_eq!(
        manifest.capabilities(),
        [Capability::TypingIntercept, Capability::UiModify],
        "and the dialog's order is the contract crate's"
    );
}
