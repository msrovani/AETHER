# measure_window.ps1 — sequenciador de medicao COM VERIFICACAO DE TAXA (v2, 2026-11-18).
#
# Historico: v1 (inline + este arquivo) falhou por 4 bugs de plataforma e por um
# evento externo:
#   - Windows dormiu 13:56:08-20:37:17 (Kernel-Power 506/507) no meio do R5 v0:
#     encode_s do ONNX virou 24192 s de relogio de parede com sono embutido e o
#     artefato publicou "contended: false" com taxa invalida (errata MSG 0696);
#   - v1: 0x80000002 vira Int32 negativo no PS5.1 (SetThreadExecutionState falha)
#     -> usar decimal [uint32]2147483650/2147483648;
#   - v1: logs/verdict em dirs relativos sem criar no ponto de uso;
#   - v1: PSCustomObject sem a propriedade 'finished' nao aceita atribuicao depois;
#   - v1: target/ (venv py-embed + pos_reboot) foi apagado por cargo clean de
#     terceiros ADOCOES -> venv canonico agora e .venv-py (MSG 0745, fora de target);
#     paths ABSOLUTOS em tudo, preflight que aborta se o python faltar.
#
# Garantias por ETAPA (nao so no inicio):
#   1. janela quieta de 30s ANTES de cada etapa;
#   2. keep-awake durante a medicao (impede sono);
#   3. monitor de contencao DURANTE (job, amostra 10s);
#   4. deteccao de sono/wake (Kernel-Power 506/507) desde o inicio da etapa;
#   5. teto de parede por etapa (estimativa limpa x3);
#   6. piso de taxa: torch >= 6.5 txt/s (60% do baseline A1 10.82) em R5 e rate1k;
#   7. artefato FRESCO (mtime >= inicio da etapa);
#   8. retry 3x com backoff 60s; veredito incremental em
#      target/pos_reboot/measure_window_verdict.json (+ tail do log por falha).
#
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_window.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_window.ps1 -SelfTest
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_window.ps1 -Only R3_rate1k

param(
  [string]$Facts = "data\rounds\20261117_b1\facts.jsonl",
  [int]$MaxAttempts = 3,
  [int]$QuietSeconds = 30,
  [int]$WaitQuietTimeoutMin = 60,
  [double]$RateFloorTxtS = 6.5,   # 60% do baseline A1 limpo (10.82 txt/s cpu fp32 batch16 thr8)
  [string]$Only = "",             # R5_n128 | R3_rate1k | R3_sweep (vazio = todos, na ordem)
  [switch]$SelfTest               # valida a canalizacao SEM janela quieta (passo fake)
)

$ErrorActionPreference = 'Continue'
$Root = if ($PSScriptRoot) { Split-Path $PSScriptRoot -Parent } else { (Get-Location).Path }
Set-Location $Root

# ---- paths ABSOLUTOS (imunes a CWD e a limpeza relativa de terceiros) --------
$Py          = Join-Path $Root '.venv-py\Scripts\python.exe'
$PosReboot   = Join-Path $Root 'target\pos_reboot'
$LogDir      = Join-Path $PosReboot 'logs'
$VerdictPath = Join-Path $PosReboot 'measure_window_verdict.json'
$FactsPath   = if ([System.IO.Path]::IsPathRooted($Facts)) { $Facts } else { Join-Path $Root $Facts }
$MyPattern   = 'onnx_backend_probe|embed_rate_1k|embed_threads_batch_sweep|measure_window'
$ForeignFilter = "Name like 'cargo%' or Name like 'rustc%' or Name like 'rust-analyzer%' or Name like 'python%'"
$BaselineClean = 10.82

# ---- preflight: aborta CLARO se o ambiente nao existe -----------------------
if (-not (Test-Path $Py)) {
  Write-Host "PREFLIGHT_FAIL: python canonico ausente: $Py (venv .venv-py — ver MSG 0745)"
  exit 1
}
if (-not (Test-Path $FactsPath)) {
  Write-Host "PREFLIGHT_FAIL: facts ausente: $FactsPath"
  exit 1
}
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null   # cria target\pos_reboot tambem

# ---------------------------------------------------------------- keep-awake
$script:HaveESA = $false
if (-not ('AetherWin.Power' -as [type])) {
  try {
    Add-Type -Namespace AetherWin -Name Power -MemberDefinition `
      '[DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint esFlags);' `
      -ErrorAction Stop
    $script:HaveESA = $true
  } catch { $script:HaveESA = $false }
} else { $script:HaveESA = $true }

