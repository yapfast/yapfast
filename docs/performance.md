# Performance

RatFast inherits Yapfast's engine, so the numbers below are the engine's, not a
re-measurement of this fork. They were taken on the upstream project's machine
on 25 October 2026; nothing in this fork has been measured on this machine yet.
Treat them as the baseline this branch is held to, not as a claim.

| Scenario | Discord (Vesktop) | Yapfast | Target for RatFast |
| --- | --- | --- | --- |
| Idle memory | 724 MB | 30 MB | 30–60 MB |
| Image-heavy channel | 706 MB | 41 MB | under 60 MB |
| In a call | 888 MB | 44 MB | under 60 MB |
| Idle CPU | 0.11% | 0.015% | near 0% |
| Cold start | 8 s | 0.22 s | under 1 s |
| Scroll | — | smooth | 60–144 Hz |

The methodology, the machine and what "memory" counts are in the upstream
[measurements](measurements/README.md). The numbers were produced with the
`measure` profile, which optimises like a release build and keeps symbols;
`fast` leaves the Slint interface unoptimised and is only for local work.

## What this branch changed, and why it should be faster

All three are on hot paths that ran per event or per keystroke:

1. **The custom emoji catalogue is cached.** `Application::emoji_sets` used to
   rebuild every server's emoji — a `format!`, a URL and a clone each — on every
   composer keystroke and every send. It now keeps the last catalogue, keyed by
   the snapshot it came from, and rebuilds only when that snapshot or the
   selected server changes.
2. **The animation pass is skipped when nothing animated changed.**
   `show_emoji_animations` scanned every loaded message, collected the animated
   emoji and rebuilt the usable catalogue on every refresh. It now records what
   it last rendered (the timelines by identity, the picture revision, the
   reaction cells, the open card) and returns early when none of that moved.
3. **The account cache is only written when it changed.** `cache::keep` woke on
   every snapshot change, slept 20 s and then serialised, encrypted and wrote
   the whole account even when nothing differed — presence churn alone was
   enough. It now compares the saved snapshot's parts by identity and writes
   only a real change, and it keeps the marker it compares against only after a
   successful write.

## Not measured yet

Local CPU, memory and startup for this fork, and the cost of the plugin layer.
The harness exists (`scripts/measure-process`, `testing/measure_*.py`) but its
display and audio pieces are Linux-only; on Windows the plan is Task Manager
plus `scripts/measure-process` for the process table, which has not been run.
When it is, this page gets the numbers and the date.
