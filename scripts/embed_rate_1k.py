#!/usr/bin/env python3
"""Mede textos/s do donor em 1k frases (MSG 0458/0461 — decide B2 CPU vs GPU).

  python scripts/embed_rate_1k.py --facts data/rounds/20261117_b1/facts.jsonl --limit 1000

ABSTAIN (exit 3) se sentence-transformers ausente. Grava target/embed_rate_1k.json.
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--limit", type=int, default=1000)
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--out", type=Path, default=Path("target/embed_rate_1k.json"))
    args = ap.parse_args()

    try:
        from sentence_transformers import SentenceTransformer
    except ImportError:
        print("ABSTAIN: sentence-transformers ausente", file=sys.stderr)
        return 3

    labels = []
    for line in args.facts.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        labels.append(f"{f['subject']}|{f['relation']}|{f['object']}")
        if len(labels) >= args.limit:
            break

    t_load = time.perf_counter()
    model = SentenceTransformer(args.donor, truncate_dim=args.dim)
    load_s = time.perf_counter() - t_load

    t0 = time.perf_counter()
    _ = model.encode(
        labels,
        batch_size=args.batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    enc_s = time.perf_counter() - t0
    rate = len(labels) / enc_s if enc_s > 0 else 0.0
    # Extrapolação ingênua 100k (só orientação — não é compromisso).
    eta_100k_h = (100_000 / rate) / 3600.0 if rate > 0 else None
    art = {
        "donor": args.donor,
        "n": len(labels),
        "batch": args.batch,
        "dim": args.dim,
        "load_s": round(load_s, 3),
        "encode_s": round(enc_s, 3),
        "texts_per_s": round(rate, 2),
        "eta_100k_hours_naive": None if eta_100k_h is None else round(eta_100k_h, 2),
        "decision_hint": (
            "CPU ok (<4h @100k)"
            if eta_100k_h is not None and eta_100k_h < 4
            else "considerar batch GPU do donor com fallback CPU"
            if eta_100k_h is not None
            else "sem taxa"
        ),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
