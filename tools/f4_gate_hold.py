#!/usr/bin/env python3
# F4 — o teto 0.974 permanece; esta geração está reprovada (MSG 0947/0953).
"""Confere o congelamento e recusa baixar o gate. Não escreve o dataset.

Uso:
  python tools/f4_gate_hold.py selftest
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATASET = ROOT / "data" / "embed_eval_set.jsonl"
# Hash publicado no freeze (MSG 0862/0875), não o sufixo abreviado do doc 255.
EXPECTED_SHA = "f868244256ee75a896f39cf4092ccfbf3bdf915002c451a1dcf450255d416bf0"
GATE = 0.974
BM25 = 0.7042
EMBED = 0.5141
HYBRID = 0.7465


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def hold() -> dict:
    failures: list[str] = []
    if not DATASET.exists():
        failures.append("dataset ausente")
        digest = ""
        gate_in_file = None
    else:
        digest = sha256_file(DATASET)
        if digest != EXPECTED_SHA:
            failures.append("sha do dataset divergiu do freeze")
        first = DATASET.open(encoding="utf-8").readline()
        meta = json.loads(first)
        gate_in_file = float(meta.get("gate", {}).get("hit5", -1))
        if gate_in_file != GATE:
            failures.append(f"gate no arquivo want={GATE} got={gate_in_file}")
        if int(meta.get("n_docs", 0)) != 722 or int(meta.get("n_queries", 0)) != 142:
            failures.append("contagem 722/142 mudou")
    if not (HYBRID < GATE):
        failures.append("hibrido nao esta abaixo do teto")
    if not (HYBRID > BM25):
        failures.append("hibrido nao supera o lexical")
    if EMBED >= GATE:
        failures.append("embed puro nao pode ser tratado como gate")
    return {
        "ok": not failures,
        "failures": failures,
        "verdict": "REPROVADO_NESTA_GERACAO",
        "gate_hit5": GATE,
        "bm25_hit5": BM25,
        "embed_hit5": EMBED,
        "hybrid_hit5": HYBRID,
        "sha256": digest,
        "sha_ok": digest == EXPECTED_SHA,
        "gate_in_file": gate_in_file,
        "lowered": False,
        "dataset_written": False,
        "ref": "MSG_0947_0953",
    }


def main() -> int:
    report = hold()
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
