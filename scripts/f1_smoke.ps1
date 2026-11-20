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
$venv = Join-Path $root "target\vega"

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
