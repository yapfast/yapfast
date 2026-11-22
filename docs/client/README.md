# The client

Yapfast's window is `crates/yapfast-iced`, written in
[iced](https://iced.rs) 0.14 on the widgets of `crates/yapfast-iced-kit`. It
drives `yapfast-core` through `Action`s, `AppState`, effects and the
backend's events ([architecture](../architecture.md)), so drafts, sending,
the member list and voice are the core's and the window draws them. It draws
with tiny-skia on the CPU, or with wgpu on the GPU when Settings → Advanced →
Render with the GPU is on.

How it works, one doc per part:

| Doc                                               | What it covers                                                                                        |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| [The window and the desktop](window.md)           | Building and running, renderers, the tray, notifications, links, the title bar, zoom, updates, themes |
| [Rail and sidebar](navigation.md)                 | Servers and folders, channels and DMs, the account's card                                             |
| [Header and timeline](timeline.md)                | The conversation's header, messages, grouping, Markdown, pictures, reactions, selection               |
| [Writing](writing.md)                             | The composer, uploads, the expression picker, apps' forms and commands                                |
| [Forums and threads](threads.md)                  | Forum channels, threads beside a channel                                                              |
| [People and sign-in](people.md)                   | The member list, profiles, friends, menus, linking an account                                         |
| [Calls](calls.md)                                 | Voice channels, DM calls, cameras and streams                                                         |
| [Settings](settings.md)                           | The account's settings                                                                                |
| [Server and channel settings](server-settings.md) | A server's and a channel's settings                                                                   |
| [Dialogs](dialogs.md)                             | Questions, forms and their frames                                                                     |

How it's made:

| Doc                                     | What it covers                                                         |
| --------------------------------------- | ---------------------------------------------------------------------- |
| [How the client is built](structure.md) | `App`, messages, areas and screens, where code goes, adding a screen   |
| [The kit](kit.md)                       | The widgets, styles, fonts and icons screens are built from            |
| [The vendored toolkit](toolkit.md)      | The patches to iced's renderers and cosmic-text                        |
| [Tests](tests.md)                       | The headless simulator, where a test goes and what to know writing one |
| [Not there yet](open.md)                | What the window doesn't do yet                                         |
