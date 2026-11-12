@echo off
rem Builds this plugin into plugin.wasm, which is what yapfast-plugin.json
rem names. The Windows twin of build.sh: nothing but clang is needed, because a
rem C plugin asks for no WASI and no SDK.
setlocal
cd /d "%~dp0"

rem Where the PDK is, from here: the template sits in pdk\c\template, and a
rem plugin copied out of the repository points this at wherever it keeps
rem yapfast.h. plugin.c includes ..\yapfast.h, so this folder must stay inside
rem pdk\c unless YAPFAST_PDK points at a checkout of it.
if not defined YAPFAST_PDK set YAPFAST_PDK=..
set "pdk=%YAPFAST_PDK%"

clang --target=wasm32-unknown-unknown -std=c11 -ffreestanding -nostdlib ^
    -O2 -Wall -Wextra -Werror ^
    -I"%pdk%" ^
    -Wl,--no-entry -Wl,--export-memory ^
    -o plugin.wasm plugin.c "%pdk%\yapfast.c" "%pdk%\json.c" "%pdk%\freestanding.c"
if errorlevel 1 exit /b 1

rem A module starts with the bytes every WebAssembly file starts with, and the
rem helper refuses one that doesn't. This is that check without needing Yapfast.
powershell -NoProfile -Command ^
  "$b=[IO.File]::ReadAllBytes('plugin.wasm'); if ($b.Length -lt 4 -or $b[0] -ne 0 -or $b[1] -ne 0x61 -or $b[2] -ne 0x73 -or $b[3] -ne 0x6d) { exit 1 }"
if errorlevel 1 (
    echo plugin.wasm: not a wasm module 1>&2
    exit /b 1
)
echo plugin.wasm: ok
