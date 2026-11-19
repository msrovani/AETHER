# measure_window.ps1 — sequenciador de medicao COM VERIFICACAO DE TAXA (revisao 2026-11-18).
#
# Substitui o inline que falhou no mesmo dia: o Windows dormiu 13:56:08-20:37:17
# (Kernel-Power 506/507) no meio do R5, o encode_s do ONNX virou 24192 s de relogio
# de parede com sono embutido, e o artefato publicou "contended: false" com taxa
# invalida (torch 1.57 / onnx 0.01 txt/s vs baseline limpo 10.82).
#
# Garantias por ETAPA (nao so no inicio da sequencia):
#   1. janela quieta de 30s ANTES de cada etapa (cargo/rustc/rust-analyzer/python estranhos);
#   2. keep-awake (SetThreadExecutionState) durante a medicao — impede sono;
#   3. monitor de contencao DURANTE a medicao (amostra a cada 10s, job proprio);
#   4. deteccao de sono/wake (Kernel-Power 506/507) desde o inicio da etapa;
#   5. teto de parede por etapa (3x a estimativa limpa) — mata passo pendurado;
#   6. piso de taxa: torch >= 6.5 txt/s (60% do baseline A1 10.82) em R5 e rate1k;
#   7. artefato precisa ser FRESCO (mtime >= inicio da etapa) — nunca aceita lixo antigo;
#   8. retry ate 3x com backoff de 60s; veredito por tentativa em
#      target/pos_reboot/measure_window_verdict.json (incremental).
#
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_window.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure_window.ps1 -Only R3_rate1k

param(
  [string]$Facts = "data\rounds\20261117_b1\facts.jsonl",
  [int]$MaxAttempts = 3,
  [int]$QuietSeconds = 30,
  [int]$WaitQuietTimeoutMin = 45,
  [double]$RateFloorTxtS = 6.5,   # 60% do baseline A1 limpo (10.82 txt/s, cpu fp32 batch16 thr8)
  [string]$Only = ""              # R5_n128 | R3_rate1k | R3_sweep (vazio = todos, na ordem)
)

$ErrorActionPreference = 'Continue'
$Root = Split-Path $PSScriptRoot -Parent
Set-Location $Root

$Py = "target\py-embed\Scripts\python.exe"
$VerdictPath = "target\pos_reboot\measure_window_verdict.json"
$LogDir = "target\pos_reboot\logs"
# Processos MEUS durante a medicao (excluidos do monitor de contencao).
$MyPattern = 'onnx_backend_probe|embed_rate_1k|embed_threads_batch_sweep|measure_window'
$ForeignFilter = "Name like 'cargo%' or Name like 'rustc%' or Name like 'rust-analyzer%' or Name like 'python%'"
$BaselineClean = 10.82

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

# ---------------------------------------------------------------- keep-awake
$script:HaveESA = $false
if (-not ('AetherWin.Power' -as [type])) {
  try {
    Add-Type -Namespace AetherWin -Name Power -MemberDefinition `
      '[DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint esFlags);' `
      -ErrorAction Stop
    $script:HaveESA = $true
  } catch {
    $script:HaveESA = $false
  }
} else {
  $script:HaveESA = $true
}
function Keep-Awake([bool]$on) {
  if (-not $script:HaveESA) { return }
  if ($on) { [void][AetherWin.Power]::SetThreadExecutionState(0x80000002) } # CONTINUOUS|SYSTEM_REQUIRED
  else     { [void][AetherWin.Power]::SetThreadExecutionState(0x80000000) } # CONTINUOUS (libera)
}

# ---------------------------------------------------------------- utilidades
function Get-Foreign {
  $procs = @(Get-CimInstance Win32_Process -Filter $ForeignFilter)
  return @($procs | Where-Object {
    $null -eq $_.CommandLine -or $_.CommandLine -notmatch $MyPattern
  })
}

function Wait-Quiet {
  param([int]$Seconds, [int]$TimeoutMin)
  $t0 = Get-Date
  $need = [Math]::Max(1, [int]($Seconds / 15))
  $quiet = 0
  while ($quiet -lt $need) {
    if (((Get-Date) - $t0).TotalMinutes -gt $TimeoutMin) { return $false }
    if ((Get-Foreign).Count -eq 0) { $quiet++ } else { $quiet = 0 }
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
      $foreign = @($procs | Where-Object {
        $null -eq $_.CommandLine -or $_.CommandLine -notmatch $Pat
      })
      $cmds = @($foreign | ForEach-Object {
        if ($_.CommandLine) { $_.CommandLine.Substring(0, [Math]::Min(90, $_.CommandLine.Length)) } else { '?' }
      })
      $samples += [pscustomobject]@{
        t    = (Get-Date).ToString('HH:mm:ss')
        n    = $foreign.Count
        cmds = ($cmds -join ' | ')
      }
      Start-Sleep -Seconds 10
    }
  } -ArgumentList $Pattern, $Filter
}

