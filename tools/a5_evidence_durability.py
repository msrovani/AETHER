#!/usr/bin/env python3
# A-5 — evidência de aceite fora de target/ (MSG 0967 MISSAO_CURSOR).
"""Auditoria offline: classifica artefatos de aceite do ciclo F1–F5.

Não move arquivos alheios. Emite manifesto em data/ (dura) + target/ (relatório).

Uso:
  python tools/a5_evidence_durability.py selftest
  python tools/a5_evidence_durability.py audit
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Ciclo VEGA F1→F5 / fórum 0759+: aceite vs efêmero. Não inventa migração de hub/B0.
CATALOG: list[dict] = [
    {
        "id": "f1_decision_smoke",
        "role": "aceite_F1_smoke",
        "target": "target/f1_decision_smoke.json",
        "durable": "data/f1_decision_smoke.json",
        "migrate": "promote_via_tools_f1_gate_promote_py_quando_engine_real",
        "priority": "P0",
    },
    {
        "id": "f1_gate_baseline",
        "role": "aceite_F1_baseline_n30",
        "target": "target/f1_gate_baseline.json",
        "durable": "data/f1_gate_baseline.json",
        "migrate": "ja_em_data_manter_gate_fora_de_target",
        "priority": "P0",
    },
    {
        "id": "f1_contract_fit124",
        "role": "evidencia_contrato_fit",
        "target": "target/f1_contract_fit124.json",
        "durable": "data/f1_contract_fit124.json",
        "migrate": "copiar_para_data_apos_rodada_limpa_BUFFY",
        "priority": "P1",
    },
    {
        "id": "embed_eval_set",
        "role": "freeze_F4_dataset",
        "target": None,
        "durable": "data/embed_eval_set.jsonl",
        "migrate": "ja_duravel_nao_mover",
        "priority": "P0",
    },
    {
        "id": "embed_eval_run",
        "role": "medicao_F4_hit5",
        "target": "target/embed_eval.json",
        "durable": "data/embed_eval_last.json",
        "migrate": "promover_numeros_do_run_REPROVADO_para_data_apos_ACK",
        "priority": "P0",
    },
    {
        "id": "embed_eval_bm25",
        "role": "piso_lexical_F4_T3",
        "target": "target/embed_eval_bm25_baseline.json",
        "durable": "data/embed_eval_bm25_baseline.json",
        "migrate": "promover_baseline_0_7042_para_data",
        "priority": "P0",
    },
    {
        "id": "f5_t3_contract",
        "role": "contrato_T3",
        "target": None,
        "durable": "data/f5_t3_contract.json",
        "migrate": "ja_duravel",
        "priority": "P1",
    },
    {
        "id": "f5_t3_eval_report",
        "role": "relatorio_T3_eval",
        "target": "target/f5_t3_eval.json",
        "durable": "data/f5_t3_eval_last.json",
        "migrate": "promover_apos_eval_com_rankings_AGENT_A",
        "priority": "P2",
    },
    {
        "id": "f5_t0_eval_report",
        "role": "relatorio_T0",
        "target": "target/f5_t0_eval.json",
        "durable": "data/f5_t0_eval_last.json",
        "migrate": "opcional_promover_ultimo_verde",
        "priority": "P2",
    },
    {
        "id": "mf2_report",
        "role": "regressao_MF2",
        "target": "target/mf2_gate_regression.json",
        "durable": "data/mf2_gate_regression_last.json",
        "migrate": "opcional_smoke_ja_em_data",
        "priority": "P2",
    },
    {
        "id": "qa_suite_probe",
        "role": "qa_flake_probe",
        "target": "target/qa_suite_probe.json",
        "durable": "data/qa_suite_probe_last.json",
        "migrate": "efemero_ok_manter_em_target",
        "priority": "P3",
    },
    {
        "id": "f4_nsgdb_dump",
        "role": "fonte_freeze_auxiliar",
        "target": "target/f4_nsgdb_dump.jsonl",
        "durable": "data/f4_nsgdb_dump.jsonl",
        "migrate": "dump_auxiliar_promover_se_entrar_no_freeze",
        "priority": "P2",
    },
    {
        "id": "research_cases",
        "role": "fixtures_F2",
        "target": None,
        "durable": "data/research_cases.json",
        "migrate": "ja_duravel",
        "priority": "P1",
    },
    {
        "id": "source_tier",
        "role": "tier_F2_5",
        "target": None,
        "durable": "data/source_tier.json",
        "migrate": "ja_duravel",
        "priority": "P1",
    },
    {
        "id": "gpu_engine_lock",
        "role": "lock_A2c_efemero",
        "target": "target/gpu_engine.lock.json",
        "durable": None,
        "migrate": "efemero_por_design_nao_versionar",
        "priority": "P3",
    },
]


def _stat(rel: str | None) -> dict:
    if not rel:
        return {"path": None, "exists": False}
    p = ROOT / rel
    if not p.is_file():
        return {"path": rel, "exists": False}
    return {
        "path": rel,
        "exists": True,
        "bytes": p.stat().st_size,
        "mtime": int(p.stat().st_mtime),
    }


def classify(entry: dict) -> dict:
    t = _stat(entry.get("target"))
    d = _stat(entry.get("durable"))
    if d.get("exists"):
        status = "durable_ok"
    elif t.get("exists"):
        status = "only_in_target_RISK"
    elif entry.get("durable") is None and entry.get("target"):
        status = "ephemeral_ok"
    else:
        status = "missing"
    return {
        **entry,
        "target_stat": t,
        "durable_stat": d,
        "status": status,
        "cargo_clean_wipes": bool(t.get("exists")) and not bool(d.get("exists")),
    }


def audit() -> dict:
    rows = [classify(e) for e in CATALOG]
    risk = [r for r in rows if r["status"] == "only_in_target_RISK"]
    durable = [r for r in rows if r["status"] == "durable_ok"]
    report = {
        "schema": "aether.a5_evidence_durability/v1",
        "ref": "MSG_0967_MISSAO_CURSOR_A5",
        "ok": True,
        "network": False,
        "n": len(rows),
        "n_durable_ok": len(durable),
        "n_only_target_risk": len(risk),
        "n_missing": sum(1 for r in rows if r["status"] == "missing"),
        "risk_ids": [r["id"] for r in risk],
        "items": rows,
        "rule": "aceite_do_ciclo_nao_pode_morrer_no_cargo_clean",
    }
    out_data = ROOT / "data" / "a5_evidence_manifest.json"
    out_target = ROOT / "target" / "a5_evidence_durability.json"
    text = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    out_data.write_text(text, encoding="utf-8")
    out_target.parent.mkdir(parents=True, exist_ok=True)
    out_target.write_text(text, encoding="utf-8")
    report["wrote"] = [str(out_data.relative_to(ROOT)), str(out_target.relative_to(ROOT))]
    return report


def selftest() -> int:
    failures: list[str] = []
    report = audit()
    if not (ROOT / "data" / "a5_evidence_manifest.json").is_file():
        failures.append("manifesto data/ ausente")
    if report.get("network"):
        failures.append("network deve ser false")
    # P0 smoke: se data existe, nao pode ser only_in_target
    for row in report["items"]:
        if row["id"] == "f1_decision_smoke" and row["status"] == "only_in_target_RISK":
            failures.append("smoke F1 so em target — promote pendente")
        if row["id"] == "embed_eval_set" and row["status"] != "durable_ok":
            failures.append("freeze F4 tem de estar em data/")
    out = {"ok": not failures, "failures": failures, "summary": {
        "n": report["n"],
        "risk": report["risk_ids"],
        "durable": report["n_durable_ok"],
    }}
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", nargs="?", default="selftest", choices=["selftest", "audit"])
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    report = audit()
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
