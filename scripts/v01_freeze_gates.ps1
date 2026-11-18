# Subconjunto G0 lab do freeze v0.1 (docs/V01_DELIVERY_PLAN.md).
# Não substitui a suite completa; gera target/v01_freeze_gates.json.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$bin = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }

New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
$out = Join-Path $Root "target\v01_freeze_gates.json"
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$steps = @()

function Step([string]$Name, [scriptblock]$Body) {
    $t0 = [System.Diagnostics.Stopwatch]::StartNew()
    & $Body
    $code = $LASTEXITCODE
    $t0.Stop()
    $script:steps += [ordered]@{
        name       = $Name
        ok         = ($code -eq 0)
        exit       = $code
        elapsed_ms = $t0.ElapsedMilliseconds
    }
    if ($code -ne 0) { throw "falhou: $Name exit=$code" }
}

try {
    Step "fmt_check" { & "$PSScriptRoot\cargo.ps1" fmt -- --check; exit $LASTEXITCODE }
    Step "check_lib" { & "$PSScriptRoot\cargo.ps1" check --lib; exit $LASTEXITCODE }
    Step "usability_gate" {
        & "$PSScriptRoot\cargo.ps1" test --release --features agent --test usability_gate
        exit $LASTEXITCODE
    }
    Step "language_port_gate" {
        & "$PSScriptRoot\cargo.ps1" test --release --features agent --test language_port_gate
        exit $LASTEXITCODE
    }
    $ok = $true
    $err = $null
}
catch {
    $ok = $false
    $err = "$_"
}
$sw.Stop()

$result = [ordered]@{
    version    = "v01_freeze_gates"
    ok         = $ok
    elapsed_ms = $sw.ElapsedMilliseconds
    steps      = $steps
    error      = $err
    note       = "subset_G0_lab_not_full_G0-G9"
    created    = (Get-Date).ToUniversalTime().ToString("o")
}
($result | ConvertTo-Json -Depth 6) | Set-Content -Path $out -Encoding utf8
Write-Host "artefato: $out ok=$ok" -ForegroundColor $(if ($ok) { "Green" } else { "Yellow" })
if (-not $ok) { exit 2 }
exit 0
