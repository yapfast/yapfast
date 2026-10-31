# The tray

Closing the window leaves Yapfast running in the system tray: the session, a call, notifications, ringing and sounds go on. The
tray icon is Yapfast's logo in Yap's marigold, with a red dot cut into its
corner while a mention or a DM waits unread, as Discord counts them. Clicking
it opens the window. Its menu has Open Yapfast, Disconnect during a call, and
Quit.

Settings → Advanced has "Close to tray", on unless turned off
(`close_to_tray` in `layout.json`). Off, or with no tray to show the icon,
closing the window quits Yapfast; the setting's description
says when there's no tray. A tray that starts after Yapfast, or comes back,
still gets the icon.

## Starting again

Starting Yapfast while it runs on the same data folder brings the running
one's window forward, making it again if it was closed to the tray, and the
new start ends (`src/instance.rs`). A start can carry one link (see
[Links from other apps](#links-from-other-apps)), which the running Yapfast
opens. A start with `--background`, as at sign-in, just ends. Settings → Advanced → "Allow multiple windows"
(`multiple_windows` in `layout.json`, off unless turned on) starts another
Yapfast instead, with its own window and tray icon. Each folder is its own:
another `--data-dir` always starts. The preview never asks.

The first start holds a lock on `instance.lock` in the folder, which the
system lets go of when the process ends, so a crash leaves nothing to clean
up. It listens on `instance.socket` beside it on Linux; on Windows on a
named event, `Local\yapfast-` and a hash of the folder's path. A later start
that finds the lock taken asks for the window there, trying for 5 seconds
while the first may still be starting or restarting with another renderer,
and starts anyway if nothing answers. On Linux it passes on the activation
token and link the desktop launched it with (`XDG_ACTIVATION_TOKEN` on Wayland,
`DESKTOP_STARTUP_ID` on X11), so the window can take the focus: a window made
again takes it through winit, and a shown one through Yapfast's own
`xdg_activation_v1` on winit's Wayland connection. On Windows the later start
lets any process take the foreground, which the first then does. Its link
waits as a small file in `instance-links` for the first to take and remove;
a new first start clears any left there by one that has gone.

## Links from other apps

The desktop hands Yapfast a link after `--open-link`: Windows runs
`"…\yapfast.exe" --open-link "%1"`, and the desktop entry
`yapfast --open-link %u`. Windows puts the link into that command line as it
comes, quotes included, so one link can split into several arguments. Nothing
after `--open-link` is read as an option, and when more than one argument
follows it, Yapfast starts, or shows the running one's window, without the
link (`src/main.rs`). A link given on its own, as in
`yapfast https://discord.com/channels/…`, opens too. Every argument under
`discord:` or `yapfast:` counts as a link; one naming no place Yapfast has,
such as Discord's library or settings, only starts Yapfast or shows its
window, as every link does in a Matrix-only build. The link passes on as the
URL standard writes it, a space percent-encoded, so it stays one word on the
socket. Yapfast opens the place once Discord is online, and a first start
waits for its snapshot in the same way. A link that comes while a dialog is
open waits until it closes, so a link from another app or a web page never
swaps what an Authorize, Redeem or Join click is for. The restart a changed renderer setting
makes doesn't open it again; the one after a failed GPU start does, as nothing
opened it yet.

Settings → Advanced → "Open Discord links in Yapfast" claims `discord://`
for this Yapfast, and its own `yapfast://` with it; turning it off gives each
back as it was (`src/protocol.rs`). Showing the switch reads a file or the
registry and starts no process.

- **Windows**: the switch is the current user's
  `Software\Classes\discord` command. The one there before is kept, in its
  own registry type, under `Software\Yapfast\DiscordProtocol`. The installer
  registers only `yapfast://`; uninstalling runs `yapfast.exe
  --release-discord-links`, which gives `discord://` back, and removes the key.
- **Linux**: the switch is the person's own `mimeapps.list`, in
  `$XDG_CONFIG_HOME` or `~/.config`, and it's on while its
  `[Default Applications]` name Yapfast's entry first for
  `x-scheme-handler/discord`. Claiming keeps each scheme's line as it was, or
  that there was none, in `discord-protocol-previous` in the data folder;
  turning the switch off puts that line back or takes Yapfast's out, and
  leaves a default someone chose since. The list is written where a link to
  it points, as home-manager or a dotfiles folder links it, keeping its
  permissions; one that can't be written, as in a read-only store, fails the
  switch and stays as it was. Yapfast's desktop entry declares only
  `x-scheme-handler/yapfast`. gio and xdg-open follow a default to an entry
  that doesn't declare the scheme, so the claim works, and without a Discord
  app installed the desktop doesn't fall back to Yapfast for Discord's links.

## Starting at sign-in

Settings → Advanced → "Start when you sign in" starts Yapfast with
`--background` when the person signs in to their computer (`src/autostart.rs`).
The entry is the setting, not `layout.json`: on Windows a value named
`Yapfast` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, on Linux
`io.github.yapfast.Yapfast.desktop` in `$XDG_CONFIG_HOME/autostart`. Each
window reads it, so turning Yapfast off in Task Manager's Startup apps (an odd
first byte in `Explorer\StartupApproved\Run`), or a desktop setting
`Hidden=true` or deleting the file, shows as off. Turning it on here clears
Task Manager's record too. Each start, outside the preview, rewrites an entry
that's on with this Yapfast's path, so moving its folder or an update keeps it
working. The entry keeps `--data-dir` if Yapfast had one; an AppImage's entry
names the AppImage, not its temporary mount. The preview, and platforms
without either, don't show the switch. The Windows installer's "Start Yapfast
automatically when Windows starts" writes the same value, and uninstalling removes it.

`--background` restores the saved session and goes to the tray without
showing the window, or shows it when there's no session to restore, since
signing in needs it. Windows knows at once whether the icon went into the
notification area; without it the window shows. On Linux the bus says a
moment later, and a tray that's missing, or goes while the window is closed,
opens the window. A window made late, from the tray, fails over to Skia
Software as the first does when the GPU fails.

## The window

`src/ui/shell.rs` holds the window. Closing it drops the whole component, its
renderer and its window, rather than hiding it: a hidden Slint window keeps
all of that, and saves nothing ([measurements](measurements/README.md)).
What only the window used stops with it: the microphone meters and the camera
test that Voice settings run, the image viewer's picture and an open file
dialog. Half a second later Yapfast gives freed memory back: `malloc_trim` on
Linux; on Windows `HeapCompact` on the process heap, which Rust allocates
from, and `SetProcessWorkingSetSize(-1, -1)`, which empties the working set.
Task Manager's Memory column is the private working set, which the window's
pages would otherwise stay in; they come back from the standby list if
touched.

The models stay with the adapter, so a new window shows them without building
them again. Releasing the timeline's rows, the window's copies of pictures and
Skia's caches as well saved almost nothing, so they stay.

Opening makes the window again with the size, place and maximized state of
the last one (Wayland ignores the place), the same conversation scrolled to
the message that was at its top, or to the newest if that showed, and
everything else from the application's state. The adapter remembers what it
last set for a few properties, such as the sidebar's title and the topic, to
set them only on a change; attaching a window forgets that, so the first
refresh fills them in. Yapfast's event loop runs until quit, so Quit, the
renderer restart and closing without a tray end it.

Without a window, backend events still apply. After each burst Yapfast rings for
an incoming DM call, or stops, and updates the tray icon. A notification
clicked while the window is closed opens it on that conversation.

## Linux

The icon is a StatusNotifierItem through [`ksni`](https://crates.io/crates/ksni),
pure Rust over the zbus Yapfast already has for notifications, with ksni's
async-io and blocking features, so it adds only `ksni` and `task-local` to
`Cargo.lock`. It connects to the session bus Yapfast is given. A thread
(`yapfast-tray`) registers it and sends it the unread dot and the call as they
change, since each update waits on the bus; ksni's own thread answers the
tray. ksni follows the StatusNotifierWatcher: Yapfast counts the tray as there
while the watcher is.

KDE Plasma, and most bars such as Waybar, run a watcher. GNOME needs the
AppIndicator extension; without it closing quits.

## Windows

The icon is `Shell_NotifyIconW` through `windows-sys`, already in the tree for
winit, on a hidden window of the UI thread, whose messages winit's loop
dispatches. A left click opens Yapfast; a right click shows the menu at the
pointer with `TrackPopupMenu`. When Explorer starts again it sends
`TaskbarCreated`, and Yapfast adds the icon again. If the notification area
won't take the icon, closing quits.

The bar is a cross-build and Wine (`scripts/build-windows`, see
[the Windows beta](windows-beta.md)). On real Windows, check:

- the icon is crisp at 100%, 125%, 150% and 200% scaling, and its dot shows
  for an unread DM or a mention;
- a left click opens the window, at its size and place, on the same
  conversation; a right click shows Open Yapfast, Disconnect in a call, and
  Quit, which closes when clicking elsewhere;
- a call keeps going with the window closed, and Disconnect ends it;
- Task Manager's Memory column drops when the window closes, and stays down
  for a minute;
- after restarting Explorer, the icon comes back;
- the icon goes when Yapfast quits, and doesn't linger until hovered;
- with "Start when you sign in" on, signing out and in starts Yapfast in the
  tray with no window, and Task Manager's Startup apps lists it; turning it
  off there shows as off in Settings;
- the installer's "Start Yapfast automatically when Windows starts" does the same, an update
  installs over a running Yapfast, and uninstalling leaves nothing in Startup
  apps.

The registry side is checked under Wine: `autostart::windows::tests`, ignored
since it writes the current user's Run key, passes with a throwaway
`WINEPREFIX`, and the installer installs, updates over Task Manager's "off",
and uninstalls under `xvfb-run` in one, leaving the data folder.

## Testing

`scripts/ui-session start --tray` runs `testing/tray_host.rs`, a minimal
StatusNotifierWatcher, on the session's private bus. It refuses to run
outside the isolated display, where it would stand in for the desktop's tray.
`do 'tray menu'`, `'tray click Disconnect'`, `'tray activate'` and
`'tray icon NAME'` act as a tray would, `'tray off'` and `'tray on'` take the
tray away and bring it back, and `'window close'` closes the window as its
close button does. `testing/measure_tray.py` measures memory through these.
