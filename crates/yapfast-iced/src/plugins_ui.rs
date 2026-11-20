//! The changes enabled plugins make to the window's own interface: buttons at
//! the foot of the server rail and in a conversation's header, rows added to a
//! menu, and the built-in rows they hide, relabel or move.
//!
//! What they ask for is their manifests' `ui` sections, checked and merged by
//! `yapfast_core::plugins::ui`; this holds the result for the window to read,
//! taken again whenever a plugin starts or stops.

use std::collections::BTreeMap;
use std::sync::Arc;

use yapfast_core::plugins::Input;
use yapfast_core::plugins::ui::{self, Checked, Subject, Target, UiOverrides};

use crate::menus::{MenuAct, MenuRow};

/// What these plugins' manifests ask of the interface, or nothing when none
/// changes it.
pub fn overrides(manifests: &[yapfast_core::plugins::Manifest]) -> Option<Arc<UiOverrides>> {
    let checked: Vec<Checked> = manifests.iter().map(ui::check).collect();
    for plugin in checked.iter().filter(|plugin| !plugin.refused.is_empty()) {
        tracing::warn!(
            plugin = %plugin.id,
            refused = plugin.refused.len(),
            "some of a plugin's changes to the interface were refused"
        );
    }
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

/// What choosing a plugin's rail item or header button tells its plugin, and
/// the plugin it is: the open conversation is the target, as a menu row's is.
///
/// A rail item or header button keeps its plain action and its plugin's
/// place in the list (`UiOverrides::rail`, `header`), unlike an added menu
/// row, whose action already names its plugin; the two are put together here.
pub fn chosen<'o>(
    overrides: &'o UiOverrides,
    plugin: usize,
    action: &str,
    target: &str,
) -> Option<(&'o yapfast_core::plugins::ui::PluginName, Input)> {
    let id = UiOverrides::row_id(plugin, action);
    let (plugin, action) = overrides.chosen(&id)?;
    Some((
        plugin,
        Input::Menu {
            plugin: plugin.id.clone(),
            action: action.to_owned(),
            target: target.to_owned(),
        },
    ))
}
