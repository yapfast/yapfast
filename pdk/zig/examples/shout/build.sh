#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names.
#
# `wasm32-freestanding` is the module Extism runs: no WASI, so nothing to preopen
# and no clocks to hand out, which is why the manifest asks for no `wasi`.
#
# The `--export=` flags are not optional: a WebAssembly module's export section
# is written by wasm-ld, so a function is exported only when the build names it.
# `export fn` and `@export` alone leave the module with no exports at all.
#
# The PDK is a module of its own, so a plugin imports it by name and can live
# anywhere; `--dep` before `-Mroot` is what makes root depend on it.
set -eu

zig build-exe \
    -target wasm32-freestanding \
    -O ReleaseSmall \
    -fno-entry \
    --export-memory \
    -femit-bin=plugin.wasm \
    --export=yf_activate \
    --export=yf_deactivate \
    --export=yf_event \
    --export=yf_hook \
    --export=yf_command \
    --dep yapfast -Mroot=shout.zig \
    -Myapfast=../../src/yapfast.zig

magic=$(od -An -tx1 -N4 plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
