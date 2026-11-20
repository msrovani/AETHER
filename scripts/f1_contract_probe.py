"""F1 - sonda de contrato: o que o `vegaml.decide` devolve DE VERDADE.

Motivo: o binding Rust (`aether-decision/src/pyo3_impl.rs`) extrai campos por
nome. Campo ausente nao levanta erro - vira `None` silencioso, a mesma classe
de bug do parser que deslocou os rotulos do B-G0 (MSG 0567). Esta sonda mede o
schema real e grava tipos, nao valores de sorte.

Medido em 2026-11-20 (MSG 0771):
  - `type=boolean` NAO devolve `choice` (so `boolean`, float, e `probs`);
  - `type=choice` devolve `choice` (str) E `abstain` pode ser True com `choice`
    preenchido (amostra nao e decisao);
  - `prediction_set` (lista) existe nos dois e nao tem casa no schema Rust;
  - topo: answers, latency_ms, n_tokens, views, mode.

Uso:
  target/vega/Scripts/python.exe scripts/f1_contract_probe.py \
      --model target/vega/vega-u-gemma-1b-prose --n 5

Exit 0 = mediu; exit 3 = ABSTAIN (sem modelo/pacote, artefato com motivo).
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import traceback

STATE = (
    "Gate cargo_test_release: 176 passed, 0 failed, clippy -D warnings limpo, fmt OK"
)
CHOICE_STATE = (
    "I was charged 320.00 twice for the booking system this week. "
    "Please refund the duplicate and confirm by email."
)


def describe(answer: dict) -> dict:
    """Tipos e presenca de campo - nunca o valor como prova de contrato."""
    return {
        "keys": list(answer),
        "types": {k: type(v).__name__ for k, v in answer.items()},
        "has_choice": "choice" in answer,
        "has_prediction_set": "prediction_set" in answer,
        "abstain": bool(answer.get("abstain", False)),
        "choice_present_with_abstain": bool(answer.get("choice")) and bool(
            answer.get("abstain", False)
        ),
        "sample": {k: repr(v)[:120] for k, v in answer.items()},
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="target/vega/vega-u-gemma-1b-prose")
    ap.add_argument("--corpus", default="data/vega_calibration_starter.json")
    ap.add_argument("--n", type=int, default=5, help="amostras de latencia (apos warmup)")
    ap.add_argument(
        "--fit-n",
        type=int,
        default=40,
        help="exemplos de calibracao no fit (124 liga os heads TTT; 40 nao)",
    )
    ap.add_argument("--out", default="target/f1_contract.json")
    args = ap.parse_args()

    rec: dict = {"schema": "aether.f1_contract/v1", "model": args.model}
    try:
        import vegaml  # noqa: PLC0415 - import tardio: ausencia vira ABSTAIN, nao crash

        rec["vegaml"] = getattr(vegaml, "__version__", "?")
        corpus = json.load(open(args.corpus, encoding="utf-8"))

        t0 = time.time()
        engine = vegaml.load(args.model)
        rec["load_s"] = round(time.time() - t0, 1)

        t0 = time.time()
        engine.fit(corpus["examples"][: args.fit_n], corpus["questions"])
        rec["fit_examples"] = min(args.fit_n, len(corpus["examples"]))
        rec["fit_s"] = round(time.time() - t0, 1)

        engine.decide(STATE, {"passou": corpus["questions"]["passou"]})  # warmup
        latencies = []
        for _ in range(args.n):
            t = time.time()
            out = engine.decide(
                STATE,
                {
                    "passou": corpus["questions"]["passou"],
                    "route": corpus["questions"]["route"],
                },
            )
            latencies.append((time.time() - t) * 1000.0)
        rec["latency_ms"] = {
            "p50": round(statistics.median(latencies), 1),
            "min": round(min(latencies), 1),
            "max": round(max(latencies), 1),
            "n": len(latencies),
        }
        rec["top_keys"] = list(out)
        rec["meta"] = {k: out[k] for k in out if k != "answers"}
        rec["boolean"] = describe(out["answers"]["passou"])
        rec["choice"] = describe(out["answers"]["route"])

        route = engine.decide(CHOICE_STATE, {"route": corpus["questions"]["route"]})
        rec["choice_on_choice_state"] = describe(route["answers"]["route"])
    except Exception:
        rec["abstain"] = True
        rec["error"] = traceback.format_exc()[-1200:]
        with open(args.out, "w", encoding="utf-8") as fh:
            json.dump(rec, fh, ensure_ascii=False, indent=1)
        print(json.dumps(rec, ensure_ascii=False, indent=1))
        return 3

    with open(args.out, "w", encoding="utf-8") as fh:
        json.dump(rec, fh, ensure_ascii=False, indent=1)
    print(json.dumps(rec, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
