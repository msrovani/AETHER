#!/usr/bin/env python3
# F1 — classificador offline do achado MSG 1025 (torch.cpu.device / vegaml).
"""Não importa torch nem toca GPU. Classifica mensagens de erro tipadas.

Uso:
  python tools/f1_torch_device_check.py check --error "AttributeError: ... cpu.device"
  python tools/f1_torch_device_check.py selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Achado OPENCODE MSG 1025 — hipótese auth 60 até HITL.
REF_MSG = "1025"
REF_VEGAML = "0.16.5"


def classify(error: str, *, vegaml_version: str | None = None) -> dict:
    err = (error or "").strip()
    low = err.lower()
    is_attr = "attributeerror" in low
    is_cpu_device = "cpu.device" in low or "torch.cpu.device" in low or (
        "module 'torch.cpu'" in low and "device" in low
    )
    is_module = "modulenotfounderror" in low or "no module named" in low

    if is_attr and is_cpu_device:
        action = "degrade_engine_unavailable"
        ok = True
        reason = "torch_cpu_device_incompativel_hipotese"
        auth_hint = 60
    elif is_module:
        action = "degrade_engine_unavailable"
        ok = True
        reason = "vegaml_ou_torch_ausente"
        auth_hint = 120
    elif not err:
        return {
            "schema": "aether.f1_torch_device_check/v1",
            "ok": False,
            "action": "abstain_contract",
            "reason": "error_vazio",
            "network": False,
        }
    else:
        action = "escalate_unknown_error"
        ok = False
        reason = "erro_nao_classificado"
        auth_hint = 60

    return {
        "schema": "aether.f1_torch_device_check/v1",
        "ok": ok,
        "action": action,
        "reason": reason,
        "matched_attr_cpu_device": bool(is_attr and is_cpu_device),
        "vegaml_version": vegaml_version or REF_VEGAML,
        "ref_msg": REF_MSG,
        "authority_hint": auth_hint,
        "hitl_required_for_255": True,
        "network": False,
        "wired_to_crate": False,
        "gpu_touched": False,
        "fix": "port deve abster engine_unavailable (Eixo E); core pode responder transplant",
        "note": "hipótese 60 — incompatibilidade torch.cpu.device × vegaml; não é OOM",
    }


def selftest() -> int:
    cases = [
        (
            classify(
                "AttributeError: module 'torch.cpu' has no attribute 'device'",
                vegaml_version="0.16.5",
            ),
            True,
            "degrade_engine_unavailable",
        ),
        (
            classify("ModuleNotFoundError: No module named 'vegaml'"),
            True,
            "degrade_engine_unavailable",
        ),
        (classify("RuntimeError: CUDA out of memory"), False, "escalate_unknown_error"),
        (classify(""), False, "abstain_contract"),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep})
    out = {
        "ok": len(failed) == 0,
        "failed": failed,
        "n": len(cases),
        "ref_msg": REF_MSG,
        "gpu_touched": False,
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    dest = ROOT / "data" / "reports" / "f1_torch_device_check_selftest.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--error", required=True)
    c.add_argument("--vegaml-version", default=REF_VEGAML)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = classify(args.error, vegaml_version=args.vegaml_version)
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
