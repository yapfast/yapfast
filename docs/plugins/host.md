# The helper

Plugins run in `yapfast-plugins`, a process of its own. Yapfast starts it the
first time a plugin has something to do, and the process ends with the last
instance, so a Yapfast whose plugins are all idle pays neither its memory nor its
threads. [`crates/yapfast-plugin-host`](../../crates/yapfast-plugin-host) is the
binary; [`crates/yapfast-plugin-api`](../../crates/yapfast-plugin-api) is the
contract it shares with the parent, described in [the ABI](abi.md).

It has four subcommands. `run` speaks the framed protocol on stdin and stdout,
which is what the parent starts. `fetch` reads a plugin out of its git
repository, for seconds, and exits, taking `gix` with it. `catalog` reads a
marketplace registry the same way, and `verify-entry` checks one of its entries
against the repository it pins, which is what a registry's CI runs
([`marketplace.md`](marketplace.md)).

A separate process buys three things at once. Nothing of a plugin's outlives it,
wasmtime's compile threads and a guest's linear memory included. A wasmtime or
Cranelift bug takes the helper down and not the chat or the call. And the parent
keeps the Discord token, the network stack and the consent dialogs, while the
helper holds only WebAssembly and its scheduler, so no guest can reach what the
parent never passes it. The parent starts the helper with an empty environment,
because one `EXTISM_*` variable sends a guest's stdout to the helper's stdout,
which is where the frames travel, and because an inherited environment would
hand it the Discord token Yapfast was started with. Only what it needs is passed:
`RAYON_NUM_THREADS=2`, so wasmtime's compile pool doesn't size itself to the
machine, and `YAPFAST_LOG`, so its warnings land in the app's log.

## Threads

| Thread | Count | Lifetime |
| --- | --- | --- |
| Pipe reader | 1 | The helper |
| Workers | 2, or 1 on a machine with two cores or fewer | The helper |
| Watchdog | 1, asleep unless there is a deadline | The helper |
| Extism's timer | 1 | The helper |
| Load pool | up to 4, or the core count | Ends after a load batch |
| Wasmtime's compile pool | 2 | Only after a cold compile |

The number of plugins never changes the number of threads. There is no pool of
instances per CPU: a pool multiplies exactly the memory this design is trying to
keep down, and each instance would carry its own copy of the plugin's globals, so
a plugin's own state would disagree with itself. Instead each plugin has one FIFO
mailbox, a worker takes a plugin with work and runs one message, and a plugin is
on at most one worker at a time, which is what makes the guest's
single-threaded-ness safe. Calls for different plugins interleave on the workers,
but they never overlap inside the runtime: one value behind one lock hands out
the WebAssembly side, so a second worker waits for the first rather than running
a second guest call at once.

## The two lanes

Work goes into one of two lanes. The interactive lane is what a person is
waiting for: a hook on an outgoing message, a command they typed, a menu row
they clicked. The background lane is everything else: events, timers. Interactive
work goes first, and one worker never takes background work at all, so a slow
event handler can't delay a send no matter how many events arrive. Whichever
worker frees up takes the person's work first, and the runtime behind them holds
one lock per plugin rather than one for the whole thing, so a call on one plugin
never waits for a call on another. That is what makes the lanes more than a queue
order: two plugins run at once.

Nothing blocks a worker either. An HTTP request, a storage write, a timer and a
consent prompt are all requests the parent answers later, and a guest that wants
to wait returns instead, getting its answer in a later batch. A worker is
therefore never asleep inside a guest's call.

## Deadlines

A hook is on the path the person is waiting on, so each has a deadline. The
watchdog holds each running call's deadline and the idle time of every instance,
sleeps until the earliest of them, and fires the cancellation when a call's
deadline passes. With nothing running and nothing to evict, it sleeps with no
timer at all.

| Call | Lane | Deadline |
| --- | --- | --- |
| `message_pre_send`, `message_pre_edit` | Interactive | 25 ms per plugin, 100 ms for the chain |
| `typing_pre_send` | Interactive | 10 ms per plugin, 25 ms for the chain |
| `attachment_pre_upload` | Interactive | 50 ms per plugin, 150 ms for the chain |
| `activity_pre_publish` | Background | 25 ms per plugin, 100 ms for the chain |
| A batch of events | Background | 250 ms |
| A command | Interactive | 2 s |
| Loading, `yf_activate`, `yf_deactivate` | Interactive | 2 s |

