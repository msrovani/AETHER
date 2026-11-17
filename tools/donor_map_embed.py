#!/usr/bin/env python3
# Sidecar B0 — mapa HF fora do crate Rust (non-goal: sem pesos no núcleo).
# Gera data/emb/bank.f32 + atualiza/labels alinhados a facts.jsonl (MRL 768).
#
#   pip install 'sentence-transformers>=2.7' 'transformers>=4.51'
#   python tools/donor_map_embed.py --out data --donor Qwen/Qwen3-Embedding-0.6B --dim 768
#
# Depois:
#   python tools/harvest_transplant.py --jl-only --out data --revision <hf_rev> --no-whiten

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path


def nfc_facts(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def write_f32(path: Path, X: list[list[float]]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    n, d = len(X), len(X[0])
    raw = struct.pack("<II", n, d)
    for row in X:
        raw += struct.pack(f"<{d}f", *row)
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser(description="Sidecar embedder Qwen3 MRL→dim (Apache-2.0)")
    ap.add_argument("--out", type=Path, default=Path("data"))
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument(
        "--facts",
        type=Path,
        default=None,
        help="facts.jsonl (padrão: --out/facts.jsonl)",
    )
    args = ap.parse_args()
    facts_path = args.facts or (args.out / "facts.jsonl")
    if not facts_path.exists():
        print(f"falta {facts_path} — gere com harvest --dry-run ou escriba", file=sys.stderr)
        return 2

    try:
        from sentence_transformers import SentenceTransformer
    except ImportError:
        print(
            "sentence-transformers ausente. Instale:\n"
            "  pip install 'sentence-transformers>=2.7' 'transformers>=4.51'\n"
            "Pesos ficam no ambiente Python — nunca no crate AETHER.",
            file=sys.stderr,
        )
        return 3

    facts = nfc_facts(facts_path)
    labels = [f"{f['subject']}|{f['relation']}|{f['object']}" for f in facts]
    print(f"carregando {args.donor} truncate_dim={args.dim}…", flush=True)
    model = SentenceTransformer(args.donor, truncate_dim=args.dim)
    emb = model.encode(
        labels,
        batch_size=args.batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    X = [list(map(float, row)) for row in emb]
    if len(X[0]) != args.dim:
        print(f"dim inesperada {len(X[0])} != {args.dim}", file=sys.stderr)
        return 4
    sha = write_f32(args.out / "emb" / "bank.f32", X)
    meta = {
        "donor": args.donor,
        "dim": args.dim,
        "count": len(X),
        "sha256_embeds": sha,
        "mrl": True,
        "license": "Apache-2.0",
        "note": "sidecar B0 — harvest --jl-only em seguida",
    }
    (args.out / "emb" / "donor_meta.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps({"ok": True, **meta}, ensure_ascii=False))
    print("próximo: python tools/harvest_transplant.py --jl-only --out", args.out, "--no-whiten")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
