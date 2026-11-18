# Demo rápida AETHER (Onda 0): build release + organismo ~15 s + tip daemon/hub.
# Onda S (2026-11-17): o default permanece --plain. TUI só com --features tui
# e a flag --tui. BAM/JIT/governor continuam fora deste script.
# Uso (raiz do repo):
#   .\scripts\run-demo.ps1
#   .\scripts\run-demo.ps1 -Duration 10s

param(
    [string]$Duration = "15s"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$bin = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }

Write-Host "AETHER run-demo — build release (aether-organism)…" -ForegroundColor Cyan
& "$PSScriptRoot\cargo.ps1" build --release --bin aether-organism --features m4
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $Root "target\release\aether-organism.exe"
if (-not (Test-Path $exe)) {
    Write-Error "binário ausente: $exe"
    exit 1
}

Write-Host "Rodando organismo $Duration (painel --plain)…" -ForegroundColor Cyan
& $exe --duration $Duration --plain
$code = $LASTEXITCODE

Write-Host ""
Write-Host "Próximos passos:" -ForegroundColor Green
Write-Host "  Hub:    .\scripts\cargo.ps1 run --release --bin aether-dashboard --features dashboard"
Write-Host "  Daemon: .\scripts\cargo.ps1 run --release --bin aether-daemon -- --duration 10s"
Write-Host "  Doação: python tools/harvest_transplant.py --limit 200 --dry-run --out data --no-whiten"
Write-Host "  Help:   .\target\release\aether-organism.exe --help"
Write-Host "  TUI:    .\scripts\cargo.ps1 run --release --bin aether-organism --features tui -- --duration 15s --tui"
exit $code
