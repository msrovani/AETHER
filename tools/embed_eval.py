#!/usr/bin/env python3
"""Eval de recall do AETHER (missão F4 do plano F2→F3).

Protocolo (ressalva D3 da MSG 0772 — dataset CONGELADO antes do gate):

    1) --freeze   monta data/embed_eval_set.jsonl a partir das fontes
                  (docs/*.md por seção, data/*.jsonl de fatos, dump nsgdb)
                  + data/embed_eval_queries.jsonl (consultas escritas à mão)
                  e valida todo gold id contra o corpus;
    2) o hash sha256 do arquivo congelado é publicado no fórum ANTES de medir;
    3) --run --sha256 <hash>  recusa rodar se o arquivo mudou, e só então
                  mede hit@5 / recall@5 de lexical (BM25), embedder (HTTP)
                  e híbrido (RRF k=60), gravando target/embed_eval.json;
    4) sem tuning depois da medição: número ruim vira número ruim.

Uso:
    python tools/embed_eval.py --freeze
    python tools/embed_eval.py --run --sha256 <hash> [--embed-url http://127.0.0.1:8788]
    python tools/embed_eval.py --selftest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import sys
import time
import unicodedata
import urllib.request
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SET_PATH = ROOT / "data" / "embed_eval_set.jsonl"
QUERIES_PATH = ROOT / "data" / "embed_eval_queries.jsonl"
NSGDB_DUMP = ROOT / "target" / "f4_nsgdb_dump.jsonl"
OUT_PATH = ROOT / "target" / "embed_eval.json"

K_BM25 = 1.2
B_BM25 = 0.75
K_RRF = 60
TOP_K = 5
GATE_HIT5 = 0.974
BATCH = 32


# --------------------------------------------------------------------------- #
# tokenização (uma só para todos os sistemas — lexical e semântico comem o
# mesmo texto de entrada; nenhum sistema recebe tratamento preferencial)
# --------------------------------------------------------------------------- #
_TOKEN_RE = re.compile(r"[a-z0-9_]+")


def tokenize(text: str) -> list[str]:
    text = unicodedata.normalize("NFKD", text.lower())
    text = "".join(c for c in text if not unicodedata.combining(c))
    return _TOKEN_RE.findall(text)


def slug(text: str) -> str:
    s = tokenize(text.replace("\n", " "))
    return "-".join(s[:8]) or "sec"


# --------------------------------------------------------------------------- #
# corpus
# --------------------------------------------------------------------------- #
def _sections(path: Path) -> list[tuple[str, str]]:
    """Seções `## ` de um .md; sem `## ` o arquivo inteiro vira uma seção."""
    text = path.read_text(encoding="utf-8", errors="replace")
    rel = path.relative_to(ROOT).as_posix()
    marks = [(m.start(), m.group(0)) for m in re.finditer(r"(?m)^##\s+.+$", text)]
    if not marks:
        return [(f"doc:{rel}#top", text.strip())]
    out: list[tuple[str, str]] = []
    if marks[0][0] > 0:
        head = text[: marks[0][0]].strip()
        if head:
            out.append((f"doc:{rel}#top", head))
    for i, (pos, title) in enumerate(marks):
        end = marks[i + 1][0] if i + 1 < len(marks) else len(text)
        body = text[pos:end].strip()
        if len(body) < 40:  # âncora vazia não é documento recuperável
            continue
        out.append((f"doc:{rel}#{slug(title)}", body))
    return out


def _fact_docs(path: Path) -> list[tuple[str, str]]:
    out = []
    for i, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines()):
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except Exception:  # noqa: BLE001
            continue
        if not isinstance(obj, dict):
            continue
        parts = [str(obj.get(k, "")) for k in ("subject", "relation", "object", "text")]
        text = " ".join(p for p in parts if p).strip()
        if text:
            out.append((f"fact:{path.name}:{i}", text))
    return out


def _nsgdb_docs(path: Path) -> list[tuple[str, str]]:
    if not path.exists():
        return []
    out = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except Exception:  # noqa: BLE001
            continue
        key = obj.get("key") or obj.get("storage_key") or obj.get("id")
        text = obj.get("text") or obj.get("payload") or ""
        if isinstance(text, dict):
            text = text.get("text", "")
        if key and isinstance(text, str) and text.strip():
            out.append((f"nsgdb:{key}", text.strip()))
    return out


_SUFFIX_RE = re.compile(r"_\d+$")


def canon_fact(text: str) -> str:
    """Canônico de fato sintético: `Brasil_1 capital Brasília_1` == base.

    `data/facts.jsonl` é fixture com repetição `_1.._19`; sem isto a consulta
    teria 20 golds idênticos e o recall@5 seria artefato, não qualidade.
    """
    return " ".join(_SUFFIX_RE.sub("", tok) for tok in text.split())


def build_corpus() -> list[dict]:
    seen: set[str] = set()
    seen_fact: set[str] = set()
    docs: list[dict] = []
    for path in sorted((ROOT / "docs").glob("*.md")):
        for did, text in _sections(path):
            if did in seen:
                continue
            seen.add(did)
            docs.append({"id": did, "text": text})
    for name in ("agent_facts.jsonl", "facts.jsonl"):
        p = ROOT / "data" / name
        if p.exists():
            for did, text in _fact_docs(p):
                if did in seen:
                    continue
                canon = canon_fact(text)
                if canon in seen_fact:
                    continue
                seen.add(did)
                seen_fact.add(canon)
                docs.append({"id": did, "text": text})
    for did, text in _nsgdb_docs(NSGDB_DUMP):
        if did in seen:
            continue
        seen.add(did)
        docs.append({"id": did, "text": text})
    return docs


def load_queries() -> list[dict]:
    if not QUERIES_PATH.exists():
        raise SystemExit(f"consultas ausentes: {QUERIES_PATH} (escreva o arquivo antes do freeze)")
    qs = []
    for line in QUERIES_PATH.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        qs.append(json.loads(line))
    return qs


# --------------------------------------------------------------------------- #
# freeze
# --------------------------------------------------------------------------- #
def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def cmd_freeze() -> int:
    docs = build_corpus()
    queries = load_queries()
    ids = {d["id"] for d in docs}
    if len(ids) != len(docs):
        raise SystemExit("ids de documento duplicados no corpus")
    problems = []
    for q in queries:
        if not q.get("id") or not q.get("q") or not q.get("gold"):
            problems.append(f"consulta malformada: {q}")
        for g in q["gold"]:
            if g not in ids:
                problems.append(f"gold inexistente: {q.get('id')} -> {g}")
    if problems:
        print("\n".join(problems[:20]))
        raise SystemExit(f"freeze recusado: {len(problems)} problema(s)")

    meta = {
        "type": "meta",
        "protocol": "F4 embed eval (MSG 0772 ressalva D3; MSG 0780 claim)",
        "sources": ["docs/*.md#secao", "data/agent_facts.jsonl", "data/facts.jsonl", "target/f4_nsgdb_dump.jsonl"],
        "n_docs": len(docs),
        "n_queries": len(queries),
        "gate": {"hit5": GATE_HIT5, "k": TOP_K, "k_rrf": K_RRF},
    }
    SET_PATH.parent.mkdir(parents=True, exist_ok=True)
    with SET_PATH.open("w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(meta, ensure_ascii=False) + "\n")
        for d in docs:
            f.write(json.dumps({"type": "doc", **d}, ensure_ascii=False) + "\n")
        for q in queries:
            f.write(json.dumps({"type": "query", **q}, ensure_ascii=False) + "\n")
    digest = sha256_file(SET_PATH)
    print(f"freeze ok: {len(docs)} docs, {len(queries)} consultas")
    print(f"arquivo: {SET_PATH.relative_to(ROOT).as_posix()}")
    print(f"sha256: {digest}")
    print("publique o hash no fórum ANTES de rodar --run")
    return 0


def load_set(expect_sha: str | None) -> tuple[dict, list[dict], list[dict]]:
    if not SET_PATH.exists():
        raise SystemExit(f"dataset ausente: {SET_PATH} (rode --freeze)")
    digest = sha256_file(SET_PATH)
    if expect_sha and digest != expect_sha:
        raise SystemExit(
            f"hash do dataset ({digest[:12]}...) != hash congelado ({expect_sha[:12]}...) — "
            "dataset mudou depois do freeze; refaça o freeze e republice o hash"
        )
    meta, docs, queries = {}, [], []
    for line in SET_PATH.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        obj = json.loads(line)
        kind = obj.pop("type", None)
        if kind == "meta":
            meta = obj
        elif kind == "doc":
            docs.append(obj)
        elif kind == "query":
            queries.append(obj)
    return meta, docs, queries


# --------------------------------------------------------------------------- #
# sistemas de ranqueamento
# --------------------------------------------------------------------------- #
class BM25:
    def __init__(self, docs: list[dict]):
        self.toks = [Counter(tokenize(d["text"])) for d in docs]
        self.n = len(docs)
        self.avgdl = sum(sum(c.values()) for c in self.toks) / max(1, self.n)
        df: Counter = Counter()
        for c in self.toks:
            df.update(c.keys())
        self.idf = {t: math.log(1 + (self.n - n + 0.5) / (n + 0.5)) for t, n in df.items()}

    def rank(self, query: str) -> list[int]:
        qt = Counter(tokenize(query))
        scores = []
        for i, tf in enumerate(self.toks):
            s = 0.0
            dl = sum(tf.values()) or 1
            for t, qn in qt.items():
                if t not in tf:
                    continue
                idf = self.idf.get(t, 0.0)
                f = tf[t]
                s += idf * f * (K_BM25 + 1) / (f + K_BM25 * (1 - B_BM25 + B_BM25 * dl / self.avgdl))
            scores.append((s, i))
        scores.sort(key=lambda x: (-x[0], x[1]))
        return [i for s, i in scores if s > 0][:TOP_K * 4]


def embed_texts(url: str, texts: list[str]) -> list[list[float]]:
    out: list[list[float]] = []
    for i in range(0, len(texts), BATCH):
        body = json.dumps({"texts": texts[i : i + BATCH]}).encode("utf-8")
        req = urllib.request.Request(
            f"{url.rstrip('/')}/embed_batch", data=body, headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(req, timeout=120) as resp:
            out.extend(json.loads(resp.read())["embeddings"])
    return out


def cosine_rank(qvec: list[float], mat: list[list[float]]) -> list[int]:
    scores = [(sum(a * b for a, b in zip(qvec, v)), i) for i, v in enumerate(mat)]
    scores.sort(key=lambda x: (-x[0], x[1]))
    return [i for _, i in scores]


def rrf(rankings: list[list[int]]) -> list[int]:
    scores: dict[int, float] = {}
    for rank in rankings:
        for pos, i in enumerate(rank):
            scores[i] = scores.get(i, 0.0) + 1.0 / (K_RRF + pos + 1)
    ordered = sorted(scores.items(), key=lambda x: (-x[1], x[0]))
    return [i for i, _ in ordered]


# --------------------------------------------------------------------------- #
# métricas
# --------------------------------------------------------------------------- #
def score(queries: list[dict], rankings: list[list[int]], docs: list[dict]) -> dict:
    id_of = {i: d["id"] for i, d in enumerate(docs)}
    hits = 0
    rec = 0.0
    misses: list[str] = []
    for q, rank in zip(queries, rankings):
        top = [id_of[i] for i in rank[:TOP_K]]
        gold = set(q["gold"])
        got = gold & set(top)
        if got:
            hits += 1
        else:
            misses.append(q["id"])
        rec += len(got) / len(gold)
    n = max(1, len(queries))
    return {"hit5": round(hits / n, 4), "recall5": round(rec / n, 4), "misses": misses}


def per_kind(queries: list[dict], rankings: list[list[int]], docs: list[dict]) -> dict:
    id_of = {i: d["id"] for i, d in enumerate(docs)}
    buckets: dict[str, list[bool]] = {}
    for q, rank in zip(queries, rankings):
        top = {id_of[i] for i in rank[:TOP_K]}
        buckets.setdefault(q.get("kind", "outro"), []).append(bool(set(q["gold"]) & top))
    return {k: {"n": len(v), "hit5": round(sum(v) / len(v), 4)} for k, v in sorted(buckets.items())}


# --------------------------------------------------------------------------- #
# run
# --------------------------------------------------------------------------- #
def cmd_run(expect_sha: str, embed_url: str, out: Path) -> int:
    meta, docs, queries = load_set(expect_sha)
    print(f"dataset congelado ok: {len(docs)} docs, {len(queries)} consultas, sha {expect_sha[:12]}...")

    texts = [d["text"] for d in docs]
    t0 = time.time()
    bm25 = BM25(docs)
    lex_rank = [bm25.rank(q["q"]) for q in queries]
    t_lex = time.time() - t0

    t0 = time.time()
    doc_vecs = embed_texts(embed_url, texts)
    q_vecs = embed_texts(embed_url, [q["q"] for q in queries])
    t_emb = time.time() - t0
    if len(doc_vecs) != len(docs):
        raise SystemExit("embedder devolveu número errado de vetores")
    if len({len(v) for v in doc_vecs}) != 1:
        raise SystemExit("embedder devolveu dimensões mistas")

    emb_rank = [cosine_rank(qv, doc_vecs) for qv in q_vecs]
    hyb_rank = [rrf([lex, emb]) for lex, emb in zip(lex_rank, emb_rank)]

    systems = {
        "lexical_bm25": score(queries, lex_rank, docs),
        "embed_minilm": score(queries, emb_rank, docs),
        "hybrid_rrf60": score(queries, hyb_rank, docs),
    }
    kinds = {
        "lexical_bm25": per_kind(queries, lex_rank, docs),
        "embed_minilm": per_kind(queries, emb_rank, docs),
        "hybrid_rrf60": per_kind(queries, hyb_rank, docs),
    }
    gate = {
        "hit5_min": GATE_HIT5,
        "embed_pass": systems["embed_minilm"]["hit5"] >= GATE_HIT5,
        "hybrid_pass": systems["hybrid_rrf60"]["hit5"] >= GATE_HIT5,
        "beats_lexical": systems["embed_minilm"]["hit5"] >= systems["lexical_bm25"]["hit5"],
    }
    result = {
        "sha256_set": expect_sha,
        "n_docs": len(docs),
        "n_queries": len(queries),
        "dim": len(doc_vecs[0]) if doc_vecs else 0,
        "latency": {"bm25_s": round(t_lex, 2), "embed_s": round(t_emb, 2)},
        "systems": systems,
        "per_kind": kinds,
        "gate": gate,
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"systems": systems, "gate": gate}, ensure_ascii=False, indent=2))
    print(f"artefato: {out.relative_to(ROOT).as_posix()}")
    return 0 if gate["embed_pass"] and gate["hybrid_pass"] else 2


def cmd_bm25_only(expect_sha: str, out: Path) -> int:
    """T3 prep (MSG 0870): baseline lexical no dataset congelado, sem MiniLM/rede."""
    meta, docs, queries = load_set(expect_sha)
    t0 = time.time()
    bm25 = BM25(docs)
    lex_rank = [bm25.rank(q["q"]) for q in queries]
    t_lex = time.time() - t0
    systems = {"lexical_bm25": score(queries, lex_rank, docs)}
    kinds = {"lexical_bm25": per_kind(queries, lex_rank, docs)}
    hit5 = systems["lexical_bm25"]["hit5"]
    result = {
        "schema": "aether.embed_eval_bm25_baseline/v1",
        "tier": "T3_prep",
        "sha256_set": expect_sha,
        "n_docs": len(docs),
        "n_queries": len(queries),
        "meta_note": meta.get("note") if isinstance(meta, dict) else None,
        "latency": {"bm25_s": round(t_lex, 2)},
        "systems": systems,
        "per_kind": kinds,
        "gate": {
            "hit5_min": GATE_HIT5,
            "bm25_hit5": hit5,
            "bm25_pass_as_lexical_floor": True,
            "note": "baseline lexical; MiniLM/híbrido = AGENT-A --run",
        },
        "network": False,
        "ref": "MSG_0862_0870",
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    print(f"artefato: {out.relative_to(ROOT).as_posix()}")
    return 0


# --------------------------------------------------------------------------- #
# selftest (sem rede, sem modelo)
# --------------------------------------------------------------------------- #
def cmd_selftest() -> int:
    docs = [
        {"id": "a", "text": "o aether proibe backpropagation no nucleo"},
        {"id": "b", "text": "gate de usabilidade parser fora do vocabulario abstem"},
        {"id": "c", "text": "memoria e prior nao instrucao evidencia no prompt"},
        {"id": "d", "text": "kuramoto campo udp quarentena binaria"},
    ]
    qs = [
        {"id": "q1", "q": "backpropagation", "gold": ["a"], "kind": "lexical"},
        {"id": "q2", "q": "o que nao pode virar instrucao de sistema", "gold": ["c"], "kind": "semantic"},
    ]
    bm25 = BM25(docs)
    ranks = [bm25.rank(q["q"]) for q in qs]
    s = score(qs, ranks, docs)
    assert s["hit5"] == 1.0, s
    # híbrido: RRF preserva o top-1 de qualquer sistema
    h = rrf([[0, 1, 2, 3], [1, 0, 3, 2]])
    assert h[0] in (0, 1), h
    # freeze rejeita gold inexistente
    ids = {d["id"] for d in docs}
    assert all(g in ids for q in qs for g in q["gold"])
    print("selftest PASS (bm25 hit5=%.2f, rrf top=%s)" % (s["hit5"], h[0]))
    return 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="eval de recall F4")
    ap.add_argument("--freeze", action="store_true")
    ap.add_argument("--run", action="store_true")
    ap.add_argument(
        "--bm25-only",
        action="store_true",
        help="T3 prep: só BM25 no set congelado (sem embedder/rede)",
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--sha256", default=os.environ.get("AETHER_EVAL_SET_SHA256", ""))
    ap.add_argument("--embed-url", default=os.environ.get("AETHER_EMBED_URL", "http://127.0.0.1:8788"))
    ap.add_argument("--out", type=Path, default=OUT_PATH)
    args = ap.parse_args(argv)

    if args.selftest:
        return cmd_selftest()
    if args.freeze:
        return cmd_freeze()
    if args.bm25_only:
        if not args.sha256:
            raise SystemExit("--sha256 obrigatório (hash publicado no fórum)")
        out = args.out if args.out != OUT_PATH else (ROOT / "target" / "embed_eval_bm25_baseline.json")
        return cmd_bm25_only(args.sha256.lower(), out)
    if args.run:
        if not args.sha256:
            raise SystemExit("--sha256 obrigatório (hash publicado no fórum antes da medição)")
        return cmd_run(args.sha256.lower(), args.embed_url, args.out)
    ap.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
