# Duas instâncias do daemon na LAN de lab (Onda P / 1.6).
# Multicast sem auth: isto é lab, não produto aberto.
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\two-instances.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
$exe = Join-Path $Root "target\release\aether-daemon.exe"
if (-not (Test-Path $exe)) {
    & "$PSScriptRoot\cargo.ps1" build --release --bin aether-daemon
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$a = Join-Path $Root "target\daemon-a.log"
$b = Join-Path $Root "target\daemon-b.log"
$p1 = Start-Process -FilePath $exe -ArgumentList "--duration","8s","--seed","1" -RedirectStandardOutput $a -PassThru -WindowStyle Hidden
Start-Sleep -Milliseconds 400
$p2 = Start-Process -FilePath $exe -ArgumentList "--duration","8s","--seed","2" -RedirectStandardOutput $b -PassThru -WindowStyle Hidden
$p1.WaitForExit()
$p2.WaitForExit()

$text = (Get-Content $a -Raw) + (Get-Content $b -Raw)
$peers = [regex]::Matches($text, "peers=(\d+)") | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Maximum
$r = [regex]::Matches($text, "R\(LAN\)=([0-9.]+)") | ForEach-Object { [double]$_.Groups[1].Value } | Measure-Object -Maximum
$peersMax = if ($null -eq $peers.Maximum) { 0 } else { $peers.Maximum }
$rMax = if ($null -eq $r.Maximum) { 0 } else { $r.Maximum }
Write-Host "peers_heard max=$peersMax  R max=$rMax"
if ($peersMax -ge 1 -and $rMax -gt 0.9) {
    Write-Host "E2E duas instâncias: ok"
    exit 0
}
Write-Host "E2E duas instâncias: abaixo do piso (firewall ou multicast local)"
exit 1
