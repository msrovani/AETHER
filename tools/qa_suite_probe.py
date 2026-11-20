#!/usr/bin/env python3
# QA_SUITE_FLAKE — reproduz cargo test e classifica falha (MSG 0879).
"""Roda suite (default: --no-run + f1_contract) e rotula: green | flake_edit | real_fail.

Uso:
  python tools/qa_suite_probe.py
  python tools/qa_suite_probe.py --full-lib
  python tools/qa_suite_probe.py selftest
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "qa_suite_probe.json"

FLAKE_PATTERNS = [
    re.compile(r"failed to parse manifest", re.I),
    re.compile(r"no targets specified in the manifest", re.I),
    re.compile(r"Couldn't read .+ Cargo\.toml", re.I),
    re.compile(r"error: linking with", re.I),  # transient link lock
    re.compile(r"Access is denied", re.I),
    re.compile(r"being used by another process", re.I),
    re.compile(r"Permission denied", re.I),
]


def run_cmd(args: list[str], timeout: int) -> dict:
    t0 = time.time()
    try:
        p = subprocess.run(
            args,
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            env={**os.environ},
        )
        out = (p.stdout or "") + (p.stderr or "")
        return {
            "cmd": args,
            "exit": p.returncode,
            "seconds": round(time.time() - t0, 3),
            "tail": out[-4000:],
        }
    except subprocess.TimeoutExpired as e:
        out = ((e.stdout or b"") + (e.stderr or b"")).decode("utf-8", errors="replace")
        return {
            "cmd": args,
            "exit": -9,
            "seconds": timeout,
            "tail": out[-4000:],
            "timeout": True,
        }


def classify(run: dict) -> str:
    if run.get("exit") == 0:
        return "green"
    text = run.get("tail") or ""
    for pat in FLAKE_PATTERNS:
        if pat.search(text):
            return "flake_edit_or_lock"
    if "panicked at" in text or "FAILED" in text:
        return "real_fail"
    if run.get("timeout"):
        return "timeout"
    return "real_fail_unknown"


def probe(full_lib: bool, timeout: int) -> dict:
    steps = []
    # 1) compile only — barato
    steps.append(run_cmd(["cargo", "test", "--workspace", "--no-run", "--quiet"], timeout))
    # 2) f1_contract suite (gestor: 7/7 evidencia)
    steps.append(
        run_cmd(
            ["cargo", "test", "-p", "aether_engine", "--test", "f1_contract", "--quiet"],
            timeout,
        )
    )
    if full_lib:
        steps.append(run_cmd(["cargo", "test", "-p", "aether_engine", "--lib", "--quiet"], timeout))

    labels = [classify(s) for s in steps]
    overall = "green"
    if any(x.startswith("real_fail") for x in labels):
        overall = "real_fail"
    elif any(x == "flake_edit_or_lock" for x in labels):
        overall = "flake_edit_or_lock"
    elif any(x == "timeout" for x in labels):
        overall = "timeout"

    report = {
        "schema": "aether.qa_suite_probe/v1",
        "ref": "MSG_0879",
        "network": False,
        "when_utc": datetime.now(timezone.utc).isoformat(),
        "steps": [
            {"label": labels[i], "exit": steps[i]["exit"], "seconds": steps[i]["seconds"], "cmd": steps[i]["cmd"]}
            for i in range(len(steps))
        ],
        "tails": [s.get("tail", "")[-800:] for s in steps if s.get("exit", 0) != 0],
        "overall": overall,
        "ok": overall == "green",
        "guidance": {
            "green": "suite reproduzivel nesta passagem",
            "flake_edit_or_lock": "falha tipica de edicao concorrente / lock de arquivo — reexecutar apos quiet",
            "real_fail": "falha com panic/FAILED — abrir BLOQUEIO com teste exato",
        }.get(overall, ""),
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return report


def selftest() -> int:
    # classificador unitario sem cargo longo
    green = classify({"exit": 0, "tail": ""})
    flake = classify({"exit": 101, "tail": "error: failed to parse manifest at ... no targets specified"})
    real = classify({"exit": 101, "tail": "test foo ... FAILED\nthread panicked at src/x.rs:1"})
    ok = green == "green" and flake == "flake_edit_or_lock" and real == "real_fail"
    print(json.dumps({"ok": ok, "green": green, "flake": flake, "real": real}, indent=2))
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--full-lib", action="store_true")
    ap.add_argument("--timeout", type=int, default=600)
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    report = probe(args.full_lib, args.timeout)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
