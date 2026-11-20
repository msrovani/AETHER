#!/usr/bin/env python3
# MF2 — espelho Python de derive_abstain_reason (aether-decision types.rs F1_3).
"""Regressão do contrato: unknown só com unbound; margem fina = ambiguous.

Uso:
  python tools/f1_abstain_map.py selftest
  python tools/f1_abstain_map.py map --abstain --unbound
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "data" / "f1_gate_verdicts_fixture.json"


def derive_abstain_reason(
    abstain: bool, unbound: bool, prediction_set_len: int | None
) -> str | None:
    """Espelho de aether_decision::types::derive_abstain_reason (MSG 0793 F1_3)."""
    if not abstain:
        return None
    if unbound:
        return "unknown"
    # Margem fina ou abstain sem classe -> HITL, nunca lacuna.
    if prediction_set_len is not None and prediction_set_len > 1:
        return "ambiguous"
    return "ambiguous"


def admits_research(reason: str | None) -> bool:
    return reason == "unknown"


def selftest() -> int:
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))
    failures: list[str] = []
    for case in fixture["cases"]:
        got = derive_abstain_reason(
            case["abstain"], case["unbound"], case["prediction_set_len"]
        )
        want = case["expect_reason"]
        if got != want:
            failures.append(f"{case['id']}: reason want={want} got={got}")
        adm = admits_research(got)
        if adm != case["expect_admits_research"]:
            failures.append(
                f"{case['id']}: admits_research want={case['expect_admits_research']} got={adm}"
            )

    # harness gate: só unknown passa
    from research_harness import run_research, DEFAULT_CASES, DEFAULT_TIER  # noqa: WPS433

    for reason, expect_code in (("unknown", 0), ("ambiguous", 2), ("conflict", 2)):
        _, code = run_research(reason, "terra_lua", DEFAULT_CASES, DEFAULT_TIER, None)
        if reason == "unknown" and code != 0:
            failures.append(f"harness unknown exit={code}")
        if reason != "unknown" and code != 2:
            failures.append(f"harness {reason} exit={code} want=2")

    report = {
        "ok": not failures,
        "failures": failures,
        "n_fixture": len(fixture["cases"]),
        "rule": fixture["rule"],
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    p_map = sub.add_parser("map")
    p_map.add_argument("--abstain", action="store_true")
    p_map.add_argument("--unbound", action="store_true")
    p_map.add_argument("--set-len", type=int, default=None)
    args = ap.parse_args()
    if args.cmd == "selftest":
        sys.path.insert(0, str(ROOT / "tools"))
        return selftest()
    reason = derive_abstain_reason(args.abstain, args.unbound, args.set_len)
    print(
        json.dumps(
            {
                "abstain_reason": reason,
                "admits_research": admits_research(reason),
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
