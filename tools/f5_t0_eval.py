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
    # F1 offline (MSG 0948): A2/A2c/A3/A4/F1_5/A5 — sem crate, sem rede
    ("f1_offline_suite", [sys.executable, str(ROOT / "tools" / "f1_offline_suite.py"), "selftest"]),
    ("vega_path_audit", [sys.executable, str(ROOT / "tools" / "vega_path_audit.py"), "selftest"]),
    ("qa_suite_probe", [sys.executable, str(ROOT / "tools" / "qa_suite_probe.py"), "selftest"]),
    ("f5_t1_agent_loop", [sys.executable, str(ROOT / "tools" / "f5_t1_agent_loop.py"), "selftest"]),
    ("f5_t2_allowlist", [sys.executable, str(ROOT / "tools" / "f5_t2_allowlist.py"), "selftest"]),
    ("f5_t3_eval", [sys.executable, str(ROOT / "tools" / "f5_t3_eval.py"), "selftest"]),
    # MSG 0978 — A5 evidência (0967) no T0; tool alheia só selftest
    ("a5_evidencia_audit", [sys.executable, str(ROOT / "tools" / "a5_evidencia_audit.py"), "selftest"]),
    # MSG 0998 — P0/P2 promote durável no T0
    ("a5_p0_promote", [sys.executable, str(ROOT / "tools" / "a5_p0_promote.py"), "selftest"]),
    ("a5_p2_promote", [sys.executable, str(ROOT / "tools" / "a5_p2_promote.py"), "selftest"]),
    # MSG 1010 — validação readonly pin BUFFY 1002
    ("valida_buffy_1002_pin", [sys.executable, str(ROOT / "tools" / "valida_buffy_1002_pin.py"), "selftest"]),
    # MSG 1015 — F2_WIRE port→pesquisa
    ("f2_wire_port", [sys.executable, str(ROOT / "tools" / "f2_wire_port.py"), "selftest"]),
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
