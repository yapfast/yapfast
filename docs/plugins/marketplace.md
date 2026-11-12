# The marketplace

The marketplace is a git repository. There is no server, no API and no account:
browsing fetches the registry the way installing fetches a plugin, reads JSON and
pictures out of it, and keeps nothing once Settings → Plugins is closed.

```
yapfast-plugin-registry/
  plugins/com.example.word-filter.json   one entry per plugin or theme
  icons/com.example.word-filter.webp     128×128, at most 24 KiB
  index.json                             what the repository lists, built by CI
```

The registry lives at <https://codeberg.org/yapfast/plugin-registry>. A build can
be pointed at another one with `YAPFAST_PLUGIN_REGISTRY=<url>`, which is how a
registry is tried before it is published, and how the tests reach a repository on
this disk (`file:///…`).

## What a listing promises

An entry pins one commit and the SHA-256 of the module at that commit, names the
public `source` a maintainer read, and carries the manifest's own words so that
browsing reads one repository:

```json
{
  "api": 1,
  "id": "com.example.word-filter",
  "name": "Word Filter",
  "version": "1.4.0",
  "description": "Hides words you'd rather not read.",
  "authors": ["Ada"],
  "repo": "https://codeberg.org/ada/word-filter",
  "path": "",
  "rev": "051b765bb6abe4fe95d5e4f489f03ad7b137edbc",
  "sha256": "0c1779c48f56f94b3e3624d76f55e38215870c59ccb3d41f6ba8b2bc22f218f5",
  "category": "Chat",
  "tags": ["filtering"],
  "source": "https://codeberg.org/ada/word-filter",
  "license": "MIT",
  "icon": "icons/com.example.word-filter.webp",
  "permissions": {
    "capabilities": ["messages.read", "messages.intercept"],
    "network": ["api.example.com"]
  }
}
```

| Field | What it is, and what is refused |
| --- | --- |
| `kind` | `plugin` or `theme`. An entry without one is a plugin, so a registry written before themes still reads; a kind this Yapfast doesn't know is refused rather than listed as a plugin. |
| `api` | The plugin API version. Names another one and the entry isn't read. |
| `id` | Reverse-DNS, the plugin's own id. It is the entry's file name: `plugins/<id>.json`. |
| `name`, `version` | What the row shows. They are the manifest's, and CI compares them. |
| `description`, `authors`, `category`, `tags` | What the row shows and the tab filters on. |
| `repo` | The repository the plugin installs from: `https`. `file` is for development only. |
| `path` | The folder inside it holding `yapfast-plugin.json`, empty for the root. It can't be absolute or climb out with `..`. |
| `rev` | The pin: the full 40-character commit. A branch or a short id leaves nothing to check an install against. |
| `sha256` | The SHA-256 of the module at that commit, in lowercase hex; for a theme, of its `theme.json`. |
| `source` | The public link a maintainer read. It must name `repo`, or the folder or ref in it: review read the code that is installed. |
| `license` | An OSI-approved license, as SPDX writes it: `MIT`, `Apache-2.0`, `GPL-3.0-or-later`, `MIT OR Apache-2.0`. The manifest's `license` must be the same. |
| `icon` | `icons/<id>.webp`, a 128×128 WebP of at most 24 KiB, in the registry itself. Pointing at an image host is refused: browsing contacts one repository, and no plugin author learns who is looking. |
| `permissions.capabilities`, `permissions.network` | What the plugin reaches, which is what the row sums up as `messages, 1 host`. A bare `*` host is refused; a wildcard is `*.example.com`. A theme has no code, and a theme entry that asks for either is refused. |

### Themes

A theme is listed by the same registry, in the same `plugins/` folder and the
same `index.json`, as an entry with `"kind": "theme"`. It has no module and no
manifest: `rev` pins its commit, `sha256` pins its `theme.json`, and `id` is the
id that file declares. Everything else is a plugin entry's, except
`permissions`, which a theme leaves out. Settings → Plugins lists only plugins;
the catalog keeps the theme listings beside them, marked by
`Listing::kind`, for the page that installs themes.

## The index

`index.json` says what the repository lists. CI builds it from the entry files:

