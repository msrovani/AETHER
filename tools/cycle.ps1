#requires -Version 5.1
<#
  AETHER cycle - le o projeto, compara com o repositorio, integra PRs,
  versiona (0.0.x), commita, push opcional e registra o ciclo.

  Uso:
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1 -PrMerge -Push
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\cycle.ps1 -Full

  Variaveis de ambiente:
    GITHUB_TOKEN   token com escopo repo (habilita -PrMerge)
    AETHER_PUSH=1  liga push sem passar -Push
    AETHER_REMOTE  URL do origin, usada se o repo ainda nao tiver remote
#>
[CmdletBinding()]
param(
    [switch]$Push,
    [switch]$PrMerge,
    [switch]$Full,
    [switch]$NoCommit,
    [int]$LockMinutes = 10
)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$env:GIT_TERMINAL_PROMPT = '0'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $RepoRoot

$CycleDir = Join-Path $RepoRoot '.cycle'
if (-not (Test-Path $CycleDir)) { New-Item -ItemType Directory -Path $CycleDir | Out-Null }
$LogFile = Join-Path $CycleDir 'cycle.log'
$StateFile = Join-Path $CycleDir 'state.json'
$LockFile = Join-Path $CycleDir 'lock'

function Write-Log {
    param([string]$Message)
    $line = "{0} {1}" -f (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'), $Message
    Add-Content -Path $LogFile -Value $line -Encoding utf8
    Write-Host $line
}

function Resolve-Git {
    $candidates = @(
        (Get-Command git -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
        'C:\Program Files\Git\cmd\git.exe',
        'C:\Program Files (x86)\Git\cmd\git.exe',
        (Join-Path $env:LOCALAPPDATA 'Programs\Git\cmd\git.exe'),
        (Join-Path $env:USERPROFILE '.local\share\git\cmd\git.exe'),
        (Join-Path $env:USERPROFILE 'scoop\shims\git.exe')
    ) | Where-Object { $_ -and (Test-Path $_) }

    foreach ($candidate in $candidates) {
        & $candidate --version *> $null
        if ($LASTEXITCODE -eq 0) { return $candidate }
    }
    return $null
}

$Git = Resolve-Git
if (-not $Git) {
    Write-Log 'ERRO git nao encontrado; ciclo abortado'
    exit 2
}

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs)
    $output = & $Git @GitArgs 2>&1
    $text = (($output | ForEach-Object { "$_" }) -join "`n")
    return [pscustomobject]@{
        Code   = $LASTEXITCODE
        Output = $text.TrimEnd()
    }
}

# ---------- lock (evita ciclos simultaneos) ----------
if (Test-Path $LockFile) {
    $age = (Get-Date) - (Get-Item $LockFile).LastWriteTime
    if ($age.TotalMinutes -lt $LockMinutes) {
        Write-Host "ciclo anterior em andamento (lock com $([int]$age.TotalMinutes) min); saindo."
        exit 0
    }
    Write-Log 'lock expirado; assumindo ciclo'
    Remove-Item $LockFile -Force -ErrorAction SilentlyContinue
}
Set-Content -Path $LockFile -Value $PID -Encoding ascii

