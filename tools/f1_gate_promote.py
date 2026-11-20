#!/usr/bin/env python3
# A-5 — promove smoke/gate de target/ para data/ (sobrevive a cargo clean).
"""Copia artefato de aceite F1 para fora de target, com sha256.

Uso:
  python tools/f1_gate_promote.py --src target/f1_decision_smoke.json
  python tools/f1_gate_promote.py selftest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SRC = ROOT / "target" / "f1_decision_smoke.json"
DEFAULT_DST = ROOT / "data" / "f1_decision_smoke.json"
DEFAULT_META = ROOT / "data" / "f1_decision_smoke.meta.json"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def resolve_under_root(p: Path) -> Path:
    return p if p.is_absolute() else (ROOT / p)


def rel_to_root(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return str(path.resolve()).replace("\\", "/")


def promote(src: Path, dst: Path, meta_path: Path) -> dict:
    src = resolve_under_root(src).resolve()
    dst = resolve_under_root(dst).resolve()
    meta_path = resolve_under_root(meta_path)
    if not src.exists():
        return {"ok": False, "error": f"src ausente: {src}"}
    if "target" in dst.parts:
        return {"ok": False, "error": "destino nao pode ficar em target/"}
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    digest = sha256_file(dst)
    data = json.loads(dst.read_text(encoding="utf-8"))
    answers = {a.get("question"): a for a in data.get("answers", [])}
    passou = answers.get("passou") or {}
    checks = {
        "passou_not_unknown": passou.get("abstain_reason") != "unknown",
        "conformal_set_present": bool(data.get("conformal_set_present")),
        "has_answers": bool(answers),
    }
    meta = {
        "schema": "aether.f1_smoke_promote/v1",
        "ref": "docs/F1_AUDIT.md A-5",
        "src": rel_to_root(src),
        "dst": rel_to_root(dst),
        "sha256": digest,
        "bytes": dst.stat().st_size,
        "promoted_at_utc": datetime.now(timezone.utc).isoformat(),
        "checks": checks,
        "ok": all(checks.values()),
    }
    meta_path.parent.mkdir(parents=True, exist_ok=True)
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    return meta


def _smoke_ok_for_selftest(path: Path) -> bool:
    """Aceita só smoke com answers + conformal — ignora stub engine_unavailable em target/."""
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return False
    answers = data.get("answers") or []
    return bool(answers) and bool(data.get("conformal_set_present"))


def selftest() -> int:
    # Preferir data/ durável se target/ foi sobrescrito por smoke degradado (sem vegaml).
    durable = DEFAULT_DST
    src = DEFAULT_SRC
    if durable.is_file() and _smoke_ok_for_selftest(durable):
        src = durable
    elif not (src.is_file() and _smoke_ok_for_selftest(src)):
        src = ROOT / "target" / "f1_gate_promote_selftest_src.json"
        src.parent.mkdir(parents=True, exist_ok=True)
        src.write_text(
            json.dumps(
                {
                    "answers": [
                        {"question": "passou", "abstain": True, "abstain_reason": "ambiguous"},
                        {"question": "route", "abstain": False, "choice": "reformular"},
                    ],
                    "conformal_set_present": True,
                },
                ensure_ascii=False,
            ),
            encoding="utf-8",
        )
    dst = ROOT / "data" / "f1_decision_smoke_selftest.json"
    meta = ROOT / "data" / "f1_decision_smoke_selftest.meta.json"
    report = promote(src, dst, meta)
    bad = promote(src, ROOT / "target" / "nope.json", meta)
    ok = bool(report.get("ok")) and not bad.get("ok") and dst.exists()
    print(json.dumps({"ok": ok, "promote": report, "reject_target": bad}, ensure_ascii=False, indent=2))
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", default=str(DEFAULT_SRC))
    ap.add_argument("--dst", default=str(DEFAULT_DST))
    ap.add_argument("--meta", default=str(DEFAULT_META))
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    report = promote(Path(args.src), Path(args.dst), Path(args.meta))
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
