#!/usr/bin/env python3
"""Gate do ciclo — o "verde" do AETHER em UM comando.

O protocolo exige (FORUM_PROTOCOL §6.1): *check antes de sair; build vermelho
bloqueia todos*. Até aqui esse "check" era um ritual manual — e o CI
(`clippy --all-targets -- -D warnings` + `cargo test --release`) só roda no
GitHub, não na máquina do ciclo.

Este tool reúne o gate inteiro e o **congela o veredito fora de `target/`**
(o `cargo clean` apaga `target/`: fórum MSG 0748):

  1. `cargo fmt --all --check`
  2. `cargo clippy --all-targets -- -D warnings`
  3. `cargo test --lib` (a crate raiz)
  4. `cargo test -p aether_decision` (o crate do port)
  5. `cargo test --test f1_contract` (o gate de aceite da F1)
  6. `python tools/f1_offline_suite.py selftest` (o contrato offline)
  7. `python tools/f1_prod_acceptance.py selftest` (o classificador do aceite)
  8. `python tools/engine_lock.py selftest` (a tranca da GPU)
  9. integridade: todo artefato congelado existe, mora fora de `target/` e
     bate com o `sha256` do seu `.meta.json`

Uso:
  python tools/cycle_gate.py            # roda tudo
  python tools/cycle_gate.py --list     # só os passos
  python tools/cycle_gate.py --fast     # pula cargo (só python + integridade)

Exit 0 = verde; exit 3 = vermelho (com o passo que falhou).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "data" / "cycle_gate.json"

# Artefatos congelados do ciclo: precisam existir, morar fora de `target/` e
# bater com o sha do `.meta.json` (a política D3: evidência com hash).
FROZEN = [
    "data/f1_gate_baseline.json",
    "data/f1_prod_decision.json",
    "data/vega_env.lock.json",
    "data/f1_offline_suite.json",
]

CARGO_STEPS = [
    ("fmt", ["cargo", "fmt", "--all", "--check"]),
    ("clippy", ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]),
    ("test_lib", ["cargo", "test", "--lib"]),
    ("test_decision", ["cargo", "test", "-p", "aether_decision"]),
    ("test_f1_contract", ["cargo", "test", "--test", "f1_contract"]),
]

PY_STEPS = [
    ("suite_offline", ["python", "tools/f1_offline_suite.py", "selftest"]),
    ("f1_prod_selftest", ["python", "tools/f1_prod_acceptance.py", "selftest"]),
    ("engine_lock_selftest", ["python", "tools/engine_lock.py", "selftest"]),
]


def run(cmd: list[str], timeout: int = 1800) -> dict:
    t0 = time.perf_counter()
    try:
        p = subprocess.run(
            cmd,
            cwd=str(ROOT),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout,
        )
        code, out = p.returncode, (p.stdout or "") + (p.stderr or "")
    except subprocess.TimeoutExpired:
        code, out = 3, "timeout"
    except OSError as e:
        code, out = 3, str(e)
    return {
        "returncode": code,
        "ok": code == 0,
        "seconds": round(time.perf_counter() - t0, 1),
        "tail": [l for l in out.splitlines() if l.strip()][-3:],
    }


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def integrity() -> dict:
    """Confere a evidência congelada: existe, fora de `target/`, hash bate."""
    items = []
    for rel in FROZEN:
        p = ROOT / rel
        item = {"path": rel, "exists": p.is_file()}
        if not item["exists"]:
            item["ok"] = False
            items.append(item)
            continue
        item["outside_target"] = not rel.startswith("target/")
        meta_path = p.with_suffix(p.suffix + ".meta.json")
        item["meta_present"] = meta_path.is_file()
        item["sha256"] = sha256_of(p)
        if item["meta_present"]:
            try:
                meta = json.loads(meta_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                meta = {}
            expected = meta.get("sha256")
            item["sha256_expected"] = expected
            item["sha_match"] = bool(expected) and expected == item["sha256"]
        else:
            item["sha_match"] = None
        item["ok"] = bool(item["outside_target"]) and item["sha_match"] is not False
        items.append(item)
    return {
        "ok": all(i["ok"] for i in items),
        "items": items,
        "note": "artefato sem .meta.json nao falha (sem hash publicado); fora de target/ e obrigatorio",
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--fast", action="store_true", help="pula cargo (so python + integridade)")
    a = ap.parse_args()

    steps = PY_STEPS + [("integridade", None)]
    cargo_steps = [] if a.fast else CARGO_STEPS

    if a.list:
        for name, cmd in cargo_steps + PY_STEPS:
            print(f"{name}: {' '.join(cmd) if cmd else '(integridade)'}")
        print("integridade: sha256 dos artefatos congelados")
        return 0

    results = []
    for name, cmd in cargo_steps + PY_STEPS:
        r = run(cmd)
        r["name"] = name
        results.append(r)
        status = "ok" if r["ok"] else "FALHOU"
        print(f"[cycle_gate] {name}: {status} ({r['seconds']}s)")

    integ = integrity()
    integ["name"] = "integridade"
    results.append(integ)
    print(
        f"[cycle_gate] integridade: {'ok' if integ['ok'] else 'FALHOU'} "
        f"({len(integ['items'])} artefatos)"
    )

    failed = [r["name"] for r in results if not r["ok"]]
    report = {
        "schema": "aether.cycle_gate/v1",
        "ok": not failed,
        "failed": failed,
        "measured_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "steps": results,
        "frozen_artifacts": FROZEN,
        "network": False,
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    with open(REPORT, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
        f.write("\n")
    # Congela o veredito com hash (fora de `target/`).
    meta = {
        "schema": "aether.cycle_gate_meta/v1",
        "src": "data/cycle_gate.json",
        "dst": "data/cycle_gate.json",
        "sha256": sha256_of(REPORT),
        "bytes": REPORT.stat().st_size,
        "promoted_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "ok": not failed,
    }
    with open(REPORT.with_suffix(".json.meta.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2, ensure_ascii=False)
        f.write("\n")

    print(
        f"[cycle_gate] {'VERDE' if not failed else 'VERMELHO'} "
        f"(falhando: {failed or 'nada'}) — {REPORT.relative_to(ROOT)}"
    )
    return 0 if not failed else 3


if __name__ == "__main__":
    sys.exit(main())