A hook's chain is run one plugin at a time, and each plugin has its own
deadline as well as the chain having one: the plugin gets the smaller of its own
budget and what is left of the chain's. That is what a hung plugin costs — its
own 25 ms, not the chain's 100 and not an activation's two seconds — and a chain
that has spent its budget stops before the plugins whose turn never came. A
plugin that misses its own deadline is thrown away and left out of the answer,
and the chain goes on with the subject as it was. Events and commands get a
quarter of a second each, and a command was once reported as 250 ms here when
the parent had given it two seconds: a command that is still running after it
has missed its deadline is better reported than left to run. Instantiation and
activation get two seconds because they are slow on a cold cache and a slow
start isn't a plugin misbehaving.

The parent holds a hook's whole-chain deadline too, and starts it when the
person pressed send rather than when the helper had the frame: the helper's
start, the hello and a module's first compile all come out of the same 100 ms,
so a slow start can't hold a message either. A chain whose budget is spent
before the helper sees it is answered unchanged rather than given a fresh one.

A missed deadline cancels the call at whatever point it reached, and the
instance is therefore thrown away rather than reused: it stopped in the middle of
something, so it has no state worth keeping. That is a `deadline` fault, and the
plugin is instantiated fresh on its next trigger. The person sees the message go
out as it was.

## Fail-open hooks

A hook fails open. A plugin that traps, runs late or answers something the ABI
doesn't define leaves the message as it was, and the chain moves on: the next
plugin sees the subject as it was before the failure, not the failed plugin's
half-finished change. That is the default, and it is why a plugin can't stop a
message by accident.

A plugin that must stop one — one that encrypts what leaves, so sending it
anyway would put plain text on the wire — says so in its manifest:

```json
{ "on_failure": "block" }
```

That needs `messages.intercept`, which the consent dialog shows in its bold tone,
one step below the danger tone a send without the person gets: a manifest that
asks for it without the capability is refused outright, so an update can't
quietly turn a filter into something that stops messages. With it, the plugin's
`block` outcome holds the send back and its reason is shown where the person
typed; from a plugin whose manifest doesn't say it, a `block` is the fail-open
answer and the message goes as it was.

## Faults and strikes

An instance is discarded when it traps, goes past its page limit, misses a
deadline, fails to instantiate, fails or is late in `yf_activate`, or answers
something that isn't the ABI's shape. Each of those is a fault with a kind and a
sentence for a person to read, such as `ran out of its 16 MiB`.

Discarding is one rule for all of them, because the alternative is an instance
nobody can vouch for: one that trapped may have stopped anywhere in its own
state, and memory that reached its page limit can't be given back. The faulted
plugin is instantiated fresh next time, so its memory and its state are both
new.

A fault is a strike. Three strikes within a minute suspend the plugin for the
session: it stays off, its row says why, and it starts again only when the person
enables it. Three is enough for a plugin that dies on one odd message to recover,
and few enough that a plugin that dies on every message stops costing anything
within seconds.

A faulted plugin isn't stuck in that state: it is instantiated again on its next
trigger, and comes back unless it faults again inside the window.

## Eviction and the memory budget

An instance is evicted in three cases:

- after five minutes with no calls;
- at once when the window closes to the tray, as the rest of Yapfast gives memory
  back there;
- under the memory budget, least recently used first.

A plugin holding a timer or a request in flight isn't idle and isn't evicted
under it. Eviction is not a fault, so it costs no strike: the plugin goes back to
`enabled` and is instantiated again on its next trigger.

The budget is admission control rather than polling. Before an instance is
activated the helper adds up the page limits of the live instances, and over
256 MiB it evicts the least recently used idle instance or refuses with a notice.
Summing limits is deliberately pessimistic and costs nothing at run time; adding
up what instances actually touch would need a reading every time, for a number
nobody acts on. The Plugins page shows the helper's actual total while it is
open, sampled only then.

The page limit itself is a ceiling, not a reservation. Only the pages a guest
touches cost anything, and WebAssembly can't shrink: an instance that reaches its
limit traps, is discarded, and is replaced by a smaller fresh one, rather than
holding the peak for the rest of the session. The limit is per instance, from the
manifest, and defaults to 256 pages, 16 MiB.

## The compile cache

Compiled modules are cached under `plugins/cache/` in the data directory.
Compiling a 1.87 MiB Rust module cold takes about 22 ms and loading it from the
cache about 2.8 ms, which is the difference between a first trigger a person
notices and one they don't. The cache is safe to delete: the worst it costs is
one compile per module, so a cache directory that can't be created is worth a
line in the log and nothing more.

Modules are loaded from their file rather than from bytes held in memory. Each
cached module costs about 0.56 MiB per plugin that way, against 4.1 MiB when the
bytes stay in the manifest, since every instance clones the manifest.

## When it crashes, or isn't there

A helper that exits on its own is a crash; one that exits because nothing is left
to run is doing its job. Crashes are counted inside a minute, and the wait before
the next start grows with each one: 1 second, then 5, then 30. Three crashes
inside that minute stop the retries, so the third wait is never reached in
practice: the helper stays off, the Plugins page says how many times it stopped,
and the rest of Yapfast carries on without plugins. The growing wait is there so
a helper that dies on every input can't spin the disk and the CPU with respawns.

