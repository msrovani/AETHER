#!/usr/bin/env python3
# A5 P2 — promove tier reports target/ → data/reports/ (MSG claim A5_P2).
"""Não toca P0 (embed/mf2) nem P1 (gil/vram/qa). Só relatórios de tier.

Uso:
  python tools/a5_p2_promote.py
  python tools/a5_p2_promote.py selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

CATALOG: list[tuple[str, str, str]] = [
    ("target/f5_t0_eval.json", "data/reports/f5_t0_eval.json", "data/reports/f5_t0_eval.meta.json"),
    ("target/f5_t3_eval.json", "data/reports/f5_t3_eval.json", "data/reports/f5_t3_eval.meta.json"),
    (
        "target/vega_path_audit.json",
        "data/reports/vega_path_audit.json",
        "data/reports/vega_path_audit.meta.json",
    ),
    (
        "target/f1_offline_suite.json",
        "data/reports/f1_offline_suite.json",
        "data/reports/f1_offline_suite.meta.json",
    ),
]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def promote_one(src_rel: str, dst_rel: str, meta_rel: str) -> dict:
    src = ROOT / src_rel
    dst = ROOT / dst_rel
    meta_path = ROOT / meta_rel
    if not src.is_file():
        return {"ok": False, "src": src_rel, "error": "src_ausente"}
    if "target" in Path(dst_rel).parts:
        return {"ok": False, "src": src_rel, "error": "dst_em_target"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    meta = {
        "schema": "aether.a5_p2_promote/v1",
        "ref": "MSG_A5_P2",
        "src": src_rel,
        "dst": dst_rel,
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "ok": True,
    }
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    return meta


def promote_all() -> dict:
    items = [promote_one(s, d, m) for s, d, m in CATALOG]
    ok = all(i.get("ok") for i in items)
    report = {
        "schema": "aether.a5_p2_promote_batch/v1",
        "ok": ok,
        "network": False,
        "n": len(items),
        "items": items,
    }
    out = ROOT / "data" / "reports" / "a5_p2_promote_report.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    report["report"] = "data/reports/a5_p2_promote_report.json"
    return report


def selftest() -> int:
    report = promote_all()
    fails: list[str] = []
    if not report.get("ok"):
        fails.append("batch_not_ok")
    # Respeito P0/P1: não sobrescrever esses destinos.
    forbidden = [
        "data/embed_eval_last.json",
        "data/mf2_gate_regression.json",
        "data/f1_gil_gate.json",
        "data/f1_vram_watchdog.json",
    ]
    for src_rel, dst_rel, meta_rel in CATALOG:
        if dst_rel in forbidden:
            fails.append(f"overlaps_p0_p1:{dst_rel}")
        dst = ROOT / dst_rel
        meta_path = ROOT / meta_rel
        if not dst.is_file():
            fails.append(f"missing:{dst_rel}")
            continue
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        if meta.get("sha256") != sha256_file(dst):
            fails.append(f"sha_mismatch:{dst_rel}")
    out = {"ok": not fails, "failures": fails, "n": len(CATALOG)}
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main(argv: list[str]) -> int:
    if argv and argv[0] == "selftest":
        return selftest()
    print(json.dumps(promote_all(), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
