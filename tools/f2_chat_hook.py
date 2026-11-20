#!/usr/bin/env python3
# F2 — o chat só pesquisa quando o port diz unknown (MSG 1022).
"""Não usa terra_lua como caso padrão. Texto sem caso fica sem_caso, autoridade 60.

Uso:
  python tools/f2_chat_hook.py selftest
  python tools/f2_chat_hook.py once --text "zxq"
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from research_harness import (  # noqa: E402
    DEFAULT_CASES,
    DEFAULT_TIER,
    digest_certificate,
    run_research,
    write_quarantine,
)
from research_reliability import find_case, load_json  # noqa: E402

OUT = ROOT / "data" / "reports" / "f2_chat_quarantine.jsonl"


def _norm(text: str) -> str:
    return "".join(ch for ch in text.casefold() if ch.isalnum() or ch.isspace()).split()


def match_case(text: str, cases_path: Path) -> str | None:
    bundle = load_json(cases_path)
    words = set(_norm(text))
    if len(words) < 2:
        return None
    for case in bundle.get("cases", []):
        q_words = set(_norm(str(case.get("question") or "")))
        if q_words and q_words <= words:
            return str(case["id"])
    return None


def once(text: str, reason: str, out: Path) -> dict:
    if reason != "unknown":
        return {"opens": False, "line": "pesquisa: nao_abre", "network": False}
    dig = digest_certificate(f"edge=fora_lexico text={text}")
    if dig["n_tokens"] > 320 or not dig.get("ok", True):
        return {"opens": False, "line": "pesquisa: digest_reject", "network": False}
    case_id = match_case(text, DEFAULT_CASES)
    if case_id is None:
        rec = {
            "kind": "external_research",
            "abstain_reason": "unknown",
            "question": text[:240],
            "verdict": "sem_caso",
            "authority": 60,
            "n_independent": 0,
            "reason": "nenhum_caso_da_regua",
            "hitl_required_for_255": True,
            "never_255": True,
            "network": False,
            "state_digest": {"n_tokens": dig["n_tokens"], "preview": dig["digest"][:120]},
        }
        write_quarantine(out, rec)
        return {
            "opens": True,
            "line": "pesquisa: quarentena veredito=sem_caso autoridade=60",
            "network": False,
            "case": None,
        }
    payload, code = run_research("unknown", case_id, DEFAULT_CASES, DEFAULT_TIER, dig["digest"], out)
    q = payload.get("quarantine") or {}
    verdict = (payload.get("result") or {}).get("verdict")
    auth = q.get("authority")
    return {
        "opens": code == 0,
        "line": f"pesquisa: quarentena veredito={verdict} autoridade={auth} caso={case_id}",
        "network": False,
        "case": case_id,
        "authority": auth,
    }


def selftest() -> int:
    path = ROOT / "target" / "f2_chat_hook_selftest.jsonl"
    if path.exists():
        path.unlink()
    failures: list[str] = []
    closed = once("margem", "ambiguous", path)
    if closed["opens"] or path.exists():
        failures.append("ambiguous abriu")
    gap = once("zxq qwerty fora do lexico", "unknown", path)
    if not gap["opens"] or gap.get("case") is not None:
        failures.append(f"sem_caso: {gap}")
    if "terra_lua" in gap["line"]:
        failures.append("sem_caso virou terra_lua")
    known = once("Qual a distancia media Terra-Lua?", "unknown", path)
    if known.get("case") != "terra_lua" or known.get("authority") != 120:
        failures.append(f"terra_lua: {known}")
    if find_case(load_json(DEFAULT_CASES), "terra_lua") is None:
        failures.append("regua ausente")
    report = {"ok": not failures, "failures": failures, "ref": "MSG_1022"}
    print(json.dumps(report, ensure_ascii=False))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    p = sub.add_parser("once")
    p.add_argument("--text", required=True)
    p.add_argument("--reason", default="unknown")
    p.add_argument("--out", type=Path, default=OUT)
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    result = once(args.text, args.reason, args.out)
    print(result["line"])
    return 0 if result["line"] != "pesquisa: digest_reject" else 1


if __name__ == "__main__":
    sys.exit(main())
