# Dot-sourced by build-iced.ps1. All downloads are pinned in tools.json.
# No installers are executed, and no persistent environment/registry is changed.

function Set-BuildVariable([string]$Name, [string]$Value) {
    if (!$script:SavedEnvironment.ContainsKey($Name)) {
        $script:SavedEnvironment[$Name] = [Environment]::GetEnvironmentVariable($Name, 'Process')
    }
    [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
}

function Add-BuildPath([string]$Directory) {
    Set-BuildVariable 'PATH' ($Directory + [IO.Path]::PathSeparator + $env:PATH)
}

function Read-ToolVersion([string]$Executable, [string]$Arguments = '--version') {
    $script:LastToolProbeError = $null
    $process = New-Object Diagnostics.Process
    $process.StartInfo.FileName = $Executable
    $process.StartInfo.Arguments = $Arguments
    $process.StartInfo.UseShellExecute = $false
    $process.StartInfo.CreateNoWindow = $true
    $process.StartInfo.RedirectStandardOutput = $true
    $process.StartInfo.RedirectStandardError = $true
    # Do not let rustup auto-install a toolchain through this checkout's override.
    $process.StartInfo.WorkingDirectory = $env:SystemRoot
    try {
        [void]$process.Start()
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit(30000)) { $process.Kill(); $script:LastToolProbeError = 'timed out'; return $null }
        if ($process.ExitCode -eq 0) { return ($stdout.Result + $stderr.Result).Trim() }
        $script:LastToolProbeError = "exit $($process.ExitCode): $($stderr.Result)"
    } catch { $script:LastToolProbeError = $_.Exception.Message; return $null } finally { $process.Dispose() }
}

function Find-BuildTool([string]$Name, [string]$Pattern, [string]$Arguments = '--version') {
    if ($script:LocalTools) { return $null }
    foreach ($command in @(Get-Command $Name -CommandType Application -All -ErrorAction SilentlyContinue)) {
        $version = Read-ToolVersion $command.Source $Arguments
        if ($version -and $version -match $Pattern) { return $command.Source }
    }
    return $null
}

