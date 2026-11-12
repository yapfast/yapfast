# The manifest schema

[`yapfast-plugin.schema.json`](yapfast-plugin.schema.json) is a JSON Schema
(draft 2020-12) for `yapfast-plugin.json`. The manifest is strict: the parser
refuses a field it doesn't know, so a misspelled key refuses the plugin at
install with a serde message. An editor that validates against this file prints
the same objection while the manifest is being typed instead.

## In VS Code

Either put `"$schema"` in the manifest, which needs no setting at all:

```json
{
  "$schema": "../../schemas/yapfast-plugin.schema.json",
  "api": 1,
  "id": "com.example.plugin"
}
```

or point every `yapfast-plugin.json` in the workspace at the schema, in
`.vscode/settings.json`:

```json
{
  "json.schemas": [
    {
      "fileMatch": ["**/yapfast-plugin.json"],
      "url": "./schemas/yapfast-plugin.schema.json"
    }
  ]
}
```

`url` is resolved from the workspace root, so the copy in this repository points
at this file. A plugin that lives in its own repository can name the schema by
its raw URL instead, in either place:

```text
https://raw.githubusercontent.com/yapfast/yapfast/main/schemas/yapfast-plugin.schema.json
```

Yapfast itself ignores the `"$schema"` field; it is there for the editor.

## What it checks, and what it leaves to Yapfast

Everything serde checks: `additionalProperties: false` where the Rust type is
`#[serde(deny_unknown_fields)]`, `required` where there is no `#[serde(default)]`
and no `Option`, closed enums under their `rename_all` casing, the numeric bounds
from `crates/yapfast-plugin-api/src/limits.rs`, the id and version patterns from
`ids.rs` and `Version`/`VersionReq`, and the label and id shapes from `plain_label`
and `plain_id`.

A few of the parser's checks are not about one field's shape and JSON Schema
cannot state them; they stay with Yapfast, and the schema's top-level `$comment`
lists them: a plugin that depends on itself, two settings with one key, a `when`
that names a setting declared after it, `min` above `max`, `local-port` without
`network.local`, `on_failure: "block"` without `messages.intercept`, a manifest
with nothing to do, and colour or size names this build doesn't have.

The `settings` array is the one place where the two parsers in the tree disagree:
`yapfast-plugin-api`'s `SettingDecl` is narrower than the shape
`crates/yapfast-core/src/plugins/settings.rs` reads, which is what the
repository's manifests are written in. This schema describes the latter.