function Keep-Awake([bool]$on) {
  if (-not $script:HaveESA) { return }
  try {
    # PS5.1: literais 0x800000xx viram Int32 negado -> usar decimal UInt32.
    if ($on) { [void][AetherWin.Power]::SetThreadExecutionState([uint32]2147483650) } # CONTINUOUS|SYSTEM_REQUIRED
    else     { [void][AetherWin.Power]::SetThreadExecutionState([uint32]2147483648) } # CONTINUOUS
  } catch { }
}

# ---------------------------------------------------------------- utilidades
function Get-Foreign {
  $procs = @(Get-CimInstance Win32_Process -Filter $ForeignFilter)
  return @($procs | Where-Object { $null -eq $_.CommandLine -or $_.CommandLine -notmatch $MyPattern })
}

function Wait-Quiet {
  param([int]$Seconds, [int]$TimeoutMin)
  $t0 = Get-Date
  $need = [Math]::Max(1, [int]($Seconds / 15))
  $quiet = 0
  $lastPrint = Get-Date
  while ($quiet -lt $need) {
    if (((Get-Date) - $t0).TotalMinutes -gt $TimeoutMin) {
      Write-Host "  janela NAO abriu em ${TimeoutMin}min (foreign=$((Get-Foreign).Count))"
      return $false
    }
    $n = (Get-Foreign).Count
    if ($n -eq 0) { $quiet++ } else { $quiet = 0 }
    if (((Get-Date) - $lastPrint).TotalSeconds -ge 60) {
      Write-Host "  aguardando janela: foreign=$n quiet=$quiet/$need t=$((Get-Date).ToString('HH:mm:ss'))"
      $lastPrint = Get-Date
    }
    if ($quiet -lt $need) { Start-Sleep -Seconds 15 }
  }
  return $true
}

function Test-SleepSince {
  param([datetime]$Since)
  try {
    $null = Get-WinEvent -FilterHashtable @{
      LogName = 'System'
      ProviderName = 'Microsoft-Windows-Kernel-Power'
      Id = 506, 507
      StartTime = $Since
    } -MaxEvents 1 -ErrorAction Stop
    return $true
  } catch { return $false }
}

function Start-ContentionMonitor {
  param([string]$Pattern, [string]$Filter)
  Start-Job -ScriptBlock {
    param($Pat, $Filt)
    $samples = @()
    while ($true) {
      $procs = @(Get-CimInstance Win32_Process -Filter $Filt)
      $foreign = @($procs | Where-Object { $null -eq $_.CommandLine -or $_.CommandLine -notmatch $Pat })
      $cmds = @($foreign | ForEach-Object {
        if ($_.CommandLine) { $_.CommandLine.Substring(0, [Math]::Min(90, $_.CommandLine.Length)) } else { '?' }
      })
      $samples += [pscustomobject]@{ t = (Get-Date).ToString('HH:mm:ss'); n = $foreign.Count; cmds = ($cmds -join ' | ') }
      Start-Sleep -Seconds 10
    }
  } -ArgumentList $Pattern, $Filter
}

function Write-Verdict {
  param($V)
  $dir = Split-Path $VerdictPath -Parent
  if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  $V | ConvertTo-Json -Depth 8 | Set-Content -Path $VerdictPath -Encoding UTF8
}

# texts_per_s pode ser escalar ou objeto {torch,onnx}.
function Get-RateFromArtifact {
  param([string]$Path, [string]$Key)
  try {
    $j = Get-Content $Path -Raw | ConvertFrom-Json
    $tps = $j.texts_per_s
    if ($null -eq $tps) { return $null }
    if ($tps -is [System.Management.Automation.PSCustomObject]) {
      if ($Key -and ($tps.PSObject.Properties.Name -contains $Key)) { return [double]$tps.$Key }
      if ($tps.PSObject.Properties.Name -contains 'torch') { return [double]$tps.torch }
      return $null
    }
    return [double]$tps
  } catch { return $null }
}

function Get-LogTail {
  param([string]$Path)
  if (Test-Path $Path) { return ((Get-Content $Path -Tail 3) -join ' ~ ') }
  return 'NO_LOG'
}

