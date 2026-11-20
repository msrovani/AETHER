# Extrai prosa real do projeto (fórum em texto corrido + docs em frases) para o
# attach do vega-U. Sem linhas estruturadas KEY=VALUE — o guide do attach pede
# texto natural; "diverse beats big". Saída: data/vega_attach_corpus_prose.txt
$ErrorActionPreference = "Stop"
$out = New-Object System.Collections.Generic.List[string]

# 1) Fórum: linhas longas que NAO sao KEY=VALUE (prosa de relatorios/debates/notes)
$forum = Get-Content "C:\Users\msrov\Desktop\FORUM.txt" -Encoding UTF8
foreach ($l in $forum) {
    $t = $l.Trim()
    if ($t.Length -ge 25 -and $t.Length -le 400 -and
        $t -notmatch '^[A-Z0-9_]+=' -and $t -notmatch '^===' -and
        ($t -split ' ').Count -ge 5) {
        $out.Add($t)
    }
}

# 2) Docs: paragrafos em frases (AETHER + neural-sgdb)
$roots = @("docs", "C:\Users\msrov\Documents\neural-sgdb\docs", "C:\Users\msrov\Documents\neural-sgdb")
foreach ($root in $roots) {
    Get-ChildItem $root -Filter '*.md' -ErrorAction SilentlyContinue | ForEach-Object {
        $txt = (Get-Content $_.FullName -Encoding UTF8 -Raw)
        $txt = $txt -replace '```[\s\S]*?```', ' '
        $txt = $txt -replace '\|[^\n]*\|', ' '
        $txt = $txt -replace '[#*>\[\]`]', ' '
        $paras = $txt -split '\n' | Where-Object { $_.Trim().Length -ge 40 }
        foreach ($p in $paras) {
            $p.Trim() -split '(?<=[.!?])\s+' | ForEach-Object {
                $s = $_.Trim()
                if ($s.Length -ge 25 -and $s.Length -le 400 -and ($s -split ' ').Count -ge 5) {
                    $out.Add($s)
                }
            }
        }
    }
}

$uniq = $out | Select-Object -Unique
$uniq | Set-Content data\vega_attach_corpus_prose.txt -Encoding UTF8
Write-Output "prose_corpus=$($uniq.Count) linhas"