```json
{
  "api": 1,
  "plugins": ["com.example.word-filter", "org.example.timestamps"],
  "delisted": [
    {
      "id": "org.example.leaky",
      "reason": "it uploaded the messages it read",
      "security": true
    }
  ]
}
```

An id listed twice, an id that isn't a plugin id, and an index that lists nothing
are refused. A delisted entry is one whose file was removed and whose id moved
to `delisted`. One with `security: true` was taken down because running it can't be
trusted: a person who has it is told so, with a line offering removal, the next
time they open the page. One delisted for another reason — abandoned, renamed —
is simply not listed.

## Review and CI

*Reviewed* is a person's claim, and the repository is where it is made. A pull
request adds one entry and its icon, and CI runs:

```
cargo build --profile fast -p yapfast-plugin-host
target/fast/yapfast-plugins verify-entry plugins/com.example.word-filter.json
```

`verify-entry` checks what a machine can check (a theme's entry: the first
three, then `theme.json`'s hash and id in place of 4 to 6, then the icon):

1. the schema, the entry's file name against its `id`, the API version;
2. an OSI-approved `license`, and a `source` that names the repository;
3. the commit: it fetches `repo` **at `rev`**, exactly as an install does, and
   refuses a repository that gives another commit;
4. the hash: it hashes the module the manifest names and compares it with
   `sha256`;
5. the manifest: schema, limits, the module's imports, and its permission hosts,
   where a bare `*` never gets past `HostPattern::parse`;
6. what the row will show: the entry's name, version, capabilities, hosts and
   license must be the manifest's;
7. the icon: a 128×128 WebP of at most 24 KiB, read from its header rather than
   decoded.

What no machine does is read the source. **A maintainer reads the code at
`source` and its permissions before merging**, for every new entry and every
updated pin — an update re-pins `rev` and `sha256`, so it is a new review. That
reading is what the *Reviewed* badge on the row and in the review dialog stands
for. A URL install shows *Unreviewed source* instead, because nobody read it.

The registry is a repository like any other: merging the pull request publishes
the entry, and taking an entry out of `index.json` delists it.

## What Yapfast does with it

**Browsing.** The catalog is read when the page opens, once per time it is open,
and it is held only while the page is open. `Request::Close` drops it —
`Catalog::Unloaded` — along with the entries behind the rows and the icons' bytes,
so a closed page holds nothing of a marketplace it once showed. A registry that
arrives after the page closed is dropped unread for the same reason, since the
fetch runs on a thread of its own and the page can close first. Each icon is
decoded only when its row is on screen: the row carries an address of
`plugin-icon://<id>`, the picture store asks for it like any other picture, and
the application decodes it from the catalog's own bytes at that moment. A listing
that is scrolled away keeps its picture only as long as the picture cache keeps
any other.

**Installing.** *Install* on a row fetches the entry's pinned commit through the
same helper a URL install uses, then compares what came back with the entry: the
commit and the hash of the module. A repository that changed under its pin is
refused, in a sentence saying so, and nothing is installed. What is installed is
the directory `store/<sha256>/`, the lock entry, and the reviewed dialog's
permissions, as for any install.

**Updating.** An installed plugin whose pin is behind the entry's has an update,
with the version it would become and the capabilities it would add. One that adds
no capability is one click: the new commit is fetched, checked against the entry,
and installed. One that adds a capability shows the review first, because a
permission given once isn't given for what comes later. Roll back returns to what
the install replaced, as it does for a URL install.

A plugin nobody lists is a URL install, and its *Update* fetches its own ref and
installs what it finds: a URL install is updated when the person asks, and
nothing checks a remote ref behind their back.

## What is not there yet

- Browsing fetches the registry each time the page is opened. Fetching only when
  `HEAD` changed needs a remote-refs query, which neither side has yet; the
  registry is small, so this is a fetch of seconds per page open rather than a
  problem, but it is a fetch.
- URL installs have no daily remote-ref check, so an update there is asked for
  rather than offered: the row has no *Update* until the person asks for one.
- Auto-update for marketplace plugins is not implemented, and nothing offers it
today: every update is a click, and an install that follows a registry's pin on
its own would be a new setting rather than something the page already carries.
- The registry repository itself is not in this repository: the schema, the
  validator and the checks above are what this tree ships.
