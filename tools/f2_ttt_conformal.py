#!/usr/bin/env python3
# F2 — head TTT não abre pesquisa por conjunto conformal ausente (MSG 0853/0859).
"""O head TTT não tem prediction_set. Ausência do conjunto não é `unknown`.

Pesquisa externa continua só com AbstainReason unknown de lacuna (unbound).
Pergunta respondida pelo head (cv_accuracy presente, conjunto ausente) fica
fechada: o F2 não pode contar com conformal_set nesse caminho.

Uso:
  python tools/f2_ttt_conformal.py selftest
  python tools/f2_ttt_conformal.py --smoke target/f1_decision_smoke.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SMOKE = ROOT / "target" / "f1_decision_smoke.json"


def _head_ttt(answer: dict) -> bool:
    readout = str(answer.get("readout") or "").strip().lower()
    if readout in {"ttt", "head", "head_ttt"}:
        return True
    return answer.get("cv_accuracy") is not None


def _set_present(answer: dict) -> bool:
    if "conformal_set" not in answer:
        return False
    value = answer.get("conformal_set")
    return value not in (None, [], {})


def research_gate(answer: dict) -> dict:
    """Decide se o F2 abre pesquisa. Não promove conjunto ausente a unknown."""
    reason = answer.get("abstain_reason")
    head = _head_ttt(answer)
    present = _set_present(answer)
    expected_absent = head and not present
    # Head sem conjunto: ausência é o contrato do TTT, não lacuna.
    opens = reason == "unknown" and not expected_absent
    return {
        "opens_research": opens,
        "abstain_reason": reason,
        "head_ttt": head,
        "conformal_present": present,
        "expected_absent": expected_absent,
        "note": (
            "conjunto_ausente_no_head_nao_e_unknown"
            if expected_absent
            else "pesquisa_so_em_unknown"
        ),
    }


def check_smoke(path: Path) -> list[str]:
    errs: list[str] = []
    if not path.exists():
        return [f"smoke ausente: {path}"]
    data = json.loads(path.read_text(encoding="utf-8"))
    by_q = {a.get("question"): a for a in data.get("answers", [])}
    route = by_q.get("route")
    if route is None:
        errs.append("smoke sem route")
    else:
        gate = research_gate(route)
        if gate["opens_research"] or route.get("abstain_reason") == "unknown":
            errs.append("route sem conjunto nao abre pesquisa e nao e unknown")
        if route.get("cv_accuracy") is not None and not gate["expected_absent"]:
            errs.append("route com cv_accuracy e sem conjunto e ausencia do head")
    passou = by_q.get("passou")
    if passou is None:
        errs.append("smoke sem passou")
    else:
        gate = research_gate(passou)
        if gate["opens_research"]:
            errs.append("passou ambiguous nao abre pesquisa")
        if passou.get("abstain_reason") != "ambiguous":
            errs.append(f"passou reason want=ambiguous got={passou.get('abstain_reason')}")
    return errs


def selftest() -> int:
    failures: list[str] = []
    head = {
        "question": "route",
        "abstain": False,
        "choice": "reformular",
        "cv_accuracy": 0.7419,
        "readout": "and_gate",
    }
    gate = research_gate(head)
    if gate["opens_research"] or not gate["expected_absent"]:
        failures.append(f"head sem set: {gate}")

    stuffed = dict(head)
    stuffed["abstain_reason"] = "unknown"
    if research_gate(stuffed)["opens_research"]:
        failures.append("unknown inferido no head com set ausente nao abre")

    lacuna = {
        "question": "fora",
        "abstain": True,
        "abstain_reason": "unknown",
        "unbound": True,
        "readout": "engine",
    }
    if not research_gate(lacuna)["opens_research"]:
        failures.append("unbound unknown deve abrir")

    margem = {
        "question": "passou",
        "abstain": True,
        "abstain_reason": "ambiguous",
        "conformal_set": [False],
        "readout": "and_gate",
    }
    if research_gate(margem)["opens_research"]:
        failures.append("ambiguous nao abre")

    failures.extend(check_smoke(DEFAULT_SMOKE))
    report = {
        "ok": not failures,
        "failures": failures,
        "ref": "MSG_0853_0859",
        "rule": "head_ttt sem conformal_set nao abre pesquisa; so unknown de lacuna",
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser(description="F2: head TTT sem conformal nao pesquisa")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("selftest")
    p = sub.add_parser("check")
    p.add_argument("--smoke", default=str(DEFAULT_SMOKE))
    args = ap.parse_args()
    if args.cmd == "check":
        errs = check_smoke(Path(args.smoke))
        print(json.dumps({"ok": not errs, "failures": errs}, ensure_ascii=False))
        return 0 if not errs else 1
    return selftest()


if __name__ == "__main__":
    sys.exit(main())
