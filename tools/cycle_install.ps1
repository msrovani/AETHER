#requires -Version 5.1
<#
  Instala/remove a tarefa agendada do ciclo AETHER (a cada 5 minutos).

    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 install
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 status
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 remove
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle_install.ps1 run
#>
[CmdletBinding()]
param(
    [ValidateSet('install', 'remove', 'status', 'run')]
    [string]$Action = 'install',
    [int]$Minutes = 5,
    [switch]$Network
)

$ErrorActionPreference = 'Stop'
$TaskName = 'AETHER-Cycle'
$CyclePs1 = (Resolve-Path (Join-Path $PSScriptRoot 'cycle.ps1')).Path
$PowerShellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'

switch ($Action) {
    'install' {
        $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$CyclePs1`""
        if ($Network) { $arguments += ' -Push -PrMerge' }
        $result = & schtasks.exe /Create /TN $TaskName /TR "`"$PowerShellExe`" $arguments" /SC MINUTE /MO $Minutes /RL LIMITED /F 2>&1
        $result
        if ($LASTEXITCODE -ne 0) { throw "schtasks falhou ($LASTEXITCODE)" }
        Write-Host "Tarefa '$TaskName' ativa a cada $Minutes min."
        Write-Host "Log: .cycle\cycle.log   Estado: .cycle\state.json"
    }
    'remove' {
        & schtasks.exe /Delete /TN $TaskName /F 2>&1
        if ($LASTEXITCODE -ne 0) { throw "schtasks falhou ($LASTEXITCODE)" }
        Write-Host "Tarefa '$TaskName' removida."
    }
    'status' {
        & schtasks.exe /Query /TN $TaskName /FO LIST /V 2>&1
    }
    'run' {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $CyclePs1
    }
}
