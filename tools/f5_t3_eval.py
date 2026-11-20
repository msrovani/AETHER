#!/usr/bin/env python3
# F5 T3 prep — contrato hit@k offline. Não baixa modelo e não liga rede.
"""Mede hit@k quando já existe um ranking. Sem ranking, o prep só confere o freeze.

O MiniLM e o run F4 continuam com o AGENT-A. A linha lexical (BM25) só
estabelece o piso que o embedder terá de igualar. O teto 0.974 não baixa.

Uso:
  python tools/f5_t3_eval.py selftest
  python tools/f5_t3_eval.py prepare
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVAL_SET = ROOT / "data" / "embed_eval_set.jsonl"
# Hash conferido no disco em 2026-11-20. Bate com a deliberação OPENCODE MSG 0862.
FREEZE_SHA256 = "f868244256ee75a896f39cf4092ccfbf3bdf915002c451a1dcf450255d416bf0"
GATE_HIT5 = 0.974
K = 5


def hit_at_k(gold: list[str], ranked: list[str], k: int = K) -> bool:
    """Acerta se algum id ouro aparece nos k primeiros do ranking."""
    if k < 1 or not gold:
        return False
    top = set(ranked[:k])
    return any(item in top for item in gold)


def aggregate(rows: list[dict], k: int = K) -> dict:
    n = len(rows)
    hits = sum(1 for row in rows if hit_at_k(row["gold"], row["ranked"], k))
    rate = (hits / n) if n else 0.0
    return {
        "n": n,
        "hits": hits,
        "k": k,
        "hit_at_k": round(rate, 4),
        "gate": GATE_HIT5,
        "gate_pass": n > 0 and rate + 1e-12 >= GATE_HIT5,
        "network": False,
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


_TOKEN = re.compile(r"[a-z0-9]{2,}")


def _tokens(text: str) -> list[str]:
    return _TOKEN.findall(text.lower())


def load_eval(path: Path = EVAL_SET) -> tuple[list[dict], list[dict]]:
    docs: list[dict] = []
    queries: list[dict] = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row.get("type") == "doc":
                docs.append(row)
            elif row.get("type") == "query":
                queries.append(row)
    return docs, queries


def bm25_hit5(docs: list[dict], queries: list[dict], k: int = K) -> dict:
    """Ranking lexical puro. Não lê embedding e não abre socket."""
    tokenized = [_tokens(str(doc.get("text") or "")) for doc in docs]
    df: Counter[str] = Counter()
    lengths: list[int] = []
    for tokens in tokenized:
        lengths.append(len(tokens))
        df.update(set(tokens))
    n_docs = len(docs)
    avgdl = (sum(lengths) / n_docs) if n_docs else 0.0
    k1, b = 1.2, 0.75
    rows: list[dict] = []
    for query in queries:
        q_tokens = _tokens(str(query.get("q") or ""))
        scores: list[tuple[float, str]] = []
        for idx, tokens in enumerate(tokenized):
            if not tokens or not q_tokens:
                scores.append((0.0, str(docs[idx].get("id"))))
                continue
            tf = Counter(tokens)
            dl = lengths[idx]
            score = 0.0
            for term in q_tokens:
                freq = tf.get(term, 0)
                if freq == 0:
                    continue
                n_q = df.get(term, 0)
                idf = math.log(1.0 + (n_docs - n_q + 0.5) / (n_q + 0.5))
                score += idf * (freq * (k1 + 1.0)) / (freq + k1 * (1.0 - b + b * dl / avgdl))
            scores.append((score, str(docs[idx].get("id"))))
        scores.sort(key=lambda item: (-item[0], item[1]))
        ranked = [doc_id for _, doc_id in scores[:k]]
        rows.append({"gold": list(query.get("gold") or []), "ranked": ranked})
    out = aggregate(rows, k)
    out["ranker"] = "bm25"
    out["embeds"] = False
    return out


def prepare(path: Path = EVAL_SET) -> dict:
    """Confere o freeze. Sem ranking não há hit@k — abstenção, não invento número."""
    if not path.is_file():
        return {"ok": False, "why": "eval_set_ausente", "network": False}
    digest = sha256_file(path)
    n_docs = 0
    n_queries = 0
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            kind = json.loads(line).get("type")
            if kind == "doc":
                n_docs += 1
            elif kind == "query":
                n_queries += 1
    frozen = digest == FREEZE_SHA256
    return {
        "schema": "aether.f5_t3_prep/v1",
        "ok": frozen and n_docs == 722 and n_queries == 142,
        "sha256": digest,
        "freeze_match": frozen,
        "n_docs": n_docs,
        "n_queries": n_queries,
        "gate_hit5": GATE_HIT5,
        "k": K,
        "ranking_present": False,
        "hit_at_k": None,
        "why": "sem_ranking_minilm_fica_com_AGENT_A" if frozen else "sha_diverge_do_freeze_0862",
        "network": False,
    }


def selftest() -> int:
    failures: list[str] = []
    easy = aggregate(
        [
            {"gold": ["a"], "ranked": ["a", "b", "c", "d", "e", "f"]},
            # z na 5ª posição — dentro de k=5
            {"gold": ["z"], "ranked": ["a", "b", "c", "d", "z", "e"]},
        ]
    )
    if not easy["gate_pass"] or easy["hits"] != 2:
        failures.append(f"fixture fácil devia passar o gate: {easy}")
    hard = aggregate(
        [
            {"gold": ["a"], "ranked": ["a"]},
            # z na 6ª posição — fora de k=5 → miss
            {"gold": ["z"], "ranked": ["a", "b", "c", "d", "e", "z"]},
        ]
    )
    if hard["hit_at_k"] != 0.5 or hard["gate_pass"]:
        failures.append(f"miss no rank 6 devia ficar em 0.5 e fora do gate: {hard}")
    if hit_at_k([], ["a"]):
        failures.append("ouro vazio não é hit")
    prep = prepare()
    if not prep.get("ok"):
        failures.append(f"freeze: {prep}")
    if prep.get("network") or prep.get("hit_at_k") is not None:
        failures.append("prep não mede hit e não liga rede")
    report = {"ok": not failures, "failures": failures, "prepare": prep, "network": False}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else "selftest"
    if cmd == "selftest":
        return selftest()
    if cmd == "prepare":
        report = prepare()
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if report.get("ok") else 1
    print(json.dumps({"ok": False, "why": "comando_desconhecido", "network": False}))
    return 2


if __name__ == "__main__":
    sys.exit(main())
