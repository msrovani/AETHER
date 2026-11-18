#!/usr/bin/env python3
# B-G0 sidecar (OPENCODE) — similaridade de pares PAWS-x no espaço do donor.
#
# Lê facts.jsonl do round 20261117_b0_paws (5k pares com label real), embute
# s1 e s2 SEPARADAMENTE (mesmo donor/dim/seed/temperature=0 do A1/A2),
# escreve:
#   data/rounds/20261117_b0_paws/pair_cosines.json  [{cos,label},...]
#   data/rounds/20261117_b0_paws/s2_query_hv.bin    (opcional: queries s2
#     projetadas para 10240 bits com o build_P do harvest — p/ recall no bank)
#
#   python tools/b0_pair_cosines.py --out data/rounds/20261117_b0_paws --device cpu

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description="B-G0: cos(s1,s2) + queries hv")
    ap.add_argument("--out", type=Path, default=Path("data/rounds/20261117_b0_paws"))
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--dim-bits", type=int, default=10240)
    ap.add_argument("--skip-hv", action="store_true", help="só cosines")
    args = ap.parse_args()

    facts_path = args.out / "facts.jsonl"
    facts = [json.loads(l) for l in facts_path.read_text(encoding="utf-8").splitlines() if l.strip()]
    s1 = [f["subject"] for f in facts]
    s2 = [f["object"] for f in facts]

    from sentence_transformers import SentenceTransformer

    # Runtime L1 compartilhado (MSG 0494: fp32/batch16/threads — 6.9x).
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from embed_runtime import (
        default_batch,
        resolve_device,
        set_determinism,
        tune_cpu_threads,
    )

    set_determinism(args.seed)
    device = resolve_device(args.device) or "cpu"
    batch = default_batch(device)
    threads = tune_cpu_threads() if device == "cpu" else 0

    model = SentenceTransformer(args.donor, truncate_dim=args.dim, device=device)
    emb1 = model.encode(s1, batch_size=batch, normalize_embeddings=True, show_progress_bar=True)
    emb2 = model.encode(s2, batch_size=batch, normalize_embeddings=True, show_progress_bar=True)

    # Guarda anti-deslocamento (MSG 0567/0599, PEP 618): lengths divergentes
    # nunca zip silenciosamente — fail-fast antes de gravar cosines.
    if not (len(emb1) == len(emb2) == len(facts)):
        raise SystemExit(
            f"len mismatch emb1={len(emb1)} emb2={len(emb2)} facts={len(facts)}"
        )

    rows = []
    for a, b in zip(emb1, emb2, strict=True):
        if len(a) != len(b):
            raise SystemExit(f"dim mismatch {len(a)} vs {len(b)}")
        acc = 0.0
        for x, y in zip(a, b, strict=True):
            acc += float(x) * float(y)
        rows.append(acc)

    labels = [f["relation"] for f in facts]
    if len(rows) != len(labels):
        raise SystemExit(f"cosines={len(rows)} labels={len(labels)}")
    n1 = sum(1 for r in labels if r == "parafraseia")
    sha = hashlib.sha256(
        b"".join(struct.pack("<f", c) for c in rows)
    ).hexdigest()
    (args.out / "pair_cosines.json").write_text(
        json.dumps(
            {
                "ok": True,
                "n": len(rows),
                "label1": n1,
                "label0": len(rows) - n1,
                "cosines": rows,
                "labels": labels,
                "seed": args.seed,
                "device": args.device,
                "donor": args.donor,
                "dim": args.dim,
                "sha256_cosines": sha,
            },
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"ok": True, "n": len(rows), "label1": n1, "sha256": sha[:16]}))

    if not args.skip_hv:
        # Queries s2 projetadas com o MESMO build_P do harvest (seed do manifest).
        import numpy as np  # noqa: F401 — build_P usa numpy quando disponível
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from harvest_transplant import build_P, project_bits

        manifest = json.loads((args.out / "manifest.json").read_text(encoding="utf-8"))
        P = build_P(args.dim, args.dim_bits, manifest.get("seed", 0xDEADBEEF))
        words_per = args.dim_bits // 64
        out = bytearray()
        for q in emb2:
            bits = project_bits(P, [float(v) for v in q], args.dim_bits, args.dim)
            for w in bits:
                out += struct.pack("<Q", w)
        (args.out / "s2_query_hv.bin").write_bytes(bytes(out))
        print(json.dumps({"ok": True, "queries": len(emb2), "words": words_per}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
