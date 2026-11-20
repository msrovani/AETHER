"""F1 — calibra o vega-U com o corpus rotulado do projeto (data/vega_calibration_starter.json).

A calibracao refita temperatura, limiares conformal/abstain/unbound por pergunta
com NOSSOS desfechos e persiste na copia local do checkpoint (o Hub nunca e tocado).

Uso:
  target/vega/Scripts/python.exe tools/vega_calibrate.py [--data data/vega_calibration_starter.json] [--json]
"""

from __future__ import annotations

import argparse
import json
import sys

import vegaml


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", default="data/vega_calibration_starter.json")
    ap.add_argument("--model", default=None, help="caminho do checkpoint (vazio = vega-U default)")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()

    with open(a.data, encoding="utf-8") as f:
        cal = json.load(f)
    questions = cal["questions"]
    examples = [ex for ex in cal["examples"] if "state" not in ex[:1]]  # pares [state, labels]
    n_by_q: dict[str, int] = {}
    for _, labels in examples:
        for qid in labels:
            n_by_q[qid] = n_by_q.get(qid, 0) + 1

    v = vegaml.load(a.model) if a.model else vegaml.load()
    report = vegaml.calibrate(v, examples, questions)

    ece_before = report.get("ece_before")
    ece_after = report.get("ece_after")
    if a.json:
        print(json.dumps({
            "examples_total": len(examples),
            "examples_per_question": n_by_q,
            "ece_before": ece_before,
            "ece_after": ece_after,
            "raw": report,
        }, ensure_ascii=False, indent=2, default=str))
    else:
        print(f"exemplos: {len(examples)} | por pergunta: {n_by_q}")
        print(f"ECE antes -> depois: {ece_before} -> {ece_after}")
        raw = report if isinstance(report, dict) else {}
        for k, val in raw.items():
            if k not in ("ece_before", "ece_after"):
                print(f"{k}: {json.dumps(val, ensure_ascii=False, default=str)[:200]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
