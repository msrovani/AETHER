"""Sonda F0 do vega-U no host AETHER (RTX 3050 6GB / CPU fallback).

Cadeiras eleitas (docs/VEGA_U_ANALISE.md):
  C1 arbitragem de conflito   -> choice entre versoes com certificados
  C2 triagem de quarentena    -> boolean/noul: o fato passou na verificacao?
  C3 escalada do chat         -> choice: escalar | reformular | responder

Regras da casa: decisao do vega e EVIDENCIA (proveniencia vega, quarentena);
so age acima do teto medido — este probe MEDE, nao promove.

Uso:
  target/vega/Scripts/python.exe tools/vega_probe.py [--n 20] [--json]
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time

import vegaml


def pct(xs: list[float], p: float) -> float:
    xs = sorted(xs)
    idx = min(len(xs) - 1, int(round(p * (len(xs) - 1))))
    return xs[idx]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=20)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--device", default=None, help="cuda | mps | cpu (default: auto)")
    ap.add_argument(
        "--model",
        default=None,
        help="caminho de checkpoint (ex.: target/vega/vega-u-gemma-1b) ou vazio = vega-U default",
    )
    a = ap.parse_args()

    t0 = time.time()
    load_kw: dict = {}
    if a.device:
        load_kw["device"] = a.device
    if a.model:
        v = vegaml.load(a.model, **load_kw)
    else:
        v = vegaml.load(**load_kw)  # vega-U, reader Qwen3.5-0.8B (o padrao)
    load_s = time.time() - t0
    device = str(getattr(v, "device", "?"))

    import torch

    report: dict = {"load_s": round(load_s, 1), "device": device}
    if torch.cuda.is_available():
        report["gpu"] = torch.cuda.get_device_name(0)
        torch.cuda.reset_peak_memory_stats()

    # ---- T0: latencia de decisao (facto medido) --------------------------
    state = (
        "I was charged 320.00 twice for the booking system this week. "
        "Please refund the duplicate and confirm by email."
    )
    questions = {
        "team": {
            "type": "choice",
            "instructions": "Which team should handle this?",
            "criteria": {
                "billing": "payments and invoices",
                "technical": "product faults",
                "sales": "new business",
            },
        }
    }
    v.decide(state, questions)  # warmup
    lat: list[float] = []
    for _ in range(a.n):
        t = time.time()
        v.decide(state, questions)
        lat.append((time.time() - t) * 1000.0)
    report["latency_ms"] = {
        "p50": round(statistics.median(lat), 1),
        "p95": round(pct(lat, 0.95), 1),
        "min": round(min(lat), 1),
        "max": round(max(lat), 1),
        "n": a.n,
    }
    if torch.cuda.is_available():
        report["vram_peak_gb"] = round(torch.cuda.max_memory_allocated() / 1e9, 2)

    # ---- T1: doutrina act — choice com probs e flags ---------------------
    out = v.decide(state, questions)
    team = out["answers"]["team"]
    report["T1_act"] = {
        "choice": team.get("choice"),
        "probs": team.get("probs"),
        "abstain": team.get("abstain"),
        "unbound": team.get("unbound"),
        "readout": team.get("readout"),
    }

    # ---- T2: doutrina abstain — estado sem sentido (OOD) ----------------
    ood_questions = {
        "area": {
            "type": "choice",
            "instructions": "Which area is this about?",
            "criteria": {
                "billing": "money and invoices",
                "technical": "devices and software",
                "ops": "logistics and incidents",
            },
        }
    }
    out2 = v.decide("zxq qwerty uiop 42 ??? %%", ood_questions)
    area = out2["answers"]["area"]
    report["T2_abstain_ood"] = {
        "choice": area.get("choice"),
        "probs": area.get("probs"),
        "abstain": area.get("abstain"),
        "unbound": area.get("unbound"),
        "readout": area.get("readout"),
    }

    # ---- C1: arbitragem de conflito (caso real MSG 0520 vs 0521) --------
    c1_state = {
        "conflict": "Capacidade do shard B2 (fatos do forum, ambos medidos)",
        "A": "capacidade 4096 (lei 0.4xD), recall 1.000, p99 11us em release",
        "B": "10000 monolitico, recall 0.7676 (over capacity, degradacao prevista)",
    }
    c1_questions = {
        "winner": {
            "type": "choice",
            "instructions": "Which version should win this conflict?",
            "criteria": {
                "A": "shard dentro da capacidade com recall 1.000 medido",
                "B": "monolitico acima da capacidade com recall degradado",
            },
        }
    }
    out3 = v.decide(c1_state, c1_questions)
    w = out3["answers"]["winner"]
    report["C1_arbitragem"] = {
        "choice": w.get("choice"),
        "probs": w.get("probs"),
        "abstain": w.get("abstain"),
        "readout": w.get("readout"),
        "esperado": "A",
    }

    # ---- C2: triagem de quarentena (3 fatos reais) -----------------------
    c2_questions = {
        "passou": {
            "type": "boolean",
            "instructions": "Does this fact pass verification (gate verde, medido, sem violacao)?",
        }
    }
    c2_states = [
        "Gate cargo_test_release: 176 passed, 0 failed, clippy -D warnings limpo, fmt OK",
        "Fato com certificado E=-1.0, gap 0.1, violacoes=3 reportadas pelo solver",
        "Receita de bolo leva ovos, farinha e acucar por 40 minutos",
    ]
    c2_rows = []
    for s in c2_states:
        o = v.decide(s, c2_questions)
        pa = o["answers"]["passou"]
        c2_rows.append({
            "state": s[:60],
            "boolean_p_true": pa.get("boolean"),
            "abstain": pa.get("abstain"),
            "unbound": pa.get("unbound"),
        })
    report["C2_triagem"] = c2_rows

    # ---- C3: escalada do chat (residuo ambiguo vs claro) ----------------
    c3_questions = {
        "route": {
            "type": "choice",
            "instructions": "What should the chat edge do with this user turn?",
            "criteria": {
                "escalar": "ambiguous, needs the human",
                "reformular": "ask the user a short clarifying question",
                "responder": "edge can answer from lexicon context",
            },
        }
    }
    c3_states = [
        "e a Franca?",  # follow-up sem contexto (parser OOV)
        "qual e a capital do Brasil",  # claro, no lexico
        "pfvyw?? !! 123",  # gibberish
    ]
    c3_rows = []
    for s in c3_states:
        o = v.decide(s, c3_questions)
        r = o["answers"]["route"]
        c3_rows.append({
            "state": s,
            "choice": r.get("choice"),
            "probs": r.get("probs"),
            "abstain": r.get("abstain"),
        })
    report["C3_escalada"] = c3_rows

    if a.json:
        print(json.dumps(report, ensure_ascii=False, indent=2, default=str))
    else:
        print(f"vega-U carregado em {report['load_s']}s | device={device}")
        print(f"latencia: p50={report['latency_ms']['p50']}ms "
              f"p95={report['latency_ms']['p95']}ms (n={a.n})")
        for key in ("T1_act", "T2_abstain_ood", "C1_arbitragem"):
            print(f"{key}: {json.dumps(report[key], ensure_ascii=False, default=str)}")
        print("C2_triagem:")
        for row in report["C2_triagem"]:
            print(f"  {json.dumps(row, ensure_ascii=False, default=str)}")
        print("C3_escalada:")
        for row in report["C3_escalada"]:
            print(f"  {json.dumps(row, ensure_ascii=False, default=str)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
