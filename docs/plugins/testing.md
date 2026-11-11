# Testing

The plugin system is tested at four levels, because four different things can be
wrong: a guest that breaks the ABI, the helper's ordering and deadlines, the
parent's permissions and network gate, and whether any of it happens at all in a
running Yapfast. Everything runs in the dev shell, with the `fast` profile, as
[the rest of the repository](../development.md) does.

## The `.wat` fixtures

`testing/plugins/*.wat` are hand-written WebAssembly text files, one per way a
plugin can behave. They need no WebAssembly toolchain: the helper compiles a
`.wat` as readily as a `.wasm`, and Extism calls an export with no arguments and
reads the answer out of the guest's memory, so a fixture is a short module and a
status code. Zero means the call succeeded.

| Fixture | What it does | What a test expects |
| --- | --- | --- |
| `echo.wat` | `yf_activate` succeeds and does nothing; `yf_event` copies its input back; `yf_hook` answers `"unchanged"` | The bytes a test sent come back through the pipe, the scheduler, the runtime and the guest's memory |
| `spin.wat` | Never leaves `yf_activate` | A `deadline` fault, and a helper still serving afterwards |
| `hog.wat` | Grows linear memory 64 pages at a time until the limit refuses | An `out_of_memory` fault, with the instance loaded under a small page limit |
| `trap.wat` | `unreachable` in `yf_activate` | A `trap` fault, a strike, and the helper carrying on with other plugins |
| `bad_activate.wat` | Returns a non-zero status from `yf_activate` | An `activate_failed` fault, and a helper that is still running |
| `bad_answer.wat` | Answers its own question where a `HookOutcome` belongs | A `bad_answer` fault, the message going out as it was, and the rest of the chain still running |
| `slow_host.wat` | Asks for `http.fetch` in `yf_event` and returns | The call arriving at the parent, its answer arriving as a later event, and another plugin still running while it waits |
| `filter.wat` | Answers `yf_hook` with a draft of its own: another room, other text | The chain carrying that draft, and the send going out with it — M4's first exit criterion |
| `spin_hook.wat` | Activates, then never leaves `yf_hook` | A `deadline` fault, the chain answering without it, and a helper still serving — M4's second |

`echo.wat` exports all six guest functions because a hook chain may include it;
the exports a scenario never reaches return 0 with no answer, which the ABI reads
as nothing.

## The helper

```sh
cargo test --profile fast -p yapfast-plugin-host
```

That runs the crate's unit tests, which cover the parts that are decisions rather
than WebAssembly: the frame coding, the last report written before a pipe closes,
a log ring that keeps the newest lines and belongs to one plugin, the runtime's
classification of a trap against a deadline against running out of pages, a
hand-off cut on a character boundary, and a cache configuration whose path can't
be broken by a quote.

The scheduler is driven through the `Backend` trait with a fake runtime, so lane
priority, one FIFO mailbox per plugin, deadline firing, strike counting and
admission against the helper's budget are tested without loading a module. That
separation is why `backend.rs` exists: the scheduler decides when and in what
order, and knows nothing about Extism.

The fixtures are what `crates/yapfast-plugin-host/tests/e2e.rs` drives: it starts
`yapfast-plugins run` on a pair of pipes, says `hello`, loads a fixture, and reads
the frames back, checking the fault kinds and statuses in the table above. Each
test starts its own helper and kills it when it ends, so a scenario that hangs is
that test's problem alone, and every wait has a budget. A deadline test that
passes only because the fixture was never called is a test that says nothing, so
each one also checks that the helper is still serving afterwards.

## The parent

```sh
cargo test --profile fast -p yapfast-core --features plugins plugins
cargo test --profile fast -p yapfast-core --features plugins --test plugins_gate
```

The filter in the first line matches test names, so it runs the plugin modules'
unit tests; the gate's rules are integration tests of their own and need the
second line.

The plugin system is behind the `plugins` feature, so these tests only exist in a
build that has it. They cover the network gate, permissions, grants, the store,
the registry and the crash counter. The gate's tests are the security ones, in
`crates/yapfast-core/tests/plugins_gate.rs`, one per rule, each refused with a
sentence rather than a status code:

- a request to a name that resolves to a private address, and a redirect that
  does the same on its second hop;
- one to Discord's own hosts;
- one to a host nobody declared and the person hasn't allowed, and one whose
  grant was revoked a moment ago;
