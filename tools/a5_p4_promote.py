#!/usr/bin/env python3
# A5 P4 — promove probes f1_contract* de target/ → data/ (MSG CLAIM 1009).
"""Não edita tests/f1_contract.rs. Só cópia + meta sha.

Uso:
  python tools/a5_p4_promote.py selftest
  python tools/a5_p4_promote.py
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

ITEMS = [
    ("target/f1_contract.json", "data/f1_contract.json"),
    ("target/f1_contract_fit124.json", "data/f1_contract_fit124.json"),
    ("target/f1_contract_primary.json", "data/f1_contract_primary.json"),
    ("target/f1_contract_probe.json", "data/f1_contract_probe.json"),
    ("target/f1_contract_probe2.json", "data/f1_contract_probe2.json"),
    ("target/mf2_gate_regression_0841.json", "data/mf2_gate_regression_0841.json"),
]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def promote_one(src_rel: str, dst_rel: str) -> dict:
    src = ROOT / src_rel
    dst = ROOT / dst_rel
    if not src.is_file():
        return {"src": src_rel, "dst": dst_rel, "ok": False, "skipped": True, "error": "src_ausente"}
    if "target" in Path(dst_rel).parts:
        return {"src": src_rel, "dst": dst_rel, "ok": False, "error": "dst_em_target"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    meta = {
        "schema": "aether.a5_p4_promote/v1",
        "src": src_rel.replace("\\", "/"),
        "dst": dst_rel.replace("\\", "/"),
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "ref": "MSG_1009 A5_P4",
    }
    meta_path = dst.with_suffix(dst.suffix + ".meta.json")
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    return {"ok": True, "skipped": False, **meta}


def run() -> dict:
    results = [promote_one(s, d) for s, d in ITEMS]
    promoted = [r for r in results if r.get("ok")]
    skipped = [r for r in results if r.get("skipped")]
    failed = [r for r in results if not r.get("ok") and not r.get("skipped")]
    out = {
        "schema": "aether.a5_p4_promote/v1",
        "ok": len(failed) == 0 and len(promoted) >= 1,
        "n_promoted": len(promoted),
        "n_skipped": len(skipped),
        "n_failed": len(failed),
        "results": results,
        "network": False,
    }
    (ROOT / "data" / "a5_promote_p4_report.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    out["report"] = "data/a5_promote_p4_report.json"
    return out


def selftest() -> int:
    # precisa de pelo menos um src
    if not (ROOT / "target" / "f1_contract.json").is_file():
        p = ROOT / "target" / "f1_contract.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"schema": "f1_contract", "selftest": True}), encoding="utf-8")
    rep = run()
    ok = rep.get("ok") and (ROOT / "data" / "f1_contract.json").is_file()
    print(json.dumps({"ok": ok, "promote": rep}, ensure_ascii=False, indent=2))
    return 0 if ok else 1


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    rep = run()
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
