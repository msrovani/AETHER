#!/usr/bin/env python3
# F1 A-2 — classificador offline de VRAM (MSG 0908). Não liga ao crate.
"""Decide se o port deve abster (engine_unavailable) antes do decide.

Orçamento canónico (docs/F1_AUDIT.md, Eixo E): 5120 MiB.
Pico isolado medido (2 readers AND): 4393 MiB — cabe com folga.

Uso:
  python tools/f1_vram_watchdog.py check --used-mib 4393
  python tools/f1_vram_watchdog.py check --used-mib 5300 --scope box
  python tools/f1_vram_watchdog.py selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUDGET_MIB = 5120
# Pico isolado auditado (não muda o orçamento).
PEAK_ISOLATED_MIB = 4393


def classify(used_mib: float, budget_mib: float = BUDGET_MIB, scope: str = "process") -> dict:
    """Retorna veredito tipado. Sem side-effect, sem rede, sem nvidia-smi."""
    if used_mib < 0:
        return {
            "ok": False,
            "error": "used_mib_negativo",
            "action": "abstain_engine_unavailable",
            "reason": "invalid_measurement",
        }
    over = used_mib > budget_mib
    return {
        "schema": "aether.f1_vram_watchdog/v1",
        "ok": not over,
        "used_mib": used_mib,
        "budget_mib": budget_mib,
        "scope": scope,  # process | box — box exige A-2c lock (gpu_engine_lock)
        "over_budget": over,
        "action": "proceed" if not over else "abstain_engine_unavailable",
        "meta": {"vram_used_mb": used_mib},
        "network": False,
        "wired_to_crate": False,
        "note": "port deve consultar ANTES do decide; crate = BUFFY/OPENCODE",
    }


def selftest() -> int:
    cases = [
        (classify(4393), True, "proceed"),
        (classify(5120), True, "proceed"),  # teto inclusivo: só `>` aborta
        (classify(5120.1), False, "abstain_engine_unavailable"),
        (classify(5757, scope="box"), False, "abstain_engine_unavailable"),
        (classify(-1), False, "abstain_engine_unavailable"),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep})
    # Gate: pico isolado cabe
    peak = classify(PEAK_ISOLATED_MIB)
    if not peak["ok"]:
        failed.append({"peak": peak})
    out = {
        "ok": len(failed) == 0,
        "budget_mib": BUDGET_MIB,
        "peak_isolated_mib": PEAK_ISOLATED_MIB,
        "failed": failed,
        "n": len(cases),
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--used-mib", type=float, required=True)
    c.add_argument("--budget-mib", type=float, default=BUDGET_MIB)
    c.add_argument("--scope", choices=("process", "box"), default="process")
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = classify(args.used_mib, args.budget_mib, args.scope)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    out = ROOT / "target" / "f1_vram_watchdog.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