- a bare `*` in a manifest, and a host pattern that isn't one;
- a response past the plugin's cap, and a fifth request while four are in flight;
- a loopback address, a port other than the one the manifest named, and a
  `network.local` with no port;
- a header the plugin may not set, and a `User-Agent` that isn't Yapfast's.

The application's side is tested with a fake bridge in
`crates/yapfast-core/src/application/tests.rs`: a send with an intercepting
plugin goes out once, in order, and a chain that fails or runs late lets it go
unchanged.

A hook's budgets are tested where they are enforced, which is two places. In the
helper, `crates/yapfast-plugin-host/tests/e2e.rs` sends a chain to
`spin_hook.wat` and measures the whole thing from the frame to `HookDone`: the
plugin's own 25 ms, not the chain's 100 and not an activation's two seconds, with
a `deadline` fault against it and the helper still serving afterwards. The same
file times 200 hook exchanges against `echo.wat` and 200 JSON round trips of the
same messages, and prints the share JSON is of a hook;
[the measurements](../measurements/README.md#a-hooks-json) record the number and
what it means for MessagePack. In the parent, `plugins_runtime.rs` runs both M4
scenarios through the real helper: `filter.wat` rewrites the text and the
rewritten draft is the answer the send goes out with, and `spin_hook.wat` holds a
send for no longer than its budget while the message goes out as it was written.
`plugins::bridge`'s unit tests cover the rest without a helper: a `block` from a
plugin whose manifest doesn't say `on_failure: "block"` fails open, and a plugin
that asked for `settings.on_change` is answered `pending` and then told of a
change, while one that never asked is told nothing.

Two tests run the real helper, which is the only way to see the two halves
agree, and both say so and pass when `YAPFAST_PLUGINS` names no binary:

```sh
cargo build --profile fast -p yapfast-plugin-host
YAPFAST_PLUGINS=$PWD/target/fast/yapfast-plugins \
  cargo test --profile fast -p yapfast-core --features plugins --test plugins_runtime
YAPFAST_PLUGINS=$PWD/target/fast/yapfast-plugins \
  cargo test --profile fast -p yapfast-core --features plugins --test plugins_install
```

`plugins_runtime.rs` is M1's exit: a plugin installed in a store receives
`message_create` from the helper, and disabling it ends the helper.
`plugins_install.rs` is M2's: a repository made in a temporary directory is
fetched through a `file://` URL, reviewed, installed, rolled back and removed,
and a second folder claiming an installed id is refused.

The Settings → Plugins page itself is checked in `src/ui/tests.rs`: binding the
window gives the page the runtime, and a view handed over while it is open is
what it draws.

## Against the fake Discord

```sh
scripts/test-fake-discord plugins
```

runs the tests against `testing/fake_discord.ts`, whose gateway events are what
the Discord event taps turn into plugin events. The argument is a filter, as it is
for any other area; run it with no filter before the last commit of work on the
plugin bridge or the core's Discord side, which takes about 30 seconds.
`scripts/check` runs `cargo test --workspace`, which builds and tests both plugin
crates, and has steps of its own for `yapfast-core` with the `plugins` feature,
which the workspace build doesn't reach. It doesn't run the fake Discord suite
either.

`scripts/ui-session` runs the app on a private display with screenshots and input,
which is how the Plugins page, a plugin's settings rows and a consent dialog are
looked at rather than asserted.

A test binary of `yapfast-core` links the native media stack, so in the Nix
development shell it needs a 64-bit `libstdc++` on the library path to run at
all: `LD_LIBRARY_PATH=$(dirname "$(gcc -print-file-name=libstdc++.so)"):$LD_LIBRARY_PATH`.
Without it the binary exits before a test starts, with a missing
`libstdc++.so.6`.

## Memory

`testing/measure_plugins.py` measures what the design promises, each number in
its own run: the helper's footprint before any plugin is loaded, with one plugin
enabled but idle (which should be no helper at all), with one instance active, and
after the last one is evicted. It reports the helper's own memory the way
[the measurements](../measurements/README.md) do, as the private anonymous
memory of its process, so a number from it can be compared with Yapfast's own.

Build with `--profile measure` for that, not `fast`: `fast` leaves the interface
unoptimized, and the point of the comparison is the shipped shape of the app. The
budgets the system holds itself to are a 256 MiB helper, 16 MiB per instance by
default, and nothing at all while no plugin is enabled.
