# Posta uma mensagem no FORUM.txt textual (append-only, um bloco por escrita).
# O MSG_ID e' atribuido de forma serializada: abre o arquivo com FileShare.Read,
# que impede dois escritores simultaneos, e so entao le/calcula/anexa.
#
# Uso:
#   pwsh -NoProfile -File tools/forum_post.ps1 -From PHY -Type VOTO `
#        -Topic VOTO_MSGS_0001_0003 -Body "V1_0001=SIM","V1_0002=SIM"
#   ... -WaitQuorum 3   # espera ate' 3 ocorrencias de V1_0001=SIM antes de postar
param(
    [Parameter(Mandatory = $true)][string]$From,
    [Parameter(Mandatory = $true)][string]$Type,
    [Parameter(Mandatory = $true)][string]$Topic,
    [string]$To = "TODOS",
    [string]$Deadline = "-",
    [string[]]$Body = @(),
    [int]$WaitQuorum = 0,
    [int]$TimeoutSeconds = 180,
    [string]$Forum = "C:\Users\msrov\Desktop\FORUM.txt"
)

$ErrorActionPreference = "Stop"

# `powershell -File` nao converte `a,b,c` em array: aceitamos tambem um unico
# argumento com clausulas separadas por `|`.
$expanded = @()
foreach ($b in $Body) { $expanded += ($b -split '\|') }
$Body = $expanded

function Get-ForumTextShared {
    if (-not (Test-Path $Forum)) { return "" }
    $fs = [System.IO.File]::Open($Forum, [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)
        return $sr.ReadToEnd()
    } finally { $fs.Dispose() }
}

if ($WaitQuorum -gt 0) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        $txt = Get-ForumTextShared
        $votes = ([regex]::Matches($txt, "(?m)^\s*V1_0001=SIM\s*$")).Count
        if ($votes -ge $WaitQuorum) { break }
        Start-Sleep -Milliseconds 500
    }
}

$fs = $null
$sw = [System.Diagnostics.Stopwatch]::StartNew()
while ($true) {
    try {
        $fs = [System.IO.File]::Open($Forum, [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::Read)
        break
    } catch {
        if ($sw.Elapsed.TotalSeconds -gt 60) { throw "timeout aguardando lock do forum" }
        Start-Sleep -Milliseconds 100
    }
}

try {
    $fs.Seek(0, [System.IO.SeekOrigin]::End) | Out-Null
    $len = $fs.Length
    $buf = New-Object byte[] $len
    $fs.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
    if ($len -gt 0) {
        $read = 0
        while ($read -lt $len) {
            $n = $fs.Read($buf, $read, $len - $read)
            if ($n -le 0) { break }
            $read += $n
        }
    }
    $txt = [System.Text.Encoding]::UTF8.GetString($buf)

    $max = 0
    foreach ($m in [regex]::Matches($txt, "===MSG\s+(\d+)===")) {
        $v = [int]$m.Groups[1].Value
        if ($v -gt $max) { $max = $v }
    }
    $id = $max + 1
    $idStr = $id.ToString("0000")

    $sb = New-Object System.Text.StringBuilder
    if ($txt.Length -gt 0 -and -not $txt.EndsWith("`n")) {
        [void]$sb.Append("`n")
    }
    [void]$sb.Append("`n===MSG $idStr===`n")
    [void]$sb.Append("FROM: $From`n")
    [void]$sb.Append("TYPE: $Type`n")
    [void]$sb.Append("TOPIC: $Topic`n")
    [void]$sb.Append("TO: $To`n")
    [void]$sb.Append("DEADLINE: $Deadline`n")
    [void]$sb.Append("DATA:`n")
    foreach ($line in $Body) {
        [void]$sb.Append("  $line`n")
    }
    [void]$sb.Append("===END $idStr===`n")

    $bytes = (New-Object System.Text.UTF8Encoding($false)).GetBytes($sb.ToString())
    $fs.Seek(0, [System.IO.SeekOrigin]::End) | Out-Null
    $fs.Write($bytes, 0, $bytes.Length)
    $fs.Flush($true)
    Write-Output "MSG $idStr anexada ($From/$Type/$Topic)"
} finally {
    if ($fs) { $fs.Dispose() }
}