function Get-BuildArchive([string]$Name) {
    $package = $script:ToolManifest.packages.$Name
    $filename = [Uri]::UnescapeDataString(([Uri]$package.url).Segments[-1])
    $download = Join-Path $script:ToolsDirectory "downloads/$filename"
    if (Test-Path -LiteralPath $download) {
        if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ieq $package.sha256) {
            return $download
        }
        throw "Cached archive has an invalid SHA-256: $download. Remove it and retry."
    }
    if ($script:OfflineBuild) { throw "Offline mode: missing $filename. Run once without -Offline." }
    New-Item -ItemType Directory -Force (Split-Path $download) | Out-Null
    Write-Host "Downloading $filename"
    # HttpClient streams large LLVM/Rust archives without loading them in memory.
    Add-Type -AssemblyName System.Net.Http
    $client = New-Object Net.Http.HttpClient
    $client.Timeout = [TimeSpan]::FromMinutes(30)
    $client.DefaultRequestHeaders.UserAgent.ParseAdd('Anomi-Windows-Build/1.0')
    try {
        for ($attempt = 1; $attempt -le 3; $attempt++) {
            try {
                $response = $client.GetAsync($package.url, [Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
                [void]$response.EnsureSuccessStatusCode()
                try {
                    $stream = [IO.File]::Create("$download.part")
                    try { $response.Content.CopyToAsync($stream).GetAwaiter().GetResult() } finally { $stream.Dispose() }
                } finally { $response.Dispose() }
                if ((Get-FileHash -LiteralPath "$download.part" -Algorithm SHA256).Hash -ine $package.sha256) {
                    throw "SHA-256 mismatch for $filename"
                }
                Move-Item -LiteralPath "$download.part" -Destination $download -Force
                return $download
            } catch {
                if ($attempt -eq 3) { throw }
                Write-Host "Download failed; retrying ($attempt/3)."
            }
        }
    } finally { $client.Dispose() }
}

function Install-BuildArchive([string]$Name, [string]$Destination, [string]$RequiredFile) {
    $package = $script:ToolManifest.packages.$Name
    $stamp = Join-Path $Destination ".$Name.sha256"
    if ((Test-Path -LiteralPath (Join-Path $Destination $RequiredFile)) -and
        (Test-Path -LiteralPath $stamp) -and
        ((Get-Content -LiteralPath $stamp -Raw).Trim() -eq $package.sha256)) { return }
    $archive = Get-BuildArchive $Name
    New-Item -ItemType Directory -Force $Destination | Out-Null
    Write-Host "Extracting $Name"
    & tar.exe -xf $archive -C $Destination "--strip-components=$($package.strip)"
    if ($LASTEXITCODE -ne 0) { throw "Could not extract $Name" }
    if (!(Test-Path -LiteralPath (Join-Path $Destination $RequiredFile))) { throw "Missing $RequiredFile after extracting $Name" }
    Set-Content -LiteralPath $stamp -Value $package.sha256 -Encoding Ascii
}

function Select-BuildTool([string]$Name, [string]$RelativeExe, [string]$Pattern, [string]$Arguments = '--version', [string]$RuntimeDirectory = '') {
    $exe = Find-BuildTool ([IO.Path]::GetFileName($RelativeExe)) $Pattern $Arguments
    if ($exe) { Write-Host "Using installed $Name`: $exe"; return $exe }
    $directory = Join-Path $script:ToolsDirectory $Name
    Install-BuildArchive $Name $directory $RelativeExe
    $exe = Join-Path $directory $RelativeExe
    if ($RuntimeDirectory) {
        # xwin's official binary needs this DLL, which a fresh Windows may lack.
        Copy-Item -LiteralPath (Join-Path $RuntimeDirectory 'vcruntime140.dll') -Destination (Split-Path $exe) -Force
    }
    $actual = Read-ToolVersion $exe $Arguments
    if (!$actual) { throw "Could not run $exe`: $script:LastToolProbeError" }
    if ($actual -notmatch $Pattern) { throw "Unexpected $Name version: $actual" }
    return $exe
}

function Test-BuildRust([string]$Compiler, [string]$Pattern) {
    if ((Read-ToolVersion $Compiler) -notmatch $Pattern) { return $false }
    $bin = Split-Path $Compiler
    if (!(Test-Path -LiteralPath "$bin/cargo.exe")) { return $false }
    $sysroot = Read-ToolVersion $Compiler '--print sysroot'
    if (!$sysroot -or !(Test-Path "$sysroot/lib/rustlib/x86_64-pc-windows-msvc/lib/libstd-*.rlib")) { return $false }
    return [bool](Read-ToolVersion "$bin/cargo.exe")
}

function Select-BuildRust {
    $version = $script:ToolManifest.rustVersion
    $pattern = '^rustc ' + [regex]::Escape($version) + ' '
    if (!$script:LocalTools) {
        $rustup = Get-Command rustup.exe -CommandType Application -ErrorAction SilentlyContinue
        if ($rustup) {
            $rustc = & $rustup.Source which --toolchain $version rustc 2>$null
            if ($LASTEXITCODE -eq 0 -and (Test-BuildRust $rustc $pattern)) {
                return Split-Path $rustc
            }
        }
        # Skip rustup's proxies: probing them can install the repository override globally.
        foreach ($rustc in @(Get-Command rustc.exe -CommandType Application -All -ErrorAction SilentlyContinue)) {
            $bin = Split-Path $rustc.Source
            if (Test-Path -LiteralPath (Join-Path $bin 'rustup.exe')) { continue }
            if (Test-BuildRust $rustc.Source $pattern) { return $bin }
        }
    }
    $root = Join-Path $script:ToolsDirectory "rust-$version"
    Install-BuildArchive 'rustc' $root 'bin/rustc.exe'
    Install-BuildArchive 'cargo' $root 'bin/cargo.exe'
    Install-BuildArchive 'rust-std' $root 'lib/rustlib/x86_64-pc-windows-msvc/lib'
    if (!(Test-BuildRust "$root/bin/rustc.exe" $pattern)) { throw 'Incomplete or incompatible Rust toolchain' }
    return Join-Path $root 'bin'
}

function Select-BuildLlvm {
    $compiler = Find-BuildTool 'clang-cl.exe' '^clang version 21\.1\.7\b'
    if ($compiler) {
        $bin = Split-Path $compiler
        $missing = @('libclang.dll','llvm-lib.exe','lld-link.exe','llvm-rc.exe','../lib/clang/21/include/stddef.h') | Where-Object { !(Test-Path (Join-Path $bin $_)) }
        if (!$missing) { return Split-Path $bin }
    }
    $root = Join-Path $script:ToolsDirectory 'llvm-21.1.7'
    $stamp = Join-Path $root '.llvm.ready'
    $expected = $script:ToolManifest.packages.llvm.sha256 + $script:ToolManifest.packages.'llvm-installer'.sha256
    $ready = (Test-Path -LiteralPath $stamp) -and ((Get-Content -LiteralPath $stamp -Raw).Trim() -eq $expected)
    if (!$ready) {
        # The smaller installer is only an archive here; it is never executed.
        $sevenZip = Get-Command 7z.exe -CommandType Application -ErrorAction SilentlyContinue
        $sevenZipPath = if ($sevenZip) { $sevenZip.Source } else { Join-Path $env:ProgramFiles '7-Zip/7z.exe' }
        $sevenZipVersion = if (Test-Path -LiteralPath $sevenZipPath) { Read-ToolVersion $sevenZipPath 'i' } else { $null }
        if ($sevenZipVersion -match '7-Zip (?:2[3-9]|[3-9][0-9])\.') {
            $archive = Get-BuildArchive 'llvm-installer'
            New-Item -ItemType Directory -Force $root | Out-Null
            & $sevenZipPath x $archive "-o$root" -y -bso0 -bsp0
            if ($LASTEXITCODE -ne 0) { throw 'Could not extract LLVM' }
        } else { Install-BuildArchive 'llvm' $root 'bin/clang-cl.exe' }
    }
    foreach ($file in @('clang-cl.exe','libclang.dll','llvm-lib.exe','lld-link.exe','llvm-rc.exe')) {
        if (!(Test-Path "$root/bin/$file")) { throw "Incomplete LLVM installation: $file" }
    }
    if (!(Test-Path "$root/lib/clang/21/include/stddef.h")) { throw 'LLVM builtin headers are missing' }
    if ((Read-ToolVersion "$root/bin/clang-cl.exe") -notmatch '^clang version 21\.1\.7\b') { throw 'Unexpected LLVM version' }
    Set-Content -LiteralPath $stamp -Value $expected -Encoding Ascii
    return $root
}

function Find-InstalledWindowsSdk {
    if ($script:LocalTools) { return $null }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (!(Test-Path $vswhere)) { return $null }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
    $registered = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10 -ErrorAction SilentlyContinue
    if ($registered) { $kits = $registered.KitsRoot10 }
    $sdk = Get-ChildItem "$kits/Include" -Directory -Filter '10.0.26100.*' -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    if (!$sdk) { return $null }
    $installations = & $vswhere -all -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    foreach ($installation in $installations) {
        $crt = Get-ChildItem "$installation/VC/Tools/MSVC" -Directory -Filter '14.44.*' -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
        if ($crt -and (Test-Path "$($crt.FullName)/bin/Hostx64/x64/ml64.exe") -and
            (Test-Path "$($crt.FullName)/include/vcruntime.h") -and
            (Test-Path "$($crt.FullName)/lib/x64/libcmt.lib") -and
            (Test-Path "$kits/Lib/$($sdk.Name)/um/x64/kernel32.lib")) {
            return @{
                Includes = @("$($crt.FullName)/include") + @('ucrt','um','shared','winrt','cppwinrt' | ForEach-Object { "$($sdk.FullName)/$_" })
                Libraries = @("$($crt.FullName)/lib/x64", "$kits/Lib/$($sdk.Name)/um/x64", "$kits/Lib/$($sdk.Name)/ucrt/x64")
                Bin = "$($crt.FullName)/bin/Hostx64/x64"
                Vc = "$installation/VC"
            }
        }
    }
}

function Select-BuildWindowsSdk {
    $installed = Find-InstalledWindowsSdk
    if ($installed) { Write-Host 'Using installed MSVC 14.44 and Windows SDK 10.0.26100'; return $installed }
    $msvc = Join-Path $script:ToolsDirectory 'msvc'
    Install-BuildArchive 'msvc' $msvc 'Contents/VC/Tools/MSVC'
    $assembler = Get-ChildItem $msvc -Recurse -Filter ml64.exe | Where-Object { $_.FullName -match 'Hostx64[\\/]x64[\\/]' } | Select-Object -First 1
    if (!$assembler) { throw 'MSVC archive has no x64 assembler' }
    $sdk = Join-Path $script:ToolsDirectory 'sdk/native'
    # xwin replaces its output directories; keep that operation inside our cache.
    $cacheBoundary = [IO.Path]::GetFullPath($script:ToolsDirectory).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $sdk = [IO.Path]::GetFullPath($sdk)
    if (!$sdk.StartsWith($cacheBoundary, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe SDK output path' }
    $crt = Join-Path $sdk 'VC/Tools/MSVC/14.44.17.14'
    $kit = Join-Path $sdk 'Windows Kits/10'
    $stamp = Join-Path $sdk '.anomi-sdk.ready'
    $expected = '14.44.17.14|10.0.26100|xwin-0.10.0|winsysroot'
    $ready = (Test-Path -LiteralPath $stamp) -and ((Get-Content -LiteralPath $stamp -Raw).Trim() -eq $expected)
    if (!$ready -or !(Test-Path "$crt/include/vcruntime.h") -or !(Test-Path "$kit/Lib/10.0.26100/um/x64/kernel32.lib")) {
        if ($script:OfflineBuild) { throw 'Offline mode: Windows SDK is missing. Run once without -Offline.' }
        $xwin = Select-BuildTool 'xwin' 'xwin.exe' '^xwin 0\.10\.0\b' '--version' $assembler.DirectoryName
        # Native Windows layout avoids xwin's version aliases, which otherwise
        # require symlink privileges even with case aliases disabled.
        Write-Host 'Preparing MSVC 14.44 (17.14) and Windows SDK 10.0.26100 (xwin verifies Microsoft payload hashes).'
        & $xwin --accept-license --arch x86_64 --variant desktop --manifest-version 17 --crt-version 14.44.17.14 --sdk-version 10.0.26100 --cache-dir (Join-Path $script:ToolsDirectory 'sdk/xwin') splat --output $sdk --disable-symlinks --preserve-ms-arch-notation --use-winsysroot-style | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the Windows SDK' }
        Set-Content -LiteralPath $stamp -Value $expected -Encoding Ascii
    }
    return @{
        Includes = @("$crt/include") + @('ucrt','um','shared','winrt','cppwinrt' | ForEach-Object { "$kit/Include/10.0.26100/$_" })
        Libraries = @("$crt/lib/x64", "$kit/Lib/10.0.26100/um/x64", "$kit/Lib/10.0.26100/ucrt/x64")
        Bin = $assembler.DirectoryName
        Vc = "$sdk/VC"
    }
}

function Initialize-WindowsBuildTools([string]$Root, [switch]$Offline, [switch]$ForceLocal) {
    $script:ToolsDirectory = $Root
    $script:OfflineBuild = [bool]$Offline
    $script:LocalTools = [bool]$ForceLocal
    $script:ToolManifest = Get-Content (Join-Path $PSScriptRoot 'tools.json') -Raw | ConvertFrom-Json
    $channel = (Get-Content (Join-Path $script:RepositoryRoot 'rust-toolchain.toml') -Raw)
    if ($channel -notmatch ('channel\s*=\s*"' + [regex]::Escape($script:ToolManifest.rustVersion) + '"')) {
        throw 'Update scripts/windows/tools.json for the pinned Rust toolchain before building.'
    }
    if (!(Get-Command tar.exe -ErrorAction SilentlyContinue)) { throw 'tar.exe is required (included in Windows 10/11).' }
    $rustBin = Select-BuildRust
    Add-BuildPath $rustBin
    $llvm = Select-BuildLlvm
    Add-BuildPath "$llvm/bin"
    $git = Select-BuildTool 'git' 'cmd/git.exe' '^git version 2\.(?:[4-9][0-9]|[1-9][0-9]{2,})\.'
    Add-BuildPath (Split-Path $git)
    foreach ($tool in @(
        @('cmake','bin/cmake.exe','^cmake version 4\.5\.0\b','--version'),
        @('ninja','ninja.exe','^1\.13\.2\b','--version'),
        @('nasm','nasm.exe','^NASM version 2\.16\.03\b','-v'),
        @('protoc','bin/protoc.exe','^libprotoc 36\.2\b','--version')
    )) {
        $exe = Select-BuildTool $tool[0] $tool[1] $tool[2] $tool[3]
        Add-BuildPath (Split-Path $exe)
        if ($tool[0] -eq 'nasm') { Set-BuildVariable 'NASM' $exe }
        if ($tool[0] -eq 'protoc') { Set-BuildVariable 'PROTOC' $exe }
    }
    $sdk = Select-BuildWindowsSdk
    foreach ($directory in @($sdk.Includes) + @($sdk.Libraries)) {
        if (!(Test-Path -LiteralPath $directory)) { throw "Incomplete Windows SDK: $directory" }
    }
    Add-BuildPath $sdk.Bin
    Set-BuildVariable 'INCLUDE' ($sdk.Includes -join ';')
    Set-BuildVariable 'LIB' ($sdk.Libraries -join ';')
    Set-BuildVariable 'VCINSTALLDIR' $sdk.Vc
    Set-BuildVariable 'VSCMD_ARG_TGT_ARCH' 'x64'
    Set-BuildVariable 'CC' "$llvm/bin/clang-cl.exe"
    Set-BuildVariable 'CXX' "$llvm/bin/clang-cl.exe"
    Set-BuildVariable 'AR' "$llvm/bin/llvm-lib.exe"
    Set-BuildVariable 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER' "$llvm/bin/lld-link.exe"
    Set-BuildVariable 'LIBCLANG_PATH' "$llvm/bin"
    Set-BuildVariable 'LLVM_HOME' $llvm
    $clangHeaders = Join-Path $llvm 'lib/clang/21'
    $isystem = $sdk.Includes | ForEach-Object { '-isystem "' + ($_ -replace '\\','/') + '"' }
    Set-BuildVariable 'BINDGEN_EXTRA_CLANG_ARGS' ('--target=x86_64-pc-windows-msvc -resource-dir="' + ($clangHeaders -replace '\\','/') + '" ' + ($isystem -join ' '))
    $webrtc = Join-Path $Root 'webrtc'
    Install-BuildArchive 'webrtc' $webrtc 'win-x64-release/lib/webrtc.lib'
    Set-BuildVariable 'LK_CUSTOM_WEBRTC' "$webrtc/win-x64-release"
    return Join-Path $rustBin 'cargo.exe'
}
