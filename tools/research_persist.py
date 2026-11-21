#!/usr/bin/env python3
"""Persiste evidência de pesquisa viva na memória durável (quarentena ≤120).

Grava via `aether-sgdb put` no escopo canônico — o chat OOV cita no turno
seguinte (`MemoryCitations.recall`). Nunca 255 sem HITL.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _fact_text(question: str, live: dict) -> str:
    evid = live.get("evidence") or []
    bits = []
    for e in evid[:4]:
        title = (e.get("title") or "").strip()
        url = (e.get("url") or "").strip()
        if title and url:
            bits.append(f"{title} <{url}>")
        elif title:
            bits.append(title)
    evid_s = "; ".join(bits) if bits else "(sem títulos)"
    auth = min(int(live.get("authority") or 60), 120)
    verd = live.get("verdict") or "open"
    n = int(live.get("n_independent") or 0)
    prov = ",".join(live.get("providers") or []) or "?"
    return (
        f"PESQUISA_EXTERNA quarentena: pergunta={question[:180]!r} "
        f"veredito={verd} autoridade={auth} fontes={n} provider={prov} "
        f"evidencia={evid_s}"
    )


def persist_research(question: str, live: dict) -> dict:
    """Grava fato em quarentena. Retorna {ok, authority, key?, error?}."""
    n = int(live.get("n_independent") or 0)
    if n <= 0:
        return {"ok": False, "error": "sem_fontes"}
    auth = min(int(live.get("authority") or 60), 120)
    if auth >= 255:
        auth = 120
    text = _fact_text(question, live)
    env = dict(os.environ)
    # Prefer binário já buildado; senão cargo run.
    bin_rel = ROOT / "target" / "release" / "aether-sgdb.exe"
    bin_dbg = ROOT / "target" / "debug" / "aether-sgdb.exe"
    if bin_rel.exists():
        cmd = [str(bin_rel), "put", text, "--entity", "aether/ops", "--authority", str(auth)]
    elif bin_dbg.exists():
        cmd = [str(bin_dbg), "put", text, "--entity", "aether/ops", "--authority", str(auth)]
    else:
        cargo = ROOT / "scripts" / "cargo.ps1"
        if cargo.exists() and sys.platform.startswith("win"):
            cmd = [
                "powershell",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(cargo),
                "run",
                "--quiet",
                "--bin",
                "aether-sgdb",
                "--",
                "put",
                text,
                "--entity",
                "aether/ops",
                "--authority",
                str(auth),
            ]
        else:
            cmd = [
                "cargo",
                "run",
                "--quiet",
                "--bin",
                "aether-sgdb",
                "--",
                "put",
                text,
                "--entity",
                "aether/ops",
                "--authority",
                str(auth),
            ]
    try:
        p = subprocess.run(
            cmd,
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=90,
        )
    except Exception as e:
        return {"ok": False, "error": str(e), "authority": auth}
    out = (p.stdout or "") + "\n" + (p.stderr or "")
    key = None
    for ln in out.splitlines():
        if "gravado:" in ln.casefold() or "md/L" in ln:
            key = ln.strip()
            break
    return {
        "ok": p.returncode == 0,
        "authority": auth,
        "key": key,
        "exit": p.returncode,
        "tail": "\n".join(out.strip().splitlines()[-4:]),
    }


if __name__ == "__main__":
    demo = {
        "authority": 60,
        "verdict": "open",
        "n_independent": 1,
        "providers": ["wiki"],
        "evidence": [{"title": "França", "url": "https://pt.wikipedia.org/wiki/França"}],
    }
    print(json.dumps(persist_research("capital da Franca", demo), ensure_ascii=False, indent=2))
