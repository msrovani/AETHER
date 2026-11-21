"""Vetor de pesquisa EXTERNO vivo (G4 do plano de fechamento).

F2/F2.5 construíram o cabo offline (fixtures, gate unknown-only, quarentena).
Este módulo é a tomada REAL: busca web com cascata de provedores, tier por
domínio (data/source_tier.json), convergência numérica e a tripla do Eixo F:

    { evidência citada, score_de_convergência, veredito: settled | open }

Cascata paralela (default `cascade`):
  1. DuckDuckGo lite (sem chave; sujeita a 403/anomaly)
  2. SearXNG — default público `https://search.lumy.live` (JSON ok em 2026-11-21);
     override via AETHER_SEARX_URL; `0`/`off`/`-` desliga a etapa
  3. Wikipedia/Wikidata API (pt→en) — estável para fatos enciclopédicos
  Provedores disparam em threads I/O; fontes são fundidas (não first-hit).
  ≥2 providers com evidência independente → autoridade fraca 80 (Eixo F).
  `AETHER_RESEARCH_PARALLEL=0` volta ao first-hit sequencial.

Override: AETHER_RESEARCH_PROVIDER=ddg|searx|wiki|cascade (default cascade).

Regras de doutrina (inegociáveis):
- a resposta de pesquisa é EVIDÊNCIA em quarentena (autoridade 60–120),
  nunca fato 255 — só o HITL sanciona;
- `ampliar` é comando do HITL, nunca automático;
- pergunta irreductível (opinião/debate) → open 60 SEMPRE;
- sem juiz semântico na borda, a convergência NUMÉRICA sobe até settled≤120;
  cobertura multi-provider sem número → fraco 80 (não settled falso).

Rede: só roda com AETHER_RESEARCH_NETWORK=1 no ambiente (opt-in HITL).
"""

from __future__ import annotations

import html
import json
import os
import re
import statistics
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
UA = "Mozilla/5.0 (compatible; AETHER-research-vector/1.0; +local)"
# Instância pública que aceita format=json (smoke 2026-11-21). Frágil — override local preferível.
DEFAULT_SEARX_URL = "https://search.lumy.live"
_SEARX_OFF = frozenset({"0", "off", "false", "no", "-", "none"})

_DOMAIN_TIER_DEFAULTS = {
    "nasa.gov": "primary",
    "arxiv.org": "primary",
    "github.com": "primary",
    "docs.rs": "primary",
    "rust-lang.org": "primary",
    "python.org": "primary",
    "mit.edu": "primary",
    "wikipedia.org": "aggregator",
    "wikidata.org": "aggregator",
    "stackoverflow.com": "secondary",
    "news.ycombinator.com": "secondary",
    "medium.com": "secondary",
    "dev.to": "secondary",
}

_NUM_RE = re.compile(r"-?\d+(?:[.,]\d+)?")
# Ortografia PT sem acento → forma canônica (busca Wiki).
_PT_FIX = {
    "franca": "França",
    "japao": "Japão",
    "brasil": "Brasil",
    "portugal": "Portugal",
    "alemanha": "Alemanha",
    "espanha": "Espanha",
    "italia": "Itália",
    "mexico": "México",
    "canada": "Canadá",
    "china": "China",
    "eua": "Estados Unidos",
    "usa": "Estados Unidos",
}
# pistas leves para título Wiki (não é NER — só melhora o search).
_WIKI_HINTS = (
    (re.compile(r"capital\s+d[aeo]s?\s+(\w+)", re.I), "country"),
    (re.compile(r"quem\s+escreveu\s+(.+)", re.I), "book"),
    (re.compile(r"distancia|distância|terra.?lua", re.I), "terra_lua"),
)


def _fold_ascii(s: str) -> str:
    """Remove acentos p/ search API (Wiki às vezes falha com %C3%)."""
    table = str.maketrans(
        "áàâãäéèêëíìîïóòôõöúùûüçÁÀÂÃÄÉÈÊËÍÌÎÏÓÒÔÕÖÚÙÛÜÇ",
        "aaaaaeeeeiiiiooooouuuucAAAAAEEEEIIIIOOOOOUUUUC",
    )
    return s.translate(table)


