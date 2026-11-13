# Plugin settings

Settings → Plugins is section 10 of the settings screen. It exists only in a
build of the `plugins` Cargo feature, which is on by default: without it,
Settings doesn't list the page. The page is behind an `if` in
`ui/yapfast.slint`, so it costs nothing until it's opened.

It has three choices at the top:

- **Marketplace**: a search field, a category filter and a list of the
  registry's plugins (its themes, `kind: "theme"`, aren't listed here), each with its icon, *Reviewed* when a maintainer read
  its source, what it reaches (`messages, 1 host`) and Install. The list is
  a `ListView` in a `SettingsFrame` with `fill`, so only the rows in sight
  exist, and a row's icon goes through `PictureStore` only while the row
  does.
- **Installed**: a row per plugin with a switch that takes effect at once,
  its status (*Idle*, *Running*, *Suspended: ran out of memory*) and Update
  when there is one, and below them the memory the plugins use while the
  helper runs. A row opens the plugin's own page: its settings, drawn from
  its manifest's `settings` ([below](#a-plugins-own-settings)), what it can
  do and the hosts it may reach, each with Revoke, its source, commit and
  license, Update and Roll back, its recent log lines, and Uninstall, which
  asks whether to delete its data too.
- **Add from URL**: a git repository's address and *Find plugin*. Progress and
  errors are said plainly, as *That repository has no yapfast-plugin.json.*

Install, from either the marketplace or a URL, first shows the review: the
plugin's name, icon, source and commit, what it can do in plain words, its
memory cap and what it depends on, *Unreviewed source* for a URL, and Install
or Cancel. Reading messages with any host to connect to gets a sentence of its
own, saying the plugin could send your messages there, and
`messages.send.background` shows its warning in the danger tone.

