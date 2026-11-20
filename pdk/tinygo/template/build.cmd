@echo off
rem Builds this plugin into plugin.wasm, which is what yapfast-plugin.json
rem names. The Windows twin of build.sh.
rem
rem Needs TinyGo on PATH. TinyGo and never the standard Go toolchain: the
rem standard one is 1,833.1 KiB and 32 pages before a line of plugin code, and
rem this template is 10.2 KiB and two pages. docs\measurements\README.md has
rem both measurements.
rem
rem -target=wasip1 is what the Extism Go PDK is built for and is not optional:
rem the module imports four WASI functions, which is why the manifest says
rem "wasi": true. -buildmode=c-shared is what makes a reactor -- the module
rem exports _initialize and no _start -- so the helper can instantiate it and
rem call yf_activate, yf_command and the rest.
rem
rem The flags and what each is worth here (measured):
rem   -opt=z           optimize for size
rem   -no-debug        drop DWARF: 466.3 KiB without it, 10.2 KiB with it
rem   -gc=leaking      no collector: nothing is ever freed
rem   -scheduler=none  no goroutines: the default build is 5 pages in use
rem                    after these calls and this is 3
rem   -panic=trap      a panic traps the instance instead of writing to stderr
rem                    and exiting; it also leaves four WASI imports instead
rem                    of five, `fd_write` being the one it drops
rem -gc=conservative in place of -gc=leaking gives the collector back for
rem 15.0 KiB. go mod download needs the network once, for the Extism Go PDK.
setlocal
cd /d "%~dp0"

tinygo build ^
    -target=wasip1 ^
    -buildmode=c-shared ^
    -opt=z ^
    -no-debug ^
    -gc=leaking ^
    -scheduler=none ^
    -panic=trap ^
    -o plugin.wasm ^
    .
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
