#!/usr/bin/env python3
# F2_WIRE — unknown do DecisionPort abre pesquisa (MSG 1014/1015).
"""Cabo offline: port/edge → digest ≤320 → research_harness → quarentena 60–120.

Não edita `aether-decision` nem `cli.rs`. Rede default OFF.
Não roda engine GPU (respeita tools/engine_lock.py / .vega/engine.lock).

Uso:
  python tools/f2_wire_port.py selftest
  python tools/f2_wire_port.py wire --fixture data/f2_wire_turns.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from cert_digest import digest_certificate_fields  # noqa: E402
from f1_abstain_map import admits_research, derive_abstain_reason  # noqa: E402
from f2_ttt_conformal import research_gate  # noqa: E402
from research_harness import (  # noqa: E402
    DEFAULT_CASES,
    DEFAULT_TIER,
    digest_certificate,
    run_research,
)

FIXTURE = ROOT / "data" / "f2_wire_turns.json"
DEFAULT_OUT = ROOT / "target" / "f2_wire_quarantine.jsonl"
DURABLE_OUT = ROOT / "data" / "reports" / "f2_wire_quarantine_selftest.jsonl"


def resolve_reason(turn: dict) -> str | None:
    port = turn.get("port") or {}
    if port.get("abstain_reason") is not None:
        return port.get("abstain_reason")
    if port.get("abstain"):
        return derive_abstain_reason(
            True, bool(port.get("unbound")), port.get("prediction_set_len")
        )
    return None


def should_open(turn: dict) -> dict:
    """Porta F2: só unknown; head TTT sem set não abre; OOV sozinho não basta."""
    port = dict(turn.get("port") or {})
    reason = resolve_reason(turn)
    port_answer = {
        "abstain_reason": reason,
        "readout": port.get("readout"),
        "cv_accuracy": port.get("cv_accuracy"),
        "conformal_set": port.get("conformal_set"),
    }
    gate = research_gate(port_answer)
    open_ok = bool(gate["opens_research"] and admits_research(reason))
    # DoD 1014: turno fora do léxico + unknown do port dispara.
    if turn.get("oov") or turn.get("edge") == "fora_lexico":
        open_ok = open_ok and reason == "unknown"
    return {
        "opens": open_ok,
        "reason": reason,
        "gate": gate,
        "oov": bool(turn.get("oov")),
    }


def build_digest(turn: dict) -> dict:
    cert = turn.get("cert")
    if isinstance(cert, dict) and cert:
        return digest_certificate_fields(cert)
    state = turn.get("state") or (
        f"edge={turn.get('edge')} text={turn.get('text')} "
        f"engine={(turn.get('port') or {}).get('engine', 'unknown')}"
    )
    return digest_certificate(str(state))


def wire_turn(turn: dict, quarantine_path: Path) -> dict:
    decision = should_open(turn)
    out: dict = {
        "turn_id": turn.get("id"),
        "text": turn.get("text"),
        "edge": turn.get("edge"),
        "oov": bool(turn.get("oov")),
        "network": False,
        "opens_research": decision["opens"],
        "abstain_reason": decision["reason"],
        "gate": decision["gate"],
    }
    dig = build_digest(turn)
    out["digest"] = {
        "n_tokens": dig.get("n_tokens"),
        "truncated": dig.get("truncated"),
        "ok": dig.get("ok", True),
        "preview": (dig.get("digest") or "")[:120],
    }
    if dig.get("n_tokens", 0) > 320 or not dig.get("ok", True):
        out["action"] = "digest_reject"
        out["research"] = None
        return out
    if not decision["opens"]:
        reason = decision["reason"]
        if reason in ("ambiguous", "conflict"):
            out["action"] = "escalate_hitl"
        elif reason == "engine_unavailable":
            out["action"] = "degrade"
        else:
            out["action"] = "age"
        out["research"] = {"opened": False, "reason": reason}
        return out

    case_id = turn.get("research_case", "terra_lua")
    state_text = dig.get("digest") or turn.get("text") or case_id
    payload, code = run_research(
        "unknown",
        case_id,
        DEFAULT_CASES,
        DEFAULT_TIER,
        state_text,
        quarantine_path,
    )
    q = payload.get("quarantine") or {}
    out["action"] = "quarantine"
    out["research"] = {
        "opened": True,
        "exit": code,
        "case": case_id,
        "ok": payload.get("ok"),
        "verdict": (payload.get("result") or {}).get("verdict"),
        "n_independent": (payload.get("result") or {}).get("n_independent"),
        "authority": q.get("authority"),
        "write": payload.get("write"),
        "triple": {
            "evidencia_n_independent": (payload.get("result") or {}).get("n_independent"),
            "convergencia_verdict": (payload.get("result") or {}).get("verdict"),
            "quarentena_authority": q.get("authority"),
        },
    }
    return out


def run_fixture(fixture_path: Path, out_path: Path) -> dict:
    if out_path.exists():
        out_path.unlink()
    data = json.loads(fixture_path.read_text(encoding="utf-8"))
    results = [wire_turn(t, out_path) for t in data["turns"]]
    n_q = 0
    if out_path.exists():
        n_q = sum(1 for ln in out_path.read_text(encoding="utf-8").splitlines() if ln.strip())
    return {
        "schema": "aether.f2_wire_port/v1",
        "ok": True,
        "n_turns": len(results),
        "n_quarantine_lines": n_q,
        "results": results,
        "quarantine_path": str(out_path),
        "network": False,
        "mutated_crate": False,
    }


def selftest() -> int:
    out = ROOT / "target" / "f2_wire_quarantine_selftest.jsonl"
    report = run_fixture(FIXTURE, out)
    # espelho durável do JSONL de selftest
    DURABLE_OUT.parent.mkdir(parents=True, exist_ok=True)
    if out.exists():
        DURABLE_OUT.write_text(out.read_text(encoding="utf-8"), encoding="utf-8")
    failures: list[str] = []
    by_id = {r["turn_id"]: r for r in report["results"]}
    data = json.loads(FIXTURE.read_text(encoding="utf-8"))
    for turn in data["turns"]:
        got = by_id.get(turn["id"])
        if not got:
            failures.append(f"missing:{turn['id']}")
            continue
        want = bool(turn.get("expect_open"))
        if bool(got.get("opens_research")) != want:
            failures.append(
                f"{turn['id']}: opens want={want} got={got.get('opens_research')}"
            )
        dig_n = (got.get("digest") or {}).get("n_tokens", 999)
        if dig_n > 320:
            failures.append(f"{turn['id']}: digest>{320}")
        if want:
            if got.get("action") != "quarantine":
                failures.append(f"{turn['id']}: action!=quarantine")
            auth = (got.get("research") or {}).get("authority", 999)
            if auth is None or auth > 120 or auth >= 255:
                failures.append(f"{turn['id']}: authority_bad={auth}")
            triple = (got.get("research") or {}).get("triple") or {}
            if triple.get("convergencia_verdict") is None:
                failures.append(f"{turn['id']}: sem_veredito")
            if not triple.get("evidencia_n_independent"):
                failures.append(f"{turn['id']}: sem_n_independent")
    if report["n_quarantine_lines"] < 1:
        failures.append("quarantine_empty")
    # digest oversize reject
    fat = {
        "id": "fat_digest",
        "oov": True,
        "port": {"abstain": True, "unbound": True, "abstain_reason": "unknown"},
        "state": " ".join(f"tok{i}" for i in range(500)),
        "research_case": "terra_lua",
    }
    # digest_certificate truncates — still ok; cert path with huge question:
    fat2 = {
        "id": "fat_cert",
        "oov": True,
        "port": {"abstain": True, "unbound": True, "abstain_reason": "unknown"},
        "cert": {
            "energy": 0,
            "gap": 0,
            "violations": 0,
            "consensus": 0,
            "provenance": "t",
            "authority": 0,
            "question": " ".join(f"q{i}" for i in range(400)),
        },
        "research_case": "terra_lua",
    }
    rfat = wire_turn(fat2, out)
    if (rfat.get("digest") or {}).get("n_tokens", 999) > 320:
        failures.append("cert_digest_not_capped")

    final = {
        "ok": not failures,
        "failures": failures,
        "n": len(data["turns"]),
        "report": report,
        "durable_quarantine": str(DURABLE_OUT) if DURABLE_OUT.exists() else None,
        "ref": "MSG_1014_1015",
    }
    print(json.dumps(final, ensure_ascii=False, indent=2))
    (ROOT / "data" / "reports" / "f2_wire_port_selftest.json").write_text(
        json.dumps(final, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    return 0 if not failures else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="F2_WIRE port→pesquisa")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    p_w = sub.add_parser("wire")
    p_w.add_argument("--fixture", type=Path, default=FIXTURE)
    p_w.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args(argv)
    if args.cmd == "selftest":
        return selftest()
    report = run_fixture(args.fixture, args.out)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