Advanced, at the page's foot, opens a dialog with **Install a .wasm file**, the
memory budget and how long an idle plugin is kept. Choosing a file asks for it in
the system's dialog: its `.wasm`, or the `yapfast-plugin.json` itself. The two
pickers are drawn from the page's view and the choices are remembered by the page
while it is open, but nothing applies them yet: both requests are logged as the
runtime's (`SetMemoryBudget`, `SetIdleMinutes`), so the helper keeps its own
256 MiB and five minutes ([the helper](host.md#eviction-and-the-memory-budget)).
The dialog also says when the helper can't apply the OS sandbox here and runs
under wasmtime's alone.

**The manifest is what a plugin is** — its name and id, the commands it answers,
its settings, what it may reach and when it runs — so it is what an install
reads, and it is looked for beside the picked file and then in the folders above
it: a built module usually sits in `target/wasm32-unknown-unknown/release`, two
folders under its own manifest. A module with no manifest anywhere is refused
rather than installed, with a sentence saying what the manifest is for and to
choose the `plugin.wasm` beside one — a module on its own would be a plugin
nothing could call, which is exactly what a person installing one would then
spend an hour puzzling over.

What it becomes is installed and **off**: the row then offers the switch, and the
store's `lock.json` is what remembers that the person turned it on.

## A plugin's own settings

A manifest's `settings` array declares a plugin's settings, which its page
draws as a form.
[`plugins/settings.rs`](../../crates/yapfast-plugins/src/settings.rs)
checks it as the manifest is read, into a `Schema`: a setting that can't be
used refuses the whole manifest, with the reason, as
`setting "mode": option "x" is there twice`.

```json
"settings": [
  { "key": "mode", "type": "select", "label": "When a word matches", "default": "replace",
    "options": [{ "value": "hide", "label": "Hide the message" }, { "value": "replace", "label": "Replace the word" }] },
  { "key": "replacement", "type": "string", "label": "Replacement text", "default": "***",
    "pattern": "[*#!]+", "pattern_message": "Use symbols, such as * or #.",
    "when": { "show": { "setting": "mode", "is": "replace" } } },
  { "key": "words", "type": "list", "label": "Words", "max_rows": 50,
    "fields": [{ "key": "word", "type": "string", "label": "Word" }, { "key": "with", "type": "string", "label": "Replace with" }] }
]
```

Every setting has a `key` (lowercase letters, digits and `_`, starting with
a letter, up to 64 bytes, each once), a `type`, a one-line `label` of up to
64 characters, and may have a `description` of up to 256, a `default` and
`when`. Each type takes its own fields, and no others:

| Type | Its fields | Without a default | The form draws |
| --- | --- | --- | --- |
| `bool` | | `false` | A switch |
| `string` | `max_length` (256, at most 4096), `placeholder`, `pattern`, `pattern_message` | Empty | A text field |
| `select` | `options`: 1 to 32, each a string or `{ "value", "label" }` | The first option | A drop-down |
| `slider` | `min` and `max`, `step` (a hundredth of the range); at most 1000 steps | `min` | A slider |
| `number` | `min`, `max`, `step` | 0, or the end of the range nearest it | A text field |
| `integer` | `min`, `max`, `step`, whole numbers | As `number` | A text field |
| `list` | `fields`: 1 to 8, each a `bool`, `string`, `select`, `number` or `integer` with its own fields and no `when`; `max_rows` (64, at most 256) | No rows | Rows with Move up, Move down and Remove, and Add |
| `widget` | | None: it holds no value | What the plugin draws |

A default, given or not, must suit the setting's own rules. `pattern` is a
regular expression of up to 256 bytes that a whole non-empty value must
match; an empty value always may be, so a list's new row can start empty.
It runs in the `regex` crate, in time linear in the text, whatever the
pattern. A plugin has at most 64 settings.

`when` holds `show`, `enable` or both, each a test or a list of up to 8 that
must all hold. A test names a setting declared earlier, so there are no
cycles, and has one of `is`, `not`, `in` (a list of values) or `empty`
(`true` or `false`, for a string or a list). A setting shows only while its
`show` tests hold and the settings they name show; it can change only while
it shows, its `enable` tests hold and the settings its tests name can
change. A `widget` setting is drawn by the plugin: until the runtime gives
it a widget tree, the form shows *The plugin draws this while it runs.*

### Values

`Schema::defaults` gives each setting's default, and `Values::to_json` the
values as an object by key, a list as an array of objects, which is what
`settings.json` holds and `settings.get` will give the plugin. Each plugin's
values are kept in `data/<id>/settings.json` under the plugins' folder,
`<data>/plugins`, where `<data>` is Yapfast's data folder or `--data-dir`.

`settings::load` reads one plugin's file, of up to 1 MiB, and keeps each
value the schema takes: a key it doesn't have, a value it refuses, a row of
a list that isn't an object or is past `max_rows`, and a field of a row it
can't use are dropped and logged, and their defaults fill in. A file that
isn't JSON gives the defaults. It reads a small file, and the runtime calls
it on its own thread as it loads a plugin.

A `Store` writes values on a thread of its own, so the interface never waits
on the disk. `save` hands it a plugin's values; it writes them once 500 ms
pass without another change, or 3 s after the first while changes keep
coming, and only if the file would change, to a temporary file in the same
folder that is then renamed over `settings.json`. The thread starts with the
first change and ends after 30 s of none, and dropping the store writes what
waits.

### The form

A `Form` is a plugin's schema, its values, what the form refused and the
trees its widget settings draw, and is what `Installed::settings` holds.
The page sends each change as `Request::Setting` with an `Edit`: setting a
value, setting a field of a list's row, adding a row of the fields'
defaults, removing or moving one, or `Reset` to the defaults. The runtime
applies it with `Form::edit`, which parses what was typed (a select's
option, a number's text) and returns the keys whose values changed. When
there are any, the runtime calls `Store::save` and gives the plugin
`settings.on_change` with those keys, while it runs, without a restart.
A value the form can't take changes nothing: `Form::refused` keeps the
message, in plain words such as *Enter a whole number from 1 to 20.* or the
plugin's `pattern_message`, with what was typed, until that field changes.
A hidden or disabled setting takes no edit. Using a widget a widget
setting draws sends `Request::SettingWidget`.

What the form refused is kept by the runtime, not by the page: a refused value
is never stored, so a page that read its form back from the store would forget
why it refused. `PluginState::page` hands each plugin's refusals to the form it
builds, and a valid value for that field takes them away.

`settings.on_change` is how a running plugin hears of a change. It asks with
that call, is answered `pending`, and gets the keys and their new values as the
answer to that call in its next event batch — the same way a pending
`http.fetch` finishes ([the ABI](abi.md#how-a-pending-call-finishes)). Nothing
restarts, and nothing is kept for an instance that isn't running: the next one
reads the values with `settings.get`, which answers what the person set.
Uninstalling with *delete its data* removes the file with the plugin's storage.

On the page, the form is the `PluginPage.form` model: one row per setting
that shows, and for a list its heading, a row per record and a foot with
Add. `src/ui/plugin_settings.rs` keeps that model, and each list row's
fields, while the plugin's page is open and changes only the rows that
changed, so a field keeps its focus and what was typed as the form changes
around it. A refusal shows under its field, and the field keeps the text
that was refused. Each control has its label as its accessible name, a list
row's fields name their row (*Word, row 2*), and its buttons say what they
do (*Move row 2 up*). Reset to defaults asks first, as a list's rows go with
it. The form's rows exist only while the plugin's page is open.

A `bool`, `string`, `select`, `slider`, `number`, `integer` and `list` setting is
drawn, and its change takes effect at once. A `widget` setting is drawn by the
plugin as well: the page hands what it has to `PluginWidgets`, the same
component a slot's tree goes to, and a widget the person uses comes back as
`Request::SettingWidget` and on to the plugin the way a change is
([the ABI](abi.md#the-payloads)). Until the plugin has drawn one, the form says
*The plugin draws this while it runs.* Those eight are the types
[`settings.rs`](../../crates/yapfast-plugins/src/settings.rs) checks, and a
manifest that names another is refused.

`--preview`'s Word Filter has a setting of every type, from
[`testing/plugins/word-filter`](../../testing/plugins/word-filter), and starts
with a refused value to show one; its form keeps its values in memory.

## Where it lives

`crates/yapfast-plugins/src/page.rs` holds what the page shows, `View`,
what it asks for, `Request`, and the plain words for capabilities, statuses
and fetch errors. `ui/plugin_settings.slint` draws the page from the
`PluginPage` global, and `src/ui/plugin_settings.rs` fills that global from a
`View` and turns the page's callbacks into requests.
The plugin runtime is the page's `Source`, and `src/ui/plugin_settings.rs`
implements it over the application: `plugin_settings::attach` gives it to the
page while the window is built, and `plugin_settings::set_view` hands it a fresh
`View` whenever one moves, which redraws the page if it's open.

The `View` itself is built in `crates/yapfast-plugins/src/view.rs` from the
registry, the store's lock file and the grant file: a copy, taken when the page
draws, so a page that is closed holds nothing and holds no lock while it is
open. `crates/yapfast-core/src/application/plugins/page.rs` answers the page's requests: each request
is an `application::Action`, `Effect::Plugin` carries a status back to it, and
`Effect::Plugin(Effect::Page)` redraws it once the runtime has applied a change.
A request for something the runtime doesn't apply — the memory budget and the
idle time, today — is logged and changes nothing.

`--preview` keeps the sample plugins instead: the page shows `page::Fixture`,
which answers at once, and `page::Unavailable` says plugins can't run only in a
build whose runtime was never attached.

The page holds rows only while it's open. Opening it sends `Request::Open`;
leaving it, for another section or by closing Settings, sends
`Request::Close`, on which the runtime drops the marketplace's catalog, and
empties the page's models. The catalog is asked for when Marketplace shows.

## Tests

`cargo test --profile fast --features plugins plugin_settings` runs the page's
tests, `cargo test --profile fast -p yapfast-core --features plugins page`
the wording's, `-p yapfast-core --features plugins plugins::settings` the
settings schema's, values' and store's, and `... tweaks` the setting
points'. `scripts/check` runs all of them: the default build has the `plugins`
feature, and its plugin steps run Clippy and the tests on `yapfast-core` with the
feature named.
