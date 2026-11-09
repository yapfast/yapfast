# Plugin settings

Settings → Plugins is section 9 of the settings screen. It exists only in a
build with the `plugins` Cargo feature, which is off by default: without it,
Settings doesn't list the page. The page is behind an `if` in
`ui/yapfast.slint`, so it costs nothing until it's opened.

It has three choices at the top:

- **Marketplace**: a search field, a category filter and a list of the
  registry's plugins, each with its icon, *Reviewed* when a maintainer read
  its source, what it reaches (`messages, 1 host`) and Install. The list is
  a `ListView` in a `SettingsFrame` with `fill`, so only the rows in sight
  exist, and a row's icon goes through `PictureStore` only while the row
  does.
- **Installed**: a row per plugin with a switch that takes effect at once,
  its status (*Idle*, *Running*, *Suspended: ran out of memory*) and Update
  when there is one, and below them the memory the plugins use while the
  helper runs. A row opens the plugin's own page: settings made from its
  manifest's `settings`, what it can do and the hosts it may reach, each with
  Revoke, its source, commit and license, Update and Roll back, its recent log
  lines, and Uninstall, which asks whether to delete its data too.
- **Add from URL**: a git repository's address and *Find plugin*. Progress and
  errors are said plainly, as *That repository has no yapfast-plugin.json.*

Install, from either the marketplace or a URL, first shows the review: the
plugin's name, icon, source and commit, what it can do in plain words, its
memory cap and what it depends on, *Unreviewed source* for a URL, and Install
or Cancel. Reading messages with any host to connect to gets a sentence of its
own, saying the plugin could send your messages there, and
`messages.send.background` shows its warning in the danger tone.

Advanced, at the page's foot, opens a dialog with Load folder, the memory budget
and how long an idle plugin is kept.

## Where it lives

`crates/yapfast-core/src/plugins/page.rs` holds what the page shows, `View`,
what it asks for, `Request`, and the plain words for capabilities, statuses
and fetch errors. `ui/plugin_settings.slint` draws the page from the
`PluginPage` global, and `src/ui/plugin_settings.rs` fills that global from a
`View` and turns the page's callbacks into requests.

The plugin runtime is the page's `Source`: it keeps a `View` current, answers
each `Request`, and once the view moves calls `plugin_settings::changed`,
which shows it again if the page is open. It gives itself to the page with
`plugin_settings::attach`. Until then, the page shows `page::Fixture`, sample
plugins that answer at once, in `--preview`, and anywhere else
`page::Unavailable`, which says plugins can't run.

The page holds rows only while it's open. Opening it sends `Request::Open`;
leaving it, for another section or by closing Settings, sends
`Request::Close`, on which the runtime drops the marketplace's catalog, and
empties the page's models. The catalog is asked for when Marketplace shows.

## Tests

`cargo test --profile fast --features plugins plugin_settings` runs the page's
tests, and `cargo test --profile fast -p yapfast-core --features plugins page`
the wording's. Neither runs in `scripts/check`, whose builds leave the feature
off.
