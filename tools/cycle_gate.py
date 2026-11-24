"""Gate contínuo de evals (G6 do plano de fechamento) — corre ANTES de cada
`chore(cycle)` (commit de versão). Falha de tier = não versiona.

Tiers rodados (todos offline, sem rede):
  T0     tools/f5_t0_eval.py            — reliability + harness + corpus
  MF2    tools/mf2_gate_regression.py   — taxonomia F1_3 × harness F2
  OFF    tools/f1_offline_suite.py      — A2/A2c/A3/A4/F1_5/A5 + torch device
  HOOK   tools/f2_chat_hook.py selftest — cabo do chat (offline)
  TESTS  cargo test --lib (aether_engine + aether_decision)
  A1     cargo test --test agent_tab_gate  (T-TAB, docs/PLANO_A1_D4.md)
  A2     cargo test --test agent_eval_report_gate (T-EVAL, run_id/hash)
  A3     cargo test --test agent_p2p_gate (T-P2P, delta Merkle/contradicts)
  A4     cargo test --test agent_clip_gate (T-CLIP, stdin/redact/code.patch)

Uso:
  python tools/cycle_gate.py            # roda tudo, sai 0 se verde
  python tools/cycle_gate.py --fast     # só Python (sem cargo test)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

# Saída sempre UTF-8: o JSON traz setas/acentos e no Windows o default cp1252
# mata o `> data/cycle_gate.json` com UnicodeEncodeError.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parents[1]
PY = str(ROOT / ".vega" / "venv" / "Scripts" / "python.exe")
if not Path(PY).exists():
    PY = sys.executable

CHECKS = [
    ("T0_eval", [PY, "tools/f5_t0_eval.py"]),
    ("MF2_regression", [PY, "tools/mf2_gate_regression.py"]),
    ("F1_offline_suite", [PY, "tools/f1_offline_suite.py", "selftest"]),
    ("F2_chat_hook", [PY, "tools/f2_chat_hook.py", "selftest"]),
]


def _ok_json(text: str) -> bool | None:
    """Extrai um `ok` booleano do stdout (quando o check é JSON)."""
    try:
        d = json.loads(text)
        if isinstance(d, dict) and "ok" in d:
            return bool(d["ok"])
    except Exception:
        pass
    return None


def run(name: str, cmd: list[str]) -> dict:
    try:
        # UTF-8 explícito: no Windows o default cp1252 estoura com saída UTF-8
        # do cargo (nomes/acentos nos testes) e derruba o gate inteiro.
        done = subprocess.run(
            cmd,
            cwd=ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=900,
        )
        out = (done.stdout or "") + (done.stderr or "")
        j = _ok_json(out)
        ok = done.returncode == 0 and (j is not False)
        tail = [l for l in out.splitlines() if l.strip()][-3:]
        return {"name": name, "ok": ok, "exit": done.returncode, "tail": tail}
    except Exception as e:
        return {"name": name, "ok": False, "exit": -1, "tail": [f"exceção: {e}"]}


def main() -> int:
    fast = "--fast" in sys.argv
    results = [run(name, cmd) for name, cmd in CHECKS]
    if not fast:
        # A1 / T-TAB — gate vermelho->verde; grava target/aether_tab_gate.json.
        results.append(
            run(
                "cargo_test_agent_tab_gate",
                ["cargo", "test", "-p", "aether_engine", "--test", "agent_tab_gate"],
            )
        )
        # A2 / T-EVAL — placar reproduzível; grava target/aether_agent_scorecard.json.
        results.append(
            run(
                "cargo_test_agent_eval_report_gate",
                ["cargo", "test", "-p", "aether_engine", "--test", "agent_eval_report_gate"],
            )
        )
        # A3 / T-P2P — delta Merkle-CRDT; grava target/aether_p2p.json.
        results.append(
            run(
                "cargo_test_agent_p2p_gate",
                ["cargo", "test", "-p", "aether_engine", "--test", "agent_p2p_gate"],
            )
        )
        # A4 / T-CLIP — clipboard sem shell; grava target/aether_clip_gate.json.
        results.append(
            run(
                "cargo_test_agent_clip_gate",
                ["cargo", "test", "-p", "aether_engine", "--test", "agent_clip_gate"],
            )
        )
        for pkg in ("aether_engine", "aether_decision"):
            r = run(f"cargo_test_{pkg}", ["cargo", "test", "-p", pkg, "--lib"])
            results.append(r)
    green = all(r["ok"] for r in results)
    print(json.dumps({"ok": green, "checks": results}, ensure_ascii=False, indent=2))
    return 0 if green else 1


if __name__ == "__main__":
    sys.exit(main())