try {
    # ---------- identidade minima ----------
    if (-not (Invoke-Git config user.name).Output) {
        Invoke-Git config user.name 'AETHER Cycle' | Out-Null
    }
    if (-not (Invoke-Git config user.email).Output) {
        Invoke-Git config user.email 'aether-cycle@local' | Out-Null
    }

    # ---------- remote ----------
    if ($env:AETHER_REMOTE -and -not (Invoke-Git remote get-url origin).Output) {
        Invoke-Git remote add origin $env:AETHER_REMOTE | Out-Null
        Write-Log "origin configurado: $env:AETHER_REMOTE"
    }
    $remoteRes = Invoke-Git remote get-url origin
    $remoteUrl = if ($remoteRes.Code -eq 0) { $remoteRes.Output.Trim() } else { '' }
    $hasRemote = [bool]$remoteUrl

    # ---------- comparar com o repositorio ----------
    $behind = 0
    if ($hasRemote) {
        $fetch = Invoke-Git fetch --prune origin
        if ($fetch.Code -ne 0) { Write-Log "fetch falhou: $($fetch.Output)" }

        $headOk = (Invoke-Git rev-parse --verify HEAD).Code -eq 0
        if ($headOk) {
            $count = (Invoke-Git rev-list --count 'HEAD..origin/main').Output
            if ($count -match '^\d+$') { $behind = [int]$count }
        }
    }

    # ---------- PRs ----------
    $mergedPrs = @()
    if ($PrMerge) {
        $token = $env:GITHUB_TOKEN
        if (-not $token) {
            # reusa a credencial HTTPS ja armazenada (GCM/Windows) sem imprimi-la
            try {
                $credIn = "protocol=https`nhost=github.com`n`n"
                $credOut = (($credIn | & $Git credential fill 2>$null) -join "`n")
                if ($credOut -match '(?m)^password=(.+)$') { $token = $Matches[1].Trim() }
            } catch { }
        }
        if (-not $token) {
            Write-Log 'PrMerge pedido mas sem token (GITHUB_TOKEN ou credencial git); etapa ignorada'
        }
        elseif ($remoteUrl -match 'github\.com[:/](?<owner>[^/]+)/(?<repo>[^/.]+?)(\.git)?/?$') {
            $owner = $Matches.owner
            $repoName = $Matches.repo
            $api = "https://api.github.com/repos/$owner/$repoName"
            $headers = @{
                Authorization          = "token $token"
                Accept                 = 'application/vnd.github+json'
                'User-Agent'           = 'aether-cycle'
                'X-GitHub-Api-Version' = '2022-11-28'
            }
            try {
                $pulls = Invoke-RestMethod -Uri "$api/pulls?state=open&per_page=30" -Headers $headers -Method Get
                foreach ($pr in $pulls) {
                    $labels = @($pr.labels | ForEach-Object { $_.name })
                    $trusted = ($labels -contains 'automerge') -or ($pr.head.ref -like 'aether/*')
                    if (-not $trusted) { continue }
                    if ($pr.head.repo.full_name -ne "$owner/$repoName") { continue }

                    $sha = $pr.head.sha
                    $checks = Invoke-RestMethod -Uri "$api/commits/$sha/check-runs" -Headers $headers -Method Get
                    $runs = @($checks.check_runs)
                    if ($runs.Count -eq 0) { continue }
                    $allGreen = -not ($runs | Where-Object { $_.status -ne 'completed' -or $_.conclusion -ne 'success' })
                    if (-not $allGreen) { continue }

                    $detail = Invoke-RestMethod -Uri "$api/pulls/$($pr.number)" -Headers $headers -Method Get
                    if (-not $detail.mergeable -or $detail.mergeable_state -ne 'clean') { continue }

                    $body = @{
                        merge_method  = 'squash'
                        commit_title  = "merge(pr): #$($pr.number) $($pr.title)"
                        commit_message = "integrado pelo ciclo AETHER"
                    } | ConvertTo-Json
                    Invoke-RestMethod -Uri "$api/pulls/$($pr.number)/merge" -Headers $headers -Method Put -Body $body -ContentType 'application/json' | Out-Null
                    $mergedPrs += "#$($pr.number)"
                    Write-Log "PR #$($pr.number) integrado (squash)"

                    if ($pr.head.ref -ne 'main') {
                        try {
                            Invoke-RestMethod -Uri "$api/git/refs/heads/$($pr.head.ref)" -Headers $headers -Method Delete | Out-Null
                        } catch {
                            Write-Log "branch $($pr.head.ref) nao removida: $($_.Exception.Message)"
                        }
                    }
                }
            } catch {
                Write-Log "PR API falhou: $($_.Exception.Message)"
            }
        }
        else {
            Write-Log 'remote nao e GitHub; etapa de PR ignorada'
        }
    }

    # ---------- pull com seguranca ----------
    if ($hasRemote -and $behind -gt 0) {
        $dirtyNow = [bool](Invoke-Git status --porcelain).Output
        if ($dirtyNow) {
            Write-Log "origin/main a frente ($behind commits) mas worktree sujo; pull adiado"
        } else {
            $pull = Invoke-Git pull --rebase --autostash origin main
            if ($pull.Code -eq 0) { Write-Log "pull --rebase ok ($behind commits)" }
            else { Write-Log "pull falhou: $($pull.Output)" }
            $behind = 0
        }
    }

    # ---------- mudancas locais ----------
    $statusOut = (Invoke-Git status --porcelain).Output
    $changed = @()
    if ($statusOut) {
        $changed = $statusOut -split "`n" | Where-Object { $_.Trim() } | ForEach-Object { $_.Substring(3).Trim('"') }
    }
    $changedCount = @($changed).Count

    # ---------- gate ----------
    $gate = 'skip'
    if ($changedCount -gt 0) {
        $cargoPs1 = Join-Path $RepoRoot 'scripts\cargo.ps1'
        $gateArgs = @('check', '--all-targets', '--offline')
        if ($Full) { $gateArgs = @('test', '--release', '--offline') }

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $gateOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $cargoPs1 @gateArgs 2>&1
        $gateCode = $LASTEXITCODE
        if ($gateCode -ne 0) {
            # fallback sem --offline (primeira execucao / cache incompleto)
            $fallbackArgs = $gateArgs[0..($gateArgs.Count - 2)]
            $gateOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $cargoPs1 @fallbackArgs 2>&1
            $gateCode = $LASTEXITCODE
        }
        $sw.Stop()
        if ($gateCode -ne 0) {
            $gateLog = Join-Path $CycleDir 'gate.log'
            $gateOutput | Out-File -FilePath $gateLog -Encoding utf8
        }
        $gate = if ($gateCode -eq 0) { "ok ($([int]$sw.Elapsed.TotalSeconds)s)" } else { "fail ($([int]$sw.Elapsed.TotalSeconds)s)" }
        Write-Log "gate $gate"
    }

    # ---------- versao 0.0.x ----------
    $cargoPath = Join-Path $RepoRoot 'Cargo.toml'
    $cargoText = Get-Content $cargoPath -Raw
    $currentVersion = '0.0.0'
    if ($cargoText -match '(?m)^version\s*=\s*"([^"]+)"') { $currentVersion = $Matches[1] }

    $newVersion = $currentVersion
    $sourceChanged = @($changed | Where-Object { ($_ -replace '\\', '/') -match '^(src|tests)/' }).Count
    $isRelease = ($changedCount -gt 0) -and ($gate -like 'ok*') -and ($sourceChanged -gt 0)
    if ($isRelease) {
        if ($currentVersion -match '^0\.0\.(\d+)$') {
            $newVersion = "0.0.$([int]$Matches[1] + 1)"
        } else {
            $newVersion = '0.0.1'
        }
        $cargoText = [regex]::Replace($cargoText, '(?m)^version\s*=\s*"[^"]+"', "version = `"$newVersion`"", 1)
        Set-Content -Path $cargoPath -Value $cargoText -Encoding utf8 -NoNewline
    }

    # ---------- resumo por area ----------
    $areas = @{}
    foreach ($file in $changed) {
        $norm = $file -replace '\\', '/'
        $area = ($norm -split '/')[0]
        if ($area -eq $norm) { $area = '.' }
        if (-not $areas.ContainsKey($area)) { $areas[$area] = 0 }
        $areas[$area]++
    }
    $areaSummary = (($areas.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Key):$($_.Value)" }) -join ', ')

    # ---------- changelog + log do ciclo ----------
    if ($changedCount -gt 0) {
        $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
        if ($isRelease) {
            $bulletFiles = ($changed | Select-Object -First 12 | ForEach-Object { "- ``$_``" }) -join "`n"
            $entry = "## [$newVersion] - $stamp`n`n### Changed`n`n- Ciclo automatico: $changedCount arquivo(s) ($areaSummary).`n- Gate: $gate.`n`n$bulletFiles`n`n"

            $changelogPath = Join-Path $RepoRoot 'CHANGELOG.md'
            if (Test-Path $changelogPath) {
                $cl = Get-Content $changelogPath -Raw
                $idx = $cl.IndexOf("`n## [")
                if ($idx -ge 0) {
                    $cl = $cl.Substring(0, $idx + 1) + $entry + $cl.Substring($idx + 1)
                } else {
                    $cl = $cl.TrimEnd() + "`n`n" + $entry
                }
                Set-Content -Path $changelogPath -Value $cl -Encoding utf8 -NoNewline
            }
        }

        $cycleLog = Join-Path $RepoRoot 'docs\CYCLE_LOG.md'
        if (-not (Test-Path $cycleLog)) {
            Set-Content -Path $cycleLog -Value "# CYCLE_LOG - historico do ciclo automatico`n`n| UTC | versao | arquivos | areas | gate | PRs | push |`n|---|---|---|---|---|---|---|" -Encoding utf8
        }
        $pushCell = if ($Push -or $env:AETHER_PUSH -eq '1') { 'sim' } else { 'nao' }
        $prCell = if ($mergedPrs.Count -gt 0) { $mergedPrs -join ' ' } else { '-' }
        $versionCell = if ($isRelease) { $newVersion } else { 'wip' }
        Add-Content -Path $cycleLog -Value "| $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')) | $versionCell | $changedCount | $areaSummary | $gate | $prCell | $pushCell |" -Encoding utf8
    }

    # ---------- commit + tag ----------
    $commitHash = ''
    if ($changedCount -gt 0 -and -not $NoCommit) {
        Invoke-Git add -A | Out-Null
        if ($isRelease) {
            $title = "chore(cycle): $newVersion"
        } else {
            $title = "wip(cycle): snapshot $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))"
        }
        $body = "ciclo automatico $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))`narquivos: $changedCount ($areaSummary)`ngate: $gate"
        $commit = Invoke-Git commit -m $title -m $body
        if ($commit.Code -eq 0) {
            $commitHash = (Invoke-Git rev-parse --short HEAD).Output
            if ($isRelease) {
                Invoke-Git tag -a "v$newVersion" -m "AETHER $newVersion - ciclo automatico ($gate)" | Out-Null
                Write-Log "commit $commitHash versao $newVersion"
            } else {
                Write-Log "commit wip $commitHash (gate: $gate)"
            }
        } else {
            Write-Log "commit falhou: $($commit.Output)"
        }
    } elseif ($changedCount -eq 0) {
        Write-Log 'sem mudancas; nada a versionar'
    }

    # ---------- push ----------
    $pushed = $false
    $shouldPush = $Push -or ($env:AETHER_PUSH -eq '1')
    if ($shouldPush -and $hasRemote) {
        $pushMain = Invoke-Git push origin main
        if ($pushMain.Code -ne 0) {
            # outro agente pode ter empurrado antes: rebase e uma unica retentativa
            Write-Log "push rejeitado; tentando pull --rebase e retry"
            $pull = Invoke-Git pull --rebase --autostash origin main
            if ($pull.Code -eq 0) { $pushMain = Invoke-Git push origin main }
            else { Write-Log "rebase falhou: $($pull.Output)" }
        }
        $pushTags = Invoke-Git push origin --tags
        if ($pushMain.Code -eq 0 -and $pushTags.Code -eq 0) {
            $pushed = $true
            Write-Log 'push main + tags ok'
        } else {
            Write-Log "push falhou: $($pushMain.Output) $($pushTags.Output)"
        }
    } elseif ($shouldPush -and -not $hasRemote) {
        Write-Log 'push pedido mas sem remote origin; configure AETHER_REMOTE'
    }

    # ---------- estado legivel por maquina ----------
    $state = [pscustomobject]@{
        timestamp   = (Get-Date).ToUniversalTime().ToString('o')
        version     = $newVersion
        previous    = $currentVersion
        commit      = $commitHash
        changed     = $changedCount
        areas       = $areas
        gate        = $gate
        merged_prs  = $mergedPrs
        pushed      = $pushed
        remote      = $remoteUrl
        behind      = $behind
    }
    $state | ConvertTo-Json -Depth 4 | Set-Content -Path $StateFile -Encoding utf8
}
finally {
    Remove-Item $LockFile -Force -ErrorAction SilentlyContinue
}
