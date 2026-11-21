# Settings

Settings (`settings/`) is a card over the window dimmed by 70%, 72 px in
from its
edges and at most 1400 px wide, centred; on its left, on the chat's ground, the account with Edit Profile,
a search that keeps the sections whose names hold it, the sections under
Account, Experience and App Settings (Discord and Game Activity only
outside the preview), and which build this is, which a click copies; on
the right the section's title and the close button over its page, a
column at most 696 px wide that starts at its top for each section. The
account card's gear opens it on the section it last showed (My Account at
first), a device menu's Voice Settings on Voice & Video. Edit Profile
closes it under the profile editor. Escape closes an open dropdown, then stops recording a
key, then Settings; so does a click outside or the close button.

It's a screen on the contract in [Screens](structure.md#screens): its
state (`Settings`)
lives in the app's slot only while it's open, and closing it lets go of
the theme being edited, the camera being tried, the gate's meter and the
recording. Each change is kept at once in
`layout.json` under its own key (`layout::Prefs` and
`layout::Voice` read them) and handed to the core.

- My account (`pages/`, a file a page, with the preferences' changes in
  its `mod.rs`): the account's card, where its login is kept,
  Reconnect while offline, Edit profile, the custom status, and Sign out
  (Back to sign in in the preview).
- Voice & video (`voice.rs`): the microphone, speakers and camera, each a
  dropdown of the devices the core lists (listed again as the page shows),
  the microphone's and speakers' volumes, which play while dragged and are
  kept when let go; Discord's mic test (Let's check), which runs the
  gate's meter with playback and lights its notches to the level, with
  Discord's captions (read as they're drawn), and the call's deafening
  while it runs (see [Voice](../voice.md#microphone-test)); the camera's
  preview, filmed only while Test video
  is on (`camera:settings` among the window's video sources, at 400 by
  225); then joining unmuted, evening out the voice, and in Discord calls
  noise suppression and its strength, the soundboard's volume (Discord
  keeps it; the slider follows a drag and what it's left at is sent), the entrance sound for every server or one (Choose, with
  Nitro, opens the sounds Discord has as the call's soundboard does,
  `soundboard::sound_board`, a click choosing one), the room's echo and, on Windows,
  Windows' own effects; the input mode, and either the gate's sensitivity
  over the microphone's level as the gate hears it (read as it's drawn,
  twenty times a second, only while it shows) or push to talk's keybind and release delay. Where no global key works
  (the root crate's `keybinds` says why), the window's own key events
  hold it while the window has the focus: an event
  filter hands each key's scan code to the root crate with no message.
  What the watcher tells of where keybinds work comes from its thread as
  a message (`Message::Keybinds`), kept by the root (`keybinds.rs`) for
  the note under the key whether Settings show or not.
  Record keybind records it as the Keybinds page records one (below).
- Keybinds (`keybinds.rs`), as Discord's page: Add a Keybind, and each
  keybind on the sunken ground with its action (Discord's six), its keys
  with Record keybind, a remove button, Discord's words for what the
  action does, and what the desktop gave it while the desktop runs them;
  over them, where keybinds work. Recording takes the keys pressed
  together, wherever the keys are, up to four, showing them as they're
  pressed, and ends once they're all up again (or the focus goes);
  Escape, pressed first, stops without changing anything; on Windows
  Mouse 4 and 5 record too. Each key is its scan code as winit gives it
  (iced's key codes are winit's by name), its name and its keysym, as the
  core's `keybinds::Key` keeps it, and the keybinds are kept in
  `layout.json` as `keybinds`. While one records, keybinds rest (the root
  crate's `pause`). The root crate's watcher watches push to talk's and
  the page's as one list, started again as they change; a toggle's press
  comes as a message and mutes, deafens or switches the input mode as
  its button does. [Keybinds](../keybinds.md) has the rest.
- Messages: a message as the clock and text size show it; the text size
  (which the timeline's text takes), the clock and its seconds, the date's
  order (message times are worked out again for each), when GIFs and emoji
  play, link previews (hidden ones leave "A card, hidden while link
  previews are off." where a card was the whole message), long messages
  folding, boost gems in the member list, sharing that this account types
  (off, the core's typing commands go nowhere), mentioning those replied
  to and tracking taken out of links.
- Notifications, Sounds (the set, which plays a sound when chosen, the
  volume, and each sound's own with its play button) and Advanced.
  Advanced holds Start when you sign in, Open Discord
  links in Yapfast (outside the preview), Close to tray, saying where
  there's no tray, and the renderer's switch: while it differs from the
  renderer drawing, when it takes effect and Restart Yapfast.
- Appearance (`appearance.rs`): each theme as a small window in its own
  colours, as many 150 px or wider across as fit; a click shows one and
  keeps it. New theme opens the editor on a copy of the theme shown (Start
  from takes another, and its name unless it was renamed); each custom
  theme's pencil edits it. A colour's swatch opens its `ColourPad` card and
  its field shows its change at once, or says "Not a colour"; Save keeps the
  theme among the person's own and chooses it, Cancel (or leaving Appearance or Settings) shows the theme
  from before, and a saved one's Delete asks once more. Then the system
  title bar's switch, where Yapfast can draw its own, and the profile
  switches, which the core is told of.
- Discord: linking, as the sign-in screen's panel, while none is linked;
  then Unlink in a build with Matrix, hidden channels, reads marked on
  Discord, idle while away, blocked people's messages hidden, and those
  blocked, five more at a time, each with Unblock. Whether someone is
  using Yapfast (`away.rs`): a key, a click or the
  wheel counts; once nobody has for 15 s, Discord hears since when, with
  idle while away, and goes AFK and idle on its own clock, and moving
  pictures hold still until someone is back. Input while someone is using
  it costs no message, only a time noted; one timer checks it.
- Game Activity: sharing the game played (Discord keeps it), and the games
  named by hand, added, edited and removed, which detection takes at once.
- About: the build with Copy version, the update switches, the releases
  (Updates in [the window](window.md#the-window-and-the-desktop)), and
  the links.

A setting that came with an update has a NEW tag after its name, and its
page's row in the list one too, until Settings leaves that page; what was
seen is kept in `layout.json` (`seen_new`), with the version a fresh
install has met (`first_version`). Which settings are tagged and when one
is new is the core's (`yapfast_core::new_tags`); `form::new_tag` draws
it.