def _fix_pt_token(tok: str) -> str:
    key = re.sub(r"[^a-z]", "", tok.casefold())
    return _PT_FIX.get(key, tok.strip(" ?!."))


def _pt_search_forms(tok: str) -> list[str]:
    """Formas canônica + ascii para a mesma entidade."""
    fixed = _fix_pt_token(tok)
    forms = [fixed, _fold_ascii(fixed)]
    key = re.sub(r"[^a-z]", "", tok.casefold())
    if key:
        forms.append(key)
    seen: set[str] = set()
    out: list[str] = []
    for f in forms:
        k = f.casefold()
        if k in seen or len(f) < 2:
            continue
        seen.add(k)
        out.append(f)
    return out


def network_enabled() -> bool:
    return os.environ.get("AETHER_RESEARCH_NETWORK", "").strip() == "1"


def _provider_mode() -> str:
    return (os.environ.get("AETHER_RESEARCH_PROVIDER") or "cascade").strip().lower()


def _http_get(url: str, timeout: float = 20.0, accept: str = "text/html") -> str | None:
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": UA,
            "Accept": accept,
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            # Wikimedia às vezes devolve 200 vazio sob throttle.
            return body if body.strip() else None
    except Exception:
        return None


def _tier_for(domain: str, tier_cfg: dict) -> str:
    if domain.endswith(".gov") or domain.endswith(".gov.br") or domain.endswith(".edu"):
        return "primary"
    for known, tier in _DOMAIN_TIER_DEFAULTS.items():
        if domain == known or domain.endswith("." + known):
            return tier
    if "wikipedia.org" in domain or "wikidata.org" in domain or "stackexchange" in domain:
        return "aggregator"
    return "seo_llm_farm"


def _cap_for(tier_id: str, tier_cfg: dict) -> int:
    for t in tier_cfg.get("tiers", []):
        if t.get("id") == tier_id:
            return int(t.get("authority_cap", 60))
    return 60


def _ddg_sources(query: str, k: int = 6) -> list[dict]:
    """DuckDuckGo LITE. 403/anomaly/timeout → []."""
    url = "https://lite.duckduckgo.com/lite/?" + urllib.parse.urlencode({"q": query})
    page = _http_get(
        url,
        accept="text/html",
    )
    if not page:
        return []
    if "anomaly" in page or "Unfortunately, bots use DuckDuckGo too" in page:
        return []
    out: list[dict] = []
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
            continue
        tail = page[m.end() : m.end() + 1200]
        sn = re.search(r"class=.result-snippet.>(.*?)</td>", tail, re.S)
        snippet = (
            re.sub(r"<[^>]+>", "", html.unescape(sn.group(1))).strip() if sn else ""
        )
        out.append(
            {
                "title": title,
                "url": href,
                "domain": domain,
                "snippet": snippet,
                "provider": "ddg",
            }
        )
        if len(out) >= k:
            break
    return out


def _searx_base() -> str | None:
    """Resolve base SearXNG: env > default público; off explícito → None."""
    raw = (os.environ.get("AETHER_SEARX_URL") or "").strip()
    if raw.casefold() in _SEARX_OFF:
        return None
    if raw:
        return raw.rstrip("/")
    return DEFAULT_SEARX_URL.rstrip("/")


def _searx_sources(query: str, k: int = 6) -> list[dict]:
    """SearXNG JSON. Default público; AETHER_SEARX_URL override; 0/off desliga."""
    base = _searx_base()
    if not base:
        return []
    url = (
        base
        + "/search?"
        + urllib.parse.urlencode({"q": query, "format": "json", "language": "pt-BR"})
    )
    raw = _http_get(url, accept="application/json")
    if not raw:
        return []
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return []
    out: list[dict] = []
    for hit in data.get("results") or []:
        href = str(hit.get("url") or "")
        title = str(hit.get("title") or "").strip()
        snippet = str(hit.get("content") or hit.get("snippet") or "").strip()
        if not href or not title:
            continue
        domain = urllib.parse.urlparse(href).netloc.removeprefix("www.")
        if not domain:
            continue
        out.append(
            {
                "title": title,
                "url": href,
                "domain": domain,
                "snippet": snippet[:500],
                "provider": "searx",
            }
        )
        if len(out) >= k:
            break
    return out


