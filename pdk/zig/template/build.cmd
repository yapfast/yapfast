@echo off
rem Builds this plugin into plugin.wasm, which is what yapfast-plugin.json
rem names. The Windows twin of build.sh.
rem
rem Needs Zig on PATH. The --export= flags are not optional: a module's export
rem section is written by wasm-ld, so a function is exported only when the build
rem names it, and `export fn` alone produces no exports at all.
setlocal
cd /d "%~dp0"

zig build-exe ^
    -target wasm32-freestanding ^
    -O ReleaseSmall ^
    -fno-entry ^
    --export-memory ^
    -femit-bin=plugin.wasm ^
    --export=yf_activate ^
    --export=yf_deactivate ^
    --export=yf_event ^
    --export=yf_hook ^
    --export=yf_command ^
    --dep yapfast -Mroot=hello.zig ^
    -Myapfast=..\src\yapfast.zig
if errorlevel 1 exit /b 1

rem A module starts with the bytes every WebAssembly file starts with.
powershell -NoProfile -Command ^
  "$b=[IO.File]::ReadAllBytes('plugin.wasm'); if ($b.Length -lt 4 -or $b[0] -ne 0 -or $b[1] -ne 0x61 -or $b[2] -ne 0x73 -or $b[3] -ne 0x6d) { exit 1 }"
if errorlevel 1 (
    echo plugin.wasm: not a wasm module 1>&2
    exit /b 1
)
echo plugin.wasm: ok
