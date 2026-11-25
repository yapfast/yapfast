# Yapfast

- On Windows, build, run and test natively with Cargo: `cargo build --locked`.
  Use Developer PowerShell for VS with Visual Studio C++ Build Tools, a Windows SDK, LLVM/Clang
  (`clang-cl` and `libclang`) and NASM; set `LIBCLANG_PATH` as described in
  `docs/building.md#windows`. Nix is not required on Windows.
- On Linux, run every cargo command in the Nix dev shell:
  `nix develop path:./nix --command cargo build --locked`.
- While working, build and test only what changed, such as
  `cargo test -p yapfast-core history`; incremental builds take
  seconds. On Linux, run `scripts/check` once, before the last commit of a piece
  of work, not before each commit. It enters the dev shell itself and checks
  the default build, the window with Discord; `--matrix` adds the builds with
  Matrix, for changes to code Matrix shares and before a release. It prints
  each step's time. Its steps run one after another: running compiles
  together ran the machine out of memory.
  On Windows, run the Cargo checks directly as listed in `CONTRIBUTING.md`,
  and report any checks that need Linux and could not be run.
- In a new Linux worktree, run `scripts/warm-worktree` before building. It copies
  the main checkout's build, so the first check compiles only the crates in
  the repository, about 3.5 minutes.
- An edit under `vendor/iced_*` or `vendor/cosmic-text` recompiles iced and
  the window's crates; settle such a patch in as few builds as possible.
- See the app on Linux: `scripts/ui-session` (private display, screenshots,
  input; run it with `--help`). On Windows, use `cargo run --locked -- --preview`,
  or run against the fake as described in `CONTRIBUTING.md`. Discord runs only against `testing/fake_discord.ts`.
- Measure and profile with the `local` profile, whose builds stay apart
  from everyday builds in `target/debug/`.
- Discord's API, as its own apps use it: https://docs.discord.food. Look up
  an endpoint, payload or gateway event there before adding or changing one,
  and give the fake the same shape. How Yapfast does each Discord feature,
  one doc per area: `docs/discord/`; adding a feature, layer by layer:
  `docs/discord/adding-a-feature.md`.
- Discord service tests on Linux: `scripts/test-fake-discord [FILTER]`. While
  working, run the tests of the area you changed by name (`forum`,
  `calls_in_dms`), a second or so each. Run the whole suite, about 30 s, once
  before the last commit of work on `crates/yapfast-discord`,
  `discord_text` or `testing/fake_discord.ts`; `scripts/check` doesn't run
  it. On Windows, run the available Cargo tests directly and report any
  Linux service checks not run. Matrix and LiveKit: `scripts/test-stack up`.
- Where code lives, and how a UI change flows from the window to
  `application/`: `docs/architecture.md`. Changing the window
  (`crates/yapfast-iced`, its kit `crates/yapfast-iced-kit`): where an
  area's code goes and adding a screen, `docs/client/structure.md`; how each
  area behaves, its doc in `docs/client/`. Shells, logging (`YAPFAST_LOG`,
  not `RUST_LOG`), dependency sources and media libraries:
  `docs/building.md`.
- Docs describe how things work now; history and measurements go in the
  commit message. A change in behaviour updates its doc in the same commit.
- Memory is a product goal. Keep every change lean without being asked, and
  check the code it runs through as well: share data held in two places
  (`Arc`, a borrow), make work that runs on every event or frame touch only
  what changed, and stream data to a file or socket as it's made. Fix a
  needless copy you find on that path, or report it. Targets, and how to
  measure: `docs/measurements/README.md`. A commit that saves memory gives
  the numbers it measured.
- Start new branches from `origin/main` after `git fetch`.
