# Keybinds

Keys held together that do something in any app, as Discord's keybinds:
push to talk, push to mute, toggling mute, deafen and the input mode. They
work in Discord and Matrix calls alike.

## In settings

Settings → Keybinds lists them as Discord's page does. "Add a Keybind" adds
one doing nothing (Unassigned) with no keys, as Discord's does; each has an
action, its keys with "Record keybind", a remove button, and Discord's words
for what the action does. The actions offered are Discord's Push to Talk
(Normal), Push to Mute, Toggle Mute, Toggle Deafen and Toggle VAD (push to
talk or voice activity). Push to Talk does something only in push to talk,
as in Discord, and is watched only then.

Settings → Voice & video records the push-to-talk keybind, as Discord's page
does: it's watched beside the page's (see
[Discord's microphone](discord/voice.md#push-to-talk) for what push to talk
sends).

Recording takes the keys pressed together, up to four, and ends once they're
all up again; Escape, pressed first, stops without changing anything. On
Windows, Mouse 4 (Back) and Mouse 5 (Forward) record too. While anything
records, keybinds don't fire, so pressing a toggle's keys to record them
doesn't toggle it; ones held already go on until their keys come up.
`crates/yapfast-iced/src/settings/keybinds.rs` has the page and records the
keys with the core's `Recorder` (`yapfast_core::keybinds`); the client's
`keybinds.rs` starts the watcher, which is the root crate's
(`src/keybinds.rs`).

## Kept in layout.json

A key is kept by its scan code, as winit gives it, so it's the same key on any
keyboard layout, with its name and, for the portal below, its keysym. Mouse 4
and 5 have reserved values in the same field. The keys of a keybind are kept as
its last key, with the others before it in `with`:

```json
"push_to_talk_key": { "scancode": 50, "name": "M", "trigger": "m",
                      "with": [{ "scancode": 29, "name": "Left Ctrl", "trigger": "Control_L" }] },
"keybinds": [{ "id": 1, "action": "toggle_deafen", "keys": { "scancode": 32, "name": "D" } }]
```

A single key is kept as before keybinds had more, and a build from then
reads a combination as its last key: `layout.json` that fails to read loses
every setting, so nothing here may fail on an older or newer file. An action
from a later build reads as Unassigned. Each keybind's `id` names its shortcut
for the portal (`keybind-<id>`), so the desktop keeps what it allowed when
others are removed; the Voice page's is `push-to-talk`, as before.

## Matching keys

What a keybind is (its keys, its action, what `layout.json` keeps, the
recorder and the notes on where keybinds work) is the core's
(`yapfast_core::keybinds`), which both clients read. `src/keybinds.rs`
turns key events into keybinds on whichever thread hears
them, looking through the few keys bound; it allocates only when the
keybinds change.

- A keybind fires when its last key goes down with the others held. Other
  keys held as well don't stop it, so push to talk works while moving in a
  game. A key going down again before coming up is a repeat and changes
  nothing, so a toggle happens once a press.
- In a combination, Ctrl, Shift, Alt and Super hold on either side, and
  settings name them without one ("Ctrl + Shift + M"); a modifier alone is
  the side recorded ("Right Ctrl").
- Among the keybinds one key completes, a toggling one gives way to one that
  holds all its keys and more, so Ctrl+Shift+D doesn't also fire Ctrl+D, as
  Discord's do. Holding ones always hold.
- Two push-to-talk keybinds hold until both are up.
- Losing the keys, as when the window loses the focus or the X server goes,
  lets everything go.

Push to talk and push to mute set `yapfast_core::push_to_talk`'s atomics
from that thread, so the microphone never waits on the window. Toggles go to
the window's event loop and run as its mute and deafen buttons do, sounds
included; Toggle VAD changes the Voice page's input mode.

## Hearing keys in other apps

| Desktop | How | Checked |
| --- | --- | --- |
| Wayland | The xdg-desktop-portal GlobalShortcuts portal: one session binds a shortcut for each keybind, described by its action and offering its keys as the preferred trigger in the shortcuts specification's words (`CTRL+SHIFT+m`), and runs it from Activated to Deactivated. The desktop matches the keys, may ask to allow each shortcut or give it others, and the page says what it gave each. KDE Plasma, Hyprland and GNOME from 48 have the portal | Against a stand-in portal on a private bus (`the_portal_binds_the_keybinds_and_runs_them`); not on a real desktop |
| X11 | XInput 2's raw key events on the root window, which come whatever window has the focus and leave the keys to it | Under Xvfb, with xdotool typing (`x11_hears_the_keys_whatever_has_the_focus`) |
| Windows | A low-level keyboard hook (`WH_KEYBOARD_LL`), and a mouse hook (`WH_MOUSE_LL`) when Mouse 4 or 5 is bound, on a thread of their own, which see releases, unlike `RegisterHotKey`, and pass every event on | Keyboard under Wine 10.0 with `SendInput` (`the_hook_hears_the_keys_go_down_and_up`); scan codes and the mouse in unit tests; not on real Windows |

On Windows the hook names a key as winit does, so it matches what the window
recorded: a key with a scan code of 0, as keyboard software and remappers
send, takes it from its virtual key in the focused window's keyboard layout,
and every code goes through winit's own mapping. Before this the hook missed
such keys altogether, in other apps and in Yapfast. Windows' made-up Left
Ctrl that AltGr sends holds nothing.

Windows may never send a key-up: the secure desktop (UAC, the lock screen)
takes the keys, and Windows keeps input into a program run as administrator,
such as some games, from the hook, as Discord warns of its own. So the hook
checks with `GetAsyncKeyState`, by the virtual key the key last went down
with: a held key that goes down again but reads up lost its key-up, the other
keys of a combination are checked as its last key goes down, and while push to
talk or push to mute holds, a timer checks every 250 ms and lets go of a key
that's up. `YAPFAST_LOG=debug` logs a few key events that might be a bound key
heard as another (a scan code of 0, or a bound key's virtual key with another
scan code); other keys are never logged.

The hooks heard nothing while Yapfast's own window had the focus, in every
release up to 0.4.1 (issue #249): keybinds worked with another app in front
and stopped as Yapfast came to the front. winit registers the process for raw
keyboard and mouse input as its event loop starts, for device events Yapfast
never reads, and Windows calls no low-level hook of a process registered so
while one of its windows has the focus. So the hook's start removes that
registration (`RIDEV_REMOVE`), and logs it. As a second line, the window's own
key and side-button events go to the hook's thread, which runs keybinds by
them until a hook has heard a bound key with Yapfast focused. Windows hands a
key to the hook before the window, so each key counts once. If that ever
happens the log says "the window's keys stand in". Wine calls the hooks
whatever is registered, so neither half shows there: the raw-input cause
comes from other programs' reports of the same symptom, not from a run on
real Windows.

Where none of these works, as on a wlroots desktop such as Sway, which has no
GlobalShortcuts, the window's own key events stand in, so keybinds work while
Yapfast is in front, and settings say so, as Discord's browser client does.
The watcher runs while any keybind is set, in calls or not, and with the
window closed to the tray: a thread blocked on the X server or in the hooks'
message loop, or the portal's task on the network runtime. Changing a keybind
starts it again, which on Wayland is a new portal session.
