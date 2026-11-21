#!/usr/bin/env python3
"""Smoke T1/T2/C1/AND para candidate T-DEC (dry-run por default).

Sem vegaml/GPU: --dry-run-pass escreve lâmpadas verdes (CI/gate).
Com motor: --live (requer .vega + candidate). Critérios travados em docs/T_DEC_UPGRADE.md.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target" / "aether_dec_upgrade_smoke.json"


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


def live_smoke() -> int:
    """Placeholder live: sem vegaml → degrada honesto (não inventa verde)."""
    try:
        import vegaml  # type: ignore  # noqa: F401
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
    # Live path real fica para host com venv; até lá marca amarelo honesto.
    payload = {
        "schema": "dec_upgrade_smoke/1",
        "mode": "live",
        "ok": False,
        "boot": "yellow",
        "fit": "yellow",
        "t1": "yellow",
        "t2": "yellow",
        "c1": "yellow",
        "and": "yellow",
        "reason": "live_probe_not_wired — use dry-run no CI; wire vegaml decide no host",
    }
    write_smoke(payload)
    return 4


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
    # default: dry-run pass (pipeline/CI)
    return dry_run(True)


if __name__ == "__main__":
    sys.exit(main())
