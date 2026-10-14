# Dot-source this script to enable shared tools with project-local development caches.
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
# Keep development downloads, state and temporary files separate from app data.
# pnpm 11 reads storeDir/cacheDir from pnpm-workspace.yaml; stateDir is session-only.
$taskTempRoot = Join-Path $taskProjectRoot '.tools\tmp'
New-Item -ItemType Directory -Path $taskTempRoot -Force | Out-Null
$env:TEMP = $taskTempRoot
$env:TMP = $taskTempRoot
$env:PNPM_CONFIG_STATE_DIR = Join-Path $taskProjectRoot '.tools\pnpm-state'
$env:CARGO_HOME = Join-Path $taskProjectRoot '.tools\cargo-cache'
New-Item -ItemType Directory -Path $env:CARGO_HOME -Force | Out-Null
$taskCargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$env:RUSTUP_HOME = Join-Path $env:USERPROFILE '.rustup'
$env:CARGO_INSTALL_ROOT = Join-Path $env:USERPROFILE '.cargo'
$taskVswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$taskVsRoot = if (Test-Path -LiteralPath $taskVswhere) {
    & $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $taskVsRoot) { throw 'Install the shared Windows C++ Build Tools before building the desktop app.' }
$taskDevCommand = Join-Path $taskVsRoot 'VC\Auxiliary\Build\vcvars64.bat'
if (Test-Path -LiteralPath $taskDevCommand) {
    $taskEnvironment = & $env:ComSpec /d /c "call `"$taskDevCommand`" >nul && set"
    if ($LASTEXITCODE -ne 0) { throw 'Windows C++ toolchain initialization failed.' }
    foreach ($taskLine in $taskEnvironment) {
        if ($taskLine -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
}
# vcvars may restore a pre-existing PATH; prepend Cargo after importing its environment.
if (Test-Path -LiteralPath (Join-Path $taskCargoBin 'cargo.exe')) {
    $env:PATH = $taskCargoBin + ';' + $env:PATH
}
