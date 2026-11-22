#!/bin/sh
# A C plugin is three PDK files linked with yours, and plain clang builds it.
#
# `--target=wasm32-unknown-unknown` with `-nostdlib` is the module Extism runs:
# no WASI, so nothing to preopen and no clocks to hand out, which is why the
# manifest asks for no `wasi`. `--export-memory` is what lets Extism read what
# the plugin wrote.
set -eu

clang --target=wasm32-unknown-unknown -std=c11 -ffreestanding -nostdlib \
    -O2 -Wall -Wextra -Werror \
    -I../.. \
    -Wl,--no-entry -Wl,--export-memory \
    -o plugin.wasm shout.c ../../yapfast.c ../../json.c ../../freestanding.c

# A module starts with the bytes every WebAssembly file starts with, and the
# helper refuses one that doesn't. This is that check without needing Yapfast.
magic=$(od -An -tx1 -N4 plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