def _wiki_query_variants(question: str) -> list[str]:
    q = question.strip()
    # Dicas normalizadas primeiro (evita "Franca" cidade BR antes de "França").
    variants: list[str] = []
    for rx, kind in _WIKI_HINTS:
        m = rx.search(q)
        if not m:
            continue
        if kind == "country":
            country = _fix_pt_token(m.group(1))
            art = "a" if country[-1:].casefold() == "a" else "o"
            for form in _pt_search_forms(m.group(1)):
                variants.append(form)
                variants.append(f"Capital d{art} {form}")
        elif kind == "book":
            title = m.group(1).strip(" ?!.")
            variants.append(re.sub(r"^[\"']|[\"']$", "", title).strip())
        elif kind == "terra_lua":
            variants.extend(["Distancia da Terra a Lua", "Distância da Terra à Lua"])
    for tok in re.findall(r"[A-Za-zÀ-ú]+", q):
        variants.extend(_pt_search_forms(tok))
    variants.append(q)
    clean = re.sub(r"[?!.,;:]+", " ", q).strip()
    if clean:
        variants.append(clean)
    seen: set[str] = set()
    out: list[str] = []
    for v in variants:
        key = v.casefold()
        if key in seen or len(v) < 2:
            continue
        seen.add(key)
        out.append(v)
    return out[:6]


def _wikipedia_opensearch(lang: str, query: str, k: int = 3) -> list[dict]:
    api = f"https://{lang}.wikipedia.org/w/api.php?" + urllib.parse.urlencode(
        {
            "action": "opensearch",
            "search": query,
            "limit": k,
            "namespace": 0,
            "format": "json",
        }
    )
    raw = _http_get(api, accept="application/json")
    if not raw:
        return []
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return []
    # [query, [titles], [descs], [urls]]
    if not isinstance(data, list) or len(data) < 4:
        return []
    titles, descs, urls = data[1], data[2], data[3]
    out: list[dict] = []
    for title, desc, href in zip(titles, descs, urls):
        if not title or not href:
            continue
        out.append(
            {
                "title": str(title),
                "url": str(href),
                "domain": f"{lang}.wikipedia.org",
                "snippet": str(desc or "")[:500],
                "provider": "wiki",
            }
        )
    return out


def _wikipedia_summary(lang: str, title: str) -> dict | None:
    api = (
        f"https://{lang}.wikipedia.org/api/rest_v1/page/summary/"
        + urllib.parse.quote(title.replace(" ", "_"))
    )
    raw = _http_get(api, accept="application/json")
    if not raw:
        return None
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return None
    extract = str(data.get("extract") or "").strip()
    if not extract:
        return None
    page = data.get("content_urls", {}).get("desktop", {}).get("page") or (
        f"https://{lang}.wikipedia.org/wiki/" + urllib.parse.quote(title.replace(" ", "_"))
    )
    return {
        "title": str(data.get("title") or title),
        "url": str(page),
        "domain": f"{lang}.wikipedia.org",
        "snippet": extract[:600],
        "provider": "wiki",
    }


def _wiki_sources(query: str, k: int = 6) -> list[dict]:
    """Wikipedia pt→en via opensearch + summary. Sem chave."""
    out: list[dict] = []
    seen_url: set[str] = set()
    want_capital = bool(re.search(r"\bcapital\b", query, re.I))
    for variant in _wiki_query_variants(query):
        for lang in ("pt", "en"):
            hits = _wikipedia_opensearch(lang, variant, k=3)
            for hit in hits:
                # evita homônimo óbvio: "Franca" (cidade BR) quando perguntam capital
                title_l = hit["title"].casefold()
                if want_capital and title_l in {"franca", "franca basquetebol clube"}:
                    continue
                summary = _wikipedia_summary(lang, hit["title"])
                row = summary or hit
                if row["url"] in seen_url:
                    continue
                seen_url.add(row["url"])
                out.append(row)
                if len(out) >= k:
                    return out
    return out


def _parallel_enabled() -> bool:
    return os.environ.get("AETHER_RESEARCH_PARALLEL", "1").strip() != "0"