The helper is looked for beside the running Yapfast, which is where a package
puts it, and keeps it the same version as the window it talks to.
`YAPFAST_PLUGINS` names a file outright, which is how a build of the helper in a
target directory is tried without installing it. If neither is there, the
Plugins page says that the helper isn't installed and that plugins can't run;
everything else in Yapfast works, and installing a plugin is still possible
because `fetch` is the same binary's other subcommand.

The helper is started fresh rather than repaired, so a crashed one leaves no
half-state behind: its replacement is told which plugins to load and reads them
from the store.

## Installing one plugin, and the JSON it prints

`yapfast-plugins fetch <URL>` runs for seconds in a process of its own, so the
git machinery leaves with it. It fetches one commit of the repository, shallowly
and without a checkout, into a temporary bare repository that is deleted before
it exits; it then reads `yapfast-plugin.json` at the folder the URL names, the
module that manifest names, and its icon, straight from the tree objects. Every
one of those is measured from its git object header before its bytes are
decoded, so a file over its cap is refused without ever being held whole.

What it found comes back as **one JSON object on stdout**, tagged by `kind`:

```json
{"kind":"plugin","source":"https://codeberg.org/ada/word-filter","path":"",
 "reference":"main","rev":"051b765b…","manifest":"{…}",
 "wasm":"/tmp/.yapfast-plugin-1-0/plugin.wasm","icon":null}
{"kind":"index","source":"https://codeberg.org/ada/plugins","paths":["plugins/word-filter"]}
{"kind":"error","error":"That repository has no yapfast-plugin.json at its root."}
```

An error is an object like any other and comes with a non-zero exit status. The
module and the icon are written to a temporary directory whose path the object
carries; the parent copies them into the store and deletes the directory, so a
fetch that prints nothing successful leaves nothing behind, and one that does
leaves exactly the files it named. That directory is left in place only once the
object naming it has been written, because a report the parent never read would
leave files nothing would remove.

The caps are a 64 MiB repository, 120 seconds, an 8 MiB module and a 64 KiB
icon, all from [`limits`](../../crates/yapfast-plugin-api/src/limits.rs). A
symbolic link, a submodule, a path that climbs out of the plugin's folder and a
blob that is a Git LFS pointer are refused by name: LFS keeps the bytes on
another server, so the message says to check the file in instead.

## Updating a plugin another one needs

An update, a roll back and a development reload all happen with the helper
running, and none of them leaves a plugin that depends on the one being replaced
running against nothing. `Bridge::refresh` works out the dependents from the
registry that is running now, reads the store again, and sends one `Swap`:

1. every dependent is stopped, outermost first;
2. the plugin's own `yf_deactivate` says what it hands on, and its module is
   forgotten;
3. the new module is loaded and activated with that hand-off, so a plugin's
   state crosses the version change as it crosses an eviction;
4. the dependents are started again, innermost first.

Each step is a request whose answer moves the sequence on, so the helper has
confirmed a dependent is gone before the swap begins and confirmed the swap is
done before a dependent comes back. Until the sequence ends, the parent holds
the work addressed to those plugins — events, hooks, commands — rather than
sending it to instances that are mid-swap, and a `plugin.call` into or out of a
plugin in the sequence waits for it: a dependent that calls its dependency
during an update is answered by it afterwards, never with "not installed".

A plugin whose module is gone from the store, as after an uninstall, is the one
case with nothing to swap in: the plugins that needed it are turned off first,
and the helper stops and starts again from what the store holds.

## Development reload

A plugin can be loaded from a folder on this computer instead of a repository:
`Request::LoadFolder` reads the folder's `yapfast-plugin.json` and the module it
names into the store, turns the plugin on, and remembers the folder. The folder
is what the lock file records as its `source`, so a folder plugin and a
repository plugin can't take each other's place. Its storage and settings live
under `data/<id>/` and survive every reload.

A save in that folder reloads it, through the swap above, so its state crosses
too. There is **no watcher crate and no thread for it**: the session looks at the
folder's manifest and module once a second — two `stat` calls — on the clock it
already runs timers and coalescing windows on, and only while a folder plugin is
loaded. A crate like `notify` would be a thread and an inotify watch per editor,
which is more than one person's development loop needs; a second is also the
debounce, since a manifest caught half-written is read again a moment later
rather than unloading a plugin that runs.

The helper still takes `--dev DIR`, which resolves a relative module path inside
that folder rather than in the directory Yapfast was started in; the swap above
doesn't need it, since the store's own paths are what an activation carries.
