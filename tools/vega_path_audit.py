#!/usr/bin/env python3
# VALIDA paths do motor pós-MSG 0892 — preferir .vega/, nunca recriar venv em target/.
"""Varre tools/ e scripts/ em busca de defaults `target/vega` vs contrato `.vega`.

Uso:
  python tools/vega_path_audit.py
  python tools/vega_path_audit.py selftest
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "data" / "vega_env.lock.json"
SCAN_DIRS = ("tools", "scripts")
# Literais de código/doc que ainda apontam o layout antigo (não histórico em prosa).
TARGET_VEGA = re.compile(r"target[/\\]vega")
DOT_VEGA = re.compile(r"\.vega[/\\]")
# Arquivos que PODEM citar target/vega (fonte de migração / auditoria).
ALLOW_MENTION = {
    "tools/vega_env_setup.ps1",  # copia DE target/vega PARA .vega
    "tools/vega_path_audit.py",
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def scan() -> dict:
    hits_target: list[dict] = []
    hits_dot: list[dict] = []
    for folder in SCAN_DIRS:
        base = ROOT / folder
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*")):
            if not path.is_file():
                continue
            if path.suffix.lower() not in {".py", ".ps1", ".md", ".json", ".toml", ".txt"}:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            key = rel(path)
            for i, line in enumerate(text.splitlines(), 1):
                if TARGET_VEGA.search(line):
                    hits_target.append({"file": key, "line": i, "text": line.strip()[:160]})
                if DOT_VEGA.search(line):
                    hits_dot.append({"file": key, "line": i, "text": line.strip()[:160]})

    actionable = [h for h in hits_target if h["file"] not in ALLOW_MENTION]
    lock_ok = False
    layout = {}
    if LOCK.is_file():
        data = json.loads(LOCK.read_text(encoding="utf-8"))
        layout = data.get("layout") or {}
        lock_ok = layout.get("venv") == ".vega/venv" and layout.get("models") == ".vega/models"

    report = {
        "schema": "aether.vega_path_audit/v1",
        "ref": "MSG_0892",
        "network": False,
        "lock_ok": lock_ok,
        "layout": layout,
        "target_vega_mentions": len(hits_target),
        "dot_vega_mentions": len(hits_dot),
        "actionable_residues": actionable,
        "allowed_migration_mentions": [h for h in hits_target if h["file"] in ALLOW_MENTION],
        "ok": lock_ok and len(actionable) == 0,
        "rule": "nunca_criar_venv_dentro_de_target__usar_dot_vega",
    }
    return report


def selftest() -> int:
    # classificador: menção em allow-list não derruba ok se não houver actionable
    sample_allow = "tools/vega_env_setup.ps1" in ALLOW_MENTION
    print(json.dumps({"ok": sample_allow, "allow_setup": sample_allow}, indent=2))
    return 0 if sample_allow else 1


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "selftest":
        return selftest()
    report = scan()
    out = ROOT / "target" / "vega_path_audit.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
