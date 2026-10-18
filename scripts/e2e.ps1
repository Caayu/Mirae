<#
.SYNOPSIS
    Desktop end-to-end tests: the real tela.exe driven by OS input.

.DESCRIPTION
    Builds the release executable and runs the tela-e2e tests (ignored by
    a plain `cargo test`, because they take over the screen, the mouse and
    the keyboard). Each test starts its own tela.exe in test mode, writes
    screenshots, the app's log and report.md into target\e2e\<test>\, and
    stops only the process it started. Leave the machine alone while it
    runs: another window taking the focus fails a test (it never types
    into another program).

.PARAMETER Filter
    Only tests whose name contains this text.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\e2e.ps1 -Filter gradient
#>
param(
    [string]$Filter = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and -not (($env:PATH -split ';') -contains $cargoBin)) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$ErrorActionPreference = 'Continue'
& cargo build --release -p tela-app
if ($LASTEXITCODE -ne 0) { Write-Host 'FAILED: build' -ForegroundColor Red; exit 1 }
$args = @('test', '--release', '-p', 'tela-e2e', '--', '--ignored', '--test-threads=1')
if ($Filter) { $args += $Filter }
& cargo @args
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
Write-Host "Reports: $root\target\e2e\<test>\report.md"
if ($code -ne 0) { Write-Host 'E2E: FAILED' -ForegroundColor Red; exit $code }
Write-Host 'E2E: passed' -ForegroundColor Green
