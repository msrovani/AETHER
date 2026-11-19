# Smoke Produto C (Onda 1): --help, --human (CALMO|ALERTA|SONHANDO), zip opcional.
# Uso: powershell -NoProfile -File scripts\produto-c-smoke.ps1 [-Package]

param([switch]$Package)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$fail = 0
$steps = New-Object System.Collections.Generic.List[object]

function Step([string]$Name, [bool]$Ok, [string]$Detail) {
    $script:steps.Add([ordered]@{ name = $Name; ok = $Ok; detail = $Detail }) | Out-Null
    if (-not $Ok) { $script:fail++ }
    Write-Host ("{0} {1} {2}" -f ($(if ($Ok) { "PASS" } else { "FAIL" }), $Name, $Detail))
}

$exe = Join-Path $Root "target\release\aether-daemon.exe"
if (-not (Test-Path $exe)) {
    & "$PSScriptRoot\cargo.ps1" build --release --bin aether-daemon
    if ($LASTEXITCODE -ne 0) { throw "build daemon falhou" }
}

$help = & $exe --help 2>&1 | Out-String
Step "help" ($LASTEXITCODE -eq 0 -and $help -match "human" -and $help -match "CALMO|Produto C|surpresa") "exit=$LASTEXITCODE"

$out = & $exe --duration 5s --human 2>&1 | Out-String
$hasBanner = $out -match "Produto C"
$hasState = $out -match "CALMO|ALERTA|SONHANDO"
$hasSR = $out -match "S=\d" -and $out -match "R="
Step "human_banner" $hasBanner "glossario"
Step "human_states" $hasState "CALMO|ALERTA|SONHANDO"
Step "human_metrics" $hasSR "S= R="

$log = Join-Path $Root "target\produto_c_smoke.csv"
if (Test-Path $log) { Remove-Item $log -Force }
& $exe --duration 2s --human --log $log 2>&1 | Out-Null
$logOk = (Test-Path $log) -and ((Get-Content $log -TotalCount 1) -eq "cycles,R_lan,peers,F,S")
Step "log_header" $logOk $log

& "$PSScriptRoot\two-instances.ps1"
Step "two_instances" ($LASTEXITCODE -eq 0) "peers/R"

if ($Package) {
    & "$PSScriptRoot\package-daemon.ps1"
    $zip = Join-Path $Root "target\aether-daemon-win.zip"
    Step "package_zip" ((Test-Path $zip) -and ((Get-Item $zip).Length -gt 1MB)) $zip
}

$report = [ordered]@{
    ok         = ($fail -eq 0)
    fail_count = $fail
    steps      = @($steps)
    created    = (Get-Date).ToUniversalTime().ToString("o")
}
New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
($report | ConvertTo-Json -Depth 5) | Set-Content (Join-Path $Root "target\produto_c_smoke.json") -Encoding utf8
Write-Host ("RESUMO fail_count={0} -> target\produto_c_smoke.json" -f $fail)
if ($fail -gt 0) { exit 1 }
exit 0
