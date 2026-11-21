#!/usr/bin/env python3
"""Confirma: pesquisa N grava memória → turno N+1 cita a evidência.

Uso:
  $env:AETHER_RESEARCH_NETWORK=1
  python tools/research_recall_confirm.py
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOOK = ROOT / "tools" / "f2_chat_hook.py"
BIN = ROOT / "target" / "release" / "aether-agent.exe"
Q = "e a capital da Franca?"


def run(cmd: list[str], env: dict, timeout: int = 180) -> tuple[int, str]:
    p = subprocess.run(
        cmd,
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    return p.returncode, (p.stdout or "") + "\n" + (p.stderr or "")


def main() -> int:
    env = dict(os.environ)
    env["AETHER_RESEARCH_NETWORK"] = "1"
    env["AETHER_RESEARCH_PROVIDER"] = "cascade"
    env.setdefault("RUST_LOG", "error")
    if not BIN.exists():
        print(json.dumps({"ok": False, "error": "build aether-agent release"}, ensure_ascii=False))
        return 1

    # 1) Pesquisa viva + persist
    code1, out1 = run([sys.executable, str(HOOK), "once", "--text", Q, "--reason", "unknown"], env)
    line1 = next((ln for ln in out1.splitlines() if ln.strip()), "")
    persisted = "mem=ok" in line1
    researched = line1.casefold().startswith("pesquisa")

    time.sleep(0.5)

    # 2) Chat subsequente — deve CITAR memória com PESQUISA_EXTERNA / França
    code2, out2 = run([str(BIN), "chat", Q], env)
    cites_research = "PESQUISA_EXTERNA" in out2 or "pesquisa_externa" in out2.casefold()
    cites_franca = "França" in out2 or "franca" in out2.casefold() or "Paris" in out2
    has_memory_block = "memória (citação" in out2.casefold() or "memoria (citacao" in out2.casefold()

    report = {
        "ok": researched and persisted and has_memory_block and (cites_research or cites_franca),
        "turn1": {
            "exit": code1,
            "line": line1[:220],
            "researched": researched,
            "persisted": persisted,
        },
        "turn2": {
            "exit": code2,
            "has_memory_block": has_memory_block,
            "cites_research_marker": cites_research,
            "cites_franca_or_paris": cites_franca,
            "tail": "\n".join(
                [
                    ln
                    for ln in out2.splitlines()
                    if ln.strip()
                    and not ln.startswith("[neural-sgdb]")
                    and "Sgdb open" not in ln
                    and "stdin fechado" not in ln
                ][-12:]
            ),
        },
        "ref": "research_persist+recall",
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
