#!/usr/bin/env python3
# A5 P0 — promove medições de aceite de target/ → data/ (MSG 0986).
"""Copia artefatos P0 do manifesto A5 para data/ com sha256.

Não toca crate/cli/dataset F4. Não promove smoke degradado.

Uso:
  python tools/a5_p0_promote.py
  python tools/a5_p0_promote.py selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# P0 do manifesto A5 (MSG 0967/0975): medições ainda só em target/.
CATALOG: list[tuple[str, str, str]] = [
    ("target/embed_eval.json", "data/embed_eval_last.json", "data/embed_eval_last.meta.json"),
    (
        "target/embed_eval_bm25_baseline.json",
        "data/embed_eval_bm25_baseline.json",
        "data/embed_eval_bm25_baseline.meta.json",
    ),
    (
        "target/mf2_gate_regression.json",
        "data/mf2_gate_regression.json",
        "data/mf2_gate_regression.meta.json",
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
        "schema": "aether.a5_p0_promote/v1",
        "ref": "MSG_0986_0967",
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
        "schema": "aether.a5_p0_promote_batch/v1",
        "ok": ok,
        "network": False,
        "ref": "MSG_0986",
        "n": len(items),
        "items": items,
    }
    out = ROOT / "data" / "a5_p0_promote_report.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    report["report"] = "data/a5_p0_promote_report.json"
    return report


def selftest() -> int:
    report = promote_all()
    fails: list[str] = []
    if not report.get("ok"):
        fails.append("batch_not_ok")
    for src_rel, dst_rel, meta_rel in CATALOG:
        dst = ROOT / dst_rel
        meta_path = ROOT / meta_rel
        if not dst.is_file():
            fails.append(f"missing:{dst_rel}")
            continue
        if not meta_path.is_file():
            fails.append(f"missing_meta:{meta_rel}")
            continue
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        if meta.get("sha256") != sha256_file(dst):
            fails.append(f"sha_mismatch:{dst_rel}")
        if "target" in Path(dst_rel).parts:
            fails.append(f"dst_in_target:{dst_rel}")
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
