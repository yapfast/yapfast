# The iced interface, task by task

Full parity with the Slint interface, cut into 140 tasks. Each is one
writer's job in modules it alone owns: no two tasks write the same file,
and no task adds a line to a file another owns (F01 creates every shared
list up front). The architecture they follow is in [README.md](README.md),
the API notes in [iced-notes.md](iced-notes.md), and what waves 1 to 3
built that later code builds from (widgets, view-model types, the pieces
the screens share, fixtures) in [widgets.md](widgets.md).

The tasks themselves, with every field, are in seven files:
[foundation](tasks/foundation.md) (F), [shell](tasks/shell.md) (N),
[chat](tasks/chat.md) (C), [people](tasks/people.md) (P),
[voice](tasks/voice.md) (V), [settings](tasks/settings.md) (S and X) and
[integration](tasks/integration.md) (I and Q).

## How to read a task

- **Title (model · size).** Sonnet writes; Opus takes the hard widgets
  (the list with anchoring, the paragraph with inline emoji, selection
  across messages, the composer, layers and focus, pictures and video in a
  memory budget, the participant grid, name effects), every integration
  task (they build against the core) and the scaffolding. Size is new
  code: S up to 300 lines, M up to 800, L up to 1 500, XL more.
- **Owns:** the files and folders the task alone edits, under `crates/`.
  F01 made each one a stub with its `mod` line in its parent, its field
  and `Intent` variant wired, so the task fills them and adds no line
  elsewhere. A writer edits nothing else; a reviewer sends back a change
  outside them. Owning a view-model module `anomi-vm/src/<path>.rs` also
  owns its fixtures (`anomi-vm/src/fixtures/<path>.rs`) and its tests,
  `tests/areas/<name>.rs` in its screens crate (`<name>` is the module's
  file name); each screen module the task owns has a `shots()` its crate's
  `shots.rs` already collects. Owning `x.rs` owns the folder `x/` for its
  submodules. See the README's [shared files](README.md#shared-files) for
  the rest: the lines F01 wrote into one task's file for another, what a
  view-model and an `Intent` keep, manifests. `scripts/iced-coverage` fails
  when two tasks own the same path, one owns a path inside another's
  folder, or a file of the writers' crates (every `crates/anomi-*` but
  `anomi-app`) has no owner.
- **After:** the tasks whose code it builds on, which must have passed review.
- **Ports:** the Slint components it replaces (`file:line`) and the Rust glue
  to read (`src/ui/refresh.rs` for what the area shows, `src/ui/callbacks.rs`
  for what it does). Each component of the 309 in `ui/` is named by exactly
  one task that ports it; `scripts/iced-coverage` checks this (a component
  upstream adds fails it until a task names it).
- **Shots:** the reference screenshots of today's app in
  `~/.cache/anomi-iced/golden/` it must match (see their `index.md`); "no
  golden shot" means comparing with the Slint app under `scripts/ui-session`.
- **Done when:** the acceptance list, on top of what every task owes:
  `scripts/check-iced` passes, files under 400 lines, text through `tr!`,
  styles from the theme, motion through `Motion`, nothing ticking behind
  another window, tests of its intents in the simulator (its
  `tests/areas/<name>.rs`), its shots returned by its modules' `shots()`.
  The I task named at the end puts it on the real backend.

## What the skeleton already has

The skeleton's files belong to the task that takes their area further; the
rest are done and change only through the task named. F01 moved them into
the layout every task's **Owns** names; the table is the skeleton's code as
it stands, and the Owns lines are exact.

