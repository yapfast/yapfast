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

/// Calls `$then!` with each of the theme's colours and the name a plugin
/// gives it (`ui::COLOURS`): the field's name with `-` for `_`.
macro_rules! colour_fields {
    ($($name:ident,)*) => {
        /// The theme's colour a plugin names `token`.
        fn colour(theme: &crate::theme::Theme, token: &str) -> Option<iced::Color> {
            $(if token.replace('-', "_") == stringify!($name) {
                return Some(theme.$name);
            })*
            None
        }

        /// The theme's colour a plugin names `token`, to set.
        fn colour_mut<'t>(
            theme: &'t mut crate::theme::Theme,
            token: &str,
        ) -> Option<&'t mut iced::Color> {
            $(if token.replace('-', "_") == stringify!($name) {
                return Some(&mut theme.$name);
            })*
            None
        }
    };
}

yapfast_iced_kit::for_theme_colours!(colour_fields);

/// A colour as the ARGB a plugin's is checked in.
fn argb(colour: iced::Color) -> u32 {
    let [r, g, b, a] = colour.into_rgba8();
    u32::from_be_bytes([a, r, g, b])
}

/// `stock` with the plugins' colours laid over it, leaving out any that would
/// make text hard to read on it (`ui::unreadable`), and those refused, with
/// why, for the plugin's page and the log.
pub fn dressed(
    stock: &crate::theme::Theme,
    overrides: &UiOverrides,
) -> (crate::theme::Theme, Vec<(&'static str, String)>) {
    let overlay: Vec<(&'static str, u32)> = overrides
        .colours
        .iter()
        .map(|&(token, colour, _)| (token, colour))
        .collect();
    let refused = ui::unreadable(|token| colour(stock, token).map(argb), &overlay);
    let mut shown = stock.clone();
    for (token, value) in overlay {
        if refused.iter().any(|(gone, _)| *gone == token) {
            continue;
        }
        if let Some(field) = colour_mut(&mut shown, token) {
            let [a, r, g, b] = value.to_be_bytes();
            *field = iced::Color::from_rgba8(r, g, b, f32::from(a) / 255.0);
        }
    }
    (shown.worked_out(), refused)
}

/// Whether what shows is a screen no plugin may restyle: signing in and its
/// security question, a plugin's question about a site, the question before
/// leaving for a link, and Settings → Plugins itself, where a plugin is
/// turned off. These show the theme as the person chose it.
pub fn protected(app: &crate::app::App) -> bool {
    !crate::signin::workspace(app)
        || app.client.app.mfa_prompt().is_some()
        || app.client.state().plugin_consent.is_some()
        || app.leaving.is_some()
        || app
            .win
            .settings
            .get()
            .is_some_and(|settings| settings.section == Some(crate::settings::Section::Plugins))
}

/// The theme shown: the person's own, or with the plugins' colours while no
/// protected screen shows. Swapped only when that changes, so a frame
/// that changes neither costs one test.
pub fn dress(app: &mut crate::app::App) {
    let protected = protected(app);
    let wanted = match (&app.looks.dressed, protected) {
        (Some(dressed), false) => dressed.clone(),
        _ => app.looks.stock.clone(),
    };
    app.put_theme(wanted);
}

/// The person's theme and the plugins' version of it.
#[derive(Debug)]
pub struct Looks {
    pub stock: crate::theme::Theme,
    /// `None` while no enabled plugin sets a colour, or none could be kept.
    pub dressed: Option<crate::theme::Theme>,
}

impl Looks {
    pub fn new(stock: crate::theme::Theme) -> Self {
        Self {
            stock,
            dressed: None,
        }
    }

    /// Takes `stock` as the person's theme, and lays `overrides`' colours
    /// over it again.
    pub fn dress(&mut self, stock: crate::theme::Theme, overrides: Option<&UiOverrides>) {
        self.dressed = overrides
            .filter(|overrides| !overrides.colours.is_empty())
            .map(|overrides| {
                let (dressed, refused) = dressed(&stock, overrides);
                for (token, why) in refused {
                    tracing::info!(%token, %why, "a plugin's colour was left out");
                }
                dressed
            })
            .filter(|dressed| *dressed != stock);
        self.stock = stock;
    }
}

/// The overrides of the plugins whose looks the person kept on: a plugin's
/// whose changes to Yapfast's look are off keeps its added rows and buttons,
/// and loses its colours, sizes and hidden or moved parts.
pub fn without_looks(
    manifests: &mut [yapfast_core::plugins::Manifest],
    off: &std::collections::BTreeSet<String>,
) {
    for manifest in manifests
        .iter_mut()
        .filter(|manifest| off.contains(&manifest.id))
    {
        manifest.ui = None;
    }
}

thread_local! {
    /// The gap a plugin sets between message groups, for the timeline's rows,
    /// which are measured without the window's state at hand. The window is
    /// laid out and drawn on one thread.
    static GROUP_GAP: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

/// The gap between message groups a plugin sets, if any does.
pub fn group_gap() -> Option<f32> {
    GROUP_GAP.with(std::cell::Cell::get)
}

/// Keeps what the overrides set of the sizes the window reads without them.
pub fn keep_sizes(overrides: Option<&UiOverrides>, layout: &mut crate::layout::Layout) {
    let size = |size| overrides.and_then(|overrides| overrides.size(size));
    GROUP_GAP.with(|gap| gap.set(size(ui::Size::MessageGroupGap)));
    layout.plugin_sidebar_width = size(ui::Size::SidebarWidth);
    layout.plugin_members_width = size(ui::Size::MembersWidth);
}

/// Whether the plugins hide `part`.
pub fn hides(overrides: Option<&UiOverrides>, part: ui::Part) -> bool {
    overrides.is_some_and(|overrides| overrides.hides(part))
}

/// A colour as the `#rrggbb` a manifest writes, for the tests that make one.
#[cfg(test)]
pub fn argb_text(colour: iced::Color) -> String {
    format!("#{:06x}", argb(colour) & 0x00ff_ffff)
}
