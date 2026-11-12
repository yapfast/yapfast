#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names.
#
# `asc` builds for WebAssembly by default and needs nothing else: no WASI, so
# nothing to preopen and no clocks to hand out, which is why the manifest asks
# for no `wasi`. This template imports the PDK that sits beside it in
# `pdk/assemblyscript/assembly`, so it builds from there with `--baseDir` and
# the whole project is inside it.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
pdk=$(cd "$here/.." && pwd)

if [ ! -x "$pdk/node_modules/.bin/asc" ]; then
    echo "no asc: run \`npm install\` in $pdk first" >&2
    exit 1
fi

cd "$pdk"
"$pdk/node_modules/.bin/asc" template/index.ts \
    --baseDir . \
    -o template/plugin.wasm \
    -O3 \
    --noAssert \
    --runtime stub \
    --use abort=assembly/index/abort

magic=$(od -An -tx1 -N4 template/plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
