#!/usr/bin/env python3
# F5 T1 — laço agent: DecisionPort abstain unknown → quarentena (sem rede, sem toccli).
"""Simula o pós-wire do DecisionPort (MSG 0791/0803) sem editar src/agent/cli.rs.

Fluxo:
  veredito tipado → se unknown: research_harness offline → JSONL quarentena ≤120
                 → se ambiguous/conflict: SEM pesquisa (HITL)
                 → se age: sem pesquisa

Uso:
  python tools/f5_t1_agent_loop.py selftest
  python tools/f5_t1_agent_loop.py run --fixture data/f5_t1_turns_fixture.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from f1_abstain_map import admits_research, derive_abstain_reason  # noqa: E402
from research_harness import DEFAULT_CASES, DEFAULT_TIER, run_research  # noqa: E402

DEFAULT_FIXTURE = ROOT / "data" / "f5_t1_turns_fixture.json"
DEFAULT_OUT = ROOT / "target" / "f5_t1_quarantine.jsonl"


def process_turn(turn: dict, quarantine_path: Path) -> dict:
    """Um turno do laço T1. Rede sempre off."""
    reason = turn.get("abstain_reason")
    if reason is None and turn.get("abstain"):
        reason = derive_abstain_reason(
            True, bool(turn.get("unbound")), turn.get("prediction_set_len")
        )
    out: dict = {
        "turn_id": turn.get("id"),
        "text": turn.get("text"),
        "abstain": bool(turn.get("abstain")),
        "abstain_reason": reason,
        "network": False,
    }
    if not turn.get("abstain"):
        out["action"] = "age"
        out["research"] = None
        return out
    if not admits_research(reason):
        out["action"] = "escalate_hitl" if reason in ("ambiguous", "conflict") else "degrade"
        out["research"] = {"opened": False, "reason": reason}
        return out

    # unknown → harness offline
    case_id = turn.get("research_case", "terra_lua")
    payload, code = run_research(
        "unknown",
        case_id,
        DEFAULT_CASES,
        DEFAULT_TIER,
        turn.get("state") or f"turn={turn.get('id')}",
        quarantine_path,
    )
    out["action"] = "quarantine"
    out["research"] = {
        "opened": True,
        "exit": code,
        "case": case_id,
        "ok": payload.get("ok"),
        "quarantine_authority": (payload.get("quarantine") or {}).get("authority"),
        "write": payload.get("write"),
    }
    return out


def run_fixture(fixture_path: Path, out_path: Path) -> dict:
    if out_path.exists():
        out_path.unlink()
    data = json.loads(fixture_path.read_text(encoding="utf-8"))
    results = [process_turn(t, out_path) for t in data["turns"]]
    n_q = 0
    if out_path.exists():
        n_q = sum(1 for ln in out_path.read_text(encoding="utf-8").splitlines() if ln.strip())
    return {
        "ok": True,
        "n_turns": len(results),
        "n_quarantine_lines": n_q,
        "results": results,
        "quarantine_path": str(out_path),
    }


def selftest() -> int:
    fixture = DEFAULT_FIXTURE
    out = ROOT / "target" / "f5_t1_quarantine_selftest.jsonl"
    report = run_fixture(fixture, out)
    failures: list[str] = []
    by_id = {r["turn_id"]: r for r in report["results"]}

    u = by_id.get("t_unknown_ood")
    if not u or u.get("action") != "quarantine" or not u["research"].get("opened"):
        failures.append("unknown deve abrir pesquisa/quarentena")
    auth = (u or {}).get("research", {}).get("quarantine_authority", 999)
    if auth > 120:
        failures.append("authority > 120")

    a = by_id.get("t_ambiguous_margin")
    if not a or a.get("research", {}).get("opened"):
        failures.append("ambiguous NAO deve abrir pesquisa")

    act = by_id.get("t_act")
    if not act or act.get("action") != "age":
        failures.append("act deve age sem pesquisa")

    if report["n_quarantine_lines"] < 1:
        failures.append("jsonl quarentena vazio")

    final = {"ok": not failures, "failures": failures, "report": report}
    print(json.dumps(final, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    p_run = sub.add_parser("run")
    p_run.add_argument("--fixture", default=str(DEFAULT_FIXTURE))
    p_run.add_argument("--out", default=str(DEFAULT_OUT))
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    report = run_fixture(Path(args.fixture), Path(args.out))
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
