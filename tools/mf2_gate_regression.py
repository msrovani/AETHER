#!/usr/bin/env python3
# MF2 — regressão do harness F2 contra vereditos F1_3 (MSG 0802/0806).
"""Regra: unknown só quando unbound; prediction_set>1 → ambiguous (HITL, sem pesquisa).

Uso:
  python tools/mf2_gate_regression.py
  python tools/mf2_gate_regression.py --smoke target/f1_decision_smoke.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from research_harness import run_research  # noqa: E402
from research_reliability import DEFAULT_CASES, DEFAULT_TIER  # noqa: E402

DEFAULT_SMOKE = ROOT / "target" / "f1_decision_smoke.json"


def derive_abstain_reason(abstain: bool, unbound: bool, prediction_set_len: int) -> str | None:
    """Espelho de aether_decision::types::derive_abstain_reason (F1_3) — só regressão."""
    if not abstain:
        return None
    if unbound:
        return "unknown"
    if prediction_set_len > 1:
        return "ambiguous"
    # abstain sem unbound e set ≤1: margem fina / conservador → ambiguous (HITL)
    return "ambiguous"


def admits_research(reason: str | None) -> bool:
    return reason == "unknown"


def check_smoke(smoke_path: Path) -> list[str]:
    errs: list[str] = []
    if not smoke_path.exists():
        return [f"smoke ausente: {smoke_path}"]
    data = json.loads(smoke_path.read_text(encoding="utf-8"))
    answers = {a["question"]: a for a in data.get("answers", [])}

    passou = answers.get("passou")
    if not passou:
        errs.append("smoke sem pergunta passou")
    else:
        reason = passou.get("abstain_reason")
        if passou.get("abstain") and reason != "ambiguous":
            errs.append(f"passou abstain_reason want=ambiguous got={reason}")
        if admits_research(reason):
            errs.append("passou ambiguous NAO pode admits_research")

    # conformal_set presente no smoke corrigido (F1_2)
    if not data.get("conformal_set_present"):
        errs.append("conformal_set_present esperado true apos F1_2")

    return errs


def check_taxonomy_table() -> list[str]:
    """Tabela F1_3: unbound→unknown; set>1→ambiguous; so unknown abre pesquisa."""
    errs: list[str] = []
    cases = [
        # abstain, unbound, set_len, expected_reason, research?
        (True, True, 1, "unknown", True),
        (True, True, 2, "unknown", True),  # unbound vence
        (True, False, 2, "ambiguous", False),
        (True, False, 1, "ambiguous", False),
        (True, False, 0, "ambiguous", False),
        (False, False, 2, None, False),
    ]
    for abstain, unbound, n, exp, research in cases:
        got = derive_abstain_reason(abstain, unbound, n)
        if got != exp:
            errs.append(f"derive({abstain},{unbound},{n}) want={exp} got={got}")
        if admits_research(got) != research:
            errs.append(f"admits_research({got}) want={research}")
    return errs


def check_harness_gates() -> list[str]:
    errs: list[str] = []
    for reason in ("ambiguous", "conflict", "engine_unavailable"):
        payload, code = run_research(reason, "terra_lua", DEFAULT_CASES, DEFAULT_TIER, None)
        if code != 2 or payload.get("ok") is not False:
            errs.append(f"harness deve recusar reason={reason} exit=2")
        if payload.get("error") != "pesquisa_so_em_unknown":
            errs.append(f"harness error tipado para {reason}")

    ok, code = run_research("unknown", "terra_lua", DEFAULT_CASES, DEFAULT_TIER, "MF2 cert")
    if code != 0 or not ok.get("ok"):
        errs.append("unknown ainda deve abrir pesquisa offline")
    return errs


def main() -> int:
    ap = argparse.ArgumentParser(description="MF2 gate regression F1_3 × harness F2")
    ap.add_argument("--smoke", default=str(DEFAULT_SMOKE))
    args = ap.parse_args()

    failures: list[str] = []
    failures.extend(check_taxonomy_table())
    failures.extend(check_smoke(Path(args.smoke)))
    failures.extend(check_harness_gates())

    report = {
        "ok": not failures,
        "failures": failures,
        "smoke": args.smoke,
        "contract": "F1_3 unbound→unknown; prediction_set>1→ambiguous; so unknown abre F2",
        "ref": "MSG_0802_0806_0808",
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
