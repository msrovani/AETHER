#!/usr/bin/env python3
# F5 T0 — suite própria: research + harness + flywheel (sem rede, sem 255).
"""Roda os selftests das entregas CURSOR (F2/F2.5/F3) como eval tier T0.

Uso:
  python tools/f5_t0_eval.py
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CHECKS = [
    ("research_reliability", [sys.executable, str(ROOT / "tools" / "research_reliability.py"), "selftest"]),
    ("research_harness", [sys.executable, str(ROOT / "tools" / "research_harness.py"), "selftest"]),
    ("vega_corpus_grow", [sys.executable, str(ROOT / "tools" / "vega_corpus_grow.py"), "selftest"]),
    ("f1_abstain_map_mf2", [sys.executable, str(ROOT / "tools" / "f1_abstain_map.py"), "selftest"]),
    ("mf2_gate_regression", [sys.executable, str(ROOT / "tools" / "mf2_gate_regression.py")]),
    ("f5_t1_agent_loop", [sys.executable, str(ROOT / "tools" / "f5_t1_agent_loop.py"), "selftest"]),
    ("f5_t2_allowlist", [sys.executable, str(ROOT / "tools" / "f5_t2_allowlist.py"), "selftest"]),
]


def main() -> int:
    results = []
    failed = 0
    for name, cmd in CHECKS:
        p = subprocess.run(cmd, capture_output=True, text=True, check=False)
        ok = p.returncode == 0
        if not ok:
            failed += 1
        results.append(
            {
                "name": name,
                "ok": ok,
                "exit": p.returncode,
                "tail": (p.stdout or p.stderr)[-400:],
            }
        )
    report = {
        "schema": "aether.f5_t0_eval/v1",
        "tier": "T0",
        "ok": failed == 0,
        "failed": failed,
        "n": len(CHECKS),
        "network": False,
        "results": results,
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
