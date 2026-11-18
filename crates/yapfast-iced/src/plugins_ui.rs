//! The changes enabled plugins make to the window's own interface: buttons at
//! the foot of the server rail and in a conversation's header, rows added to a
//! menu, and the built-in rows they hide, relabel or move.
//!
//! What they ask for is their manifests' `ui` sections, checked and merged by
//! `yapfast_core::plugins::ui`; this holds the result for the window to read,
//! taken again whenever a plugin starts or stops.

use std::collections::BTreeMap;
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::plugins::Input;
use yapfast_core::plugins::ui::{self, Checked, Subject, Target, UiOverrides};

use crate::menus::{MenuAct, MenuRow};

/// What the enabled plugins ask of the interface, or nothing when no plugin
/// changes it.
pub fn overrides(state: &AppState) -> Option<Arc<UiOverrides>> {
    let plugins = state.plugins.as_deref()?;
    let checked: Vec<Checked> = plugins
        .listed()
        .into_iter()
        .filter(|listed| listed.enabled)
        .filter_map(|listed| interface(&listed.manifest))
        .map(|manifest| ui::check(&manifest))
        .collect();
    let (overrides, conflicts) = ui::derive(&checked, &BTreeMap::new());
    for conflict in &conflicts {
        tracing::warn!(
            kept = %conflict.kept,
            lost = %conflict.lost,
            "two plugins change the same part of the interface"
        );
    }
    overrides
}

/// The interface's view of a manifest, which is the same JSON read through
/// the stricter type the loader uses.
fn interface<M: serde::Serialize>(manifest: &M) -> Option<yapfast_core::plugins::Manifest> {
    serde_json::to_value(manifest)
        .ok()
        .and_then(|json| serde_json::from_value(json).ok())
}

/// The rows `target`'s menu gains, under a line, after its own.
pub fn rows(
    overrides: &UiOverrides,
    target: Target,
    subject: &Subject<'_>,
    target_id: &str,
) -> Vec<MenuRow> {
    let mut rows = Vec::new();
    for (added, _) in &overrides.added {
        if added.target != target || !added.when.matches(subject) {
            continue;
        }
        let Some((plugin, action)) = overrides.chosen(&added.action) else {
            continue;
        };
        rows.push(MenuRow {
            act: MenuAct::Plugin(Input::Menu {
                plugin: plugin.id.clone(),
                action: action.to_owned(),
                target: target_id.to_owned(),
            }),
            label: added.label.clone().into(),
            icon: None,
            danger: false,
            separated: true,
            disabled: false,
            // Whose row it is, so no plugin's row passes for Yapfast's own.
            subtitle: Some(plugin.name.clone().into()),
            check: Default::default(),
            key: String::new(),
            submenu: Vec::new(),
            click: None,
        });
    }
    rows
}

/// What choosing a plugin's rail item or header button tells its plugin: the
/// open conversation is the target, as a menu row's is.
pub fn chosen(overrides: &UiOverrides, action: &str, target: &str) -> Option<Input> {
    let (plugin, action) = overrides.chosen(action)?;
    Some(Input::Menu {
        plugin: plugin.id.clone(),
        action: action.to_owned(),
        target: target.to_owned(),
    })
}
