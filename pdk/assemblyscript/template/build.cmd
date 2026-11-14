@echo off
rem Builds this plugin into plugin.wasm, which is what yapfast-plugin.json
rem names. The Windows twin of build.sh.
rem
rem asc is run through bun rather than through node_modules\.bin\asc, because
rem that shim is a shell script on Unix and a .cmd on Windows and neither is
rem the other. Nothing needs installing beyond `bun install` in the PDK folder.
setlocal
cd /d "%~dp0.."

if not exist "node_modules\assemblyscript\bin\asc.js" (
    echo no asc: run `bun install` in %CD% first 1>&2
    exit /b 1
)

bun "node_modules\assemblyscript\bin\asc.js" template\index.ts ^
    --baseDir . ^
    -o template\plugin.wasm ^
    -O3 ^
    --noAssert ^
    --runtime stub ^
    --use abort=assembly/index/abort
if errorlevel 1 exit /b 1

rem A module starts with the bytes every WebAssembly file starts with.
powershell -NoProfile -Command ^
  "$b=[IO.File]::ReadAllBytes('template\plugin.wasm'); if ($b.Length -lt 4 -or $b[0] -ne 0 -or $b[1] -ne 0x61 -or $b[2] -ne 0x73 -or $b[3] -ne 0x6d) { exit 1 }"
if errorlevel 1 (
    echo plugin.wasm: not a wasm module 1>&2
    exit /b 1
)
echo plugin.wasm: ok
