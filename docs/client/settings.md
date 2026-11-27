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
- Keybinds (`keybinds.rs`), as Discord's page: Custom Keybinds with Add
  a Keybind beside its title, and each keybind in a row with its action
  (Discord's six), its recorded keys and an inline Record/Stop button, a
  delete button and an enable switch, with Discord's words for what the
  action does under it. Disabling keeps the recorded keys but removes the
  binding from the watcher. Each row also says what the desktop gave it
  while the desktop runs them; over them, where keybinds work. Recording takes the keys pressed
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
  Default Keybinds follows, as Discord's list (`shortcuts::GROUPS`, its
  words and order, keeping those the app has): the shortcut-list key on
  its own, then Messages, Navigation, Chat, Voice and Video, and
  Miscellaneous. Each group is a rounded card with divided rows, labels on
  the left and individual keycaps on the right; alternative chords stack
  vertically. Colours follow the selected theme.
  Ctrl+/ opens this page from the workspace, where Discord opens a dialog
  with the same list. These shortcuts run
  through `shortcuts.rs`, only in the focused window, including when a
  text field captures the key (but Ctrl+E a field took stays the
  field's, which opens the picker for it); chat actions wait while a dialog takes keys,
  and all built-ins rest while recording.
- Messages: a message as the clock and text size show it; the clock and
  its seconds, the date's order (message times are worked out again for
  each), link previews (hidden ones leave "A card, hidden while link
  previews are off." where a card was the whole message), long messages
  folding, boost gems in the member list, sharing that this account types
  (off, the core's typing commands go nowhere), mentioning those replied
  to and tracking taken out of links.
- Accessibility (`pages/accessibility.rs`), laid out as Discord's page
  with the parts Yapfast has: a preview of two messages at the text size,
  then Text Readability's text size in chat, a slider that stops only at
  Discord's sizes (12, 14, 15, 16, 18, 20 and 24 px, labelled under it),
  which the timeline's text and names take, their lines 1.3 times the
  text where that's more than the 19.5 px (names 20 px) they keep up to
  15 px (`message::line_height`), kept as `chat_text_size` when it's let
  go. 15 px is the default, as Messages' Default was, where Discord's is
  16; a layout from before keeps its Small (14), Default or Large (18)
  from `text_size`. Scroll Speed sets how far the mouse wheel or touchpad
  moves lists and message fields, from 50% to 300%. It defaults to 100%
  and is saved as `scroll_speed`. Then Reduced Motion: Enable Reduced Motion, the
  `reduce_motion` the Themes page's switch keeps too, which stills
  installed themes' animations, effects, hover motion and transitions
  ([themes](../themes.md)) and, as Discord's client overrides its
  animation settings, GIFs, emoji and stickers (`animate_stilled`), with
  Discord's note under their choice, until they're chosen again or
  reduced motion goes; Sync with computer setting (`sync_reduced_motion`,
  off by default, where Discord's is on), which follows what the desktop
  asks for (the root crate's `motion.rs`: the portal's
  `enable-animations`, or KDE's animation speed at 0, on Linux, "Show
  animations in Windows" there), read as the window opens and each time
  it takes the focus; turning it off keeps what the desktop said, and
  choosing reduced motion stops it. Last, when GIFs, emoji and stickers
  play: while Yapfast is focused, always or never (`animate`). Discord's
  Visual Density, Color & Contrast, Always underline links, its separate
  GIF, emoji and sticker switches and Minimum notification duration aren't
  there; its Display Name Styles is Appearance's Show name styles.
- Notifications, Sounds (the set, which plays a sound when chosen, the
  volume, and each sound's own with its play button) and Advanced.
  Advanced holds Discord's two startup switches on Windows and Linux
  outside the preview: Automatically open Yapfast when your computer starts
  up, and Start Yapfast minimized (to the tray), off until chosen and greyed
  out, shown off, while the first is off. Advanced also holds Open Discord
  links in Yapfast (outside the preview), Close to tray, saying where
  there's no tray, and the renderer's switch: while it differs from the
  renderer drawing, when it takes effect and Restart Yapfast.
- Appearance (`appearance.rs`): each theme as a small window in its own
  colours, as many 150 px or wider across as fit; a click shows one and
  keeps it. New theme opens the editor on a copy of the theme shown (Start
  from takes another, and its name unless it was renamed); each custom
  theme's pencil edits it. A colour's swatch opens its `ColourPad` card and
  its field shows its change at once, or says "Not a colour"; Save keeps the
  theme among the person's own and chooses it, Cancel (or leaving Appearance
  or Settings) shows the theme from before, and a saved one's Delete asks
  once more. Import, beside New theme, and Export, in the editor beside Save
  and Cancel, open the system's file dialog and share a theme as a file (see
  [themes](../themes.md#sharing-themes)); Paste and Copy beside them do the
  same through the clipboard. A line under the title says how it went. Then
  Font (`font.rs`): Choose font files opens the system's file dialog for TTF,
  OTF, TTC and WOFF files, which draw the whole window in
  their font; each file is listed with the faces it gave and a Remove, a file
  that isn't a font is left out with the reason under the list, and Reset to
  default goes back to DM Sans. The paths are kept as `ui_font_files` in
  `layout.json` and loaded at startup; the rows are measured again as the
  text size's change does. Application icon shows the colours of the
  built-in themes as preview tiles, with Yap selected by default. The tray
  uses the application icon's selection by default. Turning off “Use the
  application icon in the tray” reveals the tray's own choices. A previously
  saved different preset or separate custom image starts with the switch
  off. The tray keeps its separate choice while the switch is on.
  The tile below the presets imports an image or selects the saved one.
  Point at a saved custom tile to show Replace. Imports accept PNG, ICO,
  JPEG or WebP files up to 8 MiB and 4096 × 4096 pixels.
  Yapfast keeps a 256 × 256 PNG copy in its data folder, preserving aspect
  ratio and adding transparent padding where needed.
  The running window and tray update as soon as a choice is made. On Linux,
  the window icon applies where the desktop uses X11 window icons. Desktops
  that use the application's desktop entry show the launcher icon there.
  The theme's Draw text glow switch turns off the soft edge a theme puts
  round text, which is drawing rather than data: the shadow, the outline
  and the ink stay. Then the system title bar's switch, where Yapfast can
  draw its own, and the profile switches, which the core is told of. The
  last, "Show connections on profile cards" (off by default), is the
  window's alone: the floating card lists the connections the profile
  already carries. The member list's own section has Show activity cards,
  which keeps `activity_cards_hidden`, the pref the panel's eye writes. Off,
  the panel keeps its "Activity — N" heading and its eye, as Discord's does,
  and a click there shows the cards again.
- Discord: linking, as the sign-in screen's panel, while none is linked;
  then Unlink in a build with Matrix, hidden channels, reads marked on
  Discord, idle while away, blocked people's messages hidden, and those
  blocked, five more at a time, each with Unblock. Whether someone is
  using Yapfast (`away.rs`): a key, a click or the
  wheel counts; once nobody has for 15 s, Discord hears since when, with
  idle while away, and goes AFK and idle on its own clock, and moving
  pictures hold still until someone is back. Input while someone is using
  it costs no message, only a time noted; one timer checks it.
- Game Activity: sharing the game played (Discord keeps it), the games
  named by hand, added, edited and removed, and the programs never shown as
  a game, added, removed and switched off to allow one again
  (`blocked_games`), which detection takes at once,
  and a menu of the games detected now to choose one from
  (`Request::DetectedGamesMenu`, `MenuAct::BlockGame`).
- About: the build with Copy version, the update switches, the releases
  (Updates in [the window](window.md#the-window-and-the-desktop)), and
  the links.

A setting that came with an update has a NEW tag after its name, and its
page's row in the list one too, until Settings leaves that page; what was
seen is kept in `layout.json` (`seen_new`), with the version a fresh
install has met (`first_version`). Which settings are tagged and when one
is new is the core's (`yapfast_core::new_tags`); `form::new_tag` draws
it.
