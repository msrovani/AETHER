"""Aplica a sanção HITL de rótulos (G5) — nunca 255 em lote sem sanção.

Uso:
  python tools/hitl_apply_sanction.py data/hitl_label_batch_100.json

Lê o lote aprovado, grava o hash do lote ANTES, marca `authority: 255` nos
exemplos correspondentes em `data/vega_flywheel_v1.json` (fonte de
proveniência) e escreve o registro da sanção em `data/hitl_sanction_record.json`.
Idempotente: rótulos já em 255 não são re-tocados.
"""

from __future__ import annotations

import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FLYWHEEL = ROOT / "data" / "vega_flywheel_v1.json"
RECORD = ROOT / "data" / "hitl_sanction_record.json"


def main(argv: list[str]) -> int:
    if not argv:
        print("uso: hitl_apply_sanction.py <lote.json>")
        return 2
    batch_path = Path(argv[0]).resolve()
    raw = batch_path.read_bytes()
    batch_sha = hashlib.sha256(raw).hexdigest()
    batch = json.loads(batch_path.read_text(encoding="utf-8"))
    ids = {x["source_id"] for x in batch.get("batch", [])}

    fw = json.loads(FLYWHEEL.read_text(encoding="utf-8"))
    n = 0
    for e in fw.get("examples", []):
        if e.get("source_id") in ids and e.get("authority", 0) < 255:
            e["authority"] = 255
            n += 1
    FLYWHEEL.write_text(
        json.dumps(fw, ensure_ascii=False, indent=1), encoding="utf-8"
    )

    record = {
        "schema": "aether.hitl_sanction/v1",
        "date": datetime.now(timezone.utc).isoformat(),
        "decision": "HITL aprova tudo (chat, 2026-11-20)",
        "batch_sha256": batch_sha,
        "batch_ref": str(batch_path.relative_to(ROOT)),
        "n_in_batch": len(batch.get("batch", [])),
        "n_promoted": n,
        "approved_source_ids": sorted(ids),
        "authority": 255,
        "rule": "nunca 255 em lote sem HITL — sancionado nesta sessão",
    }
    RECORD.write_text(
        json.dumps(record, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "ok": True,
                "batch_sha256": batch_sha,
                "n_promoted": n,
                "record": str(RECORD.relative_to(ROOT)),
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
