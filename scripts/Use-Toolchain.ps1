# Dot-source this script to enable the project-local Rust and Windows C++ tools.
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
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
