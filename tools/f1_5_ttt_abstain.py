#!/usr/bin/env python3
# F1-5 — gate offline: head TTT sem contrato de abstenção (MSG CLAIM 0919).
"""Classifica resposta do head TTT sem inventar threshold de abstain.

Uso:
  python tools/f1_5_ttt_abstain.py check --readout ttt --cv-accuracy 0.7419
  python tools/f1_5_ttt_abstain.py check --readout ttt --acted-assumed true
  python tools/f1_5_ttt_abstain.py selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Valor medido no artefato (BUFFY 0909) — só referência de visibilidade, NÃO threshold.
REF_CV_ACCURACY = 0.7419


def _parse_bool(s: str | None) -> bool | None:
    if s is None:
        return None
    return s.lower() in {"1", "true", "yes", "sim"}


def classify(
    readout: str = "ttt",
    *,
    has_abstain: bool = False,
    has_prediction_set: bool = False,
    has_unbound: bool = False,
    cv_accuracy: float | None = None,
    acted_assumed: bool = False,
    invent_threshold: bool = False,
) -> dict:
    """Veredito tipado. Sem rede, sem PyO3, sem editar o crate.

    Head TTT (readout=ttt) só traz probs/confidence/cv_accuracy/choice.
    Ausência de abstain NÃO é abstain=false — é sinal ausente.
    Threshold de abstain do head = decisão F3 (HITL), fora deste gate.
    """
    is_head = (readout or "").lower() in {"ttt", "head", "head_ttt"}

    if invent_threshold:
        return {
            "schema": "aether.f1_5_ttt_abstain/v1",
            "ok": False,
            "action": "abstain_policy",
            "reason": "threshold_abstain_e_decisao_F3_nao_deste_gate",
            "network": False,
            "wired_to_crate": False,
        }

    if not is_head:
        # Caminho conformal / AND-gate: prediction_set esperado.
        if has_prediction_set:
            return {
                "schema": "aether.f1_5_ttt_abstain/v1",
                "ok": True,
                "action": "proceed_conformal",
                "reason": "nao_head_ttt_com_prediction_set",
                "readout": readout,
                "network": False,
                "wired_to_crate": False,
            }
        return {
            "schema": "aether.f1_5_ttt_abstain/v1",
            "ok": True,
            "action": "proceed_non_head",
            "reason": "fora_do_escopo_F1_5_head",
            "readout": readout,
            "network": False,
            "wired_to_crate": False,
        }

    # --- head TTT ---
    missing_contract = not (has_abstain or has_prediction_set or has_unbound)

    if acted_assumed and missing_contract:
        # get("abstain").unwrap_or(false) — bug F1-5
        return {
            "schema": "aether.f1_5_ttt_abstain/v1",
            "ok": False,
            "action": "forbid_acted_assumed",
            "reason": "sem_sinal_abstain_nao_e_agiu",
            "readout": "ttt",
            "cv_accuracy": cv_accuracy,
            "missing_contract": True,
            "ref_cv_accuracy": REF_CV_ACCURACY,
            "network": False,
            "wired_to_crate": False,
            "fix": "expor Answer.cv_accuracy; NUNCA unwrap_or(false) em abstain ausente",
        }

    if cv_accuracy is None:
        return {
            "schema": "aether.f1_5_ttt_abstain/v1",
            "ok": False,
            "action": "require_cv_accuracy",
            "reason": "head_ttt_sem_cv_accuracy_parece_confianca",
            "readout": "ttt",
            "missing_contract": missing_contract,
            "ref_cv_accuracy": REF_CV_ACCURACY,
            "network": False,
            "wired_to_crate": False,
            "fix": "mitigacao F1-5: surface cv_accuracy no artefato",
        }

    return {
        "schema": "aether.f1_5_ttt_abstain/v1",
        "ok": True,
        "action": "surface_cv_accuracy",
        "reason": "cv_visivel_sem_inventar_threshold_crate",
        "readout": "ttt",
        "cv_accuracy": cv_accuracy,
        "missing_contract": missing_contract,
        "has_abstain": has_abstain,
        "has_prediction_set": has_prediction_set,
        "has_unbound": has_unbound,
        "ref_cv_accuracy": REF_CV_ACCURACY,
        "network": False,
        "wired_to_crate": False,
        "note": "threshold abstain head = F3/HITL; OOD fica na sentinela AND-gate",
    }


def selftest() -> int:
    cases = [
        # head + cv visível + sem contrato → surface OK
        (
            classify("ttt", cv_accuracy=REF_CV_ACCURACY),
            True,
            "surface_cv_accuracy",
        ),
        # head sem cv → require
        (classify("ttt"), False, "require_cv_accuracy"),
        # host assume agiu → forbid
        (
            classify("ttt", cv_accuracy=0.7, acted_assumed=True),
            False,
            "forbid_acted_assumed",
        ),
        # inventar threshold → policy fail
        (
            classify("ttt", cv_accuracy=0.7, invent_threshold=True),
            False,
            "abstain_policy",
        ),
        # conformal path
        (
            classify("and_gate", has_prediction_set=True, has_abstain=True),
            True,
            "proceed_conformal",
        ),
        # non-head sem set
        (classify("energy"), True, "proceed_non_head"),
        # head com prediction_set (não é o bug) + cv
        (
            classify(
                "ttt",
                has_prediction_set=True,
                cv_accuracy=0.8,
                acted_assumed=False,
            ),
            True,
            "surface_cv_accuracy",
        ),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep, "expect": [expect_ok, expect_action]})
    out = {
        "ok": len(failed) == 0,
        "ref_cv_accuracy": REF_CV_ACCURACY,
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
    c.add_argument("--readout", default="ttt")
    c.add_argument("--cv-accuracy", type=float, default=None)
    c.add_argument("--has-abstain", type=_parse_bool, default=False)
    c.add_argument("--has-prediction-set", type=_parse_bool, default=False)
    c.add_argument("--has-unbound", type=_parse_bool, default=False)
    c.add_argument("--acted-assumed", type=_parse_bool, default=False)
    c.add_argument("--invent-threshold", type=_parse_bool, default=False)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = classify(
        args.readout,
        has_abstain=bool(args.has_abstain),
        has_prediction_set=bool(args.has_prediction_set),
        has_unbound=bool(args.has_unbound),
        cv_accuracy=args.cv_accuracy,
        acted_assumed=bool(args.acted_assumed),
        invent_threshold=bool(args.invent_threshold),
    )
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    out = ROOT / "target" / "f1_5_ttt_abstain.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
