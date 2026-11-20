#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names. Nothing but clang is needed: a C plugin asks for no WASI and no SDK.
set -eu

# Where the PDK is, from here: the template sits in `pdk/c/template`, and a plugin
# copied out of the repository points this at wherever it keeps `yapfast.h`.
pdk=${YAPFAST_PDK:-..}
# `plugin.c` includes `../yapfast.h`, so this folder must stay inside `pdk/c`
# unless YAPFAST_PDK points at a checkout of it.

clang --target=wasm32-unknown-unknown -std=c11 -ffreestanding -nostdlib \
    -O2 -Wall -Wextra -Werror \
    -I"$pdk" \
    -Wl,--no-entry -Wl,--export-memory \
    -o plugin.wasm plugin.c "$pdk/yapfast.c" "$pdk/json.c" "$pdk/freestanding.c"

magic=$(od -An -tx1 -N4 plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