# ---------------------------------------------------------------- etapa
function Invoke-Step {
  param(
    [string]$Name,
    [string[]]$Cmd,
    [int]$WallBudgetS,
    [string]$Artifact,
    [string]$RateKey = '',
    [double]$RateFloor = 0
  )
  $result = [pscustomobject]@{
    step = $Name; attempts = @(); verdict = 'PENDING'
    artifact = $Artifact; wall_budget_s = $WallBudgetS
    rate_floor = $RateFloor; started = (Get-Date).ToString('o'); finished = $null
  }

  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    $att = [pscustomobject]@{
      n = $attempt; t0 = $null; t1 = $null; wall_s = $null
      slept = $null; foreign_seen = $null; foreign_cmds = $null
      artifact_fresh = $null; rate = $null; rate_ok = $null
      exit = $null; verdict = $null; log_tail = $null
    }

    if ($SelfTest) {
      Write-Host "[$Name] SELFTEST tentativa ${attempt}: pulando janela quieta"
      $quietOk = $true
    } else {
      Write-Host "[$Name] tentativa ${attempt}: esperando janela quieta (${QuietSeconds}s)..."
      $quietOk = Wait-Quiet -Seconds $QuietSeconds -TimeoutMin $WaitQuietTimeoutMin
    }
    if (-not $quietOk) {
      $att.verdict = 'WINDOW_TIMEOUT'
      $result.attempts += $att
      Write-Verdict $script:Verdict
      continue
    }

    $t0 = Get-Date
    $att.t0 = $t0.ToString('HH:mm:ss')
    Keep-Awake $true
    $mon = Start-ContentionMonitor -Pattern $MyPattern -Filter $ForeignFilter
    $log = Join-Path $LogDir "${Name}_a$attempt.log"
    if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Force -Path $LogDir | Out-Null }
    if ($SelfTest -and (Test-Path $Artifact)) { Remove-Item $Artifact -Force }  # fresh check honesto

    $exe = $Cmd[0]
    $rest = @($Cmd | Select-Object -Skip 1)
    try {
      & $exe @rest *> $log
      $att.exit = $LASTEXITCODE
    } catch {
      $att.exit = 'THROW'
      $att.log_tail = $_.Exception.Message
    }

    $t1 = Get-Date
    Keep-Awake $false
    Stop-Job $mon -ErrorAction SilentlyContinue
    $samples = @(Receive-Job $mon -ErrorAction SilentlyContinue)
    Remove-Job $mon -Force -ErrorAction SilentlyContinue

    $att.t1 = $t1.ToString('HH:mm:ss')
    $att.wall_s = [Math]::Round(($t1 - $t0).TotalSeconds, 1)
    $att.slept = Test-SleepSince -Since $t0

    $foreign = @($samples | Where-Object { $_.n -gt 0 })
    $att.foreign_seen = ($foreign.Count -gt 0)
    $att.foreign_cmds = (($foreign | Select-Object -First 3 | ForEach-Object { "$($_.t) $($_.cmds)" }) -join ' ;; ')

    $fresh = (Test-Path $Artifact) -and ((Get-Item $Artifact).LastWriteTime -ge $t0.AddSeconds(-2))
    $att.artifact_fresh = $fresh

    if ($RateFloor -gt 0) {
      if ($fresh) { $att.rate = Get-RateFromArtifact -Path $Artifact -Key $RateKey }
      if ($null -ne $att.rate) { $att.rate_ok = ($att.rate -ge $RateFloor) }
      else { $att.rate_ok = $false }
    }

    $wallOk = $att.wall_s -le $WallBudgetS
    $clean = (-not $att.slept) -and $wallOk -and (-not $att.foreign_seen) -and $fresh
    if ($RateFloor -gt 0) { $clean = $clean -and $att.rate_ok }

    if ($clean) {
      $att.verdict = 'CLEAN'
    } else {
      $why = @()
      if ($att.slept) { $why += 'SLEEP' }
      if (-not $wallOk) { $why += 'WALL_BUDGET' }
      if ($att.foreign_seen) { $why += 'CONTENDED' }
      if (-not $fresh) { $why += 'ARTIFACT_STALE' }
      if (($RateFloor -gt 0) -and (-not $att.rate_ok)) { $why += 'RATE_FLOOR' }
      $att.verdict = 'INVALID:' + ($why -join '+')
      $att.log_tail = Get-LogTail $log
    }

    $result.attempts += $att
    Write-Verdict $script:Verdict
    Write-Host "[$Name] tentativa ${attempt} -> $($att.verdict) (wall=$($att.wall_s)s rate=$($att.rate) exit=$($att.exit))"

    if ($att.verdict -eq 'CLEAN') {
      $result.verdict = 'CLEAN'
      $result.finished = (Get-Date).ToString('o')
      return $result
    }
    if ($attempt -lt $MaxAttempts) { Start-Sleep -Seconds 60 }
  }

  $result.verdict = 'FAILED_ALL_ATTEMPTS'
  $result.finished = (Get-Date).ToString('o')
  return $result
}

