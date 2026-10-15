<#
.SYNOPSIS
    Local verification: the same gates every change must pass.

.DESCRIPTION
    Runs, in order, stopping at the first failure:
      1. cargo fmt --all -- --check
      2. cargo clippy --workspace --all-targets --release -- -D warnings
      3. cargo test --workspace --release          (includes golden tests)
      4. cargo doc --workspace --no-deps           (RUSTDOCFLAGS=-D warnings)
      5. cargo build --workspace --release

    Golden tests take about a second and always run with the tests. The
    benchmark is slow (about a minute) and only runs with -Bench.

.PARAMETER Quick
    Only fmt, clippy and tests (skips doc and release build).

.PARAMETER Bench
    Also runs the stamping benchmark and the quick document benchmark
    (composition gate: fails if composition allocates) and prints their
    tables (compare with docs/performance.md).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\check.ps1
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\check.ps1 -Quick
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\check.ps1 -Bench
#>
param(
    [switch]$Quick,
    [switch]$Bench
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# rustup installs into ~/.cargo/bin, which is not always on PATH.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and -not (($env:PATH -split ';') -contains $cargoBin)) {
    $env:PATH = "$cargoBin;$env:PATH"
}

$steps = @(
    @{ Name = 'fmt';    Args = @('fmt', '--all', '--', '--check') },
    @{ Name = 'clippy'; Args = @('clippy', '--workspace', '--all-targets', '--release', '--', '-D', 'warnings') },
    @{ Name = 'test';   Args = @('test', '--workspace', '--release') }
)
if (-not $Quick) {
    $steps += @{ Name = 'doc';   Args = @('doc', '--workspace', '--no-deps'); DocStrict = $true }
    $steps += @{ Name = 'build'; Args = @('build', '--workspace', '--release') }
}
if ($Bench) {
    $steps += @{ Name = 'bench'; Args = @('run', '--release', '-p', 'tela-corpus', '--example', 'stamp_bench') }
    # Composition across layer counts, features and a realistic document;
    # exits non-zero if composition allocates (invariant F3).
    $steps += @{ Name = 'doc bench'; Args = @('run', '--release', '-p', 'tela-corpus', '--example', 'doc_bench', '--', '--quick') }
}

$failed = $null
foreach ($step in $steps) {
    Write-Host "==> cargo $($step.Args -join ' ')" -ForegroundColor Cyan
    $previousDocFlags = $env:RUSTDOCFLAGS
    if ($step.DocStrict) { $env:RUSTDOCFLAGS = '-D warnings' }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    # cargo writes progress to stderr; let it through without PowerShell
    # turning it into errors.
    $ErrorActionPreference = 'Continue'
    & cargo @($step.Args)
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $env:RUSTDOCFLAGS = $previousDocFlags
    $seconds = [string]::Format([cultureinfo]::InvariantCulture, '{0:0.0}', $sw.Elapsed.TotalSeconds)
    if ($code -ne 0) {
        Write-Host "FAILED: $($step.Name) (exit $code, ${seconds}s)" -ForegroundColor Red
        $failed = $step.Name
        break
    }
    Write-Host "ok: $($step.Name) (${seconds}s)" -ForegroundColor Green
}

if ($failed) { exit 1 }
Write-Host 'All checks passed.' -ForegroundColor Green
exit 0
