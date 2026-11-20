#!/usr/bin/env python3
# F1 — suite offline dos gates CURSOR (MSG CLAIM 0929). Não liga ao crate.
"""Roda selftests A-2/A-3/A-4/F1-5/A-5 promote em um comando.

Uso:
  python tools/f1_offline_suite.py selftest
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

GATES = [
    ("A2_vram", "tools/f1_vram_watchdog.py"),
    # MSG 1014: tranca canónica em .vega/engine.lock (não target/)
    ("A2c_engine_lock", "tools/engine_lock.py"),
    ("A3_warmup", "tools/f1_warmup_gate.py"),
    ("A4_gil", "tools/f1_gil_gate.py"),
    ("F1_5_ttt", "tools/f1_5_ttt_abstain.py"),
    ("A5_promote", "tools/f1_gate_promote.py"),
    # MSG 1025/1033: classificador AttributeError torch.cpu.device (offline)
    ("torch_cpu_device", "tools/f1_torch_device_check.py"),
]


def run_one(name: str, rel: str) -> dict:
    path = ROOT / rel
    if not path.is_file():
        return {"name": name, "ok": False, "error": "missing", "path": rel}
    proc = subprocess.run(
        [sys.executable, str(path), "selftest"],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    payload = None
    raw = proc.stdout or ""
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        # tools que misturam log + JSON no stdout: pega o último objeto
        start = raw.rfind("\n{")
        if start < 0:
            start = raw.find("{")
        else:
            start += 1
        try:
            payload = json.loads(raw[start:]) if start >= 0 else None
        except json.JSONDecodeError:
            payload = {"raw_tail": raw[-400:], "parse": False}
    if payload is None:
        payload = {"raw_tail": raw[-400:], "parse": False}

    ok = proc.returncode == 0
    if isinstance(payload, dict) and "ok" in payload:
        ok = bool(payload.get("ok")) and ok
    elif isinstance(payload, dict) and "promote" in payload:
        # selftest do promote: promote.ok true + reject_target esperado
        ok = bool(payload.get("promote", {}).get("ok")) and ok

    return {
        "name": name,
        "path": rel,
        "ok": ok,
        "returncode": proc.returncode,
        "wired_to_crate": False,
    }


def selftest() -> int:
    results = [run_one(n, p) for n, p in GATES]
    failed = [r for r in results if not r["ok"]]
    out = {
        "schema": "aether.f1_offline_suite/v1",
        "ok": len(failed) == 0,
        "n": len(results),
        "failed": [r["name"] for r in failed],
        "results": results,
        "wired_to_crate": False,
        "network": False,
        "note": "suite de classificador offline; wire crate = BUFFY/OPENCODE",
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    dest = ROOT / "target" / "f1_offline_suite.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if out["ok"] else 1


def main() -> int:
    if len(sys.argv) < 2 or sys.argv[1] != "selftest":
        print("uso: python tools/f1_offline_suite.py selftest", file=sys.stderr)
        return 2
    return selftest()


if __name__ == "__main__":
    raise SystemExit(main())
