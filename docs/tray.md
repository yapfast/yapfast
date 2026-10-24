# The tray

Closing the window leaves Yapfast running in the system tray: the session, a call, notifications, ringing and sounds go on. The
tray icon is Yapfast's logo in Yap's marigold, with a red dot cut into its
corner while a mention or a DM waits unread, as Discord counts them. Clicking
it opens the window. Its menu has Open Yapfast, Disconnect during a call, and
Quit.

Settings → Advanced has "Close to tray", on unless turned off
(`close_to_tray` in `layout.json`). Off, or with no tray to show the icon,
closing the window quits Yapfast, as it did before; the setting's description
says when there's no tray. A tray that starts after Yapfast, or comes back,
still gets the icon.

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

With Skia Software, closing saves 2.3 MiB of anonymous memory in a channel
(18.6 to 16.3 MiB) and 0.9 MiB in a call; with the GPU, 24 MiB on the first
close ([measurements](measurements/README.md#2026-10-14-closing-the-window-to-the-tray)).
The models stay with the adapter, so a new window shows them without building
them again. Releasing the timeline's rows, the window's copies of pictures and
Skia's caches as well saved 0.25 MiB of heap and no anonymous memory, so they
stay.

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
async-io and blocking features, so it adds only `ksni`, `task-local`, `arc-swap` and `crossbeam-queue` to
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
- the icon goes when Yapfast quits, and doesn't linger until hovered.
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
