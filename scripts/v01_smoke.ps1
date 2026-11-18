# F1.2 / G8 / F7 — smoke v0.1: API sobe; fato conhecido → hit; OOD → abstain; solve presente.
# Artefato: target/v01_smoke.json
# Uso (raiz): .\scripts\v01_smoke.ps1
# Não é chatbot: valida cite/hit|abstain, false-act=0 no caminho feliz.

param(
    [string]$Bind = "127.0.0.1:18787",
    [int]$TimeoutSec = 90
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$bin = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }

New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
$ledger = Join-Path $Root "target\v01_smoke_ledger.jsonl"
$outJson = Join-Path $Root "target\v01_smoke.json"
$phasesPy = Join-Path $Root "target\v01_text_phases.py"
if (Test-Path $ledger) { Remove-Item $ledger -Force }

# Espelho de api::text_to_phases (FNV-1a + xorshift → 160 fasores unitários).
@'
import json, math, sys
text = sys.argv[1]
h = 0xcbf29ce484222325
for b in text.encode("utf-8"):
    h ^= b
    h = (h * 0x00000100000001b3) & 0xFFFFFFFFFFFFFFFF
st = h | 1
out = []
tau = 2.0 * math.pi
umax = float((1 << 64) - 1)
for _ in range(160):
    st ^= (st << 13) & 0xFFFFFFFFFFFFFFFF
    st &= 0xFFFFFFFFFFFFFFFF
    st ^= st >> 7
    st ^= (st << 17) & 0xFFFFFFFFFFFFFFFF
    st &= 0xFFFFFFFFFFFFFFFF
    phi = (st / umax) * tau
    out.append(math.cos(phi))
    out.append(math.sin(phi))
print(json.dumps(out))
'@ | Set-Content -Path $phasesPy -Encoding utf8

function Get-PhasesJson([string]$Text) {
    $raw = & python $phasesPy $Text
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) {
        throw "falha ao gerar phases para '$Text'"
    }
    return $raw.Trim()
}

Write-Host "v01_smoke: build aether-api (features api)…" -ForegroundColor Cyan
& "$PSScriptRoot\cargo.ps1" build --release --bin aether-api --features api
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $Root "target\release\aether-api.exe"
$proc = Start-Process -FilePath $exe -ArgumentList @($Bind, "--ledger", $ledger) `
    -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $Root "target\v01_smoke_api.out.log") `
    -RedirectStandardError (Join-Path $Root "target\v01_smoke_api.err.log")

function Invoke-AetherJson([string]$Path, [string]$Body) {
    $uri = "http://$Bind$Path"
    return Invoke-RestMethod -Uri $uri -Method Post -ContentType "application/json" -Body $Body -TimeoutSec 30
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$ok = $false
$err = $null
$known = $null
$ood = $null
$solve = $null
$props = $null
$frontier = $null

try {
    $deadline = [DateTime]::UtcNow.AddSeconds([Math]::Min(30, $TimeoutSec))
    while ([DateTime]::UtcNow -lt $deadline) {
        try {
            $null = Invoke-WebRequest -Uri "http://$Bind/health" -Method Get -TimeoutSec 2 -ErrorAction Stop
            break
        } catch {
            Start-Sleep -Milliseconds 200
        }
    }

    $props = Invoke-RestMethod -Uri "http://$Bind/props" -Method Get -TimeoutSec 10
    $frontier = [string]$props.aether.frontier
    if ([string]::IsNullOrWhiteSpace($frontier)) {
        throw "props.aether.frontier ausente"
    }

    $knownText = "Brasil capital Brasilia"
    $oodText = "xyzzy_ood_token_not_in_ledger_999"
    $phasesKnown = Get-PhasesJson $knownText
    $phasesOod = Get-PhasesJson $oodText

    $rememberBody = @"
{"key":"42","phases":$phasesKnown,"scope":{"organism":1,"run":0,"app":1},"certificate":{"energy":-1.5,"gap":0.25,"violations":0},"authority":200}
"@
    $null = Invoke-AetherJson "/v1/aether/remember" $rememberBody

    $cueKnown = @"
{"scope":{"organism":1,"run":0,"app":1},"query":$phasesKnown}
"@
    $known = Invoke-AetherJson "/v1/aether/cue" $cueKnown

    $cueOod = @"
{"scope":{"organism":1,"run":0,"app":1},"query":$phasesOod}
"@
    $ood = Invoke-AetherJson "/v1/aether/cue" $cueOod

    $solveBody = '{"n":2,"j":[[0,1],[1,0]],"h":[0,0],"steps":64,"restarts":2,"seed":7}'
    $solve = Invoke-AetherJson "/v1/aether/solve" $solveBody

    $knownHit = -not [bool]$known.abstain
    $oodAbstain = [bool]$ood.abstain
    $solveOk = ($null -ne $solve.energy) -or ($null -ne $solve.gap) -or ($null -ne $solve.certificate)
    $ok = $knownHit -and $oodAbstain -and $solveOk -and (-not [string]::IsNullOrWhiteSpace($frontier))
    if (-not $ok) {
        $err = "known_hit=$knownHit ood_abstain=$oodAbstain solve_ok=$solveOk frontier='$frontier'"
    }
}
catch {
    $err = "$_"
    $ok = $false
}
finally {
    if ($proc -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    $sw.Stop()
}

$result = [ordered]@{
    version       = "v01_smoke"
    ok            = $ok
    elapsed_ms    = $sw.ElapsedMilliseconds
    bind          = $Bind
    frontier      = $frontier
    known_abstain = if ($null -ne $known) { [bool]$known.abstain } else { $null }
    known_hits    = if ($null -ne $known) { $known.hits } else { $null }
    ood_abstain   = if ($null -ne $ood) { [bool]$ood.abstain } else { $null }
    ood_reason    = if ($null -ne $ood) { [string]$ood.reason } else { $null }
    solve_energy  = if ($null -ne $solve) { $solve.energy } else { $null }
    solve_gap     = if ($null -ne $solve) { $solve.gap } else { $null }
    error         = $err
    promise       = "hit_known__abstain_ood__frontier_props__no_chatbot"
    created       = (Get-Date).ToUniversalTime().ToString("o")
}
($result | ConvertTo-Json -Depth 6) | Set-Content -Path $outJson -Encoding utf8
Write-Host "artefato: $outJson ok=$ok elapsed_ms=$($sw.ElapsedMilliseconds)" -ForegroundColor $(if ($ok) { "Green" } else { "Yellow" })
if (-not $ok) { exit 2 }
exit 0
