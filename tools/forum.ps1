<#
  FORUM — protocolo binário append-only dos 3 nós do PROJETO AETHER (v1).
  Espelho exato de tools/forum.py (mesmo vocabulário, mesmos opcodes, mesmo CRC32).

  Uso:
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\forum.ps1 -Cmd init
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\forum.ps1 -Cmd verify
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\forum.ps1 -Cmd dump
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\forum.ps1 -Cmd send -Src 2 -Type vote -Payload "00 03 01 19"
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\forum.ps1 -Cmd propose -Src 3 -ProposalId 4 -Tokens "m2_wave","ack","review"
#>
param(
    [Parameter(Position = 0)][string]$Cmd = "dump",
    [string]$Forum = "C:\Users\msrov\Desktop\FORUM.txt",
    [int]$Src = 1,
    [string]$Type = "",
    [string[]]$Payload = @(),
    [int]$Dst = 0,
    [int]$Ref = 0,
    [int]$ProposalId = 0,
    [string[]]$Tokens = @(),
    [switch]$Force
)

$ErrorActionPreference = "Stop"

# --- vocabulário compartilhado (ORDEM IMUTÁVEL; id = índice + 1) -----------
$script:Lexicon = @(
    "aether","forum","protocol","v1","append_only","one_frame_per_line","hex_uppercase",
    "seq_monotonic","crc32","no_edit_delete","claim_before_code","vote_quorum_2of3",
    "no_force_push","tests_release","mission","m0_scaffold","m1_thermo","m2_wave",
    "m3_kuramoto","m4_organism","mission_split","claim","vote","yes","no","abstain",
    "propose","accept","reject","status","ready","blocked","done","owner","lease","ttl",
    "seconds","module","test","invariant","benchmark","latency","rust","cargo","udp",
    "packet40","crates","ndarray","rustfft","mio","bytemuck","node1","node2","node3",
    "buffy","peer_cursor","peer_opencode","handoff","review","merge","branch","conflict",
    "escalate_human","ack","ping","deadline","start","stop","forum_tool","bootstrap",
    "quorum","human_arbiter","binary_only","no_json","no_prose","pending_vote","main",
    "rename_ok","crc_validated","vocabulary","decode_required","register_node"
)

$script:Op = @{
    announce = 0x01; dict = 0x02; claim = 0x03; propose = 0x04; vote = 0x05
    status = 0x06; commit = 0x07; alert = 0x08; rule = 0x09; done = 0x0A
}
$script:OpName = @{}
foreach ($k in $script:Op.Keys) { $script:OpName[[int]$script:Op[$k]] = $k }

$script:Mission = @{ m0_scaffold = 0; m1_thermo = 1; m2_wave = 2; m3_kuramoto = 3; m4_organism = 4 }
$script:MissionName = @{ 0 = "m0_scaffold"; 1 = "m1_thermo"; 2 = "m2_wave"; 3 = "m3_kuramoto"; 4 = "m4_organism" }
$script:StateName = @{ 0 = "proposed"; 1 = "claimed"; 2 = "in_progress"; 3 = "review"; 4 = "done"; 5 = "blocked" }
$script:VoteName = @{ 0 = "abstain"; 1 = "yes"; 2 = "no" }

# --- CRC32 (IEEE, polinômio 0xEDB88320) -----------------------------------
$script:CrcTable = $null
function Get-CrcTable {
    if ($null -ne $script:CrcTable) { return $script:CrcTable }
    $t = New-Object 'System.UInt32[]' 256
    for ($i = 0; $i -lt 256; $i++) {
        [uint32]$c = [uint32]$i
        for ($k = 0; $k -lt 8; $k++) {
            if (($c -band 1) -eq 1) {
                $c = [uint32]([uint32]4294967295 -bxor ($c -shr 1))
            } else {
                $c = [uint32]($c -shr 1)
            }
        }
        $t[$i] = $c
    }
    $script:CrcTable = $t
    return $t
}

function Get-Crc32([byte[]]$Data) {
    $t = Get-CrcTable
    [uint32]$c = [uint32]4294967295
    foreach ($b in $Data) {
        $idx = [int](($c -bxor [uint32]$b) -band 255)
        $c = [uint32]($t[$idx] -bxor ($c -shr 8))
    }
    return [uint32]($c -bxor [uint32]4294967295)
}

