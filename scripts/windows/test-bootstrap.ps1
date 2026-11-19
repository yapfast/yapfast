#requires -Version 5.1
# Offline checks for extraction, cache integrity, version probes and env handling.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'bootstrap.ps1')
$script:SavedEnvironment = @{}
$script:OfflineBuild = $true
$script:ToolsDirectory = Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) ('.local/bootstrap-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force "$script:ToolsDirectory/downloads", "$script:ToolsDirectory/input with spaces" | Out-Null
$beforePath = $env:PATH
try {
    Set-Content "$script:ToolsDirectory/input with spaces/probe.txt" 'archive extraction works' -Encoding Ascii
    $archive = "$script:ToolsDirectory/downloads/probe.zip"
    Compress-Archive -LiteralPath "$script:ToolsDirectory/input with spaces/probe.txt" -DestinationPath $archive
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash
    $script:ToolManifest = [pscustomobject]@{ packages = [pscustomobject]@{
        probe = [pscustomobject]@{ url = 'https://example.invalid/probe.zip'; sha256 = $hash; strip = 0 }
        missing = [pscustomobject]@{ url = 'https://example.invalid/missing.zip'; sha256 = $hash; strip = 0 }
    } }
    Install-BuildArchive 'probe' "$script:ToolsDirectory/output with spaces" 'probe.txt'
    if ((Get-Content "$script:ToolsDirectory/output with spaces/probe.txt") -ne 'archive extraction works') { throw 'Extraction changed the contents' }
    # A valid completed install can be reused without unpacking again.
    Install-BuildArchive 'probe' "$script:ToolsDirectory/output with spaces" 'probe.txt'
    $missingRefused = $false
    try { Get-BuildArchive 'missing' | Out-Null } catch { $missingRefused = $_.Exception.Message -like 'Offline mode:*' }
    if (!$missingRefused) { throw 'Offline mode did not reject a missing download' }
    Add-Content $archive 'tampered'
    $corruptionRefused = $false
    try { Get-BuildArchive 'probe' | Out-Null } catch { $corruptionRefused = $_.Exception.Message -like 'Cached archive has an invalid SHA-256:*' }
    if (!$corruptionRefused) { throw 'Corrupted cache was accepted' }
    $shell = "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe"
    $version = Read-ToolVersion $shell '-NoProfile -Command "Write-Output bootstrap-probe"'
    if ($version -ne 'bootstrap-probe') { throw 'Version process capture failed' }
    Add-BuildPath "$script:ToolsDirectory/output with spaces"
    if ($script:SavedEnvironment.PATH -ne $beforePath) { throw 'The original PATH was not saved' }
    Write-Host 'Bootstrap checks passed: extraction with spaces, cached install, offline failure, SHA-256 rejection, process capture, environment snapshot.'
} finally {
    foreach ($name in $script:SavedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $script:SavedEnvironment[$name], 'Process')
    }
    # Only the unique test directory, resolved beneath this repository's .local.
    $resolvedTest = [IO.Path]::GetFullPath($script:ToolsDirectory)
    $localRoot = [IO.Path]::GetFullPath((Split-Path $script:ToolsDirectory)) + [IO.Path]::DirectorySeparatorChar
    if (!$resolvedTest.StartsWith($localRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolvedTest -Recurse -Force
}