function Write-Verdict {
  param($V)
  $V | ConvertTo-Json -Depth 8 | Set-Content -Path $VerdictPath -Encoding UTF8
}

# Extrai taxa do artefato: texts_per_s pode ser escalar ou objeto {torch,onnx}.
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

# ---------------------------------------------------------------- etapa
function Invoke-Step {
  param(
    [string]$Name,
    [string[]]$Cmd,
    [int]$WallBudgetS,
    [string]$Artifact,
    [string]$RateKey = '',      # '' = sem piso de taxa (ex.: sweep)
    [double]$RateFloor = 0
  )
  $result = [pscustomobject]@{
    step = $Name; attempts = @(); verdict = 'PENDING'
    artifact = $Artifact; wall_budget_s = $WallBudgetS
    rate_floor = $RateFloor; started = (Get-Date).ToString('o')
  }

  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    $att = [pscustomobject]@{ n = $attempt; t0 = $null; t1 = $null; wall_s = $null
      slept = $null; foreign_seen = $null; foreign_cmds = $null
      artifact_fresh = $null; rate = $null; rate_ok = $null; exit = $null; verdict = $null }

    Write-Host "[$Name] tentativa ${attempt}: esperando janela quieta (${QuietSeconds}s)..."
    if (-not (Wait-Quiet -Seconds $QuietSeconds -TimeoutMin $WaitQuietTimeoutMin)) {
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

    $exe = $Cmd[0]
    $rest = @($Cmd | Select-Object -Skip 1)
    & $exe @rest *> $log
    $att.exit = $LASTEXITCODE

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

    if ($clean) { $att.verdict = 'CLEAN' } else {
      $why = @()
      if ($att.slept) { $why += 'SLEEP' }
      if (-not $wallOk) { $why += 'WALL_BUDGET' }
      if ($att.foreign_seen) { $why += 'CONTENDED' }
      if (-not $fresh) { $why += 'ARTIFACT_STALE' }
      if (($RateFloor -gt 0) -and (-not $att.rate_ok)) { $why += 'RATE_FLOOR' }
      $att.verdict = 'INVALID:' + ($why -join '+')
    }

    $result.attempts += $att
    Write-Verdict $script:Verdict
    Write-Host "[$Name] tentativa $attempt -> $($att.verdict) (wall=$($att.wall_s)s rate=$($att.rate))"

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
  @{ Name = 'R5_n128'; Budget = 600; Artifact = 'target/pos_reboot/onnx_backend_probe.json'
     RateKey = 'torch'; Floor = $RateFloorTxtS
     Cmd = @($Py, 'scripts\onnx_backend_probe.py', '--facts', $Facts, '--n', '128',
             '--out', 'target\pos_reboot\onnx_backend_probe.json') },
  @{ Name = 'R3_rate1k'; Budget = 600; Artifact = 'target/pos_reboot/embed_rate_1k_cpu_pos_reboot.json'
     RateKey = ''; Floor = $RateFloorTxtS
     Cmd = @($Py, 'scripts\embed_rate_1k.py', '--facts', $Facts, '--limit', '1000',
             '--device', 'cpu', '--dtype', 'fp32', '--batch', '16', '--seed', '42',
             '--out', 'target\pos_reboot\embed_rate_1k_cpu_pos_reboot.json') },
  @{ Name = 'R3_sweep'; Budget = 600; Artifact = 'target/pos_reboot/threads_batch_sweep.json'
     RateKey = ''; Floor = 0
     Cmd = @($Py, 'scripts\embed_threads_batch_sweep.py', '--facts', $Facts, '--limit', '256',
             '--threads', '8,10,16', '--batches', '16,32', '--seed', '42',
             '--out', 'target\pos_reboot\threads_batch_sweep.json') }
)

$script:Verdict = [pscustomobject]@{
  started = (Get-Date).ToString('o')
  baseline_clean_txt_s = $BaselineClean
  rate_floor_txt_s = $RateFloorTxtS
  keep_awake_available = $script:HaveESA
  quiet_seconds = $QuietSeconds
  max_attempts = $MaxAttempts
  steps = @()
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
