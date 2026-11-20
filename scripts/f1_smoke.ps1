# F1 smoke com o pin de runtime explicito.
#
# O pin tem duas metades:
#   - build-time: PYO3_PYTHON (qual Python o PyO3 linka);
#   - runtime:    PYTHONPATH / VEGA_SITE_PACKAGES (o que o embed enxerga).
# Sem a segunda o `import vegaml` falha e o port abste para sempre com
# `engine_unavailable` — erro que parece motor quebrado, mas e caminho.
#
# Uso:  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/f1_smoke.ps1
$root = Split-Path -Parent $PSScriptRoot
# Venv canonico = .vega/venv (fora de target/). `cargo clean` apaga target/ e
# levava o engine junto (forum MSG 0748/0892); o pin do ambiente vive em
# data/vega_env.lock.json e e materializado por tools/vega_env_setup.ps1.
$venv = Join-Path $root ".vega\venv"

if (-not (Test-Path (Join-Path $venv "Scripts\python.exe"))) {
    Write-Error "venv canonico ausente: $venv. Materialize com: powershell -NoProfile -ExecutionPolicy Bypass -File tools/vega_env_setup.ps1"
    exit 3
}

$env:PYO3_PYTHON = Join-Path $venv "Scripts\python.exe"
$env:VEGA_PYTHON = $env:PYO3_PYTHON
$env:VEGA_SITE_PACKAGES = Join-Path $venv "Lib\site-packages"
$env:PYTHONPATH = $env:VEGA_SITE_PACKAGES

Push-Location $root
try {
    & (Join-Path $PSScriptRoot "cargo.ps1") run -p aether_decision --features pyo3 --example f1_smoke @args
    $code = $LASTEXITCODE
} finally {
    Pop-Location
}
exit $code