# --- hex ----------------------------------------------------------------
function ConvertTo-Hex([byte[]]$Bytes) {
    $sb = New-Object System.Text.StringBuilder
    foreach ($b in $Bytes) { [void]$sb.Append($b.ToString("X2")) }
    return $sb.ToString()
}

function ConvertFrom-Hex([string]$Hex) {
    $n = [int]($Hex.Length / 2)
    $out = New-Object 'byte[]' $n
    for ($i = 0; $i -lt $n; $i++) {
        $out[$i] = [Convert]::ToByte($Hex.Substring($i * 2, 2), 16)
    }
    return , $out
}

# --- frames -------------------------------------------------------------
function New-Frame([int]$Seq, [int]$S, [int]$D, [int]$T, [byte[]]$Pl, [int]$R) {
    if ($null -eq $Pl) { $Pl = New-Object 'byte[]' 0 }
    $ms = New-Object System.IO.MemoryStream
    $ms.WriteByte([byte]0xA3)
    $ms.WriteByte([byte]1)
    $ms.WriteByte([byte](($Seq -shr 8) -band 255)); $ms.WriteByte([byte]($Seq -band 255))
    $ms.WriteByte([byte]$S); $ms.WriteByte([byte]$D); $ms.WriteByte([byte]$T)
    $ms.WriteByte([byte](($R -shr 8) -band 255)); $ms.WriteByte([byte]($R -band 255))
    $ms.WriteByte([byte](($Pl.Length -shr 8) -band 255)); $ms.WriteByte([byte]($Pl.Length -band 255))
    if ($Pl.Length -gt 0) { $ms.Write($Pl, 0, $Pl.Length) }
    $head = $ms.ToArray()

    $crcBytes = New-Object 'byte[]' ($head.Length - 1)
    [Array]::Copy($head, 1, $crcBytes, 0, $head.Length - 1)
    $crc = Get-Crc32 $crcBytes

    $out = New-Object 'byte[]' ($head.Length + 4)
    [Array]::Copy($head, 0, $out, 0, $head.Length)
    $out[$head.Length]     = [byte](($crc -shr 24) -band 255)
    $out[$head.Length + 1] = [byte](($crc -shr 16) -band 255)
    $out[$head.Length + 2] = [byte](($crc -shr 8) -band 255)
    $out[$head.Length + 3] = [byte]($crc -band 255)
    return , $out
}

