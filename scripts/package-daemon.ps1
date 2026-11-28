# Empacota aether-daemon.exe para rodar sem cargo (Produto C / Onda 1.4).
# Uso, na raiz do repo:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-daemon.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

& "$PSScriptRoot\cargo.ps1" build --profile release-unwind --bin aether-daemon
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $Root "target\release-unwind\aether-daemon.exe"
if (-not (Test-Path $exe)) { throw "binário ausente: $exe" }

$out = Join-Path $Root "target\aether-daemon-win"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out | Out-Null
Copy-Item $exe (Join-Path $out "aether-daemon.exe")

# Runtime MinGW comum (windows-gnu); ignora se ausente.
$mingw = "C:\Users\msrov\Tools\mingw64\mingw64\bin"
foreach ($dll in @("libgcc_s_seh-1.dll", "libstdc++-6.dll", "libwinpthread-1.dll")) {
    $src = Join-Path $mingw $dll
    if (Test-Path $src) {
        Copy-Item $src (Join-Path $out $dll) -Force
    }
}

$ver = (& $exe --help 2>&1 | Select-Object -First 1) -replace '.*v', ''
@"
AETHER Produto C — escudo de host (Windows)
Versão: $ver

1. Abra um terminal nesta pasta.
2. Rode:  .\aether-daemon.exe --duration 15s --human
   (--human já liga telemetria do SO no Windows)
3. A linha mostra CALMO, ALERTA ou SONHANDO + frase curta.
   S = surpresa do organismo · R = coerência com peers na LAN.
4. Opcional:
   .\aether-daemon.exe --duration 30s --human --log host.csv
   .\aether-daemon.exe --duration 10s --human --alert 0.25

Duas máquinas na mesma LAN (lab): rode o exe em ambas; peers e R sobem.
Multicast 239.255.42.99:7777 — lab only, sem auth.

BAM / JIT / governor / chatbot NÃO entram neste pacote.
Docs: docs/PRODUTO_C.md no repositório AETHER.
"@ | Set-Content -Encoding utf8 (Join-Path $out "LEIA-ME.txt")

$zip = Join-Path $Root "target\aether-daemon-win.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $out "*") -DestinationPath $zip
Write-Host "pacote: $zip"
Get-ChildItem $out | Select-Object Name, Length | Format-Table -AutoSize