def _cascade_providers() -> list[tuple[str, object]]:
    return [
        ("ddg", _ddg_sources),
        ("searx", _searx_sources),
        ("wiki", _wiki_sources),
    ]


def _gather_sources(question: str, k: int = 6) -> tuple[list[dict], list[str]]:
    """Cascata de provedores. Devolve (fontes, trilha de tentativas).

    cascade + parallel (default): dispara DDG/SearX/Wiki em threads I/O e
    funde fontes — libera o loop do chat enquanto a rede espera.
    cascade + AETHER_RESEARCH_PARALLEL=0: first-hit sequencial legado.
    """
    mode = _provider_mode()
    trail: list[str] = []

    def try_one(name: str, fn) -> list[dict]:
        try:
            rows = fn(question, k=k)
        except Exception:
            rows = []
        trail.append(f"{name}:{len(rows)}")
        return rows

    if mode == "ddg":
        return try_one("ddg", _ddg_sources), trail
    if mode == "searx":
        return try_one("searx", _searx_sources), trail
    if mode in ("wiki", "wikipedia"):
        return try_one("wiki", _wiki_sources), trail

    providers = _cascade_providers()
    if not _parallel_enabled():
        for name, fn in providers:
            rows = try_one(name, fn)
            if rows:
                return rows, trail
        return [], trail

    # Paralelo: max(latências) em vez de soma; fusão multi-provider.
    by_name: dict[str, list[dict]] = {name: [] for name, _ in providers}
    with ThreadPoolExecutor(max_workers=len(providers), thread_name_prefix="aether-rs") as pool:
        futs = {pool.submit(fn, question, k): name for name, fn in providers}
        for fut in as_completed(futs):
            name = futs[fut]
            try:
                by_name[name] = fut.result() or []
            except Exception:
                by_name[name] = []
    # Trilha na ordem canônica da cascata (não ordem de chegada).
    for name, _ in providers:
        trail.append(f"{name}:{len(by_name[name])}")
    trail.append("parallel:1")
    fused: list[dict] = []
    seen_url: set[str] = set()
    for name, _ in providers:
        for row in by_name[name]:
            url = str(row.get("url") or "")
            if url and url in seen_url:
                continue
            if url:
                seen_url.add(url)
            fused.append(row)
    return fused, trail


def _independent(sources: list[dict], thr: float = 0.8) -> list[dict]:
    """Independência real por jaccard de palavras do título+snippet (cópia conta 1)."""

    def words(s: dict) -> set:
        return set(
            re.findall(
                r"[a-zà-ú0-9]+",
                (s["title"] + " " + s.get("snippet", "")).casefold(),
            )
        )

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

    raw, trail = _gather_sources(question)
    sources = _independent(raw)
    if not sources:
        return {
            "ok": True,
            "question": question,
            "verdict": "open",
            "authority": 60,
            "reason": "sem_fontes_utilizaveis",
            "evidence": [],
            "provider_trail": trail,
            "provider_mode": _provider_mode(),
        }
    for s in sources:
        s["tier"] = _tier_for(s["domain"], tier_cfg)

    caps = [_cap_for(s["tier"], tier_cfg) for s in sources]
    best_cap = max(caps) if caps else 60

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
                continue
            if v < 10:
                continue
            if tok in q_tokens:
                continue
            numbers.append(v)
    verdict, authority, reason, score = "open", 60, "sem_convergencia_numerica", None
    if len(sources) >= 3 and len(numbers) >= 2:
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

    providers_used = sorted({s.get("provider", "?") for s in sources})
    # Eixo F camada fraca: ≥2 providers independentes sem settled numérico → 80.
    if (
        verdict == "open"
        and authority <= 60
        and len(providers_used) >= 2
        and len(sources) >= 2
    ):
        authority = min(best_cap, 80)
        reason = "multi_provider_confirm"

    return {
        "ok": True,
        "question": question,
        "verdict": verdict,
        "authority": authority,
        "reason": reason,
        "convergence_sigma_over_mu": score,
        "n_independent": len(sources),
        "providers": providers_used,
        "provider_trail": trail,
        "provider_mode": _provider_mode(),
        "parallel": _parallel_enabled() and _provider_mode() == "cascade",
        "evidence": [
            {
                "title": s["title"],
                "url": s["url"],
                "domain": s["domain"],
                "tier": s["tier"],
                "provider": s.get("provider"),
            }
            for s in sources
        ],
        "note": (
            "convergencia_nao_avaliavel_sem_juiz"
            if verdict == "open" and reason == "sem_convergencia_numerica"
            else (
                "cobertura_multi_provider_fraca"
                if reason == "multi_provider_confirm"
                else None
            )
        ),
    }


