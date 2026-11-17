# Integration and finishing: waves 2 to 5

The I tasks put the core behind the screens: each fills `anomi-app`'s
`project/<area>` (the core's `AppState` as the area's view-model, ported
from `src/ui/refresh.rs`) and `intent/<area>` (the area's intents as
`Action`s, ported from `src/ui/callbacks.rs`), and runs the effects the area
needs. They are Opus tasks in the one integration worktree, one at a time
(its first build costs minutes and several GiB; see the README). Each runs
once its screen tasks pass review, and is accepted on the real backend under
`scripts/ui-session --anomi` against the same shots. The Q tasks finish the
move. See [the index](../tasks.md).

### I01 · The window's state and layout.json (Opus · L)
- **Owns:** `anomi-app/src/layout/` (the whole `Layout` of `src/ui.rs:1287`, read and written in the same format, the 1 s delayed write of `src/ui.rs:3296`), `anomi-app/src/project/shell.rs`, `anomi-app/src/intent/shell.rs`; and the spine every later I task adds its arms to in turn: `anomi-app/Cargo.toml`, `anomi-app/src/{lib,main,app,message,run,cli,executor,backend,logging,window}.rs`, `anomi-app/src/{effects,project,intent,platform}/mod.rs`, `anomi-app/src/project/rows.rs`, `anomi-app/src/{project,intent}/chat/mod.rs`.
- **After:** N01.
- **Ports:** `Layout` and `Adapter`'s window fields in `src/ui.rs`; what `Adapter::new` hands the core from the layout before anything starts (`src/ui.rs:2535`-`:2578`: reaction and sticker uses, the switches it dispatches as actions, pinned DMs) and `remember_channels` before each `Effect::Start` (`src/ui.rs:2954`), none of which the skeleton does yet; `src/ui/callbacks.rs:1964`-`:2012` (widths, toggles); `src/zoom.rs`.
- **Done when:** a `layout.json` the Slint app wrote opens unchanged and keeps every key it doesn't use; pinned DMs, the last channel of each server and the Slint app's switches (hidden channels, reply ping, read sync...) carry over from it; panel widths, zoom and the member list's switch survive a restart.
- **Keep:** `run.rs` calls `anomi_theme::fonts::install()` before `iced::daemon` (done; it replaces `.fonts(fonts::ALL)`, which draws emoji in the system's font and name faces at one weight). The app is the only caller of `anomi_widgets::picture::set_scale(scale_factor * zoom)`: call it when the window opens, on `window::Event::Rescaled` and when the zoom changes, before the next layout; pictures, drawn names, frames and the paragraph's emoji are copied at that scale (iced's `Rescaled` leaves the zoom out). `anomi-shots` sets it to its `--scale`.

### I02 · Places (Opus · L)
- **Owns:** `anomi-app/src/project/{rail,sidebar}/`, `anomi-app/src/intent/{rail,sidebar}.rs` (the sidebar's and Browse Channels' intents).
- **After:** N02, N03, N04, N05, N14.
- **Ports:** `src/ui/refresh.rs:1059` (`show_sidebar`, whole), `:5396`-`:5569`, `:5533` (`voice_places`), `:5558`, `:124`; `src/ui/callbacks.rs:2716`-`:3051`; the pictures of the rows that show (the core leaves list avatars to the interface: "Virtualized UI rows request their own avatars" in `request_pictures`, `yapfast-core/src/application.rs`), asked with `Action::LoadPictures` from F07's visible keys through F05.
- **Done when:** the rail, DM list and channel list match the Slint app on the fake's data, with folders, voice people, threads, folding, arranging and moving people.

### I03 · The account and status (Opus · M)
- **Owns:** `anomi-app/src/project/account.rs`, `anomi-app/src/intent/{account,status}.rs`.
- **After:** N06, P10.
- **Ports:** `src/ui/refresh.rs:1627`; `src/ui/callbacks.rs:1619`, `:3073`, `:3591`.
- **Done when:** mute and deafen outside a call, status and custom status set and shown.

### I04 · Header and inbox (Opus · M)
- **Owns:** `anomi-app/src/project/{header,inbox}.rs`, `anomi-app/src/intent/{header,inbox}.rs`.
- **After:** N07, N08.
- **Ports:** `src/ui/refresh.rs:2124`, `:2289`, `:1746`; `src/ui/callbacks.rs:1288`.
- **Done when:** every header's buttons act.

### I05 · Signing in and linking Discord (Opus · M)
- **Owns:** `anomi-app/src/project/sign_in.rs`, `anomi-app/src/intent/sign_in.rs` (with the two-factor prompt's, S11), `anomi-app/src/effects/link.rs` (`LinkDiscord`, `LinkDiscordToken`, `StopDiscordLink` as an abortable task over `discord_link::start`'s progress).
- **After:** N09, S11.
- **Ports:** `src/ui.rs:2949` (`execute`, the linking arms); `src/ui/callbacks.rs:2685`, `:2704`, `:1308`.
- **Done when:** linking by QR code and by token against the fake works (`scripts/test-fake-discord links_by_qr_code` covers the core side); two-factor prompts answer.

### I06 · Dialogs about places (Opus · M)
- **Owns:** `anomi-app/src/project/places.rs`, `anomi-app/src/intent/places.rs` (the switcher's, the new DM's and every `places` dialog's).
- **After:** N10, N11, N12, N13.
- **Ports:** `src/ui/refresh.rs:2717`-`:3149`, `:3530`, `:3566`, `:3738` (`show_dialogs`); `src/ui/callbacks.rs:1280`, `:1323`, `:1362`, `:1815`, `:1825`.
- **Done when:** each dialog's flow works against the fake.

### I07 · Messages (Opus · XL)
- **Owns:** `anomi-app/src/project/chat/{timeline,message,attachments,embeds,reactions}.rs`, `anomi-app/src/intent/chat/timeline.rs`.
- **After:** C01, C02, C03, C04, C05.
- **Ports:** `src/ui/messages.rs` (all its row building: grouping, dividers, wording, embeds' layout), `src/ui/refresh.rs:4371`, `:1838`; `Action::VisibleMessages` and the picture requests of `src/ui/callbacks.rs:31`, `:707`.
- **Done when:** #general of the fake with `control showcase`, `embeds`, `poll`, `gallery` shows as the shots; scrolling reports visible rows and previews load and go; memory on an image-heavy channel measured against Slint's.

### I08 · Components, polls, cards and bot forms (Opus · M)
- **Owns:** `anomi-app/src/project/chat/{components,poll,cards,bot_forms}.rs`, `anomi-app/src/intent/chat/bots.rs`.
- **After:** C06, C07, C08, C20.
- **Ports:** `src/ui/refresh.rs:785`, `:759`, `:948`; `src/ui/polls.rs`; `src/ui/callbacks.rs:812`-`:917`.
- **Done when:** `scripts/test-fake-discord` scenarios for components, polls and forms pass through the iced interface by hand.

### I09 · Media (Opus · L)
- **Owns:** `anomi-app/src/media/` (video frames through `yapfast_core::picture::set_frame_buffers`, the viewer, voice messages, and their intents).
- **After:** C09, C10, C11, F05.
- **Ports:** `src/ui/refresh.rs:5176`, `:5243`, `:5279`; `src/ui/callbacks.rs:4172`, `:1146`, `:1159`; `src/ui/pictures.rs` (frames).
- **Done when:** a video plays in its cell and full screen from the fake's media with CPU and memory measured against Slint's.

### I10 · Menus, editing and forwarding (Opus · L)
- **Owns:** `anomi-app/src/project/menus.rs`, `anomi-app/src/intent/{menus,edit,forward}.rs`.
- **After:** C12, C13, C28, F06.
- **Ports:** `src/ui/menus.rs` (every menu's rows: message, member, channel, server, DM, thread), `src/ui/callbacks.rs:1195`, `:1222`, `:1265`, `:3966`.
- **Done when:** each menu of the golden `menu/` shots lists the same rows and each acts.

### I11 · The composer (Opus · L)
- **Owns:** `anomi-app/src/project/chat/composer.rs`, `anomi-app/src/intent/chat/composer.rs`.
- **After:** C14, C15, C16, F10.
- **Ports:** `src/ui/refresh.rs:4161`, `:4337`, `:4359`; `src/ui/app_commands.rs`; `src/ui/callbacks.rs:940`, `:2537`-`:2570`, `:2798`-`:2908`; `src/paste.rs`, `src/drop.rs`.
- **Done when:** drafts per room never cross, mentions and emoji complete, slash commands run, pasted and dropped files stage, typing is sent unless the person turned it off.

### I12 · Pickers (Opus · M)
- **Owns:** `anomi-app/src/project/chat/pickers.rs`, `anomi-app/src/intent/chat/pickers.rs`.
- **After:** C17, C18, C19.
- **Ports:** `src/ui/callbacks.rs:426`, `:557`, `:575`, `:1856`-`:1902`; `src/ui/gifs.rs`, `src/ui/stickers.rs`, `src/ui/emoji_grid.rs`, `src/ui/refresh.rs:487`, `:510`, `:668`.
- **Done when:** the three pickers fill from the account's emoji, GIFs and stickers, and let go of far pictures.

### I13 · Threads, forums, search and pins (Opus · M)
- **Owns:** `anomi-app/src/project/chat/{threads,forum,search,pins}.rs`, `anomi-app/src/intent/chat/places.rs`.
- **After:** C21, C22, C23, C24, C25, C26, C27.
- **Ports:** `src/ui/refresh.rs:1789`, `:1921`, `:1995`, `:2065`, `:3979`, `:5779`, `:5812`; `src/ui/callbacks.rs:1642`-`:1795`.
- **Done when:** each flow of the golden `thread/`, `forum/` and `header/` shots works.

### I14 · Tray, window and single instance (Opus · L)
- **Owns:** `anomi-app/src/platform/{tray,window,instance,placement}.rs` (`platform::window` takes F18's title bar intents).
- **After:** I01, F18.
- **Ports:** F18's title bar intents (drag, resize, minimize, maximize, close) as `iced::window` tasks; `src/tray.rs`, `src/ui/shell.rs` (closing to the tray gives memory back), `src/instance.rs`, `src/ui/placement.rs`, `src/autostart.rs`, `src/desktop_entry.rs`, `src/ui/move_loop.rs` (Windows).
- **Done when:** close to tray and open again (measure the memory returned with `testing/measure_tray.py --bin`), `--background`, a second start brings the window forward, the window opens where it was.

### I15 · Notifications, sounds, links and the clipboard (Opus · M)
- **Owns:** `anomi-app/src/effects/{notify,sound,page,clipboard}.rs`, `anomi-app/src/platform/{drop,paste,links}.rs`.
- **After:** I01.
- **Ports:** `src/ui.rs:2949` (`Notify`, `Sound`, `OpenPage`, `CopyPicture`), `src/notify.rs`, `src/protocol.rs`, `src/copy.rs`, `src/drop.rs`, `src/paste.rs`, `src/ui/browser.rs`.
- **Done when:** a message behind another window notifies and flashes the taskbar, sounds follow the settings, links open, a picture copies, files drop on Wayland and X11.

### I16 · People (Opus · L)
- **Owns:** `anomi-app/src/project/people/`, `anomi-app/src/intent/people.rs`.
- **After:** P01-P09, P11-P13.
- **Ports:** `src/ui/profile.rs`, `src/ui/active_now.rs`, `src/ui/refresh.rs:2429`, `:2507`, `:3587`, `:5129`, `:6094`; `src/ui/callbacks.rs:986`-`:1062`, `:1237`, `:1249`, `:1383`, `:1398`, `:1804`, `:3037`.
- **Done when:** the golden `friends/`, `profile/` and member list shots match on the real backend; Active Now's cards keep their state while they stay.

### I17 · The profile editor (Opus · M)
- **Owns:** `anomi-app/src/project/editor.rs`, `anomi-app/src/intent/editor.rs`.
- **After:** P14, P15, P16.
- **Ports:** `src/ui/refresh.rs:3246`; `src/ui/name_style.rs`; `src/ui/callbacks.rs:1426`-`:1554`.
- **Done when:** editing and saving the main and a server profile as `scripts/test-fake-discord edits_the_main_profile_as_discords_client_does` expects.

### I18 · Voice (Opus · XL)
- **Owns:** `anomi-app/src/project/voice/`, `anomi-app/src/intent/voice.rs`, `anomi-app/src/effects/devices.rs` (`ListDevices`, `PreviewCamera`), `anomi-app/src/platform/keybinds.rs` (push to talk).
- **After:** V01-V12.
- **Ports:** `src/ui/refresh.rs:4541`, `:4592`, `:5013`, `:5052`; `src/ui/tiles.rs`, `src/ui/alone.rs`, `src/ui/call_timer.rs`, `src/ui/call_link.rs`, `src/keybinds.rs`, `src/ui/keybinds.rs`; `src/ui/callbacks.rs:3061`-`:3881`.
- **Done when:** joining, speaking rings, watching a stream, the camera, sharing a screen and the soundboard work against the fake; a call measured against Slint's memory (44 MB in `docs/measurements/README.md`).

### I19 · The person's settings (Opus · L)
- **Owns:** `anomi-app/src/project/settings.rs`, `anomi-app/src/intent/settings.rs`, `anomi-app/src/language.rs`.
- **After:** S01-S10.
- **Ports:** `src/ui/appearance.rs`, `src/ui/updates.rs`, `src/ui/games.rs`, `src/custom_font.rs`, `src/woff.rs`; `src/ui/callbacks.rs:2193`-`:2510`.
- **Done when:** every switch of every page changes and persists what the Slint app's does.

### I20 · Server and channel settings (Opus · L)
- **Owns:** `anomi-app/src/project/server_settings/`, `anomi-app/src/intent/server_settings.rs`.
- **After:** X01-X15.
- **Ports:** `src/ui/server_settings.rs`, `src/ui/channel_settings.rs`, `src/ui/refresh.rs:2679`, `:3149`.
- **Done when:** `scripts/test-fake-discord changes_server_settings_as_discords_client_does`, `changes_roles_…` and `changes_channel_permissions_…` flows work by hand through the iced interface.

### I21 · Clocks and presence (Opus · S)
- **Owns:** `anomi-app/src/clocks.rs`.
- **After:** I01.
- **Ports:** `src/ui/profile.rs:252` (`clock_interval`), `src/ui/away.rs`, `src/ui/new_tags.rs`, `activity_clock` (`src/ui/callbacks.rs:1297`).
- **Done when:** every clock is a `time::every` subscribed only while the window is in front and something shows the time; idle after 15 s sets `motion.idle`; a test checks `subscription().units()` in front and behind.

## Finishing

### Q01 · The UI tests that move (Opus · M)
- **Owns:** `anomi-app/src/project/tests/` (one module per area, under `project` so it reaches the areas' `pub(super)` builders).
- **After:** the I task of each area.
- **Ports:** the 55 tests of `src/ui/tests.rs` that test row builders and wording (not the window).
- **Done when:** each passes against the ported builder.

### Q02 · Snapshot baselines (Sonnet · S)
- **Owns:** `anomi-{screens,chat,people,voice,settings}/tests/areas/snapshots.rs` (a module of each crate's test binary, which F01 lists) and `anomi-{screens,chat,people,voice,settings}/tests/snapshots/` (the baselines).
- **After:** an area's acceptance.
- **Done when:** each accepted shot is an `iced_test` snapshot compared on every `scripts/check-iced --review`.

### Q03 · Parity measurements (Opus · M)
- **Owns:** `docs/measurements/` records for the iced interface.
- **After:** I07, I14, I18.
- **Done when:** idle, image-heavy channel, call and scrolling (`testing/measure_scroll.py`) measured for both interfaces on the same data, as `docs/measurements/README.md` asks; and a busy server (typing, presence, messages every second) behind another window, where every batch of events costs the iced interface a view and a layout: its CPU and wakeups against Slint's.

### Q04 · The test session's commands (Opus · S)
- **Owns:** `anomi-app/src/control.rs`.
- **After:** I11, I14, I19.
- **Ports:** `test_control` in `src/ui.rs:1011` (`pick`, `stage`, `stage-aside`, `window open|close|hide|show|destroy|create|trim|stats`).
- **Done when:** every `scripts/ui-session do` command works with `--anomi`.

### Q05 · Switching over (Opus · L)
- **Owns:** `Cargo.toml`, `packaging/`, `scripts/`, the docs of the interface (`docs/architecture.md`, `docs/building.md`, `docs/translations.md`, `docs/themes.md`, `docs/iced/`); it runs after every other task.
- **After:** everything.
- **Done when:** `anomi` is the shipped binary, the Slint interface, `vendor/i-slint-*`, Skia and `crates/yapfast-ui` are gone, and the docs describe the iced interface alone.
