#!/usr/bin/env python3
"""F1_PROD - aceite da F1 no CAMINHO DE PRODUTO (chat + --vega + engine real).

A F1 foi aceita num `example` (BUFFY, MSG 0793). O aceite que falta e o do
caminho que o usuario toca: `aether-agent --vega chat "..."` com o engine real
ligado, produciendo uma decisao de verdade e nao uma degradação honesta.

Este tool:
  1. roda o chat com `--vega` e o ambiente do engine (PYO3_PYTHON, PYTHONPATH,
     VEGA_MODEL, VEGA_PROSE_MODEL);
  2. mede o wall clock - que inclui o BOOT do engine, o numero que a A-3
     (warmup em thread) precisa de medir;
  3. confere o contrato do aceite:
     - o port e o real (`port=vega`), nao o stub;
     - ha veredito de verdade (idade ou abstencao com motivo != engine_unavailable);
     - `engine=vega-u-0.8b AND gemma-prose` e `degrado=false`;
     - o core ainda responde (o port e evidencia, nao veredito);
  4. congela o artefato em `data/f1_prod_decision.json` - FORA de `target/`,
     porque o `cargo clean` apaga `target/` (forum MSG 0748).

Uso:
  python tools/f1_prod_acceptance.py selftest     # offline, sem engine
  python tools/f1_prod_acceptance.py check        # roda o chat de verdade

Exit 0 = aceite cumprido; exit 3 = falha honesta (artefato com o motivo).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARTIFACT = ROOT / "data" / "f1_prod_decision.json"
BIN = ROOT / "target" / "release" / "aether-agent.exe"

PHRASE = "qual e a capital do Brasil"

RE_PORT = re.compile(r"decisor:\s*port=(?P<port>\w+)(?P<rest>.*)")
RE_ANSWER = re.compile(
    r"decisor:\s*q=(?P<q>\w+)\s+(?P<verdict>age choice=\S+|abstain motivo=\S+)\s*\|\s*"
    r"engine=(?P<engine>.*?)\s*\|(?P<rest>.*)"
)


def parse_output(text: str) -> dict:
    """Classifica a saida do chat sem executar nada."""
    port = None
    port_rest = ""
    answers = []
    for line in text.splitlines():
        line = line.strip()
        m = RE_PORT.match(line)
        if m and port is None:
            port = m.group("port")
            port_rest = m.group("rest")
            continue
        m = RE_ANSWER.match(line)
        if m:
            answers.append(
                {
                    "question": m.group("q"),
                    "verdict": m.group("verdict"),
                    "engine": m.group("engine").strip(),
                    "degraded": "degrado=true" in m.group("rest"),
                    "readout": (
                        m.group("rest").split("readout=")[1].strip()
                        if "readout=" in m.group("rest")
                        else None
                    ),
                }
            )
    core_answered = bool(re.search(r"^resposta:", text, re.M))
    return {
        "port": port,
        "port_rest": port_rest.strip(),
        "answers": answers,
        "core_answered": core_answered,
    }


def classify(parsed: dict) -> dict:
    """Verifica o contrato do aceite da F1 no produto."""
    checks = {
        "port_real": parsed["port"] == "vega",
        "tem_veredito": bool(parsed["answers"]),
        "veredito_nao_e_degradacao": all(
            "engine_unavailable" not in a["verdict"] for a in parsed["answers"]
        ),
        "engine_and_gate": any(
            "AND" in a["engine"] for a in parsed["answers"]
        ),
        "nao_degradado": all(not a["degraded"] for a in parsed["answers"]),
        "core_respondeu": parsed["core_answered"],
    }
    return {"checks": checks, "ok": all(checks.values())}


def selftest() -> int:
    """Offline: prova que o classificador sabe dizer sim e nao."""
    good = (
        "decisor: port=vega engine=vega-u-0.8b AND gemma-prose (A-1b opt-in)\n"
        "decisor: q=passou age choice=True | engine=vega-u-0.8b AND gemma-prose | readout=and_gate\n"
        "decisor: q=route age choice=responder | engine=vega-u-0.8b AND gemma-prose | readout=and_gate\n"
        "resposta: Brasilia\n"
    )
    stub = (
        "decisor: port=stub (default Eixo C; use --vega para o engine real)\n"
        "decisor: q=passou abstain motivo=engine_unavailable | engine=stub | degrado=true\n"
    )
    degraded = (
        "decisor: port=vega engine=x\n"
        "decisor: q=passou abstain motivo=engine_unavailable | engine=x | degrado=true\n"
    )
    cases = [
        ("produto_real", good, True),
        ("stub_default", stub, False),
        ("engine_fora", degraded, False),
    ]
    failed = 0
    for name, text, expected in cases:
        got = classify(parse_output(text))["ok"]
        status = "ok" if got == expected else "FALHOU"
        if got != expected:
            failed += 1
        print(f"[f1_prod] selftest {name}: {status} (ok={got}, esperado={expected})")
    return 0 if failed == 0 else 3


def build_env() -> dict:
    """Ambiente do engine: o lockfile manda, o ambiente já presente sobrescreve.

    `data/vega_env.lock.json` é a fonte da verdade (pinos medidos); quem
    quiser medir contra outro engine é só exportar as variáveis antes.
    """
    env = dict(os.environ)
    locked: dict[str, str] = {}
    lock_path = ROOT / "data" / "vega_env.lock.json"
    if lock_path.is_file():
        try:
            with open(lock_path, "r", encoding="utf-8") as f:
                data = json.load(f)
            # O lockfile mudou de forma: `env` (a primeira versao) e
            # `env_vars_for_pyo3` (a do gestor). Aceita as duas.
            raw = data.get("env") or data.get("env_vars_for_pyo3") or {}
            for k, v in raw.items():
                if not isinstance(v, str):
                    continue
                # `<root>` = a raiz do repo.
                locked[k] = v.replace("<root>", str(ROOT)).replace("/", "\\")
        except (OSError, json.JSONDecodeError):
            locked = {}

    def pick(var: str, fallback: str) -> str:
        cur = env.get(var, "").strip()
        return cur if cur else locked.get(var, fallback)

    env["PYO3_PYTHON"] = pick(
        "PYO3_PYTHON",
        pick("VEGA_PYTHON", str(ROOT / "target" / "vega" / "Scripts" / "python.exe")),
    )
    env["VEGA_PYTHON"] = pick(
        "VEGA_PYTHON", str(ROOT / "target" / "vega" / "Scripts" / "python.exe")
    )
    env["VEGA_SITE_PACKAGES"] = pick(
        "VEGA_SITE_PACKAGES", str(ROOT / "target" / "vega" / "Lib" / "site-packages")
    )
    env["PYTHONPATH"] = env["VEGA_SITE_PACKAGES"]
    env["VEGA_MODEL"] = pick(
        "VEGA_MODEL", str(ROOT / "target" / "vega" / "vega-u-gemma-1b")
    )
    env["VEGA_PROSE_MODEL"] = pick(
        "VEGA_PROSE_MODEL", str(ROOT / "target" / "vega" / "vega-u-gemma-1b-prose")
    )
    env.setdefault("HF_HUB_OFFLINE", "1")
    env["AETHER_SGDB_STDIO"] = ""
    env["AETHER_SGDB_URL"] = ""
    return env


def gpu_mem() -> tuple[int | None, int | None]:
    """(memória usada, total) da GPU em MiB. (None, None) = não deu para medir."""
    try:
        out = subprocess.run(
            [
                "nvidia-smi",
                "--query-gpu=memory.used,memory.total",
                "--format=csv,noheader,nounits",
            ],
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.SubprocessError):
        return None, None
    if out.returncode != 0:
        return None, None
    used = total = None
    for line in out.stdout.splitlines():
        parts = [p.strip() for p in line.split(",")]
        if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
            used, total = int(parts[0]), int(parts[1])
            break
    return used, total


def engine_lock(mode: str, owner: str) -> int:
    """Usa a tranca da GPU (tools/engine_lock.py). Devolve o codigo de saida."""
    lock = ROOT / "tools" / "engine_lock.py"
    if not lock.is_file():
        return 0
    try:
        out = subprocess.run(
            ["python", str(lock), mode, owner, "1800"],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.SubprocessError):
        return 0
    return out.returncode


def run_check(phrase: str) -> int:
    # A-2c: a carta e compartilhada. O AND gate medido precisa de ~4,4 GB
    # (pico isolado 4393 MiB com os DOIS readers); com outra corrida na GPU,
    # a medicao seria um OOM disfarçado de falha do motor — e a culpa cairia
    # no motor em vez de cair na corrida. Aborta honesto antes.
    used, total = gpu_mem()
    need = 4500  # MiB: o gate com dois readers, com folga
    if used is not None and total is not None and (total - used) < need:
        rec = {
            "schema": "aether.f1_prod_decision/v1",
            "abstain": True,
            "reason": "gpu_ocupada",
            "gpu_used_mib": used,
            "gpu_total_mib": total,
            "gpu_livre_mib": total - used,
            "need_mib": need,
            "hint": "esperar a GPU (tools/engine_lock.py wait) e repetir",
        }
        write(rec)
        print(
            f"[f1_prod] GPU sem folga ({used}/{total} MiB, precisa {need} livre) "
            f"- corrida adiada, nao medido"
        )
        return 3

    if not BIN.is_file():
        rec = {
            "schema": "aether.f1_prod_decision/v1",
            "abstain": True,
            "reason": "binario_ausente",
            "hint": "cargo build --release --features decision-pyo3 --bin aether-agent",
        }
        write(rec)
        print(f"[f1_prod] {rec['hint']}")
        return 3

    env = build_env()
    # A tranca entra por construção: nenhuma corrida de engine sem ela.
    if engine_lock("acquire", "OPENCODE-F1PROD") != 0:
        rec = {
            "schema": "aether.f1_prod_decision/v1",
            "abstain": True,
            "reason": "gpu_trancada",
            "hint": "outro agent esta com a GPU; esperar (tools/engine_lock.py wait)",
        }
        write(rec)
        print("[f1_prod] tranca da GPU ocupada - corrida adiada, nao medido")
        return 3
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(
            [str(BIN), "--vega", "chat", phrase],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=env,
            timeout=3600,
        )
    except subprocess.TimeoutExpired:
        engine_lock("release", "OPENCODE-F1PROD")
        rec = {
            "schema": "aether.f1_prod_decision/v1",
            "abstain": True,
            "reason": "timeout",
            "hint": "boot do engine demorou demais (A-3 warmup)",
        }
        write(rec)
        print("[f1_prod] timeout na corrida - nao medido")
        return 3
    finally:
        engine_lock("release", "OPENCODE-F1PROD")
    wall = time.perf_counter() - t0
    text = (proc.stdout or "") + (proc.stderr or "")
    parsed = parse_output(text)
    verdict = classify(parsed)

    rec = {
        "schema": "aether.f1_prod_decision/v1",
        "abstain": not verdict["ok"],
        "measured_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "phrase": phrase,
        "wall_s": round(wall, 1),
        "wall_note": "inclui o BOOT do engine (load+fit) - numero da A-3",
        "exit_code": proc.returncode,
        "port": parsed["port"],
        "checks": verdict["checks"],
        "answers": parsed["answers"],
        "core_answered": parsed["core_answered"],
        "output_head": [l for l in text.splitlines() if l.strip()][:12],
        "mutated_crate": False,
        "network": False,
    }
    write(rec)
    print(
        f"[f1_prod] aceite={'OK' if verdict['ok'] else 'FALHOU'} "
        f"wall={rec['wall_s']}s port={rec['port']} "
        f"checks={ {k: v for k, v in verdict['checks'].items() if not v} }"
    )
    return 0 if verdict["ok"] else 3


def write(rec: dict) -> None:
    ARTIFACT.parent.mkdir(parents=True, exist_ok=True)
    with open(ARTIFACT, "w", encoding="utf-8") as f:
        json.dump(rec, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print(f"[f1_prod] artefato: {ARTIFACT.relative_to(ROOT)}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["selftest", "check"])
    ap.add_argument("--phrase", default=PHRASE)
    a = ap.parse_args()
    if a.mode == "selftest":
        return selftest()
    return run_check(a.phrase)


if __name__ == "__main__":
    sys.exit(main())
