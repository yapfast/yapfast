<p align="center">
  <img src="packaging/icon.svg" width="96" height="96" alt="">
</p>

<h1 align="center">Yapfast</h1>

<p align="center">
  <b>A Discord client without the web browser.</b><br>
  About 40 MB of RAM, where Discord's own app takes 800.
</p>

<p align="center">
  <a href="https://github.com/yapfast/yapfast/releases/latest"><b>Download for Windows</b></a>
  &nbsp;·&nbsp;
  <a href="https://yapfast.com">yapfast.com</a>
  &nbsp;·&nbsp;
  <a href="https://yapfast.com/discord-invite">Discord server</a>
  &nbsp;·&nbsp;
  <a href="#features">Features</a>
  &nbsp;·&nbsp;
  <a href="#build-it-yourself">Build it yourself</a>
</p>

<p align="center">
  <img src="docs/images/yapfast.png" alt="Yapfast showing a group DM, with the server list on the left, the conversation in the middle and its members on the right"><br>
  <sub>A test account on a fake Discord server. Nobody in there said any of this, or endorses Yapfast (sadly).</sub>
</p>

## Why?

Discord's desktop app is a whole ass Chromium running Discord's website, and it
sits at about 800 MB of RAM doing nothing (insane, I know right?!?). That
annoyed me enough to write my own in Rust.

