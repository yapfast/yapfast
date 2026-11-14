#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names.
#
# TinyGo, and never the standard Go toolchain: `GOOS=wasip1 GOARCH=wasm` is
# 1,833.1 KiB and 32 pages before a line of plugin code and misses every bar
# Yapfast sets. This template is 10.2 KiB and two pages. The measurements, and
# every language measured beside Go: `docs/measurements/README.md`.
#
# `-target=wasip1` is what the Extism Go PDK is built for and is not optional.
# The module imports four WASI functions -- `clock_time_get`, `random_get`,
# `args_get`, `args_sizes_get` -- which is why `yapfast-plugin.json` says
# `"wasi": true`; without it the helper refuses the module at load rather than
# letting it trap on its first clock read.
#
# `-buildmode=c-shared` is what makes a *reactor*: the module exports
# `_initialize` and no `_start`, so the helper instantiates it, runs
# `_initialize` once, and then calls `yf_activate`, `yf_command` and the rest.
# A command module with a `_start` would run once and be gone.
#
# Fifteen exports, measured: the five the ABI names, `_initialize`, `memory`,
# the cgo allocator's `malloc`/`free`/`calloc`/`realloc`, and four float
# helpers TinyGo's runtime links. The helper looks for the five and ignores
# the rest.
#
# The flags, and what each is worth on this template (measured; the README has
# the numbers beside the other languages'):
#
#   -opt=z          optimize for size
#   -no-debug       drop DWARF: 466.3 KiB without it, 10.2 KiB with it
#   -gc=leaking     no collector: nothing is ever freed
#   -scheduler=none no goroutine scheduler, so no `go func` and nothing that
#                   waits on one; the default build is 5 pages in use after
#                   the calls this template's README measures, and this is 3
#   -panic=trap     a panic traps the instance instead of writing to stderr
#                   and exiting, which is the one answer a reactor can give;
#                   it is also what leaves the module four WASI imports
#                   (`clock_time_get`, `random_get`, `args_get`,
#                   `args_sizes_get`) instead of five, `fd_write` being the
#                   one it drops
#
# What the last three cost: there are no goroutines; every byte the plugin
# allocates stays allocated until the instance is evicted, so a plugin that
# handles many events runs out of its page budget; and a bug is a trap the
# helper discards the instance for. A plugin that would rather have the
# collector back uses `-gc=conservative` instead of `-gc=leaking`: 15.0 KiB,
# two pages at load.
#
# `go mod download` needs the network once, for the Extism Go PDK. Nothing is
# vendored, so a plugin copied out of the repository pins the same version in
# `go.mod` and downloads it again.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

tinygo build \
    -target=wasip1 \
    -buildmode=c-shared \
    -opt=z \
    -no-debug \
    -gc=leaking \
    -scheduler=none \
    -panic=trap \
    -o plugin.wasm \
    .

magic=$(od -An -tx1 -N4 plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
