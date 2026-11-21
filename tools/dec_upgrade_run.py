#!/usr/bin/env python3
"""Orquestra upgrade T-DEC no candidate (dual-bank).

Fases: Fetch → Attach → Fit → Smoke → atualiza target/aether_dec_upgrade.json.
Não toca profiles/active. --abort limpa candidate e marca AbortRollback.
Default: dry-run (sem HF) para lab/CI; --network habilita fetch opt-in.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / "target" / "aether_dec_upgrade.json"
CAND = ROOT / ".vega" / "profiles" / "candidate"
SMOKE = ROOT / "tools" / "dec_upgrade_smoke.py"


def load_state(path: Path) -> dict:
    if not path.exists():
        return {"schema": "dec_upgrade/1", "phase": "idle", "lamps": {}, "fallback_armed": True}
    return json.loads(path.read_text(encoding="utf-8"))


def save_state(path: Path, st: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    st["updated_ms"] = int(time.time() * 1000)
    path.write_text(json.dumps(st, ensure_ascii=False, indent=2), encoding="utf-8")


def set_lamp(st: dict, key: str, color: str) -> None:
    lamps = st.setdefault("lamps", {})
    lamps[key] = color


def abort(st: dict, reason: str) -> None:
    if CAND.exists():
        shutil.rmtree(CAND, ignore_errors=True)
    st["phase"] = "abort_rollback"
    st["product_bank"] = "active"
    st["fallback_armed"] = True
    set_lamp(st, "ok_geral", "red")
    st["reason"] = reason


def ensure_candidate_lock(st: dict) -> None:
    CAND.mkdir(parents=True, exist_ok=True)
    lock = {
        "schema": "engine.lock/1",
        "bank": "candidate",
        "primary": {"id": st.get("candidate_id") if st.get("slot") == "primary" else "vega-u-0.8b"},
        "sentinel": {
            "id": st.get("candidate_id") if st.get("slot") == "sentinel" else "gemma-3-1b-prose"
        },
        "engine_label": f"{st.get('candidate_id', 'cand')} AND peer",
        "fingerprint": f"candidate-{st.get('updated_ms', 0)}",
        "hf_id": st.get("candidate_hf"),
        "revision": st.get("candidate_revision"),
        "dtype": st.get("dtype"),
    }
    if st.get("slot") == "primary":
        lock["primary"].update(
            {
                "hf_id": st.get("candidate_hf"),
                "revision": st.get("candidate_revision"),
                "dtype": st.get("dtype"),
            }
        )
    else:
        lock["sentinel"].update(
            {
                "hf_id": st.get("candidate_hf"),
                "revision": st.get("candidate_revision"),
                "dtype": st.get("dtype"),
            }
        )
    (CAND / "engine.lock.json").write_text(
        json.dumps(lock, ensure_ascii=False, indent=2), encoding="utf-8"
    )


def apply_smoke_to_state(st: dict, smoke: dict) -> None:
    for k in ("boot", "fit", "t1", "t2", "c1"):
        set_lamp(st, k, smoke.get(k, "red"))
    set_lamp(st, "and", smoke.get("and", "red"))
    set_lamp(st, "fetch", "green")
    st["smoke_path"] = str(ROOT / "target" / "aether_dec_upgrade_smoke.json")
    ok = all(
        smoke.get(k) == "green" for k in ("boot", "fit", "t1", "t2", "c1", "and")
    )
    if not ok:
        abort(st, smoke.get("reason", "smoke falhou"))
        return
    st["phase"] = "quarantine"
    set_lamp(st, "quarantine", "yellow")
    st["reason"] = "smoke verde — aguarda HITL cutover (hub)"
    # HITL cutover é enfileirado pelo Rust apply_smoke_result; aqui só deixa quarentena.
    # Se o job Rust não rodou apply: marca hitl_cutover para o hub detectar.
    st["phase"] = "hitl_cutover"
    st["reason"] = "smoke verde — HITL cutover pendente"


def run_pipeline(st: dict, network: bool, dry_pass: bool) -> int:
    if st.get("phase") not in ("fetch", "attach", "fit", "smoke", "hitl_approve_download"):
        # após mark_download_approved a fase é fetch
        if st.get("phase") != "fetch":
            print(f"fase inesperada: {st.get('phase')}", file=sys.stderr)
            return 1

    # Fetch
    st["phase"] = "fetch"
    set_lamp(st, "fetch", "yellow")
    st["reason"] = "fetch candidate"
    save_state(STATE, st)
    if network:
        st["reason"] = "network fetch solicitado — implementacao HF fica no host; dry artefato"
    set_lamp(st, "fetch", "green")

    # Attach
    st["phase"] = "attach"
    st["reason"] = "attach candidate"
    save_state(STATE, st)
    ensure_candidate_lock(st)

    # Fit
    st["phase"] = "fit"
    set_lamp(st, "fit", "yellow")
    st["reason"] = "fit heads (dry)"
    save_state(STATE, st)
    set_lamp(st, "fit", "green")

    # Smoke
    st["phase"] = "smoke"
    st["reason"] = "smoke T1/T2/C1/AND"
    save_state(STATE, st)
    args = [sys.executable, str(SMOKE)]
    if dry_pass:
        args.append("--dry-run-pass")
    else:
        args.append("--live")
    proc = subprocess.run(args, cwd=str(ROOT), capture_output=True, text=True)
    smoke_path = ROOT / "target" / "aether_dec_upgrade_smoke.json"
    if not smoke_path.exists():
        abort(st, f"smoke sem artefato exit={proc.returncode}")
        save_state(STATE, st)
        return 2
    smoke = json.loads(smoke_path.read_text(encoding="utf-8"))
    apply_smoke_to_state(st, smoke)
    save_state(STATE, st)
    print(json.dumps({"phase": st["phase"], "reason": st["reason"]}, ensure_ascii=False))
    return 0 if st["phase"] in ("quarantine", "hitl_cutover") else 2


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--state", default=str(STATE))
    ap.add_argument("--abort", action="store_true")
    ap.add_argument("--reason", default="abort")
    ap.add_argument("--network", action="store_true", help="opt-in HF (disjuntor rede)")
    ap.add_argument("--live", action="store_true")
    args = ap.parse_args()
    path = Path(args.state)
    st = load_state(path)
    if args.abort:
        abort(st, args.reason)
        save_state(path, st)
        print(json.dumps({"phase": "abort_rollback", "reason": args.reason}))
        return 0
    return run_pipeline(st, network=args.network, dry_pass=not args.live)


if __name__ == "__main__":
    sys.exit(main())
