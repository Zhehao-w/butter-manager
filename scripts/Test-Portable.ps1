param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Use-Toolchain.ps1')
if (-not $SkipBuild) {
    & cargo build --locked --manifest-path (Join-Path $taskRoot 'src-tauri/Cargo.toml') --example portable_probe
    if ($LASTEXITCODE -ne 0) { throw 'Debug probe build failed' }
    & cargo build --locked --release --features tauri/custom-protocol --manifest-path (Join-Path $taskRoot 'src-tauri/Cargo.toml') --example portable_probe
    if ($LASTEXITCODE -ne 0) { throw 'Release probe build failed' }
}
$taskTarget = [IO.Path]::GetFullPath((Join-Path $taskRoot 'src-tauri/target'))
$taskFixture = Join-Path $taskTarget ('portable-probe-' + [guid]::NewGuid().ToString())
$taskOriginal = Join-Path $taskFixture 'original'
$taskMoved = Join-Path $taskFixture 'moved-manager'
New-Item -ItemType Directory -Path $taskOriginal | Out-Null
Copy-Item -LiteralPath (Join-Path $taskTarget 'debug/examples/portable_probe.exe') -Destination (Join-Path $taskOriginal 'probe-dev.exe')
Copy-Item -LiteralPath (Join-Path $taskTarget 'release/examples/portable_probe.exe') -Destination (Join-Path $taskOriginal 'probe-release.exe')

function Invoke-PortableProbe($Directory, $Executable, $Mode, $Phase) {
    $taskProcess = Start-Process -FilePath (Join-Path $Directory $Executable) -ArgumentList $Mode -WorkingDirectory $taskRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskFixture "$Phase.stdout.log") -RedirectStandardError (Join-Path $taskFixture "$Phase.stderr.log")
    if (-not $taskProcess.WaitForExit(45000)) {
        $taskProcess.Kill() # Only this script's own isolated test process.
        $taskProcess.WaitForExit()
        throw "Probe timed out: $Phase"
    }
    if ($taskProcess.ExitCode -ne 0) { throw "Probe failed: $Phase ($($taskProcess.ExitCode)); see $taskFixture" }
    $taskData = Join-Path $Directory $(if ($Executable -eq 'probe-dev.exe') { 'data-dev' } else { 'data' })
    $taskReport = Get-Content -LiteralPath (Join-Path $taskData 'probe-result.json') -Raw | ConvertFrom-Json
    $taskReportedPath = $taskReport.data
    if ($taskReportedPath.StartsWith('\\?\')) { $taskReportedPath = $taskReportedPath.Substring(4) }
    if ([IO.Path]::GetFullPath($taskReportedPath) -ne [IO.Path]::GetFullPath($taskData)) { throw 'Unexpected data directory' }
    if ($taskReport.config -ne $taskReport.data -or $taskReport.cache -ne (Join-Path $taskReport.data 'caches') -or $taskReport.logs -ne (Join-Path $taskReport.data 'logs')) { throw 'App directory APIs disagree' }
    foreach ($taskFile in @('library.sqlite3', 'appearance.json', 'imports/probe.manifest')) {
        if (-not (Test-Path -LiteralPath (Join-Path $taskData $taskFile))) { throw "Missing application data: $taskFile" }
    }
    $taskLevelDb = Join-Path $taskData 'EBWebView/Default/Local Storage/leveldb'
    if (-not (Test-Path -LiteralPath $taskLevelDb)) { throw 'No actual WebView2 localStorage directory' }
    if (@(Get-ChildItem -LiteralPath $taskLevelDb -File).Count -eq 0) { throw 'Empty localStorage profile' }
    Write-Output "$Phase passed: $taskData; localStorage=$($taskReport.localStorage); origin=$($taskReport.origin)"
}

Invoke-PortableProbe $taskOriginal 'probe-release.exe' 'write' 'release-write'
Invoke-PortableProbe $taskOriginal 'probe-dev.exe' 'write' 'development-write'
# Both source and destination are verified inside this fresh, isolated target fixture.
foreach ($taskPath in @($taskOriginal, $taskMoved)) {
    if (-not [IO.Path]::GetFullPath($taskPath).StartsWith($taskFixture + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Move escaped the fixture' }
}
Move-Item -LiteralPath $taskOriginal -Destination $taskMoved
Invoke-PortableProbe $taskMoved 'probe-release.exe' 'read' 'release-after-move'
Invoke-PortableProbe $taskMoved 'probe-dev.exe' 'read' 'development-after-move'
# Successful fixtures contain no user data and no longer need to be retained.
$taskResolvedFixture = (Resolve-Path -LiteralPath $taskFixture).Path
if (-not $taskResolvedFixture.StartsWith($taskTarget + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $taskResolvedFixture) -notlike 'portable-probe-*') { throw 'Cleanup escaped the fixture' }
Remove-Item -LiteralPath $taskResolvedFixture -Recurse -Force
Write-Output 'All four Portable checks passed; temporary fixture removed.'
