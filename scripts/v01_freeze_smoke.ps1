# F7 — smoke freeze v0.1 (docs/V01_DELIVERY_PLAN.md).
# Encadeia v01_smoke.ps1 + checagem de /props.frontier + cópia de artefatos.
# Uso: pwsh -NoProfile -File scripts/v01_freeze_smoke.ps1
param(
    [string]$Bin = "",
    [int]$Port = 8789
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$OutDir = Join-Path $Root "target\v01_freeze"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# 1) smoke base (remember/cue/OOD/agent)
& "$PSScriptRoot\v01_smoke.ps1" -Bin $Bin -Port $Port
$smokeExit = $LASTEXITCODE
if (Test-Path (Join-Path $Root "target\v01_smoke.json")) {
    Copy-Item (Join-Path $Root "target\v01_smoke.json") (Join-Path $OutDir "v01_smoke.json") -Force
}

# 2) props.frontier (exige bin; sobe de novo se preciso)
if ($Bin -eq "") {
    $candidates = @("target\release\aether-api.exe", "target_open\release\aether-api.exe")
    $Bin = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
$frontierOk = $false
$frontierDetail = "no_bin"
if ($Bin -and (Test-Path $Bin)) {
    $p2 = $Port + 1
    $proc = Start-Process -FilePath $Bin -ArgumentList @("127.0.0.1:$p2") -PassThru -WindowStyle Hidden
    try {
        $base = "http://127.0.0.1:$p2"
        foreach ($i in 1..40) {
            Start-Sleep -Milliseconds 100
            try {
                $props = Invoke-RestMethod -Uri "$base/props" -Method Get -TimeoutSec 2
                $frontier = [string]$props.aether.frontier
                $frontierOk = ($frontier -match "cita")
                $frontierDetail = "frontier=$frontier"
                break
            } catch {}
        }
    } finally {
        if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    }
}

$line = "$(if ($frontierOk) {'PASS'} else {'FAIL'}) F7_props_frontier $frontierDetail"
Write-Host $line
$line | Set-Content (Join-Path $OutDir "smoke_f7_frontier.txt") -Encoding utf8

if (Test-Path (Join-Path $Root "target\aether_scorecard.json")) {
    Copy-Item (Join-Path $Root "target\aether_scorecard.json") (Join-Path $OutDir "aether_scorecard.json") -Force
}

if ($smokeExit -ne 0 -or -not $frontierOk) { exit 1 }
Write-Host "RESUMO F7=PASS"
exit 0
