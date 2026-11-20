#!/usr/bin/env python3
# F2 — tomada de pesquisa no harness (só AbstainReason::unknown; offline default).
"""Dispara a tripla evidência→convergência→veredito quando o gate abstém unknown.

Default: HF_HUB_OFFLINE-style — NENHUMA chamada de rede. Fontes vêm de
data/research_cases.json ou --sources JSON. Rede só com --allow-network
(reserva F5 / websearch real).

Uso:
  python tools/research_harness.py --reason unknown --case terra_lua
  python tools/research_harness.py --reason ambiguous --case terra_lua   # recusa
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from research_reliability import (  # noqa: E402
    DEFAULT_CASES,
    DEFAULT_TIER,
    evaluate_case,
    load_json,
)

ALLOWED_REASON = "unknown"


def quarantine_record(result: dict, abstain_reason: str) -> dict:
    """Payload pronto para remember com autoridade ≤120 (nunca 255)."""
    return {
        "kind": "external_research",
        "abstain_reason": abstain_reason,
        "question": result["question"],
        "verdict": result["verdict"],
        "authority": min(int(result["authority"]), 120),
        "n_independent": result["n_independent"],
        "reason": result["reason"],
        "question_kind_bootstrap": result.get("question_kind_bootstrap"),
        "numeric": result.get("numeric"),
        "semantic": result.get("semantic"),
        "scope_hint": "aether/project",
        "hitl_required_for_255": True,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--reason",
        required=True,
        choices=["unknown", "ambiguous", "conflict", "engine_unavailable"],
        help="AbstainReason do DecisionPort",
    )
    ap.add_argument("--case", help="id em data/research_cases.json")
    ap.add_argument("--cases", default=str(DEFAULT_CASES))
    ap.add_argument("--tier", default=str(DEFAULT_TIER))
    ap.add_argument(
        "--allow-network",
        action="store_true",
        help="reserva: websearch real (F5). Default OFF — socket fora do inventario",
    )
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    if args.reason != ALLOWED_REASON:
        out = {
            "ok": False,
            "error": "pesquisa_so_em_unknown",
            "reason": args.reason,
            "doctrine": "ambiguous/conflict escalam HITL; engine_unavailable degrada",
        }
        print(json.dumps(out, ensure_ascii=False, indent=2))
        return 2

    if args.allow_network:
        out = {
            "ok": False,
            "error": "network_ainda_nao_ligado",
            "hint": "F2 usa fixtures; tool propria + websearch = F5",
        }
        print(json.dumps(out, ensure_ascii=False, indent=2))
        return 3

    if not args.case:
        print(json.dumps({"ok": False, "error": "--case obrigatorio no modo offline"}))
        return 2

    bundle = load_json(Path(args.cases))
    tier_cfg = load_json(Path(args.tier))
    case = next((c for c in bundle["cases"] if c["id"] == args.case), None)
    if case is None:
        print(json.dumps({"ok": False, "error": f"case ausente: {args.case}"}))
        return 2

    result = evaluate_case(case, tier_cfg)
    payload = {
        "ok": True,
        "mode": "offline_fixture",
        "network": False,
        "result": result,
        "quarantine": quarantine_record(result, args.reason),
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
