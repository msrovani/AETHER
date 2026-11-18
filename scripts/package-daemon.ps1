# Empacota aether-daemon.exe para rodar sem cargo (Onda P / 1.4).
# Uso, na raiz do repo:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-daemon.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

& "$PSScriptRoot\cargo.ps1" build --release --bin aether-daemon
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $Root "target\release\aether-daemon.exe"
if (-not (Test-Path $exe)) { throw "binário ausente: $exe" }

$out = Join-Path $Root "target\aether-daemon-win"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out | Out-Null
Copy-Item $exe (Join-Path $out "aether-daemon.exe")

@'
AETHER daemon (Windows)

1. Abra um terminal nesta pasta.
2. Rode: .\aether-daemon.exe --duration 10s --human
3. A linha mostra CALMO, ALERTA ou SONHANDO.
4. Pare com Ctrl+C se a duração não acabar sozinha.

Se o Windows pedir uma DLL do MinGW, o runtime está em
C:\Users\msrov\Tools\mingw64\mingw64\bin — copie as DLLs pedidas para esta pasta.
BAM, JIT e governor não entram neste pacote.
'@ | Set-Content -Encoding utf8 (Join-Path $out "LEIA-ME.txt")

$zip = Join-Path $Root "target\aether-daemon-win.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $out "*") -DestinationPath $zip
Write-Host "pacote: $zip"
