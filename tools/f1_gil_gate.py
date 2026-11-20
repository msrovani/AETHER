#!/usr/bin/env python3
# F1 A-4 — gate offline de política GIL (MSG CLAIM_A4). Não liga ao crate.
"""Classifica se o forward PyO3 libera o GIL (allow_threads/detach).

Uso:
  python tools/f1_gil_gate.py check --held true --concurrent-threads 2
  python tools/f1_gil_gate.py selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Auditoria: ~2,1 s/decisão com GIL preso (2 readers) — custo de fila, não FLOP.
AUDIT_HELD_S = 2.1


def classify(
    gil_held_during_forward: bool,
    concurrent_threads: int = 1,
    readers: int = 2,
) -> dict:
    """Veredito tipado. Sem rede, sem PyO3, sem editar o crate."""
    if concurrent_threads < 1 or readers < 1:
        return {
            "ok": False,
            "error": "param_invalido",
            "action": "abstain_contract",
            "reason": "threads_or_readers_lt_1",
        }

    liberated = not gil_held_during_forward
    # Serializado (1 thread de host): custo prático zero mesmo com GIL held.
    serial_ok = concurrent_threads == 1
    # Concorrência real (API/daemon): GIL held bloqueia outras threads Rust.
    concurrent_blocked = gil_held_during_forward and concurrent_threads > 1

    if liberated:
        action = "proceed"
        ok = True
        reason = "gil_liberated_allow_threads"
    elif serial_ok:
        action = "proceed_serial_only"
        ok = True
        reason = "chat_serializado_custo_pratico_zero"
    else:
        action = "require_allow_threads"
        ok = False
        reason = "gil_held_blocks_concurrent_host_threads"

    return {
        "schema": "aether.f1_gil_gate/v1",
        "ok": ok,
        "gil_held_during_forward": gil_held_during_forward,
        "liberated": liberated,
        "concurrent_threads": concurrent_threads,
        "readers": readers,
        "audit_held_s_ref": AUDIT_HELD_S,
        "concurrent_blocked": concurrent_blocked,
        "action": action,
        "reason": reason,
        "network": False,
        "wired_to_crate": False,
        "fix": "Python::detach / allow_threads em torno do forward; clonar handle antes",
        "note": "dono do wire=BUFFY pyo3_impl.rs; Eixo D re-probe n=30 antes/depois",
    }


def selftest() -> int:
    cases = [
        (classify(False, 1), True, "proceed"),
        (classify(False, 4), True, "proceed"),
        (classify(True, 1), True, "proceed_serial_only"),
        (classify(True, 2), False, "require_allow_threads"),
        (classify(True, 8, readers=2), False, "require_allow_threads"),
        (classify(True, 0), False, "abstain_contract"),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep})
    out = {
        "ok": len(failed) == 0,
        "audit_held_s_ref": AUDIT_HELD_S,
        "failed": failed,
        "n": len(cases),
        "wired_to_crate": False,
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--held", type=lambda s: s.lower() in {"1", "true", "yes", "sim"}, required=True)
    c.add_argument("--concurrent-threads", type=int, default=1)
    c.add_argument("--readers", type=int, default=2)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = classify(args.held, args.concurrent_threads, args.readers)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    out = ROOT / "target" / "f1_gil_gate.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
