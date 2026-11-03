# Development

How to build Yapfast, try it, and test it locally. [Building](building.md)
covers the toolchain, Cargo features and packaging in depth; the
[architecture](architecture.md) shows how the code fits together.

## The development shell

On NixOS, or anywhere with Nix, enter the pinned environment from the project
folder:

```sh
nix develop path:./nix
cargo run --locked
```

`path:./nix` also works before files are tracked and keeps Cargo output out of
the Nix source snapshot. Run the `cargo` commands below inside the shell;
[the shells](building.md#shells) says what each one provides. Local work
builds with `--profile fast`.

```sh
scripts/check --fast
cargo build --locked --release
```

`scripts/check` enters the Nix shell itself; [building](building.md#checks)
lists what it covers.

## Try the UI without an account

```sh
nix develop path:./nix --command cargo run --locked -- --preview
```

Use the upper-left DM button for direct messages. Server buttons sit below it;
the sidebar shows only the selected server's channels. Filter that list, and
send local preview messages with Enter or the Send button. Preview messages
disappear when you leave the preview. "Back to sign in" opens real Matrix
sign-in. Normal launch restores your saved session. Use `--data-dir PATH` for
a separate test session. Passwords and recovery keys are entered in masked
native fields.

Closing the window leaves Yapfast running in the system tray, with any call,
where its icon opens it again or quits; without a tray it quits Yapfast
([the tray](tray.md)). For Yapfast's icon on its window and in the launcher,
run `scripts/install-desktop-entry` once after a release build
([the app icon](packaging.md#the-app-icon)).

Discord runs only against the fake server, `testing/fake_discord.ts`, in
development; `scripts/ui-session start --discord` starts both on a private
display ([seeing the app](building.md#seeing-the-app)).

## Renderer

Yapfast renders with Skia Software, which uses about half the memory of Skia
GPU. To use the GPU:

```sh
nix develop path:./nix --command cargo run --locked -- --preview --renderer gpu
```

Omit `--preview` to sign in or restore your real account. If the GPU cannot
start, Yapfast restarts itself with Skia Software and explains why
([renderers](slint-client.md#renderers-and-native-builds)). Renderer
selection does not change session storage, encryption or the native audio
backend.

## Matrix

Real sending requires a verified device and an encrypted room. Verify Yapfast
in another client and press "Check again", or enter your existing recovery
key. Join rooms using another Matrix client for now. Messages support Markdown,
replies, reactions, editing, deletion and encrypted attachments. Messages
written offline wait in the SDK's send queue, and drafts survive restarts; see
[the chat guide](chat-ui.md). There is no room creation, channel discovery or
ringing Matrix DM calls yet.

Servers come from joined Matrix Spaces, including channels in joined
subspaces. Clicking a server opens the channel you last had open there, even
after a restart, or else its top channel. Text channels and group DMs list
their members on the right, grouped into admins, moderators and members; the
header's member count shows or hides the list. Only joined channels appear;
standalone rooms outside Spaces are not shown in this navigation yet.

With the local services running, check the desktop messaging backend with:

```sh
cargo test --locked -p yapfast-core matrix_chat::tests::encrypted_chat_restore_send_retry_and_security_gates -- --ignored
```

This creates disposable accounts and checks session restoration, encrypted
delivery, duplicate-free retry, Space membership and live channel removal, and
refusal of plaintext rooms and unverified devices. The UI's own checks are in
[the Slint client's validation notes](slint-client.md#validation-and-limits).

The headless build includes the Matrix and native media libraries. The
smoke check allocates a synthetic audio source. It does not sign in, connect
to a server, or access the microphone.

## Local integration services

With a running Docker daemon and Compose v2:

```sh
scripts/test-stack up
scripts/test-stack check
scripts/test-stack down
scripts/test-stack reset
```

`reset` deletes only this stack's disposable database and certificate volumes,
then restarts the services. The services bind to loopback and use test
credentials. They are for development on one machine. Remote testers require
a separate TLS deployment with reachable media ports.

The check provisions `alice`, `bob`, and `charlie` on `synapse`, with the
password `talk-test-only`. It verifies homeserver capabilities and a real
Matrix OpenID to LiveKit authorization exchange, including rejection of an
invalid token. It does not prove audio encryption or room-membership
authorization.

| Service | Local address |
| --- | --- |
| Matrix homeserver | `http://localhost:18008` |
| MatrixRTC authorization | `http://localhost:16080` |
| LiveKit | `ws://localhost:17880` |

## Sessions and the native call experiment

With the local services running:

```sh
cargo run --locked --no-default-features --features matrix,discord -- --call-experiment
```

This creates fresh disposable accounts, checks restoration and recovery, then
tests audio in both directions, invalid keys and key rotation after a
participant leaves. It uses synthetic PCM and accesses no microphone or
speaker.

Run `cargo run --locked --no-default-features --features matrix,discord --
--help` for session commands. [Session instructions](sessions.md) cover
hidden password/key prompts, secure storage, logout retention and current
authentication coverage.

## Try voice locally

Inside the development shell:

```sh
scripts/test-stack up
cargo run --locked --features matrix -- --voice-demo
```

The demo prints commands for two saved test accounts. Open them in separate
processes, select **Yapfast voice test → Lounge**, and press **Join voice**.
Each joins with the microphone on, so mute one, or use headphones. The gear
button selects microphone and speakers. The
[voice guide](voice.md) covers requirements, checks and the Windows test.

## Windows

Build the Windows packages from this Linux checkout:

```sh
nix develop path:./nix#windows --command scripts/build-windows        # Discord release
nix develop path:./nix#windows --command scripts/build-windows full   # full preview
```

The ZIP and an installer appear in `target/dist/discord` or
`target/dist/full`. The full preview's sign-in screen defaults to
`https://matrix.org`. See the [Windows guide](windows-beta.md) for
two-machine checks, and [building](building.md#windows) for the options and
native Windows prerequisites.

## Measurements

Memory is a product goal. The [resource measurements](measurements/README.md)
record their workloads and limits, and how to repeat them.
