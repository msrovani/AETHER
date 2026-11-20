#!/usr/bin/env python3
# A5 P3 — promove gates F1 CURSOR ainda só em target/ (MSG CLAIM A5_P3).
"""Copia com meta sha256 para data/. Não toca P0/P1/P2 nem dataset F4.

Uso:
  python tools/a5_p3_promote.py
  python tools/a5_p3_promote.py selftest
"""

from __future__ import annotations

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Gaps pós P0/P1/P2 — artefatos desta linha CURSOR / probes F1.
ITEMS = [
    ("target/f1_5_ttt_abstain.json", "data/f1_5_ttt_abstain.json"),
    ("target/f1_a1b_vega_check.json", "data/f1_a1b_vega_check.json"),
    ("target/f1_offline_suite.json", "data/f1_offline_suite.json"),
    ("target/f5_t1_eval.json", "data/reports/f5_t1_eval.json"),
    ("target/f5_t1_loop.json", "data/reports/f5_t1_loop.json"),
    ("target/f5_t2_allowlist.json", "data/reports/f5_t2_allowlist.json"),
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
        return {
            "src": src_rel,
            "dst": dst_rel,
            "ok": False,
            "error": "src_ausente",
            "skipped": True,
        }
    if "target" in dst.parts:
        return {"src": src_rel, "dst": dst_rel, "ok": False, "error": "dst_em_target"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    meta = {
        "schema": "aether.a5_p3_promote/v1",
        "src": src_rel.replace("\\", "/"),
        "dst": dst_rel.replace("\\", "/"),
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "ref": "MSG_0967 A5_P3",
    }
    meta_path = dst.with_suffix(dst.suffix + ".meta.json")
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    return {"ok": True, "skipped": False, **meta, "meta": str(meta_path.relative_to(ROOT)).replace("\\", "/")}


def run() -> dict:
    results = [promote_one(s, d) for s, d in ITEMS]
    promoted = [r for r in results if r.get("ok")]
    skipped = [r for r in results if r.get("skipped")]
    failed = [r for r in results if not r.get("ok") and not r.get("skipped")]
    out = {
        "schema": "aether.a5_p3_promote/v1",
        "ok": len(failed) == 0 and len(promoted) >= 1,
        "n_promoted": len(promoted),
        "n_skipped": len(skipped),
        "n_failed": len(failed),
        "results": results,
        "network": False,
        "wired_to_crate": False,
    }
    report = ROOT / "data" / "a5_promote_p3_report.json"
    report.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    out["report"] = "data/a5_promote_p3_report.json"
    return out


def selftest() -> int:
    # Fixture mínima se f1_5 ausente
    src = ROOT / "target" / "f1_5_ttt_abstain.json"
    if not src.is_file():
        src.parent.mkdir(parents=True, exist_ok=True)
        src.write_text(
            json.dumps({"schema": "aether.f1_5_ttt_abstain/v1", "ok": True, "selftest": True}),
            encoding="utf-8",
        )
    rep = run()
    # exige pelo menos f1_5 promovido
    f15 = ROOT / "data" / "f1_5_ttt_abstain.json"
    ok = rep.get("ok") and f15.is_file()
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
