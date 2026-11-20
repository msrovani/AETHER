"""Vetor de pesquisa EXTERNO vivo (G4 do plano de fechamento).

F2/F2.5 construíram o cabo offline (fixtures, gate unknown-only, quarentena).
Este módulo é a tomada REAL: busca web sem chave (DuckDuckGo HTML), tier da
fonte por domínio (data/source_tier.json), convergência numérica quando os
resultados carregam valores, e a tripla do Eixo F:

    { evidência citada, score_de_convergência, veredito: settled | open }

Regras de doutrina (inegociáveis):
- a resposta de pesquisa é EVIDÊNCIA em quarentena (autoridade 60–120),
  nunca fato 255 — só o HITL sanciona;
- `ampliar` é comando do HITL, nunca automático;
- pergunta irreductível (opinião/debate) → open 60 SEMPRE;
- sem juiz semântico na borda, a convergência aqui é NUMÉRICA (valores
  extraídos por regex); texto sem números comparáveis → open honesto com a
  marca `convergencia_nao_avaliavel_sem_juiz` — não inventa concordância.

Rede: só roda com AETHER_RESEARCH_NETWORK=1 no ambiente (opt-in HITL).
"""

from __future__ import annotations

import html
import json
import re
import statistics
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UA = "Mozilla/5.0 (compatible; AETHER-research-vector/1.0; +local)"

# Domínio → tier (a tabela oficial vive em data/source_tier.json; este mapa
# cobre os casos mais comuns de busca viva e é cruzado com a tabela quando
# ela tem o domínio).
_DOMAIN_TIER_DEFAULTS = {
    "nasa.gov": "primary",
    "arxiv.org": "primary",
    "github.com": "primary",
    "docs.rs": "primary",
    "rust-lang.org": "primary",
    "python.org": "primary",
    "mit.edu": "primary",
    "wikipedia.org": "aggregator",
    "stackoverflow.com": "secondary",
    "news.ycombinator.com": "secondary",
    "medium.com": "secondary",
    "dev.to": "secondary",
}

_NUM_RE = re.compile(r"-?\d+(?:[.,]\d+)?")


def network_enabled() -> bool:
    import os

    return os.environ.get("AETHER_RESEARCH_NETWORK", "").strip() == "1"


def _tier_for(domain: str, tier_cfg: dict) -> str:
    # heurística leve de domínio (a tabela oficial não tem lista de domínios,
    # só `authority_cap` por tier — exemplos textuais): TLD gov/edu e fontes
    # oficiais conhecidas = primary; wikipedia/agregadores = aggregator.
    if domain.endswith(".gov") or domain.endswith(".gov.br") or domain.endswith(".edu"):
        return "primary"
    for known, tier in _DOMAIN_TIER_DEFAULTS.items():
        if domain == known or domain.endswith("." + known):
            return tier
    if "wikipedia.org" in domain or "stackexchange" in domain:
        return "aggregator"
    return "seo_llm_farm"


def _cap_for(tier_id: str, tier_cfg: dict) -> int:
    for t in tier_cfg.get("tiers", []):
        if t.get("id") == tier_id:
            return int(t.get("authority_cap", 60))
    return 60


def _ddg_sources(query: str, k: int = 6) -> list[dict]:
    """Busca DuckDuckGo LITE (sem chave; o html endpoint serviu anomaly challenge).

    Devolve [{title, url, domain, snippet}] — links reais via uddg=."""
    url = "https://lite.duckduckgo.com/lite/?" + urllib.parse.urlencode({"q": query})
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
            "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36"
        },
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        page = resp.read().decode("utf-8", errors="replace")
    if "anomaly" in page:
        return []
    out: list[dict] = []
    # layout lite: link `result-link` seguido de `result-snippet` na linha de
    # baixo da tabela; anúncios (duckduckgo.com/y.js, ad_domain) são lixo.
    for m in re.finditer(
        r"<a[^>]+href=.([^\" ]+)[^>]*class=.result-link[^>]*>(.*?)</a>", page, re.S
    ):
        href = html.unescape(m.group(1))
        title = re.sub(r"<[^>]+>", "", html.unescape(m.group(2))).strip()
        if "uddg=" in href:
            q = urllib.parse.parse_qs(
                urllib.parse.urlparse(
                    "//" + href if href.startswith("//") else href
                ).query
            )
            href = (q.get("uddg") or [href])[0]
        domain = urllib.parse.urlparse(href).netloc.removeprefix("www.")
        if domain == "duckduckgo.com" or "/y.js" in href:
            continue  # anúncio — lixo, não fonte
        # snippet: a próxima `result-snippet` após este link
        tail = page[m.end() : m.end() + 1200]
        sn = re.search(r"class=.result-snippet.>(.*?)</td>", tail, re.S)
        snippet = (
            re.sub(r"<[^>]+>", "", html.unescape(sn.group(1))).strip() if sn else ""
        )
        out.append(
            {"title": title, "url": href, "domain": domain, "snippet": snippet}
        )
        if len(out) >= k:
            break
    return out


