@echo off
rem Builds this plugin into plugin.wasm, which is what yapfast-plugin.json
rem names. The Windows twin of the three commands in the README.
rem
rem .cargo\config.toml already sets the wasm32 target and the release profile,
rem so the copy is the only bookkeeping: the manifest names plugin.wasm beside
rem itself, and cargo writes it under target\.
setlocal
cd /d "%~dp0"

rustup target add wasm32-unknown-unknown
if errorlevel 1 exit /b 1

cargo build --release
if errorlevel 1 exit /b 1

rem The module is named after the crate, and a template's crate is named after
rem the project, so take whatever single .wasm cargo wrote.
for %%f in ("target\wasm32-unknown-unknown\release\*.wasm") do copy /y "%%f" plugin.wasm >nul
if not exist plugin.wasm (
    echo no module in target\wasm32-unknown-unknown\release 1>&2
    exit /b 1
)

rem A module starts with the bytes every WebAssembly file starts with.
powershell -NoProfile -Command ^
  "$b=[IO.File]::ReadAllBytes('plugin.wasm'); if ($b.Length -lt 4 -or $b[0] -ne 0 -or $b[1] -ne 0x61 -or $b[2] -ne 0x73 -or $b[3] -ne 0x6d) { exit 1 }"
if errorlevel 1 (
    echo plugin.wasm: not a wasm module 1>&2
    exit /b 1
)
echo plugin.wasm: ok
