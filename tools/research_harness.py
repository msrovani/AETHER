#!/usr/bin/env python3
# F2 — tomada de pesquisa no harness (só AbstainReason::unknown; offline default).
"""Dispara a tripla evidência→convergência→veredito quando o gate abstém unknown.

Default: HF_HUB_OFFLINE-style — NENHUMA chamada de rede. Fontes vêm de
data/research_cases.json. Rede só com --allow-network (reserva F5).

Digest de certificado ≤320 tokens (Eixo B) antes de qualquer decide/pesquisa.

Uso:
  python tools/research_harness.py --reason unknown --case terra_lua
  python tools/research_harness.py selftest
  python tools/research_harness.py digest --state "E=-1.2 ΔE=0.01 ..."
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
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
MAX_DIGEST_TOKENS = 320
_TOKEN_RE = re.compile(r"\S+")


def digest_certificate(state: str, max_tokens: int = MAX_DIGEST_TOKENS) -> dict:
    """Corta o digest do certificado em ≤max_tokens (Eixo B). Não re-serializa prosa."""
    raw = state.strip()
    tokens = _TOKEN_RE.findall(raw)
    truncated = len(tokens) > max_tokens
    kept = tokens[:max_tokens]
    digest = " ".join(kept)
    return {
        "digest": digest,
        "n_tokens": len(kept),
        "n_tokens_raw": len(tokens),
        "truncated": truncated,
        "max_tokens": max_tokens,
        "ok": len(kept) <= max_tokens,
    }


DEFAULT_QUARANTINE = ROOT / "target" / "research_quarantine.jsonl"


def quarantine_record(result: dict, abstain_reason: str, digest: dict | None = None) -> dict:
    """Payload pronto para remember com autoridade ≤120 (nunca 255)."""
    auth = min(int(result["authority"]), 120)
    if auth >= 255:
        auth = 120
    rec = {
        "kind": "external_research",
        "abstain_reason": abstain_reason,
        "question": result["question"],
        "verdict": result["verdict"],
        "authority": auth,
        "n_independent": result["n_independent"],
        "reason": result["reason"],
        "question_kind_bootstrap": result.get("question_kind_bootstrap"),
        "numeric": result.get("numeric"),
        "semantic": result.get("semantic"),
        "scope_hint": "aether/project",
        "entity_hint": "aether/ops",
        "hitl_required_for_255": True,
        "never_255": True,
    }
    if digest is not None:
        rec["state_digest"] = {
            "n_tokens": digest["n_tokens"],
            "truncated": digest["truncated"],
            "preview": digest["digest"][:120],
        }
    return rec


def write_quarantine(path: Path, record: dict) -> dict:
    """Append-only JSONL — simula outbox/remember em quarentena (≤120)."""
    if int(record.get("authority", 999)) > 120:
        return {"ok": False, "error": "authority_gt_120_bloqueado"}
    if int(record.get("authority", 0)) >= 255:
        return {"ok": False, "error": "authority_255_proibido_sem_HITL"}
    path.parent.mkdir(parents=True, exist_ok=True)
    line = json.dumps(record, ensure_ascii=False, separators=(",", ":"))
    with path.open("a", encoding="utf-8") as fh:
        fh.write(line + "\n")
    return {"ok": True, "path": str(path), "authority": record["authority"]}


def run_research(
    reason: str,
    case_id: str,
    cases: Path,
    tier: Path,
    state: str | None,
    write_path: Path | None = None,
) -> tuple[dict, int]:
    if reason != ALLOWED_REASON:
        return {
            "ok": False,
            "error": "pesquisa_so_em_unknown",
            "reason": reason,
            "doctrine": "ambiguous/conflict escalam HITL; engine_unavailable degrada",
        }, 2

    dig = digest_certificate(state or f"case={case_id} abstain=unknown")
    bundle = load_json(cases)
    tier_cfg = load_json(tier)
    from research_reliability import find_case  # noqa: WPS440

    case = find_case(bundle, case_id)
    if case is None:
        return {"ok": False, "error": f"case ausente: {case_id}"}, 2

    result = evaluate_case(case, tier_cfg)
    q = quarantine_record(result, reason, dig)
    payload: dict = {
        "ok": True,
        "mode": "offline_fixture",
        "network": False,
        "digest": dig,
        "result": result,
        "quarantine": q,
    }
    if write_path is not None:
        payload["write"] = write_quarantine(write_path, q)
        if not payload["write"].get("ok"):
            return payload, 4
    return payload, 0


def selftest() -> int:
    """Harness + reliability: 3 casos, gates, digest ≤320, write quarentena."""
    failures: list[str] = []

    long_state = " ".join(f"tok{i}" for i in range(500))
    d = digest_certificate(long_state)
    if d["n_tokens"] != 320 or not d["truncated"]:
        failures.append(f"digest_truncate got n={d['n_tokens']} trunc={d['truncated']}")
    short = digest_certificate("E=-1.0 gap=0.1 viol=0")
    if short["truncated"] or short["n_tokens"] != 3:
        failures.append(f"digest_short bad: {short}")

    rel = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "research_reliability.py"), "selftest"],
        capture_output=True,
        text=True,
        check=False,
    )
    if rel.returncode != 0:
        failures.append("research_reliability selftest failed")

    qpath = ROOT / "target" / "research_quarantine_selftest.jsonl"
    if qpath.exists():
        qpath.unlink()

    expects = {
        "terra_lua": ("settled", 120),
        "rust_vs_c": ("open", 60),
        "three_copied_blogs": ("open", 60),
    }
    for cid, (verdict, auth) in expects.items():
        ok_p, code = run_research(
            "unknown", cid, DEFAULT_CASES, DEFAULT_TIER, f"cert case={cid}", qpath
        )
        if code != 0 or not ok_p.get("ok"):
            failures.append(f"{cid} unknown failed")
            continue
        q = ok_p["quarantine"]
        if q.get("verdict") != verdict:
            failures.append(f"{cid} verdict want={verdict} got={q.get('verdict')}")
        if q.get("authority") != auth:
            failures.append(f"{cid} authority want={auth} got={q.get('authority')}")
        if q.get("authority", 999) > 120 or q.get("authority", 0) >= 255:
            failures.append(f"{cid} authority fora da quarentena")
        if not ok_p.get("write", {}).get("ok"):
            failures.append(f"{cid} write_quarantine failed")

    if qpath.exists():
        lines = [ln for ln in qpath.read_text(encoding="utf-8").splitlines() if ln.strip()]
        if len(lines) != 3:
            failures.append(f"quarantine jsonl want=3 got={len(lines)}")
        for ln in lines:
            rec = json.loads(ln)
            if int(rec["authority"]) > 120:
                failures.append("jsonl authority > 120")

    bad, code = run_research("ambiguous", "terra_lua", DEFAULT_CASES, DEFAULT_TIER, None)
    if code != 2 or bad.get("ok") is not False:
        failures.append("ambiguous should exit 2")

    conflict, code = run_research("conflict", "terra_lua", DEFAULT_CASES, DEFAULT_TIER, None)
    if code != 2:
        failures.append("conflict should exit 2")

    # MF2 (MSG 0802/0808): regressão F1_3 — só unknown abre pesquisa
    mf2 = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "mf2_gate_regression.py")],
        capture_output=True,
        text=True,
        check=False,
    )
    if mf2.returncode != 0:
        failures.append(f"mf2_gate_regression failed: {mf2.stdout[-200:]}")

    report = {
        "ok": not failures,
        "failures": failures,
        "n_checks": 3 + 3 + 2 + 2 + 1,  # + MF2
        "quarantine_path": str(qpath),
        "mf2": "PASS" if mf2.returncode == 0 else "FAIL",
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd")

    p_run = sub.add_parser("run", help="dispara pesquisa offline (default)")
    p_run.add_argument(
        "--reason",
        required=True,
        choices=["unknown", "ambiguous", "conflict", "engine_unavailable"],
    )
    p_run.add_argument("--case", required=True)
    p_run.add_argument("--cases", default=str(DEFAULT_CASES))
    p_run.add_argument("--tier", default=str(DEFAULT_TIER))
    p_run.add_argument("--state", default=None, help="digest de certificado (≤320 tokens)")
    p_run.add_argument(
        "--allow-network",
        action="store_true",
        help="reserva F5 — default OFF",
    )
    p_run.add_argument(
        "--write-quarantine",
        default=None,
        help="path JSONL append-only (default: nao grava)",
    )

    p_dig = sub.add_parser("digest", help="só corta o digest ≤320")
    p_dig.add_argument("--state", required=True)

    sub.add_parser("selftest", help="gates + digest + reliability + quarentena")

    # compat: flags no topo sem subcomando = run
    ap.add_argument("--reason", default=None)
    ap.add_argument("--case", default=None)
    ap.add_argument("--cases", default=str(DEFAULT_CASES))
    ap.add_argument("--tier", default=str(DEFAULT_TIER))
    ap.add_argument("--state", default=None)
    ap.add_argument("--allow-network", action="store_true")
    ap.add_argument("--write-quarantine", default=None)

    args = ap.parse_args()

    if args.cmd == "selftest":
        return selftest()
    if args.cmd == "digest":
        print(json.dumps(digest_certificate(args.state), ensure_ascii=False, indent=2))
        return 0

    reason = args.reason
    case = args.case
    if args.cmd == "run":
        reason = args.reason
        case = args.case
    if reason is None or case is None:
        ap.print_help()
        return 2

    if args.allow_network:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": "network_ainda_nao_ligado",
                    "hint": "F2 usa fixtures; tool propria + websearch = F5",
                    "harness_probe": "Cursor WebSearch disponivel no IDE; nao e tomada default",
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 3

    wpath = Path(args.write_quarantine) if args.write_quarantine else None
    payload, code = run_research(
        reason, case, Path(args.cases), Path(args.tier), args.state, wpath
    )
    print(json.dumps(payload, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
