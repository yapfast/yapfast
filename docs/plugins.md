# Plugins (v1)

Yapfast's native plugin system: bundled extensions that run in the same
process, identified by a manifest and held to declared capabilities. The
implementation is `crates/yapfast-core/src/plugins.rs`; the window side
(Settings page, density, themes, notification filtering) is in `src/ui.rs`,
`src/ui/theme.rs` and `ui/settings.slint`.

## Manifest

Every plugin ships a manifest with three required parts:

| Field | Meaning |
|---|---|
| `id` | Lowercase letters, digits, hyphens and dots, 1–64 characters, e.g. `compact-mode`. |
| `version` | `major.minor.patch`, e.g. `1.0.0`. |
| `capabilities` | What the plugin may do, from the list below. |

Plus `name`, `description`, `api_version` (the host API the plugin targets,
currently `1`), `bundled` and `enabled_by_default`. A manifest is rejected
at load time when the id or version is malformed, the API version is newer
than the host's, a capability is repeated, or it asks for a privileged
capability while the host policy disallows them (the default).

```json
{
  "id": "compact-mode",
  "name": "Compact mode",
  "version": "1.0.0",
  "description": "Denser rows and tighter spacing.",
  "api_version": 1,
  "capabilities": ["ui_density"],
  "bundled": true,
  "enabled_by_default": false
}
```

## Capabilities

| Capability | What it allows | Privileged? |
|---|---|---|
| `ui_theme` | Contribute a theme to Appearance. | No |
| `ui_density` | Switch the window to a denser layout. | No |
| `ui_settings` | Add a section to Settings. | No |
| `notifications_filter` | Decide whether a desktop notification shows. | No |
| `credentials` | Read stored sign-in credentials. | **Yes** |
| `filesystem` | Read or write files outside the plugin's settings. | **Yes** |
| `network` | Make network requests. | **Yes** |
| `process` | Start processes. | **Yes** |
| `discord_api` | Call the Discord API directly. | **Yes** |

Every capability is denied unless the manifest asks for it and the host
grants it. Privileged capabilities are additionally denied by default even
when asked for, and in v1 they remain unavailable even when granted: the
in-process host has no sandbox to run them in, so it refuses rather than
pretend. See [WASM follow-up](plugins-wasm.md).

## Enforcement

Plugins act through a `Host`, not directly. Every host operation checks
the calling plugin's manifest first:

- `host.set_density(...)` needs `ui_density`.
- `host.contribute_theme(...)` needs `ui_theme`.
- `host.read_credential()`, `host.read_file(...)`,
  `host.network_request(...)`, `host.start_process(...)` and
  `host.discord_request(...)` need their privileged capability, the host
  policy opt-in, and then still report `UnavailableInV1`.

The `PluginManager` validates a manifest before installing the plugin, so
a rejected plugin runs nothing. Hooks (`on_enable`, `compact_density`,
`theme_contributions`, `filter_notification`) each re-check through the
host, and a hook that fails denies that contribution rather than crashing
the window.

## PluginManager

`PluginManager` holds the installed plugins, the enabled state and each
plugin's settings. The enabled state and settings are stored in
`layout.json` (`plugins_enabled`, `plugins_config`), and unknown ids from
a newer layout are ignored on load. Aggregated views:

- `compact_mode()` — true when any enabled plugin draws denser.
- `theme_contributions()` — themes from enabled plugins, in install order.
- `filter_notification(&alert)` — any enabled filter plugin may suppress;
  suppression wins.

## Bundled extensions

| Plugin | Capabilities | What it does |
|---|---|---|
| `compact-mode` | `ui_density` | Denser rows and tighter spacing via the `Density` global in `ui/theme.slint`. |
| `theme-extension` | `ui_theme` | Adds the Midnight Pine theme to Appearance. |
| `notification-filter` | `notifications_filter` | Modes: all, mentions only, or keyword match (comma-separated, case-insensitive). Calls always show. |

All three ship disabled and are turned on in Settings → Plugins.

## Settings page

Settings section 9 (Plugins) lists every installed plugin with its
description, capabilities and an on/off switch. The notification filter
additionally shows its mode choices and keyword field. Toggling writes
`layout.json` at once, and the density and theme list update without a
restart.
