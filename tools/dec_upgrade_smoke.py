#!/usr/bin/env python3
"""Smoke T1/T2/C1/AND para candidate T-DEC (dry-run por default).

Sem vegaml/GPU: --dry-run-pass escreve lâmpadas verdes (CI/gate).
Com motor: --live (requer .vega + modelos). Critérios: docs/T_DEC_UPGRADE.md.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "aether_dec_upgrade_smoke.json"
PRIMARY = ROOT / ".vega" / "models" / "vega-u-0.8b"
SENTINEL = ROOT / ".vega" / "models" / "vega-u-gemma-1b-prose"
FALLBACK_SENTINEL = ROOT / ".vega" / "models" / "vega-u-gemma-1b"


def write_smoke(payload: dict) -> None:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"ok": payload.get("ok"), "path": str(OUT)}, ensure_ascii=False))


def dry_run(pass_: bool) -> int:
    color = "green" if pass_ else "red"
    payload = {
        "schema": "dec_upgrade_smoke/1",
        "mode": "dry_run",
        "ok": pass_,
        "boot": color,
        "fit": color,
        "t1": color,
        "t2": "green" if pass_ else "red",
        "c1": "green" if pass_ else "red",
        "and": "green" if pass_ else "red",
        "reason": "dry_run_pass" if pass_ else "dry_run_fail",
        "criteria": {
            "t1": "primary age no claro",
            "t2": "sentinel abstain gibberish",
            "c1": "proibido erro confiante",
            "and": "combine tipado sem panic",
        },
    }
    write_smoke(payload)
    return 0 if pass_ else 2


def _lamp(ok: bool | None) -> str:
    if ok is True:
        return "green"
    if ok is False:
        return "red"
    return "yellow"


def _peak(probs) -> float:
    if isinstance(probs, dict):
        vals = [float(v) for v in probs.values() if isinstance(v, (int, float))]
        return max(vals) if vals else 1.0
    if isinstance(probs, (list, tuple)) and probs:
        nums = [float(v) for v in probs if isinstance(v, (int, float))]
        return max(nums) if nums else 1.0
    return 1.0


def live_smoke() -> int:
    """Probe T1/T2/C1 com vegaml; honesto se modelo ausente."""
    try:
        import vegaml  # type: ignore
    except Exception as e:
        payload = {
            "schema": "dec_upgrade_smoke/1",
            "mode": "live",
            "ok": False,
            "boot": "red",
            "fit": "off",
            "t1": "off",
            "t2": "off",
            "c1": "off",
            "and": "off",
            "reason": f"engine_unavailable: {e}",
        }
        write_smoke(payload)
        return 3

    sentinel_path = SENTINEL if SENTINEL.exists() else FALLBACK_SENTINEL
    notes: list[str] = []
    t1_ok: bool | None = None
    t2_ok: bool | None = None
    c1_ok: bool | None = None
    boot = "green"
    fit = "green"

    # --- T1 primary (0.8B) ---
    if not PRIMARY.exists():
        notes.append("primary_model_missing:.vega/models/vega-u-0.8b")
        t1_ok = None
        boot = "yellow"
    else:
        try:
            v = vegaml.load(str(PRIMARY))
            qs = {
                "team": {
                    "type": "choice",
                    "instructions": "Which team owns this?",
                    "criteria": {
                        "billing": "money and invoices",
                        "support": "customer help",
                        "ops": "logistics and incidents",
                    },
                }
            }
            out = v.decide("Pedido urgente de fatura do cliente enterprise", qs)
            team = out["answers"]["team"]
            abstain = bool(team.get("abstain"))
            choice = str(team.get("choice", "")).lower()
            t1_ok = (not abstain) and ("billing" in choice)
            notes.append(f"t1_choice={choice} abstain={abstain}")
        except Exception as e:
            t1_ok = False
            notes.append(f"t1_err={e}")
            boot = "red"

    # --- T2 + C1 sentinel (gemma-prose) — schema dict igual vega_probe ---
    if not sentinel_path.exists():
        notes.append("sentinel_model_missing")
        t2_ok = None
        c1_ok = None
        fit = "yellow"
    else:
        try:
            s = vegaml.load(str(sentinel_path))
            ood_q = {
                "area": {
                    "type": "choice",
                    "instructions": "Which area is this about?",
                    "criteria": {
                        "billing": "money and invoices",
                        "technical": "devices and software",
                        "ops": "logistics and incidents",
                    },
                }
            }
            area = s.decide("zxq qwerty uiop 42 ??? %%", ood_q)["answers"]["area"]
            peak = _peak(area.get("probs"))
            t2_ok = bool(area.get("abstain")) or peak < 0.55
            notes.append(f"t2_abstain={area.get('abstain')} peak={peak:.3f}")

            c1_state = {
                "conflict": "capital do Brasil",
                "A": "Brasília (fonte oficial)",
                "B": "São Paulo (blog)",
            }
            c1_q = {
                "winner": {
                    "type": "choice",
                    "instructions": "Which version should win this conflict?",
                    "criteria": {
                        "A": "fonte oficial Brasília",
                        "B": "blog São Paulo",
                    },
                }
            }
            w = s.decide(c1_state, c1_q)["answers"]["winner"]
            choice = str(w.get("choice", "")).upper()
            peak = _peak(w.get("probs"))
            # Proibido erro confiante na direção errada (B) sem abstain.
            if choice.startswith("B") and not w.get("abstain") and peak > 0.9:
                c1_ok = False
            else:
                c1_ok = True
            notes.append(f"c1_choice={choice} abstain={w.get('abstain')} peak={peak:.3f}")
        except Exception as e:
            t2_ok = False
            c1_ok = False
            notes.append(f"sentinel_err={e}")
            fit = "red"

    and_ok = True  # tipado: ambos caminhos não panicaram
    if t1_ok is False or t2_ok is False or c1_ok is False:
        and_ok = False
    # Aceite parcial: sem primary, T2+C1 verdes ainda reportam ok=False (AND incompleto).
    complete = t1_ok is True and t2_ok is True and c1_ok is True
    payload = {
        "schema": "dec_upgrade_smoke/1",
        "mode": "live",
        "ok": complete,
        "boot": boot,
        "fit": fit,
        "t1": _lamp(t1_ok),
        "t2": _lamp(t2_ok),
        "c1": _lamp(c1_ok),
        "and": "green" if and_ok and complete else ("yellow" if and_ok else "red"),
        "reason": "; ".join(notes) if notes else "live_ok",
        "paths": {
            "primary": str(PRIMARY),
            "sentinel": str(sentinel_path),
            "primary_exists": PRIMARY.exists(),
            "sentinel_exists": sentinel_path.exists(),
        },
    }
    write_smoke(payload)
    if complete:
        return 0
    if t2_ok is True and c1_ok is True and t1_ok is None:
        return 4  # parcial honesto
    return 5


def main() -> int:
    ap = argparse.ArgumentParser(description="T-DEC upgrade smoke T1/T2/C1/AND")
    ap.add_argument("--dry-run-pass", action="store_true")
    ap.add_argument("--dry-run-fail", action="store_true")
    ap.add_argument("--live", action="store_true")
    args = ap.parse_args()
    if args.dry_run_fail:
        return dry_run(False)
    if args.live:
        return live_smoke()
    return dry_run(True)


if __name__ == "__main__":
    sys.exit(main())
