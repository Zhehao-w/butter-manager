param([string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskOutput = if ($OutputDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $taskRoot 'artifacts/windows' }
$taskVersion = (Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskZip = Join-Path $taskOutput "butter-manager-v$taskVersion-windows-x64-portable.zip"
$taskExe = Join-Path $taskOutput 'butter-manager.exe'
if ((Get-Item -LiteralPath $taskExe).VersionInfo.ProductVersion -cne $taskVersion) { throw 'EXE version mismatch.' }
$taskReader = [IO.BinaryReader]::new([IO.File]::OpenRead($taskExe))
try {
    if ($taskReader.ReadUInt16() -ne 0x5a4d) { throw 'Not a Windows executable.' }
    $taskReader.BaseStream.Position = 0x3c
    $taskPeOffset = $taskReader.ReadInt32()
    if ($taskPeOffset -lt 64 -or $taskPeOffset -gt $taskReader.BaseStream.Length - 26) { throw 'Invalid PE header.' }
    $taskReader.BaseStream.Position = $taskPeOffset
    if ($taskReader.ReadUInt32() -ne 0x4550 -or $taskReader.ReadUInt16() -ne 0x8664) { throw 'Expected a Windows x64 executable.' }
} finally { $taskReader.Dispose() }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$taskArchive = [IO.Compression.ZipFile]::OpenRead($taskZip)
try {
    if ($taskArchive.Entries.Count -ne 1 -or $taskArchive.Entries[0].FullName -cne 'butter-manager/butter-manager.exe') { throw 'ZIP must contain exactly the executable, with no user data.' }
    $taskStream = $taskArchive.Entries[0].Open()
    try { $taskHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($taskStream)).ToLowerInvariant() } finally { $taskStream.Dispose() }
    if ($taskHash -ne (Get-FileHash -LiteralPath (Join-Path $taskOutput 'butter-manager.exe') -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'ZIP executable differs from the standalone EXE.' }
} finally { $taskArchive.Dispose() }
$taskChecksumLines = @(Get-Content -LiteralPath ($taskZip + '.sha256'))
if ($taskChecksumLines.Count -ne 1) { throw 'Expected exactly one Portable ZIP checksum.' }
if ($taskChecksumLines[0] -cnotmatch '^([a-f0-9]{64})  (.+)$') { throw 'Invalid checksum entry.' }
$taskExpectedHash = $Matches[1]
if ($Matches[2] -cne (Split-Path -Leaf $taskZip)) { throw 'Checksum must reference only the Portable ZIP.' }
if ((Get-FileHash -LiteralPath $taskZip -Algorithm SHA256).Hash.ToLowerInvariant() -cne $taskExpectedHash) { throw 'Checksum mismatch.' }
# Failed validation/build attempts must clear previous success assets in their own output.
$taskFixture = Join-Path $taskRoot ('artifacts/portable-release-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $taskFixture | Out-Null
try {
    $taskFakeCargo = Join-Path $taskFixture 'fail-cargo.cmd'
    Set-Content -LiteralPath $taskFakeCargo -Value '@exit /b 42' -Encoding ascii
    foreach ($taskCase in @('tag', 'build')) {
        Copy-Item -LiteralPath (Join-Path $taskOutput 'butter-manager.exe') -Destination $taskFixture
        Copy-Item -LiteralPath $taskZip -Destination $taskFixture
        Copy-Item -LiteralPath ($taskZip + '.sha256') -Destination $taskFixture
        $taskFailed = $false
        try {
            if ($taskCase -eq 'tag') { & (Join-Path $PSScriptRoot 'Build-PortableRelease.ps1') -SkipFrontendBuild -ExpectedTag 'v999.999.999' -OutputDirectory $taskFixture }
            else { & (Join-Path $PSScriptRoot 'Build-PortableRelease.ps1') -SkipFrontendBuild -CargoExecutable $taskFakeCargo -OutputDirectory $taskFixture }
        } catch { $taskFailed = $true }
        if (-not $taskFailed -or (Test-Path -LiteralPath (Join-Path $taskFixture 'butter-manager.exe')) -or @(Get-ChildItem -LiteralPath $taskFixture -Filter '*.zip').Count -gt 0 -or @(Get-ChildItem -LiteralPath $taskFixture -Filter '*.zip.sha256').Count -gt 0) { throw "Failure cleanup did not work: $taskCase" }
    }
    # Test manifest disagreement in a synthetic project, never edit repository versions.
    $taskVersionProject = Join-Path $taskFixture 'version-fixture'
    foreach ($taskFolder in @('scripts', 'src-tauri', 'artifacts/windows')) {
        New-Item -ItemType Directory -Path (Join-Path $taskVersionProject $taskFolder) -Force | Out-Null
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Build-PortableRelease.ps1') -Destination (Join-Path $taskVersionProject 'scripts/Build-PortableRelease.ps1')
    Set-Content -LiteralPath (Join-Path $taskVersionProject 'package.json') -Value '{"version":"1.2.3"}'
    Set-Content -LiteralPath (Join-Path $taskVersionProject 'src-tauri/tauri.conf.json') -Value '{"version":"1.2.4"}'
    Set-Content -LiteralPath (Join-Path $taskVersionProject 'src-tauri/Cargo.toml') -Value @('[package]', 'version = "1.2.3"')
    $taskVersionOutput = Join-Path $taskVersionProject 'artifacts/windows'
    foreach ($taskName in @('butter-manager.exe', 'butter-manager-v1.2.3-windows-x64-portable.zip', 'butter-manager-v1.2.3-windows-x64-portable.zip.sha256', 'SHA256SUMS.txt')) {
        Set-Content -LiteralPath (Join-Path $taskVersionOutput $taskName) -Value 'old fixture output'
    }
    $taskFailed = $false
    try { & (Join-Path $taskVersionProject 'scripts/Build-PortableRelease.ps1') -SkipFrontendBuild }
    catch {
        if ($_.Exception.Message -notlike '*must have the same valid version*') { throw }
        $taskFailed = $true
    }
    if (-not $taskFailed -or @(Get-ChildItem -LiteralPath $taskVersionOutput -File).Count -ne 0) { throw 'Version mismatch must clear old output and fail before building.' }
} finally {
    $taskResolved = (Resolve-Path -LiteralPath $taskFixture).Path
    $taskArtifactRoot = [IO.Path]::GetFullPath((Join-Path $taskRoot 'artifacts'))
    if (-not $taskResolved.StartsWith($taskArtifactRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $taskResolved) -notlike 'portable-release-test-*') { throw 'Cleanup escaped fixture.' }
    Remove-Item -LiteralPath $taskResolved -Recurse -Force
}
Write-Output 'Portable ZIP structure, executable hash, ZIP-only checksum and failure cleanup passed.'
# The expected fake Cargo failure above must not leak exit 42 into the caller/CI.
$global:LASTEXITCODE = 0
