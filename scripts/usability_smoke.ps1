# US-smoke — cliente real contra aether-api (docs/USABILITY_TESTS.md U1/U2/U3/U4/U5/U9).
# Uso:
#   pwsh -NoProfile -File scripts/usability_smoke.ps1 [-Bin target_open\release\aether-api.exe] [-Port 8793]
# Saída: PASS/FAIL por cenário + RESUMO; exit 1 se algum cenário falhar.
param(
    [string]$Bin = "",
    [int]$Port = 8793
)
$ErrorActionPreference = "Stop"

if ($Bin -eq "") {
    $candidates = @("target_open\release-unwind\aether-api.exe", "target\release-unwind\aether-api.exe", "target\release\aether-api.exe")
    $Bin = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $Bin) { throw "aether-api.exe não encontrado; rode cargo build --release --features api --bin aether-api" }
}

$base = "http://127.0.0.1:$Port"
$results = New-Object System.Collections.Generic.List[string]

function Check($name, $ok, $detail) {
    $tag = if ($ok) { "PASS" } else { "FAIL" }
    $results.Add("$tag $name $detail") | Out-Null
    if (-not $ok) { $script:failed = $true }
}

# U1 — onboarding a frio: sobe, /health responde.
$proc = Start-Process -FilePath $Bin -ArgumentList "127.0.0.1:$Port" -PassThru -WindowStyle Hidden
try {
    $healthy = $false
    foreach ($i in 1..50) {
        Start-Sleep -Milliseconds 100
        try {
            $h = Invoke-RestMethod -Uri "$base/health" -Method Get -TimeoutSec 2
            if ($null -ne $h) { $healthy = $true; break }
        } catch {}
    }
    Check "U1_health" $healthy ("requests=" + $h.requests + " ledger=" + $h.ledger_entries)

    # U5 — solver devolve certificado, não texto.
    $solve = Invoke-RestMethod -Uri "$base/v1/aether/solve" -Method Post -ContentType "application/json" -Body '{"n":2,"j":[[0,1],[1,0]],"h":[0,0],"steps":80,"restarts":4,"seed":7}'
    Check "U5_solve_certificado" ($solve.coherent -eq $true -and $solve.violations -eq 0) ("energy=" + $solve.energy + " gap=" + $solve.gap + " decision=" + $solve.decision)

    # U2 — fato citável: remember + cue devolve key/escopo/autoridade.
    $phases = 0..159 | ForEach-Object { [math]::Round([math]::Sin(($_ + 1) * 0.05), 5) }
    $body = @{
        key          = "777"
        phases       = $phases
        scope        = @{ organism = 1; run = 1; app = 1 }
        certificate  = @{ energy = -1.5; gap = 0.25; violations = 0 }
        authority    = 100
    } | ConvertTo-Json -Depth 5 -Compress
    $rem = Invoke-RestMethod -Uri "$base/v1/aether/remember" -Method Post -ContentType "application/json" -Body $body
    $cueBody = @{ scope = @{ organism = 1; run = 1; app = 1 }; query = $phases } | ConvertTo-Json -Depth 5 -Compress
    $cue = Invoke-RestMethod -Uri "$base/v1/aether/cue" -Method Post -ContentType "application/json" -Body $cueBody
    Check "U2_fato_citavel" ($cue.abstain -eq $false -and $cue.key -eq 777 -and $cue.authority -gt 0 -and $cue.score -gt 0.5) ("score=" + $cue.score + " authority=" + $cue.authority)

    # U4 — vetor ortogonal abstém por similaridade baixa (nunca chuta).
    $perturbado = 0..159 | ForEach-Object { [math]::Round([math]::Cos(($_ + 1) * 0.05), 5) }
    $cueBad = @{ scope = @{ organism = 1; run = 1; app = 1 }; query = $perturbado } | ConvertTo-Json -Depth 5 -Compress
    $cueAbst = Invoke-RestMethod -Uri "$base/v1/aether/cue" -Method Post -ContentType "application/json" -Body $cueBad
    Check "U4_parafra_abstem" ($cueAbst.abstain -eq $true -and $cueAbst.reason -eq "low_similarity") ("reason=" + $cueAbst.reason + " score=" + $cueAbst.score)

    # U3 — escopo alheio é vazio (R8): nunca vaza fato de outro organismo.
    $cueOther = @{ scope = @{ organism = 7; run = 1; app = 1 }; query = $phases } | ConvertTo-Json -Depth 5 -Compress
    $cueEmpty = Invoke-RestMethod -Uri "$base/v1/aether/cue" -Method Post -ContentType "application/json" -Body $cueOther
    Check "U3_escopo_alheio_vazio" ($cueEmpty.abstain -eq $true -and $cueEmpty.reason -eq "empty_scope") ("reason=" + $cueEmpty.reason)

    # U9 — purge de 255 sem store: 500 ledger_io (túmulo impossível, fato fica).
    $status = ""
    try {
        Invoke-RestMethod -Uri "$base/v1/aether/forget" -Method Post -ContentType "application/json" -Body '{"key":777,"authority":255,"scope":{"organism":1,"run":1,"app":1}}' | Out-Null
        $status = "200"
    } catch {
        $status = [string][int]$_.Exception.Response.StatusCode
    }
    Check "U9_forget_255_sem_store" ($status -eq "500") "status=$status"
} finally {
    if ($proc -and -not $proc.HasExited) { $proc.Kill() }
}

$results | ForEach-Object { Write-Output $_ }
$failCount = ($results | Where-Object { $_ -like "FAIL*" }).Count
Write-Output ("RESUMO " + ($results.Count - $failCount) + " PASS / " + $failCount + " FAIL")
if ($failCount -gt 0) { exit 1 }
