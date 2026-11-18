# Gates F1–F6 do freeze v0.1 (docs/V01_DELIVERY_PLAN.md).
# F7 = scripts/v01_freeze_smoke.ps1 (binário API).
# Artefatos: target/v01_freeze/
# Uso: powershell -NoProfile -File scripts/v01_freeze_gates.ps1

$ErrorActionPreference = "Continue"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$bin = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
if (Test-Path $bin) {
    if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }
}

$OutDir = Join-Path $Root "target\v01_freeze"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$report = Join-Path $OutDir "gates_f1_f6.txt"
$lines = New-Object System.Collections.Generic.List[string]
$fail = 0
$cargoPs1 = Join-Path $PSScriptRoot "cargo.ps1"
$cargoExe = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"

function Invoke-CargoDirect {
    param([string[]]$CargoArgs)
    if (Test-Path $cargoPs1) {
        & $cargoPs1 @CargoArgs
    } elseif (Test-Path $cargoExe) {
        & $cargoExe @CargoArgs
    } else {
        & cargo @CargoArgs
    }
}

function Run-Gate([string]$Id, [string[]]$CargoArgs) {
    Write-Host "=== $Id ==="
    $logPath = Join-Path $OutDir "$Id.log"
    $out = & { Invoke-CargoDirect -CargoArgs $CargoArgs 2>&1 | Out-String }
    $code = $LASTEXITCODE
    $out | Set-Content $logPath -Encoding utf8
    $tail = ($out -split "`n" | Where-Object { $_ -match 'test result:|ACQ|false_act|FAIL|error:' } | Select-Object -Last 5) -join " | "
    if ($code -eq 0) {
        $status = "PASS"
    } else {
        $status = "FAIL"
        $script:fail++
    }
    $line = "$status $Id exit=$code $tail"
    $lines.Add($line) | Out-Null
    Write-Host $line
}

Write-Host "=== F6 fmt ==="
Invoke-CargoDirect -CargoArgs @("fmt", "--", "--check") 2>&1 | Out-Null
$fmt = $LASTEXITCODE
if ($fmt -ne 0) { $fail++ }
$lines.Add("$(if ($fmt -eq 0) {'PASS'} else {'FAIL'}) F6_fmt exit=$fmt") | Out-Null
Write-Host $lines[-1]

Write-Host "=== F6 clippy ==="
$clippyOut = & { Invoke-CargoDirect -CargoArgs @("clippy", "--release", "--all-targets", "--", "-D", "warnings") 2>&1 | Out-String }
$cl = $LASTEXITCODE
$clippyOut | Set-Content (Join-Path $OutDir "F6_clippy.log") -Encoding utf8
if ($cl -ne 0) { $fail++ }
$lines.Add("$(if ($cl -eq 0) {'PASS'} else {'FAIL'}) F6_clippy exit=$cl") | Out-Null
Write-Host $lines[-1]

Run-Gate "F6_lib" @("test", "--release", "--lib")
Run-Gate "F1_agent_eval" @("test", "--release", "--test", "agent_eval_suite", "--", "--nocapture")
Run-Gate "F2_cic" @("test", "--release", "--test", "cic_abstain_gate", "--", "--nocapture")
Run-Gate "F3_api_agent" @("test", "--release", "--features", "api", "--test", "api_agent", "--", "--nocapture")
Run-Gate "F3_api_eval" @("test", "--release", "--features", "api", "--test", "api_eval", "--", "--nocapture")
Run-Gate "F3_api_integrity" @("test", "--release", "--features", "api", "--test", "api_integrity", "--", "--nocapture")
Run-Gate "F4_usability" @("test", "--release", "--features", "agent", "--test", "usability_gate", "--", "--nocapture")
Run-Gate "F4_redteam" @("test", "--release", "--test", "agent_redteam_gate", "--", "--nocapture")
Run-Gate "F5_cognitive" @("test", "--release", "--test", "cognitive_audit", "--", "--nocapture")

foreach ($f in @("target/aether_scorecard.json", "target/cic_abstain.json", "target/usability_gate.json")) {
    if (Test-Path $f) {
        Copy-Item $f (Join-Path $OutDir (Split-Path $f -Leaf)) -Force
    }
}

$summary = "SUMMARY fail_count=$fail"
$lines.Add($summary) | Out-Null
$lines | Set-Content $report -Encoding utf8

$json = [ordered]@{
    version    = "v01_freeze_gates"
    ok         = ($fail -eq 0)
    fail_count = $fail
    lines      = @($lines)
    created    = (Get-Date).ToUniversalTime().ToString("o")
}
($json | ConvertTo-Json -Depth 5) | Set-Content (Join-Path $OutDir "gates_f1_f6.json") -Encoding utf8

Write-Host "=== RESUMO ==="
Get-Content $report
if ($fail -gt 0) { exit 1 } else { exit 0 }
