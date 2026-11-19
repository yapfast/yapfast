#requires -Version 5.1

<#
.SYNOPSIS
Build Anomi's iced interface for Windows x64 without a system-wide tool installation.
.DESCRIPTION
Uses matching tools from PATH, rustup or Visual Studio when available. Downloads
missing pinned tools into .local/windows-tools, verifies SHA-256, and extracts
them without executing installers. Rust/Cargo caches stay under .local. Windows
10/11's PowerShell and tar.exe are the only bootstrap requirements. First use
needs internet access and several GiB of disk space. System PATH is not changed.
.EXAMPLE
.\scripts\build-iced.ps1
.EXAMPLE
.\scripts\build-iced.ps1 -Offline
.EXAMPLE
.\scripts\build-iced.ps1 -PrepareOnly -LocalTools
#>
[CmdletBinding()]
param(
    [Alias('Profile')][ValidateSet('release', 'fast', 'dev')][string]$BuildProfile = 'release',
    [ValidateRange(1, 128)][int]$Jobs = 4,
    [switch]$Offline,
    [switch]$PrepareOnly,
    [switch]$LocalTools
)

$ErrorActionPreference = 'Stop'
$script:RepositoryRoot = Split-Path $PSScriptRoot -Parent
$script:SavedEnvironment = @{}
$originalDirectory = Get-Location
$originalTls = [Net.ServicePointManager]::SecurityProtocol

try {
    if ($env:OS -ne 'Windows_NT' -or ![Environment]::Is64BitOperatingSystem) {
        throw 'This script builds on 64-bit Windows 10/11.'
    }
    [Net.ServicePointManager]::SecurityProtocol = $originalTls -bor [Net.SecurityProtocolType]::Tls12
    Set-Location $script:RepositoryRoot
    . (Join-Path $PSScriptRoot 'windows/bootstrap.ps1')
    $local = Join-Path $script:RepositoryRoot '.local'
    $cargoCache = Join-Path $local 'cargo'
    $temporary = Join-Path $local 'tmp'
    New-Item -ItemType Directory -Force $cargoCache, $temporary | Out-Null
    Set-BuildVariable 'CARGO_HOME' $cargoCache
    Set-BuildVariable 'CARGO_TARGET_DIR' (Join-Path $script:RepositoryRoot 'target')
    Set-BuildVariable 'TEMP' $temporary
    Set-BuildVariable 'TMP' $temporary
    $cargo = Initialize-WindowsBuildTools (Join-Path $local 'windows-tools') -Offline:$Offline -ForceLocal:$LocalTools
    Set-BuildVariable 'CARGO_BUILD_JOBS' ([string]$Jobs)
    Set-BuildVariable 'CARGO_NET_GIT_FETCH_WITH_CLI' 'true'
    Set-BuildVariable 'GIT_TERMINAL_PROMPT' '0'
    Set-BuildVariable 'CARGO_TERM_COLOR' 'never'
    Set-BuildVariable 'CMAKE_GENERATOR' 'Ninja'
    Set-BuildVariable 'CMAKE_POLICY_VERSION_MINIMUM' '3.5'
    # Preserve existing process-local Git settings while adding long-path support.
    $gitCount = 0
    if ($env:GIT_CONFIG_COUNT) { $gitCount = [int]$env:GIT_CONFIG_COUNT }
    Set-BuildVariable "GIT_CONFIG_KEY_$gitCount" 'core.longpaths'
    Set-BuildVariable "GIT_CONFIG_VALUE_$gitCount" 'true'
    Set-BuildVariable 'GIT_CONFIG_COUNT' ([string]($gitCount + 1))
    $maps = @("$env:USERPROFILE=/home", "$cargoCache=/cargo", "$script:RepositoryRoot=/yapfast")
    $rustFlags = @('-Ctarget-feature=+crt-static') + @($maps | ForEach-Object { "--remap-path-prefix=$_" })
    Set-BuildVariable 'CARGO_ENCODED_RUSTFLAGS' ($rustFlags -join [char]31)
    $cflags = $maps | ForEach-Object { '/clang:-ffile-prefix-map="' + $_ + '"' }
    Set-BuildVariable 'CFLAGS' ($cflags -join ' ')
    Set-BuildVariable 'CXXFLAGS' ($cflags -join ' ')
    Set-BuildVariable 'AWS_LC_SYS_CFLAGS' ($cflags -join ' ')
    if ($PrepareOnly) { Write-Host 'Windows build tools are ready.'; return }

    $folder = if ($BuildProfile -eq 'dev') { 'debug' } else { $BuildProfile }
    $exe = Join-Path $env:CARGO_TARGET_DIR "x86_64-pc-windows-msvc/$folder/anomi.exe"
    if (Test-Path -LiteralPath $exe) {
        try {
            $probe = [IO.File]::Open($exe, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            $probe.Dispose()
        } catch { throw 'anomi.exe is in use. Choose Exit in Anomi''s tray menu, then run this script again.' }
    }
    $cargoArguments = @('build', '--locked', '--profile', $BuildProfile, '--target', 'x86_64-pc-windows-msvc', '--package', 'anomi-app', '--bin', 'anomi')
    if ($Offline) { $cargoArguments += '--offline' }
    Write-Host "Building iced ($BuildProfile, Windows x64)..."
    & $cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed (exit $LASTEXITCODE)." }

    # Verify the emitted EXE is a GUI application, so Explorer opens no console.
    $reader = New-Object IO.BinaryReader([IO.File]::OpenRead($exe))
    try {
        [void]$reader.BaseStream.Seek(0x3c, [IO.SeekOrigin]::Begin)
        $pe = $reader.ReadUInt32()
        [void]$reader.BaseStream.Seek($pe + 24 + 68, [IO.SeekOrigin]::Begin)
        if ($reader.ReadUInt16() -ne 2) { throw 'Expected a Windows GUI executable (PE subsystem 2).' }
    } finally { $reader.Dispose() }
    $version = Read-ToolVersion $exe
    if (!$version) { throw 'Built EXE did not pass --version.' }
    Write-Host $version
    Write-Host "Built: $exe"
} finally {
    foreach ($name in $script:SavedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $script:SavedEnvironment[$name], 'Process')
    }
    [Net.ServicePointManager]::SecurityProtocol = $originalTls
    Set-Location $originalDirectory
}
