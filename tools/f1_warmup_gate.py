#!/usr/bin/env python3
# F1 A-3 — gate offline de warmup (MSG 0916). Não liga ao crate.
"""Estados Cold/Warming/Ready/Failed + timeout. Sem rede, sem engine.

Uso:
  python tools/f1_warmup_gate.py tick --state Cold --elapsed-s 0
  python tools/f1_warmup_gate.py tick --state Warming --elapsed-s 95
  python tools/f1_warmup_gate.py selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Plano F1: bootstrap 1–2 min determinístico (lazy + warmup thread).
BUDGET_S = 120.0
STATES = ("Cold", "Warming", "Ready", "Failed")


def tick(state: str, elapsed_s: float, budget_s: float = BUDGET_S) -> dict:
    if state not in STATES:
        return {
            "ok": False,
            "error": "estado_invalido",
            "action": "abstain_engine_unavailable",
            "state": state,
        }
    if elapsed_s < 0:
        return {
            "ok": False,
            "error": "elapsed_negativo",
            "action": "abstain_engine_unavailable",
            "state": state,
        }

    if state == "Ready":
        action = "proceed"
        ok = True
        reason = "ready"
    elif state == "Failed":
        action = "abstain_engine_unavailable"
        ok = False
        reason = "warmup_failed"
    elif state == "Cold":
        action = "spawn_warmup"
        ok = True
        reason = "cold_start"
    else:  # Warming
        if elapsed_s > budget_s:
            action = "abstain_engine_unavailable"
            ok = False
            reason = "warmup_timeout"
        else:
            action = "wait_ready"
            ok = True
            reason = "warming"

    return {
        "schema": "aether.f1_warmup_gate/v1",
        "ok": ok,
        "state": state,
        "elapsed_s": elapsed_s,
        "budget_s": budget_s,
        "action": action,
        "reason": reason,
        "network": False,
        "wired_to_crate": False,
        "note": "VegaPort::new deve spawn_warmup+wait_ready; dono=BUFFY bootstrap.rs",
    }


def selftest() -> int:
    cases = [
        (tick("Cold", 0), True, "spawn_warmup"),
        (tick("Warming", 90), True, "wait_ready"),
        (tick("Warming", 120), True, "wait_ready"),  # == budget ainda espera
        (tick("Warming", 120.1), False, "abstain_engine_unavailable"),
        (tick("Ready", 200), True, "proceed"),
        (tick("Failed", 10), False, "abstain_engine_unavailable"),
        (tick("Hot", 0), False, "abstain_engine_unavailable"),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep})
    out = {"ok": len(failed) == 0, "budget_s": BUDGET_S, "failed": failed, "n": len(cases)}
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    t = sub.add_parser("tick")
    t.add_argument("--state", required=True)
    t.add_argument("--elapsed-s", type=float, required=True)
    t.add_argument("--budget-s", type=float, default=BUDGET_S)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = tick(args.state, args.elapsed_s, args.budget_s)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    out = ROOT / "target" / "f1_warmup_gate.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    # 0 = operacional (spawn/wait/proceed); 1 = abstain / erro
    return 0 if rep.get("action") in {"spawn_warmup", "wait_ready", "proceed"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
