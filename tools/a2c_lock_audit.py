#!/usr/bin/env python3
# A-2c — auditoria de paths da tranca (MSG 1026 / 1014). Offline, sem GPU.
"""Varre o repo por locks legados em target/ vs canónico .vega/engine.lock.

Uso:
  python tools/a2c_lock_audit.py
  python tools/a2c_lock_audit.py selftest
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CANON = ".vega/engine.lock"
LEGACY_PATTERNS = (
    r"target[/\\]+gpu_engine\.lock",
    r"gpu_engine_lock\.py",
    r"gpu_engine\.lock\.json",
)
SCAN_GLOBS = ("tools/**/*.py", "docs/**/*.md", "scripts/**/*.{py,ps1}")


def _status_lock() -> dict:
    sys.path.insert(0, str(ROOT / "tools"))
    from engine_lock import status  # noqa: WPS433

    return status()


def _scan() -> list[dict]:
    hits: list[dict] = []
    roots = [ROOT / "tools", ROOT / "docs", ROOT / "scripts"]
    for base in roots:
        if not base.is_dir():
            continue
        for path in base.rglob("*"):
            if not path.is_file():
                continue
            if path.suffix.lower() not in {".py", ".md", ".ps1", ".rs"}:
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            for pat in LEGACY_PATTERNS:
                if re.search(pat, text):
                    hits.append(
                        {
                            "path": path.relative_to(ROOT).as_posix(),
                            "pattern": pat,
                            "class": "legacy_target_or_old_tool",
                            "action": "prefer_engine_lock_py_dot_vega",
                        }
                    )
                    break
    return hits


def audit() -> dict:
    st = _status_lock()
    hits = _scan()
    # selftest do engine_lock: se tranca alheia viva, NAO roubar — evidência ok
    lock_ok = st.get("path") == CANON
    holder = st.get("owner")
    blocked = bool(st.get("locked")) and holder not in (None, "CURSOR_SELFTEST", "")
    return {
        "schema": "aether.a2c_lock_audit/v1",
        "ok": lock_ok,
        "canonical_path": CANON,
        "status": st,
        "selftest_policy": "nao_acquire_se_holder_alheio",
        "selftest_blocked_by_holder": blocked,
        "holder": holder,
        "legacy_hits": hits,
        "n_legacy": len(hits),
        "network": False,
        "gpu_touched": False,
        "ref": "MSG_1014_1026",
        "note": "suite A2c ja em f1_offline_suite (MSG 1020); este audit so mapeia residuos",
    }


def selftest() -> int:
    rep = audit()
    failed: list[str] = []
    if rep.get("canonical_path") != CANON:
        failed.append("canonical")
    if rep.get("status", {}).get("path") != CANON:
        failed.append("status_path")
    if not isinstance(rep.get("legacy_hits"), list):
        failed.append("hits")
    # Se BUFFY/outro segura: blocked=true e ok=true (path canónico) e' PASS
    if rep.get("status", {}).get("locked") and not rep.get("selftest_blocked_by_holder"):
        # locked sem holder marcado — anomalia
        if rep.get("holder"):
            failed.append("blocked_flag")
    out = {
        "ok": len(failed) == 0 and rep.get("ok") is True,
        "failed": failed,
        "n_legacy": rep.get("n_legacy"),
        "blocked": rep.get("selftest_blocked_by_holder"),
        "path": CANON,
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    rep = audit()
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    durable = ROOT / "data" / "a2c_lock_audit.json"
    durable.parent.mkdir(parents=True, exist_ok=True)
    durable.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    # espelho efemero
    tgt = ROOT / "target" / "a2c_lock_audit.json"
    tgt.parent.mkdir(parents=True, exist_ok=True)
    tgt.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
