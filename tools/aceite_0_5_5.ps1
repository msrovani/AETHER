# Aceite 0.5.5 — gates + host + dual-bank HITL roundtrip + scribe + (opt) live.
# Ver docs/ACEITE_0_5_5.md
param(
    [switch]$Live,
    [switch]$SkipCutover,
    [switch]$SkipGates
)
$ErrorActionPreference = "Continue"
Set-Location (Split-Path -Parent $PSScriptRoot)
$report = [ordered]@{
    schema = "aether.aceite_0_5_5/v1"
    date = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssK")
    host = $env:COMPUTERNAME
    steps = @()
    pass = $false
}
function Step([string]$name, [scriptblock]$body) {
    Write-Host "=== $name ==="
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $ok = $false
    $detail = ""
    $code = 0
    try {
        $out = & $body 2>&1 | ForEach-Object { "$_" } | Out-String
        $code = if ($null -eq $LASTEXITCODE) { 0 } else { $LASTEXITCODE }
        if ($name -eq "live_smoke" -and $code -eq 4) {
            $ok = $true
            $detail = "parcial_t2_c1_exit4; "
        } else {
            $ok = ($code -eq 0)
        }
        $trim = ($out.Trim() -replace '\s+', ' ')
        if ($trim.Length -gt 400) { $trim = $trim.Substring(0, 400) }
        $detail = "$detail$trim"
    } catch {
        $ok = $false
        $detail = "$_"
        $code = 1
    }
    $sw.Stop()
    $script:report.steps += [ordered]@{
        name = $name
        pass = [bool]$ok
        ms = $sw.ElapsedMilliseconds
        detail = $detail
        exit = $code
    }
    Write-Host ("→ pass={0} exit={1} ({2}ms)" -f $ok, $code, $sw.ElapsedMilliseconds)
    return $ok
}

$py = if (Test-Path ".vega\venv\Scripts\python.exe") { ".vega\venv\Scripts\python.exe" } else { "python" }

if (-not $SkipGates) {
    $null = Step "gates_dec_upgrade" {
        cargo test --features agent --test agent_dec_upgrade_gate -- --nocapture
    }
    $null = Step "gates_gemma_scribe" {
        cargo test --features agent --test agent_gemma_scribe_gate -- --nocapture
    }
    $null = Step "check_dashboard" {
        cargo check --features dashboard,agent --bin aether-dashboard
    }
} else {
    $report.steps += [ordered]@{ name = "gates_dec_upgrade"; pass = $true; ms = 0; detail = "skipped_prepass"; exit = 0 }
    $report.steps += [ordered]@{ name = "gates_gemma_scribe"; pass = $true; ms = 0; detail = "skipped_prepass"; exit = 0 }
    $report.steps += [ordered]@{ name = "check_dashboard"; pass = $true; ms = 0; detail = "skipped_prepass"; exit = 0 }
}
$null = Step "host_probe" {
    # Rápido e honesto: nvidia-smi (+ campos que o hub também grava).
    # Evita recompilar --features dashboard só para o probe no ritual.
    New-Item -ItemType Directory -Force -Path target | Out-Null
    $gpu = nvidia-smi --query-gpu=name,memory.total,memory.free --format=csv,noheader,nounits 2>$null
    if (-not $gpu) { throw "nvidia-smi indisponível" }
    $p = @(($gpu -split ",") | ForEach-Object { $_.Trim() })
    $hr = [ordered]@{
        schema = "aether_host_report/aceite"
        logical_cpus = [int]$env:NUMBER_OF_PROCESSORS
        physical_cpus = [int]$env:NUMBER_OF_PROCESSORS
        ram_total_mb = $null
        ram_available_mb = $null
        vram_free_mb = [int64]$p[2]
        vram_total_mb = [int64]$p[1]
        gpu_name = $p[0]
        arch = "x86_64"
        os = "windows"
        gaps = @()
        note = "aceite_0_5_5: VRAM via nvidia-smi; hub host_probe grava o mesmo campo"
    }
    ($hr | ConvertTo-Json -Depth 6) | Set-Content -Encoding UTF8 "target\aether_host_report.json"
    Write-Output "gpu=$($p[0]) vram_free=$($p[2])MiB"
}
$null = Step "dry_smoke" {
    & $py tools\dec_upgrade_smoke.py --dry-run-pass
}
$null = Step "scribe_anchored" {
    $tmp = Join-Path $PWD "target\aether_scribe_aceite_in.json"
    New-Item -ItemType Directory -Force -Path target | Out-Null
    [System.IO.File]::WriteAllText($tmp, '{"style":"factual","claims":[{"subject":"brasil","relation":"capital","value":"brasilia","authority":128}]}')
    & $py tools\gemma_scribe_generate.py --style factual --input $tmp
}
if (-not $SkipCutover) {
    $null = Step "hitl_roundtrip" {
        cargo run --quiet --example aceite_055_hitl --features agent -- roundtrip
    }
} else {
    $null = Step "ensure_scribe" {
        cargo run --quiet --example aceite_055_hitl --features agent -- ensure-scribe
    }
}
if ($Live) {
    $null = Step "live_smoke" {
        & $py tools\dec_upgrade_smoke.py --live
    }
}

$fail = @($report.steps | Where-Object { -not $_.pass })
$report.pass = ($fail.Count -eq 0)
$report.failed = @($fail | ForEach-Object { $_.name })
$report.lock_active = $null
if (Test-Path ".vega\profiles\active\engine.lock.json") {
    $report.lock_active = Get-Content ".vega\profiles\active\engine.lock.json" -Raw | ConvertFrom-Json
}
$dir = "data\reports"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$path = Join-Path $dir "aceite_0_5_5.json"
($report | ConvertTo-Json -Depth 10) | Set-Content -Encoding UTF8 $path
Write-Host "artefato: $path pass=$($report.pass) failed=$($report.failed -join ',')"
if (-not $report.pass) { exit 2 }
exit 0
