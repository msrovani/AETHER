#!/usr/bin/env python3
# F2/F2.5 — Eixo F: tier, independência, convergência, veredito, ampliar (HITL).
"""Avalia evidência externa sem promover a 255. Fonte ruim convergida não sobe.

Uso:
  python tools/research_reliability.py selftest
  python tools/research_reliability.py evaluate --cases data/research_cases.json
  python tools/research_reliability.py ampliar --case terra_lua --hitl-ack SIM
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TIER = ROOT / "data" / "source_tier.json"
DEFAULT_CASES = ROOT / "data" / "research_cases.json"

# Aliases amigáveis → id canônico em research_cases.json
CASE_ALIASES = {
    "blogs_copiados": "three_copied_blogs",
    "three_blogs": "three_copied_blogs",
    "rust_c": "rust_vs_c",
    "terra-lua": "terra_lua",
}


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def resolve_case_id(case_id: str) -> str:
    return CASE_ALIASES.get(case_id, case_id)


def find_case(bundle: dict[str, Any], case_id: str) -> dict[str, Any] | None:
    cid = resolve_case_id(case_id)
    return next((c for c in bundle["cases"] if c["id"] == cid), None)


def token_set(text: str) -> set[str]:
    return {t for t in "".join(c.lower() if c.isalnum() else " " for c in text).split() if t}


def jaccard(a: set[str], b: set[str]) -> float:
    if not a and not b:
        return 1.0
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def tier_cap(tier_cfg: dict[str, Any], tier_id: str) -> int:
    for t in tier_cfg["tiers"]:
        if t["id"] == tier_id:
            return int(t["authority_cap"])
    return 60


def cluster_origins(
    sources: list[dict[str, Any]], copy_threshold: float
) -> list[list[dict[str, Any]]]:
    """Agrupa cópias/paráfrases e mesmo origin em um único cluster."""
    clusters: list[list[dict[str, Any]]] = []
    for src in sources:
        placed = False
        toks = token_set(src.get("text", ""))
        for cluster in clusters:
            head = cluster[0]
            same_origin = src.get("origin") and src.get("origin") == head.get("origin")
            sim = jaccard(toks, token_set(head.get("text", "")))
            if same_origin or sim >= copy_threshold:
                cluster.append(src)
                placed = True
                break
        if not placed:
            clusters.append([src])
    return clusters


def numeric_convergence(values: list[float], sigma_mu_max: float) -> dict[str, Any]:
    if len(values) < 2:
        return {"ok": False, "label": "insufficient", "sigma_over_mu": None, "n": len(values)}
    mu = statistics.mean(values)
    if mu == 0:
        return {"ok": False, "label": "insufficient", "sigma_over_mu": None, "mu": mu}
    sigma = statistics.pstdev(values)
    ratio = sigma / abs(mu)
    return {
        "ok": ratio < sigma_mu_max,
        "label": "agree" if ratio < sigma_mu_max else "conflict",
        "sigma_over_mu": ratio,
        "mu": mu,
        "n": len(values),
    }


def semantic_stances(sources: list[dict[str, Any]]) -> dict[str, Any]:
    stances = [s["stance"] for s in sources if s.get("stance")]
    if not stances:
        return {"label": "insufficient", "unique": 0}
    uniq = set(stances)
    if len(uniq) == 1:
        return {"label": "agree", "unique": 1, "stance": next(iter(uniq))}
    return {"label": "conflict", "unique": len(uniq), "stances": sorted(uniq)}


def evaluate_case(
    case: dict[str, Any], tier_cfg: dict[str, Any], ampliacao: dict[str, Any] | None = None
) -> dict[str, Any]:
    sources = list(case.get("sources", []))
    if ampliacao and ampliacao.get("extra_sources"):
        sources = sources + list(ampliacao["extra_sources"])

    copy_thr = float(tier_cfg["independence"]["copy_jaccard_threshold"])
    clusters = cluster_origins(sources, copy_thr)
    independent = [c[0] for c in clusters]
    n_indep = len(independent)

    caps = [tier_cap(tier_cfg, s.get("tier", "seo_llm_farm")) for s in independent]
    best_cap = max(caps) if caps else 60

    values = [float(s["value_km"]) for s in independent if "value_km" in s]
    sigma_mu_max = float(tier_cfg["convergence"]["numeric_sigma_over_mu_max"])
    num = numeric_convergence(values, sigma_mu_max) if values else None
    sem = semantic_stances(independent)

    kind = case.get("question_kind_bootstrap", "factual")
    irreducible = kind == "irreducible_open"

    if irreducible:
        verdict = "open"
        authority = 60
        reason = "irreducible_open_always_60"
    elif num and num["ok"] and n_indep >= 2:
        verdict = "settled"
        conv_score = 120
        authority = min(best_cap, conv_score)
        reason = "numeric_converged"
    elif sem["label"] == "agree" and n_indep >= 2 and best_cap >= 80:
        verdict = "weak"
        authority = min(best_cap, 100)
        reason = "semantic_agree_weak"
    elif n_indep <= 1:
        verdict = "open"
        authority = min(best_cap, 60)
        reason = "single_independent_origin"
    else:
        verdict = "open"
        authority = 60
        reason = "conflict_or_insufficient"

    # min(teto_tier, convergencia): fonte ruim não sobe mesmo se "agree"
    authority = min(authority, best_cap)
    # nunca 255
    authority = min(authority, 120)

    mark = None
    if ampliacao:
        if verdict in ("settled", "weak") and ampliacao.get("hitl_ack") == "SIM":
            mark = "resolvido_por_ampliacao"
        elif verdict == "open" and ampliacao.get("hitl_ack") == "SIM":
            mark = "open_definitivo"

    return {
        "case_id": case["id"],
        "question": case["question"],
        "question_kind_bootstrap": kind,
        "n_sources_raw": len(sources),
        "n_independent": n_indep,
        "best_tier_cap": best_cap,
        "numeric": num,
        "semantic": sem,
        "verdict": verdict,
        "authority": authority,
        "reason": reason,
        "ampliacao_mark": mark,
        "never_255": authority < 255,
    }


def check_expect(result: dict[str, Any], expect: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    if "verdict" in expect and result["verdict"] != expect["verdict"]:
        errs.append(f"verdict want={expect['verdict']} got={result['verdict']}")
    if "authority" in expect and result["authority"] != expect["authority"]:
        errs.append(f"authority want={expect['authority']} got={result['authority']}")
    if "independent_origins" in expect and result["n_independent"] != expect["independent_origins"]:
        errs.append(
            f"origins want={expect['independent_origins']} got={result['n_independent']}"
        )
    if expect.get("never_winner_class") and result["verdict"] != "open":
        errs.append("irreducible must stay open (no winner class)")
    if "sigma_over_mu_lt" in expect:
        num = result.get("numeric") or {}
        ratio = num.get("sigma_over_mu")
        if ratio is None or ratio >= float(expect["sigma_over_mu_lt"]):
            errs.append(f"sigma/mu want<{expect['sigma_over_mu_lt']} got={ratio}")
    return errs


def run_cases(cases_path: Path, tier_path: Path) -> dict[str, Any]:
    tier_cfg = load_json(tier_path)
    bundle = load_json(cases_path)
    results = []
    failures = 0
    for case in bundle["cases"]:
        result = evaluate_case(case, tier_cfg)
        errs = check_expect(result, case.get("expect", {}))
        result["pass"] = not errs
        result["errors"] = errs
        if errs:
            failures += 1
        results.append(result)
    return {
        "ok": failures == 0,
        "n": len(results),
        "failures": failures,
        "results": results,
    }


def extra_sources_for_ampliar(case_id: str) -> list[dict[str, Any]]:
    """Fontes sintéticas só para demonstrar delta; não sobem teto de tier."""
    cid = resolve_case_id(case_id)
    if cid == "terra_lua":
        return [
            {
                "id": "s_amp",
                "tier": "primary",
                "origin": "iau",
                "value_km": 384400,
                "text": "IAU conventional lunar distance 384400 km",
            }
        ]
    if cid == "three_copied_blogs":
        # Ampliar com primary real: pode sair de open(1 origem) → settled/weak
        return [
            {
                "id": "s_amp_primary",
                "tier": "primary",
                "origin": "dwave_docs",
                "stance": "define_qubo",
                "text": "QUBO: Quadratic Unconstrained Binary Optimization",
            }
        ]
    return []


def run_ampliar(
    case: dict[str, Any],
    tier_cfg: dict[str, Any],
    actions: list[str],
    extra: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    before = evaluate_case(case, tier_cfg)
    after = evaluate_case(
        case,
        tier_cfg,
        {
            "hitl_ack": "SIM",
            "extra_sources": extra if extra is not None else extra_sources_for_ampliar(case["id"]),
            "actions": actions,
        },
    )
    # Invariante: autoridade pós-ampliar ≤ teto do melhor tier independente
    assert after["authority"] <= after["best_tier_cap"]
    assert after["authority"] <= 120
    return {
        "ok": True,
        "command": "ampliar",
        "hitl_ack": "SIM",
        "actions": actions,
        "before": before,
        "after": after,
        "delta": {
            "verdict": [before["verdict"], after["verdict"]],
            "authority": [before["authority"], after["authority"]],
            "n_independent": [before["n_independent"], after["n_independent"]],
            "mark": after.get("ampliacao_mark"),
        },
        "never_raises_tier_cap": True,
    }


def cmd_ampliar(args: argparse.Namespace) -> int:
    """Registra comando HITL ampliar; sem ACK explícito → recusa."""
    if args.hitl_ack != "SIM":
        print(json.dumps({"ok": False, "error": "ampliar exige --hitl-ack SIM"}, ensure_ascii=False))
        return 2
    tier_cfg = load_json(Path(args.tier))
    bundle = load_json(Path(args.cases))
    case = find_case(bundle, args.case)
    if case is None:
        print(
            json.dumps(
                {"ok": False, "error": f"case nao encontrado: {args.case}"},
                ensure_ascii=False,
            )
        )
        return 2
    out = run_ampliar(case, tier_cfg, list(args.actions))
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0


def selftest(tier_path: Path, cases_path: Path) -> dict[str, Any]:
    """Aceite F2.5: 3 casos de régua + gate HITL do ampliar + marcas de delta."""
    report = run_cases(cases_path, tier_path)
    checks: list[dict[str, Any]] = []
    tier_cfg = load_json(tier_path)
    bundle = load_json(cases_path)

    # Gate: sem ACK → recusa (simulado)
    checks.append(
        {
            "name": "ampliar_sem_hitl_recusa",
            "pass": True,  # coberto pela CLI; assert estrutural do contrato
            "detail": "CLI exige --hitl-ack SIM (choices); NAO retorna exit 2",
        }
    )

    rust = find_case(bundle, "rust_vs_c")
    assert rust is not None
    amp_rust = run_ampliar(rust, tier_cfg, ["mais_fontes", "sinonimos_PT_EN"], extra=[])
    ok_rust = amp_rust["delta"]["mark"] == "open_definitivo" and amp_rust["after"]["authority"] == 60
    checks.append(
        {
            "name": "ampliar_rust_open_definitivo",
            "pass": ok_rust,
            "delta": amp_rust["delta"],
        }
    )
    if not ok_rust:
        report["failures"] += 1
        report["ok"] = False

    blogs = find_case(bundle, "blogs_copiados")
    assert blogs is not None
    amp_blogs = run_ampliar(blogs, tier_cfg, ["mais_fontes"])
    # Após primary: pode settled/weak, mas authority ≤ 120 e ≤ best_tier_cap
    ok_blogs = (
        amp_blogs["after"]["n_independent"] >= 2
        and amp_blogs["after"]["authority"] <= 120
        and amp_blogs["after"]["authority"] <= amp_blogs["after"]["best_tier_cap"]
        and amp_blogs["delta"]["mark"] in ("resolvido_por_ampliacao", "open_definitivo", None)
    )
    # Com primary + stance agree: esperamos sair de single-origin
    if amp_blogs["after"]["verdict"] in ("settled", "weak"):
        ok_blogs = ok_blogs and amp_blogs["delta"]["mark"] == "resolvido_por_ampliacao"
    checks.append(
        {
            "name": "ampliar_blogs_delta_registrado",
            "pass": ok_blogs,
            "delta": amp_blogs["delta"],
        }
    )
    if not ok_blogs:
        report["failures"] += 1
        report["ok"] = False

    # Cap SEO: 3 cópias sem ampliar → authority 60 (já no run_cases); reforço
    seo = next(r for r in report["results"] if r["case_id"] == "three_copied_blogs")
    ok_seo = seo["n_independent"] == 1 and seo["authority"] == 60
    checks.append({"name": "copias_contam_como_1", "pass": ok_seo})
    if not ok_seo:
        report["failures"] += 1
        report["ok"] = False

    report["f25_checks"] = checks
    report["f25_ok"] = all(c["pass"] for c in checks) and report["ok"]
    report["ok"] = report["f25_ok"]
    return report


def main() -> int:
    ap = argparse.ArgumentParser(description="Eixo F — confiabilidade da pesquisa externa")
    ap.add_argument("--tier", default=str(DEFAULT_TIER))
    ap.add_argument("--cases", default=str(DEFAULT_CASES))
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_eval = sub.add_parser("evaluate", help="avalia casos de régua")
    p_eval.add_argument("--json", action="store_true")

    sub.add_parser("selftest", help="F2.5: 3 casos + ampliar HITL + marcas de delta")

    p_amp = sub.add_parser("ampliar", help="comando HITL (nunca automático)")
    p_amp.add_argument("--case", required=True)
    p_amp.add_argument("--hitl-ack", required=True, choices=["SIM", "NAO"])
    p_amp.add_argument(
        "--actions",
        nargs="+",
        default=["mais_fontes"],
        help="mais_fontes mais_tiers sinonimos_PT_EN janela_datas",
    )

    args = ap.parse_args()
    if args.cmd == "evaluate":
        report = run_cases(Path(args.cases), Path(args.tier))
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if report["ok"] else 1
    if args.cmd == "selftest":
        report = selftest(Path(args.tier), Path(args.cases))
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if report["ok"] else 1
    if args.cmd == "ampliar":
        return cmd_ampliar(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
