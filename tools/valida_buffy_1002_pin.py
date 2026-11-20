#!/usr/bin/env python3
# CURSOR — validação readonly do pin sys_path (BUFFY MSG 1002).
"""Não edita aether-decision. Só lê fontes/docs e emite relatório em data/.

Uso:
  python tools/valida_buffy_1002_pin.py
  python tools/valida_buffy_1002_pin.py selftest
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CHECKS = [
    (
        "pin_fn_public",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "pub fn pin_sys_path",
    ),
    (
        "pin_before_import_call",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "pin_sys_path(py, cfg)",
    ),
    (
        "erro_acionavel_path",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "python_site_packages nao existe",
    ),
    (
        "test_inserts",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "fn pin_sys_path_inserts_the_configured_site_packages",
    ),
    (
        "test_rejects",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "fn pin_sys_path_rejects_a_path_that_does_not_exist",
    ),
    (
        "test_noop",
        ROOT / "aether-decision" / "src" / "pyo3_impl.rs",
        "fn pin_sys_path_is_a_noop_without_config",
    ),
    (
        "smoke_grava_efetivo",
        ROOT / "aether-decision" / "examples" / "f1_smoke.rs",
        '"python_site_packages"',
    ),
    (
        "effective_site_packages",
        ROOT / "aether-decision" / "src" / "config.rs",
        "fn effective_site_packages",
    ),
    (
        "doc_contrato",
        ROOT / "docs" / "F1_CONTRATO_MEDIDO.md",
        "pin_sys_path",
    ),
]


def run_checks() -> dict:
    results = []
    failed = 0
    for name, path, needle in CHECKS:
        ok = path.is_file() and needle in path.read_text(encoding="utf-8", errors="replace")
        if not ok:
            failed += 1
        results.append(
            {
                "name": name,
                "ok": ok,
                "path": path.relative_to(ROOT).as_posix(),
                "needle": needle,
            }
        )
    report = {
        "schema": "aether.valida_buffy_1002_pin/v1",
        "ref": "MSG_1002_1010",
        "ok": failed == 0,
        "failed": failed,
        "n": len(CHECKS),
        "network": False,
        "mutated_crate": False,
        "results": results,
    }
    out = ROOT / "data" / "reports" / "valida_buffy_1002_pin.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    report["report"] = "data/reports/valida_buffy_1002_pin.json"
    return report


def selftest() -> int:
    report = run_checks()
    # selftest exige que o repo real passe (ambiente de desenvolvimento).
    out = {
        "ok": report["ok"],
        "failures": [r["name"] for r in report["results"] if not r["ok"]],
        "n": report["n"],
        "mutated_crate": False,
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main(argv: list[str]) -> int:
    if argv and argv[0] == "selftest":
        return selftest()
    print(json.dumps(run_checks(), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
