#!/usr/bin/env python3
# F5 T3 prep — contrato hit@k offline. Não baixa modelo e não liga rede.
"""Confere o freeze e calcula hit@k só quando um ranking é passado.

O BM25 lexical é outra missão (MSG 0873 / 0870), noutro arquivo.
O MiniLM continua com o AGENT-A. O teto 0.974 não baixa.

Uso:
  python tools/f5_t3_eval.py selftest
  python tools/f5_t3_eval.py prepare
  python tools/f5_t3_eval.py eval [rankings.json]
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


def eval_rankings(rankings_path: Path) -> dict:
    """F5_T3 full (MSG 0951): aplica o gate aos rankings materializados, offline.

    Aditivo pedido no CLAIM 0951 — `selftest` e `prepare` intocados. Recusa
    qualquer JSON que não aponte para o freeze MSG 0862 e qualquer ranking
    cujo gold divirja do dataset congelado (o dataset manda, não o JSON.
    """
    prep = prepare()
    if not prep.get("ok"):
        return {"ok": False, "why": prep.get("why"), "schema": "aether.f5_t3_eval/v1",
                "network": False}
    if not rankings_path.is_file():
        return {"ok": False, "why": "rankings_ausentes", "path": str(rankings_path),
                "network": False}
    payload = json.loads(rankings_path.read_text(encoding="utf-8"))
    if payload.get("dataset_sha256") != FREEZE_SHA256:
        return {"ok": False, "why": "rankings_nao_referenciam_o_freeze_0862", "network": False}

    gold_of: dict[str, list[str]] = {}
    kind_of: dict[str, str] = {}
    with EVAL_SET.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row.get("type") == "query":
                gold_of[row["id"]] = row["gold"]
                kind_of[row["id"]] = row.get("kind", "outro")

    failures: list[str] = []
    results: dict[str, dict] = {}
    kinds: dict[str, dict] = {}
    for name, mapping in (payload.get("systems") or {}).items():
        rows: list[dict] = []
        buckets: dict[str, list[dict]] = {}
        sys_fail = False
        for qid, gold in gold_of.items():
            ranked = mapping.get(qid)
            if not isinstance(ranked, list):
                failures.append(f"{name}:{qid}_ausente")
                sys_fail = True
                continue
            rows.append({"gold": gold, "ranked": ranked})
            buckets.setdefault(kind_of[qid], []).append({"gold": gold, "ranked": ranked})
        if sys_fail:
            continue
        results[name] = aggregate(rows)
        kinds[name] = {
            k: {"n": len(v), "hit_at_k": aggregate(v)["hit_at_k"]}
            for k, v in sorted(buckets.items())
        }

    lex = results.get("lexical_bm25", {}).get("hit_at_k", 0.0)
    embed = results.get("embed_minilm", {}).get("hit_at_k", 0.0)
    hyb = results.get("hybrid_rrf60", {}).get("hit_at_k", 0.0)
    gate = {
        "hit_at_k_min": GATE_HIT5,
        "embed_pass": embed >= GATE_HIT5,
        "hybrid_pass": hyb >= GATE_HIT5,
        "embed_beats_or_ties_lexical": embed >= lex,
        "hybrid_beats_or_ties_lexical": hyb >= lex,
        "gate_pass": embed >= GATE_HIT5 and hyb >= GATE_HIT5,
    }
    report = {
        "schema": "aether.f5_t3_eval/v1",
        "ok": not failures,
        "sha256_set": FREEZE_SHA256,
        "n_queries": len(gold_of),
        "rankings": rankings_path.name,
        "k": K,
        "systems": results,
        "per_kind": kinds,
        "gate": gate,
        "failures": failures,
        "network": False,
        "ref": "MSG_0951_0862",
    }
    out = ROOT / "target" / "f5_t3_eval.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return report


def _synthetic_rankings_for_freeze() -> Path:
    """Rankings sintéticos com gold no topo — só para selftest do subcomando eval."""
    mapping: dict[str, list[str]] = {}
    with EVAL_SET.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row.get("type") != "query":
                continue
            gold = list(row.get("gold") or [])
            # pad para k=5 sem inventar ids do corpus
            ranked = gold[:K] + [f"pad:{row['id']}:{i}" for i in range(K)]
            mapping[row["id"]] = ranked[:K]
    path = ROOT / "target" / "f5_t3_rankings_selftest_synth.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schema": "aether.f5_t3_rankings_fixture/v1",
                "dataset_sha256": FREEZE_SHA256,
                "k": K,
                "systems": {
                    "lexical_bm25": mapping,
                    "embed_minilm": mapping,
                    "hybrid_rrf60": mapping,
                },
                "note": "sintetico_selftest_nao_e_MiniLM",
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return path


def selftest() -> int:
    failures: list[str] = []
    easy = aggregate(
        [
            {"gold": ["a"], "ranked": ["a", "b", "c", "d", "e", "f"]},
            {"gold": ["z"], "ranked": ["a", "b", "c", "d", "z", "e"]},
        ]
    )
    if not easy["gate_pass"] or easy["hits"] != 2:
        failures.append(f"fixture fácil devia passar o gate: {easy}")
    hard = aggregate(
        [
            {"gold": ["a"], "ranked": ["a"]},
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

    # MSG 0951: subcomando eval aditivo — selftest/prepare intactos + prova offline
    bad_sha = ROOT / "target" / "f5_t3_rankings_bad_sha.json"
    bad_sha.write_text(
        json.dumps({"dataset_sha256": "deadbeef", "systems": {}}, ensure_ascii=False),
        encoding="utf-8",
    )
    refused = eval_rankings(bad_sha)
    if refused.get("ok") or refused.get("why") != "rankings_nao_referenciam_o_freeze_0862":
        failures.append(f"eval deve recusar sha errado: {refused}")

    synth = _synthetic_rankings_for_freeze()
    full = eval_rankings(synth)
    if not full.get("ok"):
        failures.append(f"eval synth ok=false: {full.get('failures')}")
    elif not full.get("gate", {}).get("gate_pass"):
        failures.append(f"eval synth gate_pass=false: {full.get('gate')}")
    elif full.get("systems", {}).get("embed_minilm", {}).get("hit_at_k", 0) < GATE_HIT5:
        failures.append("eval synth hit5 abaixo do teto")

    report = {
        "ok": not failures,
        "failures": failures,
        "prepare": prep,
        "eval_additive": {
            "refused_bad_sha": not refused.get("ok"),
            "synth_gate_pass": bool(full.get("gate", {}).get("gate_pass")),
            "n_queries": full.get("n_queries"),
        },
        "network": False,
    }
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
    if cmd == "eval":
        path = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "data" / "f5_t3_rankings_agent_a.json"
        if not path.is_absolute():
            path = (ROOT / path).resolve() if not path.is_file() else path.resolve()
        report = eval_rankings(path)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        if not report.get("ok"):
            return 1
        return 0 if report.get("gate", {}).get("gate_pass") else 2
    print(json.dumps({"ok": False, "why": "comando_desconhecido", "network": False}))
    return 2


if __name__ == "__main__":
    sys.exit(main())
