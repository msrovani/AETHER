#!/usr/bin/env python3
# A5 — auditoria offline de durabilidade (MSG 0967). Não move arquivos.
"""Lista artefatos de aceite em target/ vs data/ e propõe migração.

Uso:
  python tools/a5_evidencia_audit.py
  python tools/a5_evidencia_audit.py selftest
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
DATA = ROOT / "data"

# Classes de artefato de aceite do ciclo (não logs brutos).
ACCEPTANCE_GLOBS = (
    "f1_decision_smoke.json",
    "f1_gate_baseline.json",
    "f1_contract*.json",
    "f1_offline_suite.json",
    "f1_*_gate.json",
    "f1_vram_watchdog.json",
    "f1_gil_gate.json",
    "f1_5_ttt_abstain.json",
    "f1_a1b_vega_check.json",
    "embed_eval*.json",
    "mf2_gate_regression*.json",
    "qa_suite_probe*.json",
    "vega_path_audit.json",
    "f5_t*.json",
    "f4_gate_hold*.json",
    "research_quarantine*.jsonl",
)

# Já tem espelho canónico em data/ (A-5 / freeze).
DATA_CANON = {
    "f1_decision_smoke.json": "data/f1_decision_smoke.json",
    "f1_gate_baseline.json": "data/f1_gate_baseline.json",
    "embed_eval_set.jsonl": "data/embed_eval_set.jsonl",  # set, não em target
}


def _matches(name: str) -> bool:
    from fnmatch import fnmatch

    return any(fnmatch(name, g) for g in ACCEPTANCE_GLOBS)


def classify(path: Path) -> dict:
    rel = path.relative_to(ROOT).as_posix()
    name = path.name
    in_target = "target/" in rel.replace("\\", "/")
    durable = DATA_CANON.get(name)
    if name.startswith("log_") or name.endswith(".log") or name.endswith(".txt"):
        kind = "ephemeral_log"
        migrate = "manter_em_target_ou_apagar"
    elif name.endswith(".py"):
        kind = "stray_script_in_target"
        migrate = "mover_para_scripts_ou_apagar"
    elif durable and (ROOT / durable).exists():
        kind = "has_data_mirror"
        migrate = f"promover_via_f1_gate_promote_ou_confirmar_sha_em_{durable}"
    elif name.startswith("embed_eval"):
        kind = "measurement"
        migrate = "data/embed_eval_last.json (+ meta sha) — set ja em data/"
    elif name.startswith("f5_") or name.startswith("qa_") or name.startswith("vega_"):
        kind = "tier_report"
        migrate = f"data/reports/{name}" if in_target else "ok"
    elif name.startswith("f1_") or name.startswith("mf2_") or name.startswith("f4_"):
        kind = "gate_report"
        migrate = f"data/{name}" if in_target else "ok"
    elif name.startswith("research_quarantine"):
        kind = "quarantine_ephemeral"
        migrate = "manter_efemero_ou_data/research_quarantine/ se HITL quiser"
    else:
        kind = "other"
        migrate = "revisar"
    return {
        "path": rel,
        "bytes": path.stat().st_size if path.exists() else 0,
        "in_target": in_target,
        "kind": kind,
        "migrate_to": migrate,
        "ok_durable": (not in_target) or kind in {"ephemeral_log", "quarantine_ephemeral"},
    }


def audit() -> dict:
    rows = []
    if TARGET.is_dir():
        for p in sorted(TARGET.iterdir()):
            if p.is_file() and _matches(p.name):
                rows.append(classify(p))
    data_ok = []
    for name, dest in DATA_CANON.items():
        dp = ROOT / dest
        data_ok.append(
            {
                "path": dest,
                "exists": dp.exists(),
                "bytes": dp.stat().st_size if dp.exists() else 0,
            }
        )
    in_target = [r for r in rows if r["in_target"]]
    need_migrate = [
        r
        for r in in_target
        if r["kind"] not in {"ephemeral_log", "quarantine_ephemeral", "stray_script_in_target"}
    ]
    return {
        "schema": "aether.a5_evidencia_audit/v1",
        "ok": True,
        "network": False,
        "ref": "MSG_0967",
        "n_target_matched": len(in_target),
        "n_need_migrate": len(need_migrate),
        "data_canon": data_ok,
        "target_artifacts": rows,
        "need_migrate": need_migrate,
        "policy": "aceite duravel mora em data/; target/ e efemero (cargo clean)",
    }


def selftest() -> int:
    rep = audit()
    failed = []
    if not rep.get("ok"):
        failed.append("ok")
    # smoke durable deve existir em data/ (A-5 ja feito)
    smoke = ROOT / "data" / "f1_decision_smoke.json"
    if not smoke.exists():
        failed.append("data/f1_decision_smoke.json ausente")
    baseline = ROOT / "data" / "f1_gate_baseline.json"
    if not baseline.exists():
        failed.append("data/f1_gate_baseline.json ausente")
    out = {
        "ok": len(failed) == 0,
        "failed": failed,
        "n_need_migrate": rep.get("n_need_migrate"),
        "n_target_matched": rep.get("n_target_matched"),
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    rep = audit()
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    out = TARGET / "a5_evidencia_audit.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    # manifesto espelho em data/ (duravel) — so o relatorio desta auditoria
    durable = DATA / "a5_evidencia_manifest.json"
    durable.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