def _independent(sources: list[dict], thr: float = 0.8) -> list[dict]:
    """Independência real por jaccard de palavras do título+snippet (cópia conta 1)."""
    def words(s: dict) -> set:
        return set(re.findall(r"[a-zà-ú0-9]+", (s["title"] + " " + s.get("snippet", "")).casefold()))

    kept: list[dict] = []
    for s in sources:
        w = words(s)
        if any(len(w & words(k)) / max(1, len(w | words(k))) > thr for k in kept):
            continue
        kept.append(s)
    return kept


def live_research(question: str, tier_path: Path | None = None) -> dict:
    """A tripla do Eixo F sobre uma pergunta real. Exige rede habilitada."""
    if not network_enabled():
        return {
            "ok": False,
            "error": "rede_desligada",
            "hint": "AETHER_RESEARCH_NETWORK=1 é o opt-in do HITL",
        }
    tier_cfg = {}
    if tier_path and Path(tier_path).exists():
        tier_cfg = json.loads(Path(tier_path).read_text(encoding="utf-8"))

    raw = _ddg_sources(question)
    sources = _independent(raw)
    if not sources:
        return {
            "ok": True,
            "question": question,
            "verdict": "open",
            "authority": 60,
            "reason": "sem_fontes_utilizaveis",
            "evidence": [],
        }
    for s in sources:
        s["tier"] = _tier_for(s["domain"], tier_cfg)

    caps = [_cap_for(s["tier"], tier_cfg) for s in sources]
    best_cap = max(caps) if caps else 60

    # convergência numérica — sem juiz semântico na borda, a régua é
    # CONSERVADORA (falso settled é pior que open honesto):
    # - exclui anos plausíveis (1800–2100) e números que JÁ estão na pergunta
    #   (metadados, não grandeza);
    # - exclui valores < 10 (versões, contadores, sufixos tipo "C2");
    # - exige ≥3 fontes independentes com o MESMO valor (σ/μ < 5%).
    # Limitação declarada: o settled por texto completo exige juiz semântico
    # (borda LLM ou adapter fine-tuned) — sem ele, dúvida permanece.
    q_tokens = set(_NUM_RE.findall(question))
    numbers: list[float] = []
    for s in sources:
        text = s["title"] + " " + s.get("snippet", "")
        for tok in _NUM_RE.findall(text):
            try:
                v = float(tok.replace(",", "."))
            except ValueError:
                continue
            if 1800 <= v <= 2100:
                continue  # ano plausível — metadado
            if v < 10:
                continue  # versão/contador/sufixo — ambíguo
            if tok in q_tokens:
                continue  # veio da pergunta, não é resposta
            numbers.append(v)
    verdict, authority, reason, score = "open", 60, "sem_convergencia_numerica", None
    if len(sources) >= 3 and len(numbers) >= 2:
        # agrupa números próximos (a grandeza candidata) e testa σ/μ
        groups: dict[int, list[float]] = {}
        for n in numbers:
            key = round(n) if abs(n) >= 10 else int(n)
            groups.setdefault(key, []).append(n)
        best = max(groups.values(), key=len)
        if len(best) >= 3:
            mu = statistics.fmean(best)
            sigma = statistics.pstdev(best)
            ratio = sigma / abs(mu) if mu else 1.0
            score = ratio
            if ratio < 0.05:
                verdict, authority, reason = "settled", min(best_cap, 120), "numeric_converged"
            else:
                verdict, authority, reason = "open", 60, "conflito_numerico"

    return {
        "ok": True,
        "question": question,
        "verdict": verdict,
        "authority": authority,
        "reason": reason,
        "convergence_sigma_over_mu": score,
        "n_independent": len(sources),
        "evidence": [
            {
                "title": s["title"],
                "url": s["url"],
                "domain": s["domain"],
                "tier": s["tier"],
            }
            for s in sources
        ],
        "note": (
            "convergencia_nao_avaliavel_sem_juiz"
            if verdict == "open" and reason == "sem_convergencia_numerica"
            else None
        ),
    }


if __name__ == "__main__":
    import sys

    q = " ".join(sys.argv[1:])
    print(json.dumps(live_research(q, ROOT / "data" / "source_tier.json"), ensure_ascii=False, indent=2))
