# F1.2 — onboarding v0.1 (docs/V01_DELIVERY_PLAN.md G8).
# Sobe aether-api, pergunta um fato conhecido, um desconhecido, o solver e o agente.
# Saída: target/v01_smoke.json. Exit 1 se algum passo falhar.
param(
    [string]$Bin = "",
    [int]$Port = 8794
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

if ($Bin -eq "") {
    $candidates = @(
        "target\release\aether-api.exe",
        "target_open\release\aether-api.exe"
    )
    $Bin = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $Bin) {
        throw "aether-api.exe ausente. cargo build --release --features api --bin aether-api"
    }
}

$base = "http://127.0.0.1:$Port"
$steps = New-Object System.Collections.Generic.List[object]
$failed = $false

function Add-Step([string]$Name, [bool]$Ok, [string]$Detail) {
    $script:steps.Add([ordered]@{
            name   = $Name
            ok     = $Ok
            detail = $Detail
        }) | Out-Null
    if (-not $Ok) { $script:failed = $true }
    $tag = if ($Ok) { "PASS" } else { "FAIL" }
    Write-Output "$tag $Name $Detail"
}

function New-Phases([double]$Freq) {
    0..159 | ForEach-Object { [math]::Round([math]::Sin(($_ + 1) * $Freq), 5) }
}

$sw = [Diagnostics.Stopwatch]::StartNew()
$proc = Start-Process -FilePath $Bin -ArgumentList @(
    "127.0.0.1:$Port",
    "--sgdb", "mock",
    "--sgdb-outbox", "target\v01_smoke_outbox.jsonl"
) -PassThru -WindowStyle Hidden

try {
    $healthy = $false
    $health = $null
    foreach ($i in 1..50) {
        Start-Sleep -Milliseconds 100
        try {
            $health = Invoke-RestMethod -Uri "$base/health" -Method Get -TimeoutSec 2
            if ($null -ne $health) { $healthy = $true; break }
        } catch {}
    }
    Add-Step "health" $healthy ("sgdb=" + $health.sgdb)

    $phases = New-Phases 0.05
    $remember = @{
        key         = "777"
        phases      = $phases
        scope       = @{ organism = 1; run = 1; app = 1 }
        certificate = @{ energy = -1.5; gap = 0.25; violations = 0 }
        authority   = 100
    } | ConvertTo-Json -Depth 5 -Compress
    $rem = Invoke-RestMethod -Uri "$base/v1/aether/remember" -Method Post -ContentType "application/json" -Body $remember
    Add-Step "remember" ($null -ne $rem) "ok"

    $cueBody = @{
        scope = @{ organism = 1; run = 1; app = 1 }
        query = $phases
    } | ConvertTo-Json -Depth 5 -Compress
    $cue = Invoke-RestMethod -Uri "$base/v1/aether/cue" -Method Post -ContentType "application/json" -Body $cueBody
    $cueOk = ($cue.abstain -eq $false -and [string]$cue.key -eq "777")
    Add-Step "cue_conhecido" $cueOk ("key=" + $cue.key + " abstain=" + $cue.abstain)

    $other = New-Phases 0.17
    $cueMissBody = @{
        scope = @{ organism = 1; run = 1; app = 1 }
        query = $other
    } | ConvertTo-Json -Depth 5 -Compress
    $miss = Invoke-RestMethod -Uri "$base/v1/aether/cue" -Method Post -ContentType "application/json" -Body $cueMissBody
    Add-Step "cue_desconhecido" ($miss.abstain -eq $true) ("reason=" + $miss.reason)

    $solve = Invoke-RestMethod -Uri "$base/v1/aether/solve" -Method Post -ContentType "application/json" -Body '{"n":2,"j":[[0,1],[1,0]],"h":[0,0],"steps":80,"restarts":4,"seed":7}'
    $solveOk = ($solve.violations -eq 0 -and $null -ne $solve.energy)
    Add-Step "solve_certificado" $solveOk ("violations=" + $solve.violations + " energy=" + $solve.energy)

    $known = Invoke-RestMethod -Uri "$base/v1/aether/agent" -Method Post -ContentType "application/json" -Body '{"subject":"Brasil","relation":"capital"}'
    $knownOk = ($known.action -eq "act" -or $known.action -eq "cite")
    Add-Step "agent_conhecido" $knownOk ("action=" + $known.action)

    $unknown = Invoke-RestMethod -Uri "$base/v1/aether/agent" -Method Post -ContentType "application/json" -Body '{"subject":"Atlantida","relation":"capital"}'
    Add-Step "agent_desconhecido" ($unknown.action -eq "abstain") ("action=" + $unknown.action)
} finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}

$sw.Stop()
$pass = @($steps | Where-Object { $_.ok }).Count
$report = [ordered]@{
    plan     = "docs/V01_DELIVERY_PLAN.md"
    gate     = "G8"
    ok       = (-not $failed)
    pass     = $pass
    total    = $steps.Count
    elapsed_s = [math]::Round($sw.Elapsed.TotalSeconds, 3)
    bin      = $Bin
    sgdb     = "mock"
    steps    = @($steps)
}
New-Item -ItemType Directory -Force -Path "target" | Out-Null
$report | ConvertTo-Json -Depth 6 | Set-Content -Path "target\v01_smoke.json" -Encoding utf8
Write-Output ("RESUMO {0}/{1} elapsed_s={2}" -f $pass, $steps.Count, $report.elapsed_s)
if ($failed) { exit 1 }
