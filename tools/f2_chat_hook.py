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
import research_live  # noqa: E402
from research_intent import classify_request  # noqa: E402
from research_live import live_research  # noqa: E402
from research_persist import persist_research  # noqa: E402
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

    # Gate: research | memory | deny — evita limbo fontes=0 (~50% perdido).
    intent = classify_request(text)
    if intent["action"] == "memory":
        # Web fechada; citação (se houver) já saiu no CLI. Sem citação o CLI
        # imprime desconhecimento — aqui só sinaliza o ramo.
        rec = {
            "kind": "memory_only",
            "abstain_reason": "unknown",
            "question": text[:240],
            "verdict": "memory_only",
            "authority": 60,
            "n_independent": 0,
            "reason": intent["reason"],
            "hitl_required_for_255": True,
            "never_255": True,
            "network": False,
            "state_digest": {"n_tokens": dig["n_tokens"], "preview": dig["digest"][:120]},
        }
        write_quarantine(out, rec)
        return {
            "opens": False,
            "line": intent["line"],
            "network": False,
            "case": None,
            "intent": "memory",
            "intent_reason": intent["reason"],
        }
    if intent["action"] == "deny":
        rec = {
            "kind": "knowledge_deny",
            "abstain_reason": "unknown",
            "question": text[:240],
            "verdict": "denied",
            "authority": 60,
            "n_independent": 0,
            "reason": intent["reason"],
            "hitl_required_for_255": True,
            "never_255": True,
            "network": False,
            "state_digest": {"n_tokens": dig["n_tokens"], "preview": dig["digest"][:120]},
        }
        write_quarantine(out, rec)
        return {
            "opens": False,
            "line": intent["line"],
            "network": False,
            "case": None,
            "intent": "deny",
            "intent_reason": intent["reason"],
        }

    # G4 (opt-in HITL): vetor REAL só para pista factual.
    if research_live.network_enabled():
        live = research_live.live_research(text, DEFAULT_TIER)
        n_src = int(live.get("n_independent") or 0)
        trail = live.get("provider_trail") or []
        prov = ",".join(live.get("providers") or []) or "nenhum"
        # Cascata vazia → negar conhecimento (não ficar em open/fontes=0).
        if n_src <= 0:
            rec = {
                "kind": "knowledge_deny",
                "abstain_reason": "unknown",
                "question": text[:240],
                "verdict": "denied",
                "authority": 60,
                "n_independent": 0,
                "reason": "sem_fontes_apos_cascata",
                "provider_trail": trail,
                "hitl_required_for_255": True,
                "never_255": True,
                "network": True,
                "state_digest": {"n_tokens": dig["n_tokens"], "preview": dig["digest"][:120]},
            }
            write_quarantine(out, rec)
            return {
                "opens": False,
                "line": (
                    "desconhecimento: pesquisa sem fontes utilizáveis — "
                    f"não afirmo (trail={'|'.join(trail)})"
                ),
                "network": True,
                "case": None,
                "intent": "deny",
                "intent_reason": "sem_fontes_apos_cascata",
                "provider_trail": trail,
            }
        rec = {
            "kind": "external_research",
            "abstain_reason": "unknown",
            "question": text[:240],
            "verdict": live.get("verdict", "open"),
            "authority": live.get("authority", 60),
            "n_independent": n_src,
            "reason": live.get("reason"),
            "convergence_sigma_over_mu": live.get("convergence_sigma_over_mu"),
            "evidence": live.get("evidence", []),
            "hitl_required_for_255": True,
            "never_255": True,
            "network": True,
            "state_digest": {"n_tokens": dig["n_tokens"], "preview": dig["digest"][:120]},
        }
        write_quarantine(out, rec)
        # Persiste na memória durável (≤120) para o turno seguinte citar.
        mem = persist_research(text, live)
        mem_note = "mem=ok" if mem.get("ok") else f"mem=fail:{mem.get('error') or mem.get('exit')}"
        return {
            "opens": True,
            "line": (
                f"pesquisa (vivo): quarentena veredito={rec['verdict']} "
                f"autoridade={rec['authority']} fontes={rec['n_independent']} "
                f"motivo={rec['reason']} provider={prov} "
                f"trail={'|'.join(trail)} {mem_note}"
            ),
            "network": True,
            "case": None,
            "providers": live.get("providers"),
            "provider_trail": trail,
            "intent": "research",
            "persisted": mem,
        }
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
    # Selftest offline: não misturar com cascata viva.
    os_env_net = __import__("os").environ.pop("AETHER_RESEARCH_NETWORK", None)
    failures: list[str] = []
    try:
        closed = once("margem", "ambiguous", path)
        if closed["opens"] or path.exists():
            failures.append("ambiguous abriu")
        # Lixo → negar (não gastar rede / não sem_caso fantasma).
        gap = once("asdf qwer zxcv 12345", "unknown", path)
        if gap.get("opens") or gap.get("intent") != "deny":
            failures.append(f"lixo_deveria_negar: {gap}")
        if "terra_lua" in gap.get("line", ""):
            failures.append("lixo virou terra_lua")
        # Fato da régua offline (rede off) → caso terra_lua.
        known = once("Qual a distancia media Terra-Lua?", "unknown", path)
        if known.get("case") != "terra_lua" or known.get("authority") != 120:
            failures.append(f"terra_lua: {known}")
        if find_case(load_json(DEFAULT_CASES), "terra_lua") is None:
            failures.append("regua ausente")
    finally:
        if os_env_net is not None:
            __import__("os").environ["AETHER_RESEARCH_NETWORK"] = os_env_net
    report = {"ok": not failures, "failures": failures, "ref": "MSG_1022+intent_gate"}
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
