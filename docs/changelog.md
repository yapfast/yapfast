# Changelog

RatFast is a fork of [Yapfast](https://github.com/yapfast/yapfast), which keeps
its own history. Everything here is what this fork changed on top of the commit
it started from (`28174c72`).

## Unreleased

### Branding

- The Windows installer, its shortcuts, its Run-key entry, its protocol
  description, the window title, the tray title and tooltip, notifications, the
  sign-in screen and the user-facing messages say **RatFast**.
- Internal names stay `yapfast` on purpose: crate names, the executable, the
  `io.github.yapfast.Yapfast` app id, the keyring service, the data folder, the
  registry keys and the `yapfast:` URL scheme. Renaming them would break path
  dependencies, saved sessions and upgrades from an installed Yapfast.
- `src/autostart.rs` now uses the same Run-key name the installer writes
  (`RatFast`), so Settings and the installer turn the same entry on and off.

### Plugins

- A native plugin system in `crates/yapfast-core/src/plugins.rs`: manifests with
  an id, version, API version and declared capabilities; a host that refuses
  any capability a plugin did not declare, and refuses privileged ones
  (credentials, files, network, processes) while the policy disallows them; a
  manager that keeps the enabled set and per-plugin settings in `layout.json`.
- Three bundled extensions: compact mode (denser rows), a theme contribution,
  and notification filtering (all messages, mentions only, or keywords).
- Settings → Plugins lists what is installed, what each declares, and switches
  them on and off.
- **Fixed:** the plugin switch and the notification-filter controls were drawn
  but never connected, so flipping them did nothing. Both now run the plugin's
  `on_enable`/`on_disable`, persist the state and re-apply density and themes.
- `docs/plugins.md` documents the interface; `docs/plugins-wasm.md` records what
  a sandboxed runtime would cost and why it is not in this version.

### Privacy

- Gateway payloads are no longer logged whole. Voice and stream gateway logs
  record the opcode and the payload size only, so a debug log no longer carries
  session descriptions, early DMs or endpoint addresses.
- A pasted Discord token is cleared from the window as soon as it is submitted,
  whether or not the link succeeds. It used to stay in the Slint property, in
  the action and in the API client for the life of the process.
- The log file is created owner-only (`0600`) on Unix.
- Settings → **Privacy** is new: it measures what is kept on disk (pictures,
  previews, GIFs, saved messages, total), can delete the media that can be
  fetched again, and states what the client does not do.

### Performance

- The custom emoji catalogue is cached by snapshot instead of rebuilt per
  keystroke; the animation pass returns early when nothing animated changed;
  the account cache is written only when what it saves actually changed. See
  [performance](performance.md).

### Interface

- Type weights capped at 600 (204 uses of 700/800/900 removed), including three
  900-weight headings that the bundled font cannot draw and rendered as
  synthetic bold.
- Paddings and gaps snapped to the 4px grid (301 off-grid literals).
- Rail, folder and channel rows mark the selection with a marigold edge pill, so
  "where am I" is one consistent detail rather than three different ones.

### Build

- `.github/workflows/ratfast-dev.yml` builds the branch on every push and hands
  back the AppImage and the Windows ZIP as artifacts, without publishing a
  release. `Nightly` gained a `ref` input so it can build a branch instead of
  only `main`.
- `docs/ratfast-building.md` records the D:-only Windows toolchain, what
  compiles with it today, and what the desktop binary still needs.
