#!/usr/bin/env python3
# A5 P3 — promove relatórios F1 contract / A1b / F1_5 de target/ → data/ (MSG 099x).
"""Fatia distinta de P0/P1/P2. Offline. Não toca crate nem dataset F4.

Uso:
  python tools/a5_promote_p3.py
  python tools/a5_promote_p3.py selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
DATA = ROOT / "data"

P3_NAMES = (
    "f1_contract.json",
    "f1_contract_primary.json",
    "f1_contract_fit124.json",
    "f1_a1b_vega_check.json",
    "f1_5_ttt_abstain.json",
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def promote_one(src: Path, dst: Path, meta: Path) -> dict:
    if not src.is_file():
        return {"ok": False, "error": f"src ausente: {src}", "name": src.name}
    if "target" in dst.parts:
        return {"ok": False, "error": "destino em target/ recusado", "name": src.name}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    meta_obj = {
        "schema": "aether.a5_promote_p3/v1",
        "ref": "MSG_0967_P3",
        "src": str(src.relative_to(ROOT)).replace("\\", "/"),
        "dst": str(dst.relative_to(ROOT)).replace("\\", "/"),
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "ok": True,
    }
    meta.write_text(json.dumps(meta_obj, ensure_ascii=False, indent=2), encoding="utf-8")
    return meta_obj


def run_promote() -> dict:
    DATA.mkdir(parents=True, exist_ok=True)
    rows = []
    for name in P3_NAMES:
        rows.append(promote_one(TARGET / name, DATA / name, DATA / f"{name}.meta.json"))
    ok = all(r.get("ok") for r in rows)
    report = {
        "schema": "aether.a5_promote_p3_report/v1",
        "ok": ok,
        "network": False,
        "n": len(rows),
        "items": rows,
        "policy": "P3 F1 contract/A1b/F1_5; P0/P1/P2 intactos",
    }
    (DATA / "a5_promote_p3_report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    return report


def selftest() -> int:
    rep = run_promote()
    failed = []
    if not rep.get("ok"):
        failed.append("promote")
    for name in P3_NAMES:
        if not (DATA / name).is_file():
            failed.append(f"missing:{name}")
        meta = DATA / f"{name}.meta.json"
        if not meta.is_file() or not json.loads(meta.read_text(encoding="utf-8")).get("sha256"):
            failed.append(f"meta:{name}")
    bad = promote_one(DATA / P3_NAMES[0], TARGET / "nope_p3.json", DATA / "nope_p3.meta.json")
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