# ---------------------------------------------------------------- definicao das etapas
$Steps = @(
  @{ Name = 'R5_n128'; Budget = 600; Artifact = (Join-Path $PosReboot 'onnx_backend_probe.json')
     RateKey = 'torch'; Floor = $RateFloorTxtS
     Cmd = @($Py, (Join-Path $Root 'scripts\onnx_backend_probe.py'), '--facts', $FactsPath, '--n', '128',
             '--out', (Join-Path $PosReboot 'onnx_backend_probe.json')) },
  @{ Name = 'R3_rate1k'; Budget = 600; Artifact = (Join-Path $PosReboot 'embed_rate_1k_cpu_pos_reboot.json')
     RateKey = ''; Floor = $RateFloorTxtS
     Cmd = @($Py, (Join-Path $Root 'scripts\embed_rate_1k.py'), '--facts', $FactsPath, '--limit', '1000',
             '--device', 'cpu', '--dtype', 'fp32', '--batch', '16', '--seed', '42',
             '--out', (Join-Path $PosReboot 'embed_rate_1k_cpu_pos_reboot.json')) },
  @{ Name = 'R3_sweep'; Budget = 600; Artifact = (Join-Path $PosReboot 'threads_batch_sweep.json')
     RateKey = ''; Floor = 0
     Cmd = @($Py, (Join-Path $Root 'scripts\embed_threads_batch_sweep.py'), '--facts', $FactsPath, '--limit', '256',
             '--threads', '8,10,16', '--batches', '16,32', '--seed', '42',
             '--out', (Join-Path $PosReboot 'threads_batch_sweep.json')) }
)

if ($SelfTest) {
  $fake = Join-Path $PosReboot '_selftest_artifact.json'
  $fakeCode = "import json,pathlib; p=pathlib.Path(r'$fake'); p.parent.mkdir(parents=True,exist_ok=True); p.write_text(json.dumps({'texts_per_s': 99.0, 'selftest': True}))"
  $Steps = @(
    @{ Name = 'SelfTest'; Budget = 60; Artifact = $fake
       RateKey = ''; Floor = 6.5
       Cmd = @($Py, '-c', $fakeCode) }
  )
}

$script:Verdict = [pscustomobject]@{
  started = (Get-Date).ToString('o')
  selftest = [bool]$SelfTest
  python = $Py
  baseline_clean_txt_s = $BaselineClean
  rate_floor_txt_s = $RateFloorTxtS
  keep_awake_available = $script:HaveESA
  quiet_seconds = $QuietSeconds
  max_attempts = $MaxAttempts
  steps = @()
  finished = $null
}
Write-Verdict $script:Verdict

foreach ($s in $Steps) {
  if ($Only -and ($s.Name -ne $Only)) { continue }
  $res = Invoke-Step -Name $s.Name -Cmd $s.Cmd -WallBudgetS $s.Budget `
    -Artifact $s.Artifact -RateKey $s.RateKey -RateFloor $s.Floor
  $script:Verdict.steps += $res
  Write-Verdict $script:Verdict
}

$script:Verdict.finished = (Get-Date).ToString('o')
Write-Verdict $script:Verdict

Write-Host "==== RESUMO ===="
foreach ($s in $script:Verdict.steps) {
  $last = $s.attempts | Select-Object -Last 1
  Write-Host ("{0}: {1} (wall={2}s rate={3} slept={4} contended={5})" -f
    $s.step, $s.verdict, $last.wall_s, $last.rate, $last.slept, $last.foreign_seen)
}
Write-Host "veredito: $VerdictPath"
if ($SelfTest) { Write-Host "SELFTEST_DONE" }
