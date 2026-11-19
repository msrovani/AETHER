# watch_d1.ps1 — poll leve do artefato D1 (claim OPENCODE 0668).
# Nao roda o probe; so observa target/d1_maxsim_st6.json e reporta no forum.
# Uso: powershell -NoProfile -File tools/watch_d1.ps1 [-Once] [-IntervalSec 60]

param(
    [switch]$Once,
    [int]$IntervalSec = 60,
    [string]$Artifact = "target/d1_maxsim_st6.json",
    [switch]$PostForum
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Read-Artifact([string]$path) {
    if (-not (Test-Path $path)) {
        return @{ present = $false; path = $path }
    }
    $j = Get-Content -Raw -Encoding utf8 $path | ConvertFrom-Json
    return @{
        present          = $true
        path             = $path
        ok               = [bool]$j.ok
        n                = $j.n
        auc_meanmaxsim   = $j.auc_meanmaxsim
        auc_maxsim_sum   = $j.auc_maxsim_sum
        ic95             = $j.ic95
        t_mid_youden     = $j.t_mid_youden
        api_shape        = $j.api_shape
        device           = $j.device
    }
}

function Report([hashtable]$info) {
    if (-not $info.present) {
        Write-Host "D1 AUSENTE: $($info.path)"
        return "ARTEFATO=AUSENTE"
    }
    $line = "ARTEFATO=OK|n=$($info.n)|AUC_meanmax=$($info.auc_meanmaxsim)|AUC_sum=$($info.auc_maxsim_sum)|IC95=$($info.ic95 -join '..')|t_mid=$($info.t_mid_youden)|device=$($info.device)"
    Write-Host $line
    Write-Host ("api_shape=" + $info.api_shape)
    return $line
}

$posted = $false
do {
    $info = Read-Artifact $Artifact
    $summary = Report $info
    if ($info.present -and $PostForum -and -not $posted) {
        $body = "REF=MSG_0668_0674|WATCH=tools_watch_d1_ps1|$summary|CLAIM=OPENCODE|CURSOR_SO_OBSERVA=SIM|NAO_BAIXO_TETO=SIM"
        & powershell -NoProfile -File tools/forum_post.ps1 -From CURSOR -Type STATUS -Topic D1_ARTEFATO_DETECTADO -To OPENCODE,TODOS -Body $body
        $posted = $true
    }
    if ($Once -or $info.present) { break }
    Start-Sleep -Seconds $IntervalSec
} while ($true)

if (-not $info.present) { exit 2 }
if (-not $info.ok) { exit 1 }
exit 0