def selftest() -> int:
    """Testes offline das peças + smoke de rede se AETHER_RESEARCH_NETWORK=1."""
    failures: list[str] = []
    # offline: variantes wiki
    vars_ = _wiki_query_variants("e a capital da Franca?")
    if not any("capital" in v.casefold() for v in vars_):
        failures.append(f"wiki_variants: {vars_}")
    # default SearX público + off explícito
    old_searx = os.environ.pop("AETHER_SEARX_URL", None)
    try:
        if _searx_base() != DEFAULT_SEARX_URL.rstrip("/"):
            failures.append(f"searx_default: {_searx_base()}")
        os.environ["AETHER_SEARX_URL"] = "off"
        if _searx_base() is not None:
            failures.append(f"searx_off: {_searx_base()}")
    finally:
        if old_searx is None:
            os.environ.pop("AETHER_SEARX_URL", None)
        else:
            os.environ["AETHER_SEARX_URL"] = old_searx
    # cascade trail shape com rede off nos providers (ddg/wiki podem falhar) —
    # só garante que a função não explode e devolve estrutura.
    os.environ["AETHER_RESEARCH_NETWORK"] = "1"
    # força só wiki no selftest de rede curta
    old = os.environ.get("AETHER_RESEARCH_PROVIDER")
    old_par = os.environ.get("AETHER_RESEARCH_PARALLEL")
    os.environ["AETHER_RESEARCH_PROVIDER"] = "wiki"
    try:
        r = live_research("capital da Franca", ROOT / "data" / "source_tier.json")
        if not r.get("ok"):
            failures.append(f"wiki_franca_ok: {r}")
        elif r.get("n_independent", 0) < 1:
            failures.append(f"wiki_franca_fontes: {r.get('reason')} trail={r.get('provider_trail')}")
        elif "wiki" not in (r.get("providers") or []):
            failures.append(f"wiki_franca_provider: {r.get('providers')}")
    finally:
        if old is None:
            os.environ.pop("AETHER_RESEARCH_PROVIDER", None)
        else:
            os.environ["AETHER_RESEARCH_PROVIDER"] = old

    # cascade paralelo: trilha com 3 providers + parallel:1
    os.environ["AETHER_RESEARCH_PROVIDER"] = "cascade"
    os.environ["AETHER_RESEARCH_PARALLEL"] = "1"
    try:
        raw, trail = _gather_sources("capital da Franca", k=3)
        joined = "|".join(trail)
        if "parallel:1" not in trail:
            failures.append(f"cascade_parallel_trail: {trail}")
        if not all(p in joined for p in ("ddg:", "searx:", "wiki:")):
            failures.append(f"cascade_parallel_providers: {trail}")
        # fusão não deve explodir; fontes podem ser 0 offline
        _ = raw
    except Exception as e:
        failures.append(f"cascade_parallel_gather: {e}")
    finally:
        if old is None:
            os.environ.pop("AETHER_RESEARCH_PROVIDER", None)
        else:
            os.environ["AETHER_RESEARCH_PROVIDER"] = old
        if old_par is None:
            os.environ.pop("AETHER_RESEARCH_PARALLEL", None)
        else:
            os.environ["AETHER_RESEARCH_PARALLEL"] = old_par
    report = {"ok": not failures, "failures": failures, "ref": "research_live_cascade"}
    print(json.dumps(report, ensure_ascii=False))
    return 0 if not failures else 1


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        raise SystemExit(selftest())
    q = " ".join(sys.argv[1:])
    print(json.dumps(live_research(q, ROOT / "data" / "source_tier.json"), ensure_ascii=False, indent=2))
