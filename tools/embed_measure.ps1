#!/usr/bin/env pwsh
# Runner atômico de medição de um embedder no set v2 congelado.
# Usage: .\tools\embed_measure.ps1 <MODEL_ID> <TAG> [TIMEOUT_S]
# Artifacts: target/embed_eval_<tag>.json (avalição) e a sonda de latência.
param(
    [string]$ModelId = "BAAI/bge-m3",
    [string]$Tag = "bge_m3_v2",
    [int]$TimeoutS = 2400
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$py = "$Root\.venv-py\Scripts\python.exe"
if (-not (Test-Path $py)) { Write-Error "venv .venv-py não encontrado"; exit 2 }

# matar orfãos de servidores de embed (deixa o soquete limpo, 0 contenção)
Get-CimInstance Win32_Process -Filter "Name='python.exe'" |
    Where-Object { $_.CommandLine -and $_.CommandLine -match 'embed_server\.py' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2

$out = "target\embed_measure_$($Tag).log"
$srv = Start-Process -FilePath $py -ArgumentList "tools\embed_server.py", "--port", "8789", "--model", $ModelId -PassThru -WindowStyle Hidden -RedirectStandardOutput $out -RedirectStandardError "$out.err"

# health (tempo limite generoso)
$health = $false
for ($i = 0; $i -lt 12; $i++) {
    Start-Sleep -Seconds 10
    try {
        $h = Invoke-WebRequest -Uri "http://127.0.0.1:8789/health" -UseBasicParsing -TimeoutSec 10
        $health = $true
        Write-Output "health: $($h.Content.Substring(0, 120))"
        break
    } catch { }
}
if (-not $health) {
    Write-Output "falhou no startup"
    Get-Content "$out.err" -Tail 8 -ErrorAction SilentlyContinue
    if ($srv -and !$srv.HasExited) { Stop-Process -Id $srv.Id -Force }
    exit 1
}

# medição (2 doc baseline, timeout alto)
$env:AETHER_EVAL_SET = "$Root\data\embed_eval_set_v2.jsonl"
$env:AETHER_EMBED_URL = "http://127.0.0.1:8789"
$env:AETHER_EVAL_TIMEOUT_S = "$TimeoutS"

$pyE = "$Root\.vega\venv\Scripts\python.exe"
Write-Output "=== eval $Tag ($ModelId) ==="
$timed = $TimeoutS
& $pyE tools\embed_eval.py --run --out target\embed_eval_$Tag.json --sha256 c422a1a00fc18c9335b437f507f8bf250f8df2a1689f3ef2f2259c7fad0f3397
$evalExit = $LASTEXITCODE

if ($evalExit -ne 0) {
    Write-Output "eval falhou (exit $evalExit)"
}

# extraío o placar
$art = "target\embed_eval_$Tag.json"
if (Test-Path $art) {
    & $pyE -c "import json,sys; d=json.load(open(sys.argv[1], encoding='utf-8')); s=d['systems']; print('PLACAR', sys.argv[1]); print('  bm25        ', s['lexical_bm25']['hit5']); print('  embed       ', s['embed_minilm']['hit5'], '| dim', d['dim'], '| latency', d['latency'])" $art
} else {
    Write-Output "sem artefato $art"
}

# derruba o servidor (soquete limpo para a próxima medição)
if ($srv -and !$srv.HasExited) { Stop-Process -Id $srv.Id -Force }
exit $evalExit
