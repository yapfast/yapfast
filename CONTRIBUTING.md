# Contributing

Bug fixes and features are both welcome. Before a large change, say what you're
building in an issue or on the
[Yapfast Discord server](https://yapfast.com/discord-invite), so two people
don't build the same thing. A small fix can go straight to a pull request.

## Build and test

[Development](docs/development.md) covers running the app, and
[Architecture](docs/architecture.md) shows where code lives and how a UI change
flows from `ui/*.slint` to `application/`.

Nix is optional. `nix develop path:./nix` gives you the pinned toolchain and
libraries. Without it, [Without Nix](README.md#without-nix) lists what Linux
needs, and [Building](docs/building.md#windows) covers a native Windows build.
Build with the `fast` profile, and format before you check:

```sh
cargo build --locked --profile fast
cargo fmt --all
```

Run these once before you ask for a review:

```sh
scripts/check --fast
scripts/test-fake-discord
```

`scripts/check` runs formatting, doc links, Clippy, the tests and a smoke run of
the default build. It enters the dev shell when Nix is installed, and uses your
own tools otherwise. Each compile of the UI crate takes 6–8 GiB of memory, which
is why its steps run one after another. Add `--matrix` when you change code that
Matrix shares.

`scripts/test-fake-discord` runs the Discord service tests against
`testing/fake_discord.ts`, and needs Bun, Python and curl. Run it when you change
`crates/yapfast-discord`, `discord_text` or the fake; while you work, name the
area you changed, as `scripts/test-fake-discord forum`.

If you can't run a script (on Windows, say), run what you can of its steps by hand
and say what you skipped in the pull request, so someone else can run it:

```sh
cargo fmt --all -- --check
cargo clippy --locked --profile fast --all-targets --workspace --exclude feature-union -- -D warnings
cargo test --locked --profile fast --workspace --exclude feature-union
```

For a Discord feature, follow
[Adding a Discord feature](docs/discord/adding-a-feature.md), which goes through
it layer by layer. Look up the endpoint, payload or gateway event on
https://docs.discord.food, and give the fake the same shape.

To see a Discord change, run your build against the fake, with the fake in one
terminal and Yapfast in another:

```sh
bun testing/fake_discord.ts --port 18400
YAPFAST_DISCORD_TEST_SERVER=http://127.0.0.1:18400 cargo run --locked --profile fast -- --data-dir .local/fake-discord
```

`YAPFAST_DISCORD_TEST_SERVER` makes Yapfast use a test token kept in memory and
leave the system's credential store alone. Keep it on every run: without it,
Yapfast signs in to any Discord account linked on your machine. `--data-dir`
keeps the run's settings and cache apart from your own, and lets it run beside a
Yapfast you already have open. Add `YAPFAST_DISCORD_TEST_UNLINKED=1` to go through
linking an account against the fake. The comment at the top of
`testing/fake_discord.ts` lists its options, such as `--servers 3` and `--dms 50`.

With Nix, `scripts/ui-session` does the same on a private display, and adds
screenshots and scripted input; run it with `--help`.

A coding agent is fine. `CLAUDE.md` gives it these commands and rules, and
assumes Nix, so tell it if you don't use it. Read what it wrote and run the result
before you open the pull request.

## Memory and CPU

Memory is a product goal. Share data held in two places (`Arc`, a borrow), make
work that runs on every event or frame touch only what changed, and stream data
to a file or socket as it's made.

A change that adds a cache, keeps more data in memory or bundles files gives its
cost in the pull request: the heap measured as
[Measurements](docs/measurements/README.md) describes, and the size of the files
it adds. A commit that saves memory gives the numbers it measured.

A cosmetic that downloads or animates pictures needs a switch in settings. With
the switch off, it downloads and decodes nothing. Where a feature trades CPU for
quality, default to the cheaper setting and put the heavier one behind a switch.

## Pull requests

- Keep each pull request to one feature or fix, started from `origin/main` after
  `git fetch`.
- Base every pull request on `main`, including the layers of a stack, since a
  layer merged into another layer's branch never reaches `main`. In each layer's
  description, say which pull request goes in first.
- Write commit subjects as Conventional Commits, in the present tense, saying
  what changes for the user: `feat(discord): join and leave threads from their
  menu`. The body says what it does and why.
- A change in behavior updates its doc in the same commit. Docs describe how
  things work now; history and measurements go in the commit message.
- In the description, say what you ran, what you skipped and which systems you
  tried it on. Add screenshots for a UI change, and name any new dependency with
  the reason for it.
- The maintainer has no Mac, so a macOS change needs a run on a real Mac by you
  or another contributor.

Your contribution is licensed under the [GPL, version 3 or later](LICENSE), as
the rest of Yapfast is.

## Reporting a bug

Give the Yapfast version, your system and the steps that show the bug, and add a
screenshot or video if it's visual. For voice and streaming problems, include
your graphics card and the settings you used.
