#!/usr/bin/env python3
# Valida o smoke de produto da MSG 1025. Não edita o crate nem liga a rede.
"""O port absteve com engine_unavailable; o core respondeu Brasília.

engine_unavailable não abre a pesquisa da F2. O artefato tem de viver em data/.

Uso:
  python tools/f1_chat_smoke_vega_check.py selftest
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from f1_abstain_map import admits_research  # noqa: E402

SMOKE = ROOT / "data" / "f1_chat_smoke_vega.json"


def check(path: Path = SMOKE) -> dict:
    failures: list[str] = []
    if not path.is_file():
        return {"ok": False, "failures": ["artefato_ausente"], "network": False}
    if "target" in path.parts:
        failures.append("artefato_dentro_de_target")
    data = json.loads(path.read_text(encoding="utf-8"))
    port = data.get("port_result") or {}
    chat = data.get("chat_result") or {}
    reason = port.get("reason")
    if port.get("abstain") is not True:
        failures.append("port_nao_absteve")
    if reason != "engine_unavailable":
        failures.append(f"motivo={reason}")
    if admits_research(reason):
        failures.append("engine_unavailable_nao_pode_abrir_F2")
    err = str(port.get("error") or "")
    if "torch.cpu" not in err or "device" not in err:
        failures.append("erro_torch_cpu_device_ausente")
    if chat.get("action") != "act":
        failures.append("core_nao_agiu")
    answer = str(chat.get("answer") or "")
    if "Brasília" not in answer and "Brasilia" not in answer:
        failures.append("resposta_sem_brasilia")
    if "DEGRADACAO_HONESTA" not in str(data.get("verdict") or ""):
        failures.append("veredito_sem_degradacao_honesta")
    return {
        "schema": "aether.f1_chat_smoke_vega_check/v1",
        "ok": not failures,
        "failures": failures,
        "reason": reason,
        "opens_f2": admits_research(reason),
        "core_action": chat.get("action"),
        "path": path.relative_to(ROOT).as_posix(),
        "network": False,
        "crate_edited": False,
    }


def main() -> int:
    report = check()
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
