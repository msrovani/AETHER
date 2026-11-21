# Smoke do hub aether-dashboard (superficie humana) — MSG 1096.
# Nao abre a GUI interativa; valida build + faixa F4 CONGELADA no fonte.
param()
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

Write-Output "=== hub_smoke: check bin ==="
& .\scripts\cargo.ps1 check --bin aether-dashboard --features dashboard --offline
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$src = Get-Content -Raw -Encoding UTF8 "src\bin\aether_dashboard.rs"
$f4 = Get-Content -Raw -Encoding UTF8 "src\dashboard\f4_view.rs"
$checks = @(
    @{ Name = "banner_congelada"; Ok = ($src -match "CONGELADA") },
    @{ Name = "msg_1093"; Ok = ($src -match "1093") },
    @{ Name = "f4_freeze_const"; Ok = ($f4 -match "F4_FREEZE") },
    @{ Name = "program_line_fechada"; Ok = ($f4 -match "cadeira fechada") }
)
$fail = @($checks | Where-Object { -not $_.Ok })
$report = [ordered]@{
    schema = "aether.hub_smoke/v1"
    date = (Get-Date -Format "yyyy-MM-dd")
    cargo_check = "ok"
    checks = @($checks | ForEach-Object { [ordered]@{ name = $_.Name; pass = [bool]$_.Ok } })
    pass = ($fail.Count -eq 0)
    ref = "MSG_1096"
}
$dir = "data\reports"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$path = Join-Path $dir "hub_smoke.json"
($report | ConvertTo-Json -Depth 6) | Set-Content -Encoding UTF8 $path
Write-Output "artefato: $path pass=$($report.pass)"
if (-not $report.pass) { exit 2 }
exit 0
