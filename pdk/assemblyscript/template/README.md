# A plugin to start from

AssemblyScript, which is what a new Yapfast plugin should be written in: it is
the cheapest one measured — 11.4 KiB and **one page (64 KiB)** of linear memory,
against the Rust PDK's 160.8 KiB and 17 pages — and it compiles in a second with
nothing installed but `npm`.

## What is here

| File | What it is for |
| --- | --- |
| `yapfast-plugin.json` | The manifest: the plugin's id, version, permissions and the command the composer lists |
| `index.ts` | The handlers — five exports naming the PDK's, and one class with a `command` |
| `build.sh` | What makes `plugin.wasm`, which is what the manifest names |
| `plugin.wasm` | Not here yet: `build.sh` makes it |

## Build it

```sh
cd pdk/assemblyscript && npm install   # once, if node_modules is missing
sh template/build.sh                   # writes template/plugin.wasm
```

`build.sh` builds from the PDK's folder with `--baseDir`, which is what lets
`index.ts` import `../assembly`. To take this plugin out of the repository,
copy `pdk/assemblyscript/{assembly,package.json,package-lock.json,asconfig.json}`
beside it and keep the same layout:

```text
my-plugin/
  assembly/            # the PDK, copied from pdk/assemblyscript/assembly
  index.ts             # this file
  build.sh             # adjust the two paths at the top
  yapfast-plugin.json
```

On Windows, `build.cmd` is the same build, run through `node`.

## Try it

Install the folder (Plugins → Add → from a folder, or copy it into the store's
folder while developing), then type `/hello`. It answers the line the PDK hands
back, which is all a command does.

## Next

* `docs/plugins/interface.md` — every host call and export.
* `docs/plugins/manifest.md` — every field of the manifest.
* `docs/plugins/permissions.md` — what each capability allows, and what the
  review dialog says about it.
* `pdk/assemblyscript/examples/shout` — a plugin that sends a message, which is
  the next step up from answering a command.

The one thing to know before writing much: **memory is not given back.** A
guest's pages stay with its instance until it is evicted, so handle one event at
a time and let values go when the batch ends. `docs/plugins/pdk.md` has the
numbers for every language, and why AssemblyScript is first.
