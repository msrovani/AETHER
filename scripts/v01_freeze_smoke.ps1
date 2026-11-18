# F7 — smoke freeze v0.1 (docs/V01_DELIVERY_PLAN.md).
# Delega a scripts/v01_smoke.ps1 (frontier + known hit + OOD abstain + solve)
# e copia artefatos para target/v01_freeze/.
# Uso: powershell -NoProfile -File scripts/v01_freeze_smoke.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$OutDir = Join-Path $Root "target\v01_freeze"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

& "$PSScriptRoot\v01_smoke.ps1"
$code = $LASTEXITCODE

if (Test-Path (Join-Path $Root "target\v01_smoke.json")) {
    Copy-Item (Join-Path $Root "target\v01_smoke.json") (Join-Path $OutDir "v01_smoke.json") -Force
}
if (Test-Path (Join-Path $Root "target\aether_scorecard.json")) {
    Copy-Item (Join-Path $Root "target\aether_scorecard.json") (Join-Path $OutDir "aether_scorecard.json") -Force
}

$smokeJson = Join-Path $OutDir "v01_smoke.json"
$frontierPath = Join-Path $OutDir "smoke_f7_frontier.txt"
if (Test-Path $smokeJson) {
    try {
        $doc = Get-Content $smokeJson -Raw | ConvertFrom-Json
        $front = [string]$doc.frontier
        if ([string]::IsNullOrWhiteSpace($front)) { $front = "(ausente)" }
        $front | Set-Content $frontierPath -Encoding utf8
    } catch {
        "erro_lendo_v01_smoke.json" | Set-Content $frontierPath -Encoding utf8
    }
}

$summary = if ($code -eq 0) { "PASS" } else { "FAIL" }
"F7=$summary exit=$code" | Set-Content (Join-Path $OutDir "smoke_f7.txt") -Encoding utf8
Write-Host "RESUMO F7=$summary"
exit $code
