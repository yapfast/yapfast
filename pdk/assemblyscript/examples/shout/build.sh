#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names.
#
# `asc` builds for WebAssembly by default and needs nothing else: no WASI, so
# nothing to preopen and no clocks to hand out, which is why the manifest asks
# for no `wasi`. `--baseDir` is the PDK's folder, so both this plugin and the
# PDK are inside the project `asc` compiles.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
pdk=$(cd "$here/../.." && pwd)

asc="$pdk/node_modules/assemblyscript/bin/asc.js"
if [ ! -f "$asc" ]; then
    echo "no asc: run \`bun install\` in $pdk first" >&2
    exit 1
fi

cd "$pdk"
bun "$asc" examples/shout/shout.ts \
    --baseDir . \
    -o examples/shout/plugin.wasm \
    -O3 \
    --noAssert \
    --runtime stub \
    --use abort=assembly/index/abort

magic=$(od -An -tx1 -N4 examples/shout/plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
