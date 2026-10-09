# Dot-source this script to enable the project-local Rust and Windows C++ tools.
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
# Keep development downloads, state and temporary files separate from app data.
# pnpm 11 reads storeDir/cacheDir from pnpm-workspace.yaml; stateDir is session-only.
$taskTempRoot = Join-Path $taskProjectRoot '.tools\tmp'
New-Item -ItemType Directory -Path $taskTempRoot -Force | Out-Null
$env:TEMP = $taskTempRoot
$env:TMP = $taskTempRoot
$env:PNPM_CONFIG_STATE_DIR = Join-Path $taskProjectRoot '.tools\pnpm-state'
$taskCargoRoot = Join-Path $taskProjectRoot '.tools\cargo'
if (Test-Path -LiteralPath (Join-Path $taskCargoRoot 'bin\cargo.exe')) {
    $env:CARGO_HOME = $taskCargoRoot
    $env:RUSTUP_HOME = Join-Path $taskProjectRoot '.tools\rustup'
    $env:PATH = (Join-Path $taskCargoRoot 'bin') + ';' + $env:PATH
}
$taskVsRoot = Join-Path $taskProjectRoot '.tools\vs-buildtools'
$taskDevCommand = Join-Path $taskVsRoot 'VC\Auxiliary\Build\vcvars64.bat'
if (Test-Path -LiteralPath $taskDevCommand) {
    $taskEnvironment = & $env:ComSpec /d /s /c "`"`"$taskDevCommand`" >nul && set`""
    foreach ($taskLine in $taskEnvironment) {
        if ($taskLine -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
}
# vcvars may restore a pre-existing PATH; prepend Cargo after importing its environment.
if (Test-Path -LiteralPath (Join-Path $taskCargoRoot 'bin\cargo.exe')) {
    $env:PATH = (Join-Path $taskCargoRoot 'bin') + ';' + $env:PATH
}