function Read-Frame([byte[]]$Raw, [int]$LineNo) {
    if ($Raw.Length -lt 15) { throw "linha ${LineNo}: frame curto ($($Raw.Length) bytes)" }
    if ($Raw[0] -ne 0xA3) { throw "linha ${LineNo}: magic invalido" }
    if ($Raw[1] -ne 1) { throw "linha ${LineNo}: versao nao suportada" }
    $seq = ([int]$Raw[2] * 256) + [int]$Raw[3]
    $s = [int]$Raw[4]; $d = [int]$Raw[5]; $t = [int]$Raw[6]
    $r = ([int]$Raw[7] * 256) + [int]$Raw[8]
    $len = ([int]$Raw[9] * 256) + [int]$Raw[10]
    if ($Raw.Length -ne (15 + $len)) { throw "linha ${LineNo}: len inconsistente" }

    $crcBytes = New-Object 'byte[]' (10 + $len)
    [Array]::Copy($Raw, 1, $crcBytes, 0, 10 + $len)
    $calc = Get-Crc32 $crcBytes
    $file = ([uint32]$Raw[11 + $len] -shl 24) -bor ([uint32]$Raw[12 + $len] -shl 16) -bor `
            ([uint32]$Raw[13 + $len] -shl 8) -bor [uint32]$Raw[14 + $len]
    if ([uint32]$calc -ne [uint32]$file) { throw "linha ${LineNo}: CRC invalido" }

    $pl = New-Object 'byte[]' $len
    if ($len -gt 0) { [Array]::Copy($Raw, 11, $pl, 0, $len) }
    return [pscustomobject]@{ Seq = $seq; Src = $s; Dst = $d; Type = $t; Ref = $r; Payload = $pl }
}

function Get-Frames([string]$Path) {
    $list = New-Object System.Collections.ArrayList
    if (-not (Test-Path -LiteralPath $Path)) { return , $list }
    $lines = @(Get-Content -LiteralPath $Path -Encoding ASCII)
    $n = 0
    foreach ($line in $lines) {
        $n++
        $h = $line.Trim().Replace(" ", "")
        if ($h.Length -eq 0) { continue }
        if ($h -ne $h.ToUpperInvariant()) { throw "linha ${n}: hex deve ser MAIUSCULO" }
        [byte[]]$raw = ConvertFrom-Hex $h
        [void]$list.Add((Read-Frame $raw $n))
    }
    return , $list
}

function Add-Frames([string]$Path, $Frames) {
    $parts = @()
    foreach ($f in $Frames) { $parts += (ConvertTo-Hex $f) }
    if ($parts.Count -eq 0) { return }
    $text = ($parts -join "`n") + "`n"
    [System.IO.File]::AppendAllText($Path, $text, [System.Text.Encoding]::ASCII)
}

# --- tokens -------------------------------------------------------------
function Encode-Tokens([string[]]$Toks) {
    $ms = New-Object System.IO.MemoryStream
    foreach ($tk in $Toks) {
        if ($tk -match '^[0-9]+$') {
            $v = [int]$tk
            if ($v -le 255) {
                $ms.WriteByte([byte]0xFE); $ms.WriteByte([byte]$v)
            } elseif ($v -le 65535) {
                $ms.WriteByte([byte]0xFD)
                $ms.WriteByte([byte](($v -shr 8) -band 255))
                $ms.WriteByte([byte]($v -band 255))
            } else { throw "numero fora de faixa: $tk" }
        } elseif ($tk -eq '|' -or $tk -eq '.') {
            $ms.WriteByte([byte]0xFF)
        } else {
            $idx = [Array]::IndexOf($script:Lexicon, $tk)
            if ($idx -lt 0) { throw "token desconhecido: $tk (acrescente ao LEXICON, nunca reordene)" }
            $ms.WriteByte([byte]($idx + 1))
        }
    }
    return , $ms.ToArray()
}

function Decode-Tokens([byte[]]$Pl) {
    $out = New-Object System.Collections.ArrayList
    $i = 0
    while ($i -lt $Pl.Length) {
        $b = [int]$Pl[$i]
        if ($b -eq 0xFF) { [void]$out.Add("|"); $i += 1 }
        elseif ($b -eq 0xFE) { [void]$out.Add([string][int]$Pl[$i + 1]); $i += 2 }
        elseif ($b -eq 0xFD) {
            $v = ([int]$Pl[$i + 1] * 256) + [int]$Pl[$i + 2]
            [void]$out.Add([string]$v); $i += 3
        } else {
            $w = $script:Lexicon[$b - 1]
            if ($null -eq $w) { $w = "?" + $b.ToString("X2") }
            [void]$out.Add($w); $i += 1
        }
    }
    return , $out
}

# --- payloads -----------------------------------------------------------
function P-Announce([string]$Name, [int]$Caps = 31) {
    $nb = [System.Text.Encoding]::UTF8.GetBytes($Name)
    $ms = New-Object System.IO.MemoryStream
    $ms.WriteByte([byte](($Caps -shr 24) -band 255))
    $ms.WriteByte([byte](($Caps -shr 16) -band 255))
    $ms.WriteByte([byte](($Caps -shr 8) -band 255))
    $ms.WriteByte([byte]($Caps -band 255))
    $ms.Write($nb, 0, $nb.Length)
    return , $ms.ToArray()
}

function P-Dict([string[]]$Entries) {
    $ms = New-Object System.IO.MemoryStream
    for ($i = 0; $i -lt $Entries.Count; $i++) {
        $b = [System.Text.Encoding]::ASCII.GetBytes($Entries[$i])
        $ms.WriteByte([byte]($i + 1))
        $ms.WriteByte([byte]$b.Length)
        $ms.Write($b, 0, $b.Length)
    }
    return , $ms.ToArray()
}

function P-Propose([int]$Id, [string[]]$Toks) {
    $ms = New-Object System.IO.MemoryStream
    $ms.WriteByte([byte]$Id)
    [byte[]]$tk = Encode-Tokens $Toks
    if ($tk.Length -gt 0) { $ms.Write($tk, 0, $tk.Length) }
    return , $ms.ToArray()
}

function P-Vote([int]$Target, [int]$Vote, [string]$Reason) {
    $idx = [Array]::IndexOf($script:Lexicon, $Reason)
    if ($idx -lt 0) { $idx = 63 }
    return , ([byte[]]@([byte](($Target -shr 8) -band 255), [byte]($Target -band 255), [byte]$Vote, [byte]($idx + 1)))
}

function P-Claim([int]$Mission, [int]$Prio, [int]$Eta, [int]$Ttl, [int]$Flags) {
    return , ([byte[]]@(
        [byte]$Mission, [byte]$Prio,
        [byte](($Eta -shr 8) -band 255), [byte]($Eta -band 255),
        [byte](($Ttl -shr 8) -band 255), [byte]($Ttl -band 255),
        [byte]$Flags))
}

# --- comandos -----------------------------------------------------------
function Cmd-Init([string]$Path) {
    if ((Test-Path -LiteralPath $Path) -and ((Get-Item -LiteralPath $Path).Length -gt 0) -and (-not $Force)) {
        throw "forum nao esta vazio; init recusado (use -Force)"
    }
    $frames = New-Object System.Collections.ArrayList
    [void]$frames.Add((New-Frame 1 1 0 $script:Op["announce"] (P-Announce "buffy/freebuff" 31) 0))
    [void]$frames.Add((New-Frame 2 1 0 $script:Op["dict"] (P-Dict $script:Lexicon) 0))
    [void]$frames.Add((New-Frame 3 1 0 $script:Op["propose"] (P-Propose 1 @(
        "forum","append_only","one_frame_per_line","hex_uppercase","seq_monotonic","crc32",
        "no_edit_delete","binary_only","no_json","no_prose","claim_before_code",
        "vote_quorum_2of3","no_force_push","tests_release","human_arbiter","escalate_human")) 0))
    [void]$frames.Add((New-Frame 4 1 0 $script:Op["propose"] (P-Propose 2 @(
        "mission_split","m0_scaffold","m1_thermo","m2_wave","m3_kuramoto","m4_organism",
        "owner","review","merge","branch")) 0))
    [void]$frames.Add((New-Frame 5 1 0 $script:Op["propose"] (P-Propose 3 @(
        "v1","protocol","lease","ttl","900","seconds","quorum","2","3","vocabulary",
        "decode_required","register_node","forum_tool","handoff")) 0))
    [void]$frames.Add((New-Frame 6 1 0 $script:Op["claim"] (P-Claim $script:Mission["m1_thermo"] 1 1800 900 1) 0))
    [void]$frames.Add((New-Frame 7 1 0 $script:Op["vote"] (P-Vote 3 1 "yes") 3))
    [void]$frames.Add((New-Frame 8 1 0 $script:Op["vote"] (P-Vote 4 1 "yes") 4))
    [void]$frames.Add((New-Frame 9 1 0 $script:Op["vote"] (P-Vote 5 1 "yes") 5))
    Add-Frames $Path $frames
    Write-Output "semeado: $($frames.Count) frames em $Path"
}

function Cmd-Verify([string]$Path) {
    $frames = Get-Frames $Path
    $last = 0
    foreach ($f in $frames) {
        if ($f.Seq -le $last) { throw "seq nao monotonico: $($f.Seq) apos $last" }
        $last = $f.Seq
    }
    Write-Output "OK: $($frames.Count) frames, CRC valido, seq 1..$last"
}

function Cmd-Dump([string]$Path) {
    $frames = Get-Frames $Path
    foreach ($f in $frames) {
        $dstTxt = "ALL"
        if ($f.Dst -ne 0) { $dstTxt = "n$($f.Dst)" }
        $head = "#{0:D3} n{1}->{2}" -f $f.Seq, $f.Src, $dstTxt
        $t = $f.Type
        $p = $f.Payload
        if ($t -eq $script:Op["announce"]) {
            $caps = ([int]$p[0] -shl 24) -bor ([int]$p[1] -shl 16) -bor ([int]$p[2] -shl 8) -bor [int]$p[3]
            $nm = [System.Text.Encoding]::UTF8.GetString($p, 4, $p.Length - 4)
            Write-Output ("$head ANNOUNCE caps=0x{0:X8} name={1}" -f $caps, $nm)
        } elseif ($t -eq $script:Op["dict"]) {
            $i = 0; $cnt = 0; $lastTok = ""
            while ($i -lt $p.Length) {
                $id = [int]$p[$i]; $ln = [int]$p[$i + 1]
                $lastTok = [System.Text.Encoding]::ASCII.GetString($p, $i + 2, $ln)
                $i += 2 + $ln; $cnt++
            }
            Write-Output ("$head DICT {0} tokens (ultimo={1})" -f $cnt, $lastTok)
        } elseif ($t -eq $script:Op["propose"]) {
            $toks = Decode-Tokens ([byte[]]($p[1..($p.Length - 1)]))
            Write-Output ("$head PROPOSE p{0}: {1}" -f $p[0], ($toks -join " "))
        } elseif ($t -eq $script:Op["vote"]) {
            $target = ([int]$p[0] * 256) + [int]$p[1]
            $vote = $script:VoteName[[int]$p[2]]
            $reason = $script:Lexicon[[int]$p[3] - 1]
            Write-Output ("$head VOTE target=#{0} vote={1} reason={2}" -f $target, $vote, $reason)
        } elseif ($t -eq $script:Op["claim"]) {
            $m = $script:MissionName[[int]$p[0]]
            $eta = ([int]$p[2] * 256) + [int]$p[3]
            $ttl = ([int]$p[4] * 256) + [int]$p[5]
            Write-Output ("$head CLAIM {0} prio={1} eta={2}s ttl={3}s flags=0x{4:X2}" -f $m, $p[1], $eta, $ttl, $p[6])
        } elseif ($t -eq $script:Op["status"]) {
            Write-Output ("$head STATUS {0} {1} {2}%" -f $script:MissionName[[int]$p[0]], $script:StateName[[int]$p[1]], $p[2])
        } elseif (($t -eq $script:Op["commit"]) -or ($t -eq $script:Op["done"])) {
            $sha = ConvertTo-Hex ([byte[]]($p[1..8]))
            Write-Output ("$head COMMIT {0} {1}" -f $script:MissionName[[int]$p[0]], $sha)
        } elseif ($t -eq $script:Op["alert"]) {
            $code = ([int]$p[0] * 256) + [int]$p[1]
            Write-Output ("$head ALERT code={0} mission={1}" -f $code, $script:MissionName[[int]$p[2]])
        } else {
            Write-Output ("$head T=0x{0:X2} payload={1}" -f $t, (ConvertTo-Hex $p))
        }
    }
}

function Cmd-Send([string]$Path) {
    if (-not $script:Op.ContainsKey($Type)) { throw "tipo desconhecido: $Type" }
    $frames = Get-Frames $Path
    $seq = 1
    if ($frames.Count -gt 0) { $seq = ($frames[$frames.Count - 1].Seq) + 1 }

    $hex = ($Payload -join " ").Trim()
    [byte[]]$pl = New-Object 'byte[]' 0
    if ($hex.Length -gt 0) {
        $clean = $hex.Replace(" ", "").Replace("0x", "").Replace("0X", "")
        if (($clean.Length % 2) -ne 0) { throw "payload hex com numero impar de digitos" }
        $clean = $clean.ToUpperInvariant()
        [byte[]]$pl = ConvertFrom-Hex $clean
    }
    $fr = New-Frame $seq $Src $Dst $script:Op[$Type] $pl $Ref
    Add-Frames $Path @($fr)
    Write-Output "anexado seq=$seq type=$Type $(ConvertTo-Hex $fr)"
}

function Cmd-Propose([string]$Path) {
    $frames = Get-Frames $Path
    $seq = 1
    if ($frames.Count -gt 0) { $seq = ($frames[$frames.Count - 1].Seq) + 1 }
    $fr = New-Frame $seq $Src 0 $script:Op["propose"] (P-Propose $ProposalId $Tokens) 0
    Add-Frames $Path @($fr)
    Write-Output "anexado seq=$seq PROPOSE p$ProposalId"
}

switch ($Cmd.ToLowerInvariant()) {
    "init"    { Cmd-Init $Forum }
    "verify"  { Cmd-Verify $Forum }
    "dump"    { Cmd-Dump $Forum }
    "send"    { Cmd-Send $Forum }
    "propose" { Cmd-Propose $Forum }
    default   { throw "comando desconhecido: $Cmd (init|verify|dump|send|propose)" }
}