Yapfast does the same job (doing nothing) in about 40 MB. There's no browser
engine anywhere in it or near it: it talks to Discord's API itself and draws its
own window with [Slint](https://slint.dev) and Skia, like a game would. Your
servers, DMs, friends, voice and GIFs are all there, and if you know Discord,
you already know where everything is.

It opens fast af, too. I timed both, from clicking the icon to reading my
messages: Discord took 8 seconds on my PC, Yapfast took 0.22, because it shows
the messages it saved last time and catches up while you read.

Sitting in a call with the window open, it's about 60 MB, and noise suppression
alone eats 20 of those; that's as small as I could get it :( There's a GPU
renderer too, behind **Render with the GPU** in Settings, but it takes about
120 MB, and drawing on the processor already keeps a 180 Hz monitor fed, so the
processor is the default. Your numbers will depend on how many servers and DMs
you have. [How I measure](docs/measurements/README.md).

It's a 0.1 beta, and it's what I use for Discord every day, so it had better
be usable. Still, there are rough edges; when you hit one,
[open an issue](https://github.com/yapfast/yapfast/issues) and I'll fix it, or
come hang out on the [Yapfast Discord server](https://yapfast.com/discord-invite)
and tell me there. The connection drops every now and then (no more often than
Vesktop's or the official app's does for me), and the noise suppression could
be better. Know an open source one that does a better job? PLEASE tell me.

## Will Discord ban me?

> [!WARNING]
> Probably not, but I can't promise it. Third-party clients, Yapfast included,
> are against Discord's Terms of Service. In practice Discord rarely bans anyone
> for using one; the accounts that get banned are almost always spamming or
> running automation. Yapfast only does what you click, and keeps to the same
> limits Discord's own app does.
>
> If losing your account would be a disaster, don't use any third-party client,
> this one or another. And don't post screenshots of it in a server whose
> moderators might ban you for it. Yapfast tells you all this again before it
> shows the sign-in code.

## Where your messages go

Straight from your PC to Discord, or to your Matrix server. I don't run a
Yapfast server, so nothing in between could keep a copy (I'd have to pay for
servers just to snoop on your DMs, and no thanks :D).

- You link your account by scanning a QR code with Discord's phone app, so
  Yapfast never sees your password.
- The login token stays in your system's credential store, and only ever goes
  to Discord. Unlinking signs that session out and deletes it.
- It sends no analytics or crash reports, and doesn't check for updates.
- Pictures and GIFs load from wherever they're hosted, such as Discord or
  Tenor.

Don't want to take my word for it? Fair, the code's all right here.

## Get started

### Windows 10 and 11

1. Download `Yapfast-…-windows-x64-setup.exe` from the
   [latest release](https://github.com/yapfast/yapfast/releases/latest).
2. Run it. It installs just for you, with no administrator prompt. The
   installer isn't signed yet, so SmartScreen may get nervous: choose **More
   info**, then **Run anyway**.
3. Yapfast shows a QR code. On your phone, open Discord, go to **Settings →
   Scan QR Code**, and point the camera at it.

Rather skip the installer? The release has a portable ZIP too: extract it and
double-click `yapfast.exe`.

Yapfast starts with Windows and waits in the tray, so messages and calls still
reach you. Untick that in the installer, or turn it off later in **Settings →
Advanced**.

### Linux

It runs great on both X11 and Wayland, screen sharing too (Hyprland here).
There's no package yet; an AppImage and an Arch package are coming with the
next release. Until then, if you have [Nix](https://nixos.org), it's one
command (pretty sure, anyway :D), and you get the same Discord-only app the
releases ship, in your launcher:

```sh
nix profile add github:yapfast/yapfast#yapfast
```

It compiles from source, so give it a few minutes. No Nix? Then
[build it yourself](#build-it-yourself).

### macOS

Built for Apple Silicon and Intel, and it's a proper Mac app: a `.app` bundle
with its own icon, the standard menu bar, a menu bar status item, system
notifications, and closing the window to the menu bar instead of quitting.

There's no signed build to download yet, so it's one command from source:

```sh
git clone https://github.com/yapfast/yapfast
cd yapfast
./scripts/build-macos
open target/dist/macos/Yapfast.app
```

The first build downloads a prebuilt libwebrtc and prebuilt Skia, so it takes a
few minutes rather than the half hour it would take to compile them. See
[docs/macos.md](docs/macos.md) for what works, the gaps, and how to make a dmg.

### Nightly builds

Want what's on `main` right now, without waiting for a release or building it
yourself? Every now and then I put out a nightly build of the Discord-only app.
They're on the [releases page](https://github.com/yapfast/yapfast/releases),
marked **Pre-release**, newest first, each with a list of what changed:

- **Windows:** the portable ZIP. Extract it and double-click `yapfast.exe`.
- **Linux:** the AppImage, the one coming to releases too. `chmod +x` it and
  run it.
- **Arch Linux:** the `.pkg.tar.zst`, the one coming to releases too. Install
  it with `sudo pacman -U` and the file or its link.

They're untested between releases, so expect the odd rough edge.

## Features

- Servers, channels, DMs and group DMs, all live. Pin, mute or close a DM from
  its menu, and accept or ignore message requests.
- Messages you can send, reply to, edit, react to, pin, forward and attach
  files to, with Markdown, embeds, custom emoji, stickers and spoilers.
- Threads: start one from a message, browse a channel's threads, join or leave
  one, and read it beside its channel.
- Forums: browse the posts as cards, with their tags, first message, top
  reaction and thumbnail, search them, post in them, and read one beside the
  forum.
- Search a whole server with Discord's filters, and browse a channel's pins.
- The GIF picker, my favorite part :p Your favorites live on disk as 40 KB
  clips, so they're there the moment it opens, and you're done sitting there
  cringing while the GOATed GIF loads.
- Voice channels and DM calls, with mute, deafen, a volume for each person,
  noise suppression, echo removal, and a voice gate you can tune, or
  push-to-talk on any key you like, held in any app. Go Live, watch
  other people's streams and turn on your camera. The call stays up while you
  read other channels.
- Friends: requests, adding by username, nicknames, notes, profile cards, your
  status and custom status (custom emoji included). Block and ignore people
  from their menu, and hide what blocked people write.
- Running a server: make and edit channels and categories, invite people, hand
  out roles, change nicknames, time out, kick and ban. In voice, server mute,
  deafen and move people to another channel. Edit your server profile, and
  leave. Join a server by invite, from the + under your servers or a link in
  chat. Folders show up as they do in Discord.
- Notifications that follow your Discord settings, muting for channels,
  categories and servers, unread markers and member lists.
- Ten themes, Catppuccin and Tokyo Night among them, and Bonfire, which is
  Discord's own colors. Or make your own in the editor, and watch the window
  change as you type each color.
- Soft sounds for joining, leaving, muting and messages, in six sets.

### Not there yet

- Media channels.
- The soundboard.
- Activities and server settings.
- Making and changing server folders.
- A server's form for new members, such as its rules, when you join. Yapfast
  sends you to Discord in your browser to fill it in.
- A list of the people you blocked.
- Leaving a group DM.
- Animated custom emoji in messages, which show their first frame.
- Captchas, sort of. If Discord asks for one while you link or join a server,
  Yapfast opens it in your browser, but I haven't hit one against real
  Discord yet, so who knows..

Use Discord in a browser for these until then; it has all of it, and you have a
browser open anyway. [The Discord plan](docs/discord.md) has the rest, in the
order I'm working on it.

### Not for you?

- [Vesktop](https://vesktop.dev) if you want plugins. Yapfast has none yet
  (they're coming, dw), and Vesktop ships with Vencord's.
- [Ripcord](https://cancel.fm/ripcord/) if you want something even plainer.
  It's native too, but closed source, and it does Slack on the side.

## What about Matrix?

Yapfast started as a [Matrix](https://matrix.org) client, and Matrix still
works: encrypted rooms, Spaces as servers, and voice channels over MatrixRTC, in
the same window as Discord. The release is Discord-only for now, so
[build it yourself](#build-it-yourself) to get both.

After that I'd like Telegram, Signal, WhatsApp, Instagram and Messenger in there
too. No clue when, but hopefully at some point :p

## Other questions

**Why not Tauri?** Tauri apps run a website too; they just borrow your
system's web view instead of shipping their own Chromium. The browser is the
exact thing I wanted gone, so hell no.

**Can I use my Vencord plugins?** Nope. They need a JavaScript runtime, and
Yapfast's insides look nothing like what they expect. Yapfast will get plugins
of its own, though, and open source Vencord ones could be ported once it does.

**How is it different from Serein?** Serein draws with wgpu, which costs more
memory before you've opened a single channel. Yapfast draws with Slint, on the
processor by default (WAY lighter) or the GPU if you ask. It also does Matrix
(from source, for now), DeepFilterNet noise suppression on top of RNNoise,
voice gates, and the cached GIF picker. It's still missing a few things, like
media channels and the soundboard.

**Did you use AI to write it?** Yep. Opus 5.5 wrote most of the code, and 6.1
Sol and 6 Astra did adversarial review and helped with the nastiest bugs
(networking protocols mixed with VP8 is a bit much for a side project :D). It
wasn't one prompt and done, though. I've used it every day for weeks, profiled
it with perf, read the code, and bent the UX to my muscle memory, so switching
over hurts as little as it can.

## Build it yourself

Yapfast builds inside a pinned [Nix](https://nixos.org) environment, which
brings the right Rust, Clang and native libraries. From the project folder:

```sh
# Try the UI without an account
nix develop path:./nix --command cargo run --locked -- --preview

# Run it for real, with Discord and Matrix
nix develop path:./nix --command cargo run --locked

# Discord only, like the release
nix develop path:./nix --command cargo run --locked --no-default-features --features desktop,discord

# Windows installer and ZIP, cross-built from Linux, into target/dist
nix develop path:./nix#windows --command scripts/build-windows

# Format, lint and test everything
scripts/check --fast
```

| Read this | For |
| --- | --- |
| [Development](docs/development.md) | Day-to-day work: the preview, local Matrix and LiveKit servers, voice tests |
| [Architecture](docs/architecture.md) | How `yapfast-core` and the Slint client fit together |
| [Building](docs/building.md) | Toolchain, Cargo features, checks, releases |
| [Discord](docs/discord.md) | What's done, what's next, and the fake server the tests use |
| [Voice](docs/voice.md) | Calls, devices and audio processing |
| [Windows](docs/windows-beta.md) | Packaging, the installer and Windows checks |
| [Measurements](docs/measurements/README.md) | How memory and CPU are measured |

Memory is a product goal. Keep every change lean, and give the numbers you
measured in any commit that saves memory.

## License

Yapfast is free software under the [GNU General Public License](LICENSE),
version 3 or later. Builds with Matrix also include MatrixRTC libraries under
the AGPL. Every release carries its dependencies' own licenses. Take it, I
don't even care, baka.

## Thanks

Yapfast speaks to Discord the way its own apps do, as documented by
[Discord Userdoccers](https://docs.discord.food), a community effort to
document the user side of Discord's API. Thanks to everyone who writes it <3

It stands on [Slint](https://slint.dev), [Skia](https://skia.org),
[LiveKit](https://livekit.io) and WebRTC,
[matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) and
[DeepFilterNet](https://github.com/Rikorose/DeepFilterNet). My patches to
Slint live in [`vendor/`](vendor), with the rest of the code.

<sub>Yapfast isn't affiliated with or endorsed by Discord Inc. Discord is a
trademark of Discord Inc.</sub>
