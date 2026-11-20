# Rebuilds the vega engine environment OUTSIDE target/ and verifies the pins.
#
# WHY THIS EXISTS
#   The engine (python env + checkpoints) lived in target/vega (4.697 MB) and
#   `cargo clean` deletes target/ -- it already happened once (forum MSG 0748:
#   "target_py_embed FOI APAGADO por cargo_clean do ciclo"). The plan (F1)
#   requires "torch e python pinados no lockfile"; there was no env lockfile.
#
#   This script reads data/vega_env.lock.json (the measured environment) and
#   materialises it in .vega/  (venv + checkpoints), then verifies the pins.
#
# USAGE
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/vega_env_setup.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/vega_env_setup.ps1 -VerifyOnly
#
# COST
#   A fresh install pulls torch 2.14.1+cu126 (~2.5 GB) + transformers + triton.
#   With -SkipTorch the existing wheel cache is reused.
#
# EXIT CODES
#   0 = environment ready and every pin verified
#   3 = honest failure (missing python, unreachable index, pin mismatch)

param(
    [string]$Root = "",
    [string]$Lock = "data/vega_env.lock.json",
    [string]$Venv = ".vega/venv",
    [string]$CheckpointDir = ".vega/models",
    [switch]$VerifyOnly,
    [switch]$SkipTorch
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Root)) {
    $Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
}
Set-Location $Root

function Say([string]$m) { Write-Host $m }

if (-not (Test-Path $Lock)) {
    Say "[vega_env] lockfile ausente: $Lock"
    exit 3
}
$cfg = Get-Content $Lock -Raw -Encoding UTF8 | ConvertFrom-Json

# ---------------------------------------------------------------- interpreter
$basePython = $null
foreach ($cand in @("python", "py -3.14")) {
    try {
        $ver = (& $cand --version 2>&1) -join ""
        if ($ver -match "Python 3\.14") { $basePython = $cand; break }
    } catch { }
}
if (-not $basePython) {
    Say "[vega_env] python 3.14 nao encontrado no PATH (o lock exige $($cfg.python.pin))"
    exit 3
}

$venvPy = Join-Path $Root (Join-Path $Venv "Scripts/python.exe")
if (-not $Venv.StartsWith("target") -and $Venv -match "(^|[\\/])target([\\/]|$)") {
    Say "[vega_env] RECUSA: o venv nao pode morar em target/ (cargo clean apaga)"
    exit 3
}

# ------------------------------------------------------------------ build venv
if (-not $VerifyOnly) {
    if (-not (Test-Path $venvPy)) {
        Say "[vega_env] criando venv em $Venv (fora de target/)"
        & $basePython -m venv $Venv
    } else {
        Say "[vega_env] venv ja existe: $Venv"
    }
    $pip = Join-Path $Root (Join-Path $Venv "Scripts/pip.exe")
    Say "[vega_env] instalando os pins do lockfile"
    if (-not $SkipTorch) {
        & $pip install --index-url $cfg.torch.index "torch==$($cfg.torch.version)"
    } else {
        Say "[vega_env] -SkipTorch:pulando o torch"
    }
    $req = @()
    foreach ($p in $cfg.packages.PSObject.Properties) {
        if ($p.Name -eq "torch") { continue }
        $req += "$($p.Name)==$($p.Value)"
    }
    $reqFile = Join-Path $env:TEMP "vega_env_requirements.txt"
    Set-Content -Path $reqFile -Value $req -Encoding UTF8
    & $pip install -r $reqFile
}

# ------------------------------------------------------------- checkpoints
$src = Join-Path $Root "target/vega"
$dst = Join-Path $Root $CheckpointDir
if (-not $VerifyOnly) {
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    foreach ($name in @("vega-u-gemma-1b", "vega-u-gemma-1b-prose")) {
        $from = Join-Path $src $name
        $to = Join-Path $dst $name
        if ((Test-Path $from) -and -not (Test-Path $to)) {
            Say "[vega_env] copiando checkpoint $name para $CheckpointDir"
            Copy-Item -Recurse -Force $from $to
        } elseif (Test-Path $to) {
            Say "[vega_env] checkpoint ja presente: $name"
        } else {
            Say "[vega_env] checkpoint ausente (baixe do hub): $name"
        }
    }
}

# ------------------------------------------------------------------ verify
$fail = 0
$pyVer = (& $venvPy -c "import sys;print('.'.join(map(str,sys.version_info[:3])))" 2>&1) -join ""
if ($pyVer -ne $cfg.python.version) {
    Say "[vega_env] python do venv=$pyVer esperado=$($cfg.python.version)"
    $fail = 1
}
$pkgVer = (& $venvPy -c "import vegaml;print(vegaml.__version__)" 2>&1) -join ""
if ($pkgVer -ne $cfg.packages.vegaml) {
    Say "[vega_env] vegaml=$pkgVer esperado=$($cfg.packages.vegaml)"
    $fail = 1
}
$torchVer = (& $venvPy -c "import torch;print(torch.__version__)" 2>&1) -join ""
if ($torchVer -ne $cfg.torch.version) {
    Say "[vega_env] torch=$torchVer esperado=$($cfg.torch.version)"
    $fail = 1
}
$cuda = (& $venvPy -c "import torch;print(torch.cuda.is_available())" 2>&1) -join ""
Say "[vega_env] verificacao: python=$pyVer vegaml=$pkgVer torch=$torchVer cuda=$cuda"

if ($fail -ne 0) {
    Say "[vega_env] FALHA: pins nao conferem"
    exit 3
}

Say ""
Say "[vega_env] ambiente pronto. Exporte antes de construir o crate com a feature pyo3:"
Say "  `$env:PYO3_PYTHON = `"$(Resolve-Path (Join-Path $Root (Join-Path $Venv 'Scripts/python.exe')))`""
Say "  `$env:PYTHONPATH = `"$(Resolve-Path (Join-Path $Root (Join-Path $Venv 'Lib/site-packages')))`""
Say "  `$env:VEGA_MODEL = `"$(Join-Path $Root (Join-Path $CheckpointDir 'vega-u-gemma-1b'))`""
Say "  `$env:VEGA_PROSE_MODEL = `"$(Join-Path $Root (Join-Path $CheckpointDir 'vega-u-gemma-1b-prose'))`""
exit 0