| Files | Owner |
| --- | --- |
| `anomi-theme/src/lib.rs`, `anomi-widgets/src/lib.rs`, `anomi-screens/src/{lib,shots}.rs`, `anomi-screens/src/sidebar/mod.rs`, `anomi-vm/src/{lib,sidebar/mod}.rs`, `anomi-vm/src/fixtures/{mod,sidebar/mod}.rs`, `anomi-shots/`, each crate's `tests/areas/main.rs` | F01 |
| `anomi-theme/src/{palette,metrics,typography}.rs`, `catalog/`, `builtin/` | F02 |
| `anomi-theme/src/theme.rs` | done; S07 for what custom themes need |
| `anomi-theme/src/motion.rs` | F06 |
| `anomi-theme/src/fonts.rs` | F03 |
| `anomi-vm/src/kind.rs`, `anomi-widgets/src/{avatar,badge}.rs` | F14 |
| `anomi-vm/src/{shell,conversation}.rs`, `anomi-screens/src/{shell,conversation}/`, the whole window's fixtures (`fixtures/shell.rs`) | N01 |
| `anomi-vm/src/rail.rs`, `anomi-screens/src/rail/`, `anomi-widgets/src/{rail_item,brand}.rs` | N02 |
| `anomi-vm/src/sidebar/dms.rs`, `anomi-screens/src/sidebar/dms/`, the tests of clicking a DM and Friends | N03 |
| `anomi-vm/src/sidebar/channels.rs`, `anomi-screens/src/sidebar/channels/` | N04 |
| `anomi-vm/src/sidebar/server_header.rs`, `anomi-screens/src/sidebar/server_header.rs` (the server's name) | N05 |
| `anomi-vm/src/account.rs`, `anomi-screens/src/account/` | N06 |
| `anomi-vm/src/header.rs`, `anomi-screens/src/header/` (a DM's or a channel's title, moved out of the conversation) | N07 |
| `anomi-vm/src/icon.rs`, `anomi-widgets/src/icon.rs` | F04 |
| `anomi-widgets/src/field.rs` | F12 |
| `anomi-widgets/src/picture/`, `anomi-app/src/pictures/` | F05 |
| `anomi-screens/tests/patterns.rs`, the pin in `Cargo.toml` | the task that moves the pin ([iced-notes](iced-notes.md#moving-the-pin)) |
| `anomi-app`'s spine (`src/{lib,main,app,message,run,cli,executor,backend,logging,window}.rs`, `src/{effects,project,intent,platform}/mod.rs`, `src/project/rows.rs`, `src/{project,intent}/chat/mod.rs`), `src/layout/` | I01, then each I task adds its arms in turn |
| `anomi-app/src/language.rs` | I19 |
| `anomi-app/src/control.rs` | Q04 |
| `anomi-app/src/project/{rail,sidebar}/`, `intent/{rail,sidebar}.rs` | I02 |
| `anomi-app/src/project/account.rs`, `intent/account.rs` | I03 |
| `anomi-app/src/project/header.rs` (the header) | I04 |

The skeleton's projections (`project/{rail,sidebar,header}`, `project/rows.rs`)
build N02-N07's and F14's types before their I tasks run. Those tasks may
add fields freely (the literals end in `..Type::default()`, so a new field
keeps its default) and variants to an `Intent` (the app matches with a
wildcard), but a field renamed or removed, or `Header` reshaped, breaks the
app's build in `scripts/check-iced --review`: do that with the I task, or
fix the projection's line in the same review.

## Waves

| Wave | Tasks | Runs |
| --- | --- | --- |
| 0 | F01 | alone, first |
| 1 | F02-F07, F11-F18 | in parallel; F14 after F05, F15 after F03 and F05, F17 after F06, F18 after F11 |
| 2 | F08, F09, F10, N01 | Opus, in parallel once F05-F07 passed |
| 3 | N02-N14, C01-C28, P01-P16, V01-V12, S01-S11, X01-X15 | in parallel, each after its "After" |
| 4 | I01-I21 | one at a time in the integration worktree, each once its screens passed |
| 5 | Q01-Q05 | as their areas finish; Q05 last |

Waves 0 to 3 are written and reviewed. Their writers noted what they needed
from files they did not own; one pass then closed those seams across
owners (the settings' dialogs as layers, pieces two screens had copied
moved into the kit or the widgets), put what is the app's under each I
task's **Seams**, and left what is larger for later.

The first milestone worth using every day is a text client: wave 0 to 2,
then N02-N11, C01-C05, C12-C17, C22, P05, P07, P10, S01, S06, and I01-I07,
I10-I12, I14, I15, I21. Voice (V01-V12, I18) and the rest follow.

## Totals

140 tasks: 93 for Sonnet (29 S, 53 M, 11 L) and 47 for Opus (3 S, 16 M,
20 L, 8 XL). All 309 Slint components of `ui/` are ported by a task;
`MemberPanel` is shared by P05 (the list) and P06 (its Activity half) by
design.

Left out on purpose: `YAPFAST_UI=classic`, the Slint interface's earlier
presentation kept "for regression checks and comparison"
([yapfast-ui](../yapfast-ui.md)). The iced interface draws the Discord
style alone: where a Slint component branches on `ClientStyle.enabled`, the
port takes the enabled side. Screen readers wait for iced to have them
(the README's last table).

## Every task

| ID | Title | Model | Size | Wave | After | File |
| --- | --- | --- | --- | --- | --- | --- |
| F01 | Scaffold every area | Opus | S | 0 | nothing | [foundation](tasks/foundation.md) |
| F02 | The rest of the style catalog | Sonnet | M | 1 | F01 | [foundation](tasks/foundation.md) |
| F03 | Fonts, Twemoji and name faces | Opus | M | 1 | F01 | [foundation](tasks/foundation.md) |
| F04 | The whole icon set | Sonnet | S | 1 | F01 | [foundation](tasks/foundation.md) |
| F05 | Pictures: store, rounding, animation | Opus | XL | 1 | F01 | [foundation](tasks/foundation.md) |
| F06 | Layers, popups, menus and the tooltip | Opus | XL | 1 | F01, F02 | [foundation](tasks/foundation.md) |
| F07 | The virtual list | Opus | XL | 1 | F01 | [foundation](tasks/foundation.md) |
| F11 | Buttons and toggles | Sonnet | M | 1 | F02 | [foundation](tasks/foundation.md) |
| F12 | Fields, sliders and meters | Sonnet | M | 1 | F02 | [foundation](tasks/foundation.md) |
| F13 | Loading and covers | Sonnet | S | 1 | F02 | [foundation](tasks/foundation.md) |
| F14 | Identity: avatar, tag, badges | Sonnet | M | 1 | F02, F05 | [foundation](tasks/foundation.md) |
| F15 | Names and their styles | Opus | L | 1 | F03, F05 | [foundation](tasks/foundation.md) |
| F16 | Profile effects and frames | Opus | M | 1 | F05 | [foundation](tasks/foundation.md) |
| F17 | Keyboard and focus | Opus | M | 1 | F06 | [foundation](tasks/foundation.md) |
| F18 | Window chrome | Sonnet | M | 1 | F11 | [foundation](tasks/foundation.md) |
| F08 | The message paragraph | Opus | XL | 2 | F03, F05 | [foundation](tasks/foundation.md) |
| F09 | Selection across messages | Opus | L | 2 | F07, F08 | [foundation](tasks/foundation.md) |
| F10 | The composer field | Opus | XL | 2 | F08 | [foundation](tasks/foundation.md) |
| N01 | The window and its layers | Opus | L | 2 | F06, F07 | [shell](tasks/shell.md) |
| N02 | The rail | Sonnet | M | 3 | F05, F14 | [shell](tasks/shell.md) |
| N03 | The DM list | Sonnet | M | 3 | F07, F14, F15 | [shell](tasks/shell.md) |
| N04 | The channel list | Opus | L | 3 | F07, F14 | [shell](tasks/shell.md) |
| N05 | The server's header | Sonnet | S | 3 | F06 | [shell](tasks/shell.md) |
| N06 | The account card and its popover | Sonnet | M | 3 | F06, F14, F15 | [shell](tasks/shell.md) |
| N07 | The conversation header | Sonnet | M | 3 | F11 | [shell](tasks/shell.md) |
| N08 | Inbox | Sonnet | S | 3 | F06 | [shell](tasks/shell.md) |
| N09 | Sign-in | Sonnet | M | 3 | F12 | [shell](tasks/shell.md) |
| N10 | Quick switcher | Sonnet | S | 3 | F06, F14 | [shell](tasks/shell.md) |
| N11 | New DM and group DM | Sonnet | S | 3 | F06, F12 | [shell](tasks/shell.md) |
| N12 | Invite, join, event, gift, authorize | Sonnet | L | 3 | F06, F12 | [shell](tasks/shell.md) |
| N13 | Leaving a server or a link | Sonnet | S | 3 | F06 | [shell](tasks/shell.md) |
| N14 | Browse channels | Sonnet | S | 3 | F07, F12 | [shell](tasks/shell.md) |
| C01 | The timeline | Opus | XL | 3 | F07, F08, F13 | [chat](tasks/chat.md) |
| C02 | The message | Opus | L | 3 | C01, F14, F15 | [chat](tasks/chat.md) |
| C03 | Reactions | Sonnet | M | 3 | C02, F06 | [chat](tasks/chat.md) |
| C04 | Attachments | Sonnet | M | 3 | C02, F05, F13 | [chat](tasks/chat.md) |
| C05 | Embeds | Sonnet | L | 3 | C02, F05 | [chat](tasks/chat.md) |
| C06 | Bot components | Sonnet | L | 3 | C02, F11 | [chat](tasks/chat.md) |
| C07 | Polls | Sonnet | M | 3 | C02, F12 | [chat](tasks/chat.md) |
| C08 | Emoji and sticker cards | Sonnet | S | 3 | C02, F06 | [chat](tasks/chat.md) |
| C09 | Video in a message and full screen | Opus | L | 3 | C04, F05 | [chat](tasks/chat.md) |
| C10 | Image viewer | Sonnet | M | 3 | F05, F06 | [chat](tasks/chat.md) |
| C11 | Voice messages | Sonnet | S | 3 | C04 | [chat](tasks/chat.md) |
| C12 | The message menu | Sonnet | M | 3 | F06, C02 | [chat](tasks/chat.md) |
| C13 | Editing in place | Opus | M | 3 | C02, F10 | [chat](tasks/chat.md) |
| C14 | The composer's frame | Sonnet | M | 3 | F10, F11 | [chat](tasks/chat.md) |
| C15 | Suggestions | Sonnet | M | 3 | C14 | [chat](tasks/chat.md) |
| C16 | Slash commands | Sonnet | M | 3 | C14 | [chat](tasks/chat.md) |
| C17 | The emoji picker | Opus | L | 3 | F05, F07, F06 | [chat](tasks/chat.md) |
| C18 | The GIF picker | Sonnet | M | 3 | C17 | [chat](tasks/chat.md) |
| C19 | The sticker picker | Sonnet | M | 3 | C17 | [chat](tasks/chat.md) |
| C20 | Bot forms and menus | Sonnet | M | 3 | F06, F12 | [chat](tasks/chat.md) |
| C21 | Search | Sonnet | M | 3 | F07, F12 | [chat](tasks/chat.md) |
| C22 | Pins | Sonnet | S | 3 | F06, C02 | [chat](tasks/chat.md) |
| C23 | The threads popover | Sonnet | S | 3 | F06, F14 | [chat](tasks/chat.md) |
| C24 | The thread beside a channel | Sonnet | M | 3 | C01, C14 | [chat](tasks/chat.md) |
| C25 | A new thread | Sonnet | S | 3 | C14 | [chat](tasks/chat.md) |
| C26 | A forum | Sonnet | M | 3 | F07, F05 | [chat](tasks/chat.md) |
| C27 | A new forum post | Sonnet | M | 3 | C26, C14 | [chat](tasks/chat.md) |
| C28 | Forwarding | Sonnet | S | 3 | F06, F12 | [chat](tasks/chat.md) |
| P01 | Friends | Sonnet | M | 3 | F07, F14 | [people](tasks/people.md) |
| P02 | Requests | Sonnet | M | 3 | P01 | [people](tasks/people.md) |
| P03 | Add a friend | Sonnet | S | 3 | F06, F12 | [people](tasks/people.md) |
| P04 | Active Now | Sonnet | M | 3 | F14, F05 | [people](tasks/people.md) |
| P05 | The member list | Sonnet | L | 3 | F07, F14, F15 | [people](tasks/people.md) |
| P06 | Activity cards | Sonnet | M | 3 | F05, F14 | [people](tasks/people.md) |
| P07 | The profile card | Opus | L | 3 | F14, F15, F16, P06 | [people](tasks/people.md) |
| P08 | Badges | Sonnet | S | 3 | F06, F05 | [people](tasks/people.md) |
| P09 | The full profile | Sonnet | L | 3 | P07 | [people](tasks/people.md) |
| P10 | Status | Sonnet | M | 3 | F06, F12 | [people](tasks/people.md) |
| P11 | About a person | Sonnet | S | 3 | F06, F12 | [people](tasks/people.md) |
| P12 | Server, tag and role cards | Sonnet | M | 3 | F06, F14 | [people](tasks/people.md) |
| P13 | Adding people to a DM | Sonnet | S | 3 | F06, F12 | [people](tasks/people.md) |
| P14 | The profile editor | Sonnet | L | 3 | P07, F12 | [people](tasks/people.md) |
| P15 | The editor's preview and colours | Sonnet | M | 3 | P14 | [people](tasks/people.md) |
| P16 | The name style window | Sonnet | M | 3 | F15, P14 | [people](tasks/people.md) |
| V01 | The voice dock | Sonnet | M | 3 | F11, F14 | [voice](tasks/voice.md) |
| V02 | Connection details | Sonnet | M | 3 | F06 | [voice](tasks/voice.md) |
| V03 | Tiles | Opus | L | 3 | F05, F14 | [voice](tasks/voice.md) |
| V04 | The participant grid | Opus | L | 3 | V03 | [voice](tasks/voice.md) |
| V05 | Call controls | Sonnet | M | 3 | F11, F06 | [voice](tasks/voice.md) |
| V06 | Picture in picture | Sonnet | S | 3 | V03 | [voice](tasks/voice.md) |
| V07 | Joining | Sonnet | S | 3 | F11 | [voice](tasks/voice.md) |
| V08 | Incoming calls | Sonnet | S | 3 | F06, F14 | [voice](tasks/voice.md) |
| V09 | Device menus and the input level | Sonnet | M | 3 | F06, F12 | [voice](tasks/voice.md) |
| V10 | Sharing the screen | Sonnet | M | 3 | F06, F05 | [voice](tasks/voice.md) |
| V11 | A participant's menu | Sonnet | S | 3 | F06, F12 | [voice](tasks/voice.md) |
| V12 | Soundboard | Sonnet | M | 3 | F06, F03 | [voice](tasks/voice.md) |
| S01 | The settings frame and its parts | Sonnet | L | 3 | F06, F11, F12 | [settings](tasks/settings.md) |
| S02 | Account | Sonnet | S | 3 | S01 | [settings](tasks/settings.md) |
| S03 | Voice and video | Sonnet | L | 3 | S01, F12 | [settings](tasks/settings.md) |
| S04 | Messages | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| S05 | Notifications and sounds | Sonnet | M | 3 | S01, F12 | [settings](tasks/settings.md) |
| S06 | Appearance | Sonnet | L | 3 | S01 | [settings](tasks/settings.md) |
| S07 | The theme editor | Sonnet | M | 3 | S06 | [settings](tasks/settings.md) |
| S08 | Discord and game activity | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| S09 | Keybinds | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| S10 | Advanced, About and updates | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| S11 | Two-factor prompts | Sonnet | S | 3 | F06, F12 | [settings](tasks/settings.md) |
| X01 | Server settings frame and profile | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| X02 | Engagement and boosts | Sonnet | M | 3 | X01 | [settings](tasks/settings.md) |
| X03 | Members | Sonnet | L | 3 | X01, F07 | [settings](tasks/settings.md) |
| X04 | Invites | Sonnet | S | 3 | X01 | [settings](tasks/settings.md) |
| X05 | Safety | Sonnet | M | 3 | X01 | [settings](tasks/settings.md) |
| X06 | Bans | Sonnet | S | 3 | X01 | [settings](tasks/settings.md) |
| X07 | Audit log | Sonnet | M | 3 | X01, F07, F08 | [settings](tasks/settings.md) |
| X08 | The role list | Opus | M | 3 | X01, F07 | [settings](tasks/settings.md) |
| X09 | The role editor | Sonnet | L | 3 | X08 | [settings](tasks/settings.md) |
| X10 | Emoji | Sonnet | M | 3 | X01, F05 | [settings](tasks/settings.md) |
| X11 | Stickers | Sonnet | M | 3 | X01, F05 | [settings](tasks/settings.md) |
| X12 | Ownership | Sonnet | S | 3 | X01 | [settings](tasks/settings.md) |
| X13 | Channel settings: overview | Sonnet | M | 3 | S01 | [settings](tasks/settings.md) |
| X14 | Channel permissions | Sonnet | M | 3 | X13 | [settings](tasks/settings.md) |
| X15 | Making a channel or a category | Sonnet | S | 3 | F06, F12 | [settings](tasks/settings.md) |
| I01 | The window's state and layout.json | Opus | L | 4 | N01 | [integration](tasks/integration.md) |
| I02 | Places | Opus | L | 4 | N02, N03, N04, N05, N14 | [integration](tasks/integration.md) |
| I03 | The account and status | Opus | M | 4 | N06, P10 | [integration](tasks/integration.md) |
| I04 | Header and inbox | Opus | M | 4 | N07, N08 | [integration](tasks/integration.md) |
| I05 | Signing in and linking Discord | Opus | M | 4 | N09, S11 | [integration](tasks/integration.md) |
| I06 | Dialogs about places | Opus | M | 4 | N10, N11, N12, N13 | [integration](tasks/integration.md) |
| I07 | Messages | Opus | XL | 4 | C01, C02, C03, C04, C05 | [integration](tasks/integration.md) |
| I08 | Components, polls, cards and bot forms | Opus | M | 4 | C06, C07, C08, C20 | [integration](tasks/integration.md) |
| I09 | Media | Opus | L | 4 | C09, C10, C11, F05 | [integration](tasks/integration.md) |
| I10 | Menus, editing and forwarding | Opus | L | 4 | C12, C13, C28, F06 | [integration](tasks/integration.md) |
| I11 | The composer | Opus | L | 4 | C14, C15, C16, F10 | [integration](tasks/integration.md) |
| I12 | Pickers | Opus | M | 4 | C17, C18, C19 | [integration](tasks/integration.md) |
| I13 | Threads, forums, search and pins | Opus | M | 4 | C21, C22, C23, C24, C25, C26, C27 | [integration](tasks/integration.md) |
| I14 | Tray, window and single instance | Opus | L | 4 | I01 | [integration](tasks/integration.md) |
| I15 | Notifications, sounds, links and the clipboard | Opus | M | 4 | I01 | [integration](tasks/integration.md) |
| I16 | People | Opus | L | 4 | P01-P09, P11-P13 | [integration](tasks/integration.md) |
| I17 | The profile editor | Opus | M | 4 | P14, P15, P16 | [integration](tasks/integration.md) |
| I18 | Voice | Opus | XL | 4 | V01-V12 | [integration](tasks/integration.md) |
| I19 | The person's settings | Opus | L | 4 | S01-S10 | [integration](tasks/integration.md) |
| I20 | Server and channel settings | Opus | L | 4 | X01-X15 | [integration](tasks/integration.md) |
| I21 | Clocks and presence | Opus | S | 4 | I01 | [integration](tasks/integration.md) |
| Q01 | The UI tests that move | Opus | M | 5 | the I task of each area | [integration](tasks/integration.md) |
| Q02 | Snapshot baselines | Sonnet | S | 5 | an area's acceptance | [integration](tasks/integration.md) |
| Q03 | Parity measurements | Opus | M | 5 | I07, I14, I18 | [integration](tasks/integration.md) |
| Q04 | The test session's commands | Opus | S | 5 | I11, I14, I19 | [integration](tasks/integration.md) |
| Q05 | Switching over | Opus | L | 5 | everything | [integration](tasks/integration.md) |
