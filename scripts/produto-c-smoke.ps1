# Smoke Produto C (Onda 1): --help, --human (CALMO|ALERTA|SONHANDO), zip opcional.
# Uso: powershell -NoProfile -File scripts\produto-c-smoke.ps1 [-Package]

param([switch]$Package)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$fail = 0
$steps = @()

function Step([string]$Name, [bool]$Ok, [string]$Detail) {
    $script:steps += @{ name = $Name; ok = $Ok; detail = $Detail }
    if (-not $Ok) { $script:fail++ }
    Write-Host ("{0} {1} {2}" -f ($(if ($Ok) { "PASS" } else { "FAIL" }), $Name, $Detail))
}

Write-Host "=== build aether-daemon ==="
& "$PSScriptRoot\cargo.ps1" build --release --bin aether-daemon
if ($LASTEXITCODE -ne 0) { throw "build daemon falhou" }
$exe = Join-Path $Root "target\release\aether-daemon.exe"

$help = & $exe --help 2>&1 | Out-String
Step "help" ($LASTEXITCODE -eq 0 -and $help -match "human") "exit=$LASTEXITCODE"

$out = & $exe --duration 5s --human 2>&1 | Out-String
Step "human_banner" ($out -match "Produto C") "glossario"
Step "human_states" ($out -match "CALMO|ALERTA|SONHANDO") "CALMO|ALERTA|SONHANDO"
Step "human_metrics" (($out -match "S=") -and ($out -match "R=")) "S= R="

$log = Join-Path $Root "target\produto_c_smoke.csv"
if (Test-Path $log) { Remove-Item $log -Force }
& $exe --duration 2s --human --log $log 2>&1 | Out-Null
$header = if (Test-Path $log) { Get-Content $log -TotalCount 1 } else { "" }
Step "log_header" ($header -eq "cycles,R_lan,peers,F,S") "got=$header"

& "$PSScriptRoot\two-instances.ps1"
Step "two_instances" ($LASTEXITCODE -eq 0) "peers/R"

if ($Package) {
    & "$PSScriptRoot\package-daemon.ps1"
    $zip = Join-Path $Root "target\aether-daemon-win.zip"
    Step "package_zip" ((Test-Path $zip) -and ((Get-Item $zip).Length -gt 1MB)) $zip
}

$report = @{
    ok         = ($fail -eq 0)
    fail_count = $fail
    steps      = $steps
    created    = (Get-Date).ToUniversalTime().ToString("o")
}
New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
($report | ConvertTo-Json -Depth 5) | Set-Content (Join-Path $Root "target\produto_c_smoke.json") -Encoding utf8
Write-Host ("RESUMO fail_count={0} -> target\produto_c_smoke.json" -f $fail)
if ($fail -gt 0) { exit 1 }
exit 0
