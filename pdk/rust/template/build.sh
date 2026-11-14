#!/bin/sh
# Builds this plugin into `plugin.wasm`, which is what `yapfast-plugin.json`
# names. `.cargo/config.toml` already sets the wasm32 target and the release
# profile, so the copy is the only bookkeeping.
set -eu

cd "$(dirname "$0")"

rustup target add wasm32-unknown-unknown
cargo build --release

# The module is named after the crate, and a template's crate is named after the
# project, so take whatever single .wasm cargo wrote.
set -- target/wasm32-unknown-unknown/release/*.wasm
[ "$#" -eq 1 ] || { echo "no single module in target/wasm32-unknown-unknown/release" >&2; exit 1; }
cp "$1" plugin.wasm

magic=$(od -An -tx1 -N4 plugin.wasm | tr -s ' ')
if [ "$magic" != " 00 61 73 6d" ]; then
    echo "plugin.wasm: not a wasm module" >&2
    exit 1
fi
echo "plugin.wasm: ok"
