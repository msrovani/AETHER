#!/usr/bin/env python3
# A5 P1 — promove gate reports de target/ → data/ com meta sha (MSG 0990).
"""Fatia distinta do P0 (embed/mf2). Não toca crate, dataset F4 nem tools alheios.

Uso:
  python tools/a5_promote_p1.py
  python tools/a5_promote_p1.py selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
DATA = ROOT / "data"

# P1 — gate reports (P0 = embed/mf2 fica com claim 0986)
P1_NAMES = (
    "f1_vram_watchdog.json",
    "f1_gil_gate.json",
    "qa_suite_probe.json",
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def promote_one(src: Path, dst: Path, meta: Path) -> dict:
    if not src.is_file():
        return {"ok": False, "error": f"src ausente: {src}", "dst": str(dst)}
    if "target" in dst.parts:
        return {"ok": False, "error": "destino nao pode ficar em target/"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    meta_obj = {
        "schema": "aether.a5_promote_p1/v1",
        "ref": "MSG_0990_0967",
        "src": str(src.relative_to(ROOT)).replace("\\", "/"),
        "dst": str(dst.relative_to(ROOT)).replace("\\", "/"),
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "ok": True,
    }
    meta.write_text(json.dumps(meta_obj, ensure_ascii=False, indent=2), encoding="utf-8")
    return meta_obj


def ensure_f4_hold() -> Path:
    """Gera data/f4_gate_hold.json a partir do selftest (sem artefato em target/)."""
    p = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "f4_gate_hold.py"), "selftest"],
        capture_output=True,
        text=True,
        check=False,
    )
    dst = DATA / "f4_gate_hold.json"
    if p.returncode != 0:
        return dst
    try:
        payload = json.loads(p.stdout)
    except json.JSONDecodeError:
        payload = {"ok": False, "raw": (p.stdout or "")[-500:]}
    dst.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    return dst


def run_promote() -> dict:
    DATA.mkdir(parents=True, exist_ok=True)
    rows = []
    for name in P1_NAMES:
        src = TARGET / name
        dst = DATA / name
        meta = DATA / f"{name}.meta.json"
        rows.append(promote_one(src, dst, meta))

    hold = ensure_f4_hold()
    hold_meta = DATA / "f4_gate_hold.json.meta.json"
    if hold.is_file():
        digest = sha256_file(hold)
        hold_meta.write_text(
            json.dumps(
                {
                    "schema": "aether.a5_promote_p1/v1",
                    "ref": "MSG_0990_0967",
                    "src": "tools/f4_gate_hold.py selftest",
                    "dst": "data/f4_gate_hold.json",
                    "sha256": digest,
                    "bytes": hold.stat().st_size,
                    "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
                    "ok": True,
                },
                ensure_ascii=False,
                indent=2,
            ),
            encoding="utf-8",
        )
        rows.append(json.loads(hold_meta.read_text(encoding="utf-8")))
    else:
        rows.append({"ok": False, "error": "f4_gate_hold nao gerado", "dst": str(hold)})

    ok = all(r.get("ok") for r in rows)
    report = {
        "schema": "aether.a5_promote_p1_report/v1",
        "ok": ok,
        "network": False,
        "ref": "MSG_0990",
        "n": len(rows),
        "items": rows,
        "policy": "P1 gate reports em data/; P0 embed/mf2 = claim 0986",
    }
    out = DATA / "a5_promote_p1_report.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return report


def selftest() -> int:
    rep = run_promote()
    failed = []
    if not rep.get("ok"):
        failed.append("promote_ok")
    for name in (*P1_NAMES, "f4_gate_hold.json"):
        if not (DATA / name).is_file():
            failed.append(f"missing:{name}")
        meta = DATA / f"{name}.meta.json"
        if not meta.is_file():
            failed.append(f"missing_meta:{name}")
        else:
            m = json.loads(meta.read_text(encoding="utf-8"))
            if not m.get("sha256"):
                failed.append(f"no_sha:{name}")
    # destino em target/ deve ser recusado
    bad = promote_one(DATA / P1_NAMES[0], TARGET / "nope_p1.json", DATA / "nope_p1.meta.json")
    if bad.get("ok"):
        failed.append("accepted_target_dst")
    out = {"ok": len(failed) == 0, "failed": failed, "n": rep.get("n")}
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    rep = run_promote()
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
