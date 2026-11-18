#!/usr/bin/env python3
# B-G0 — re-harvest PAWS-x com labels reais (MSG 0458/0461).
#
# O round b1 gravou os pares PAWS adjacentes mas sem o label upstream; aqui o
# label vira o predicado do fato: `parafraseia` (label=1) ou `nao_parafraseia`
# (label=0). Par balanceado, seed declarada, UTF-8 sem BOM.
#
#   python tools/b0_paws_harvest.py --out data/rounds/20261117_b0_paws --pairs 5000
#   python tools/donor_map_embed.py --out data/rounds/20261117_b0_paws --dim 768
#   python tools/harvest_transplant.py --jl-only --out data/rounds/20261117_b0_paws --revision <hf_rev> --no-whiten

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description="B-G0: pares PAWS-x com label real")
    ap.add_argument("--out", type=Path, default=Path("data/rounds/20261117_b0_paws"))
    ap.add_argument("--pairs", type=int, default=5000)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--split", default="train")
    args = ap.parse_args()

    from datasets import load_dataset

    ds = load_dataset("google-research-datasets/paws-x", "en", split=args.split)
    ones = [r for r in ds if r["label"] == 1]
    zeros = [r for r in ds if r["label"] == 0]
    rnd = random.Random(args.seed)
    rnd.shuffle(ones)
    rnd.shuffle(zeros)
    half = args.pairs // 2
    chosen = ones[:half] + zeros[: args.pairs - half]
    rnd.shuffle(chosen)

    facts = []
    wall = 1700001000000
    for i, r in enumerate(chosen):
        relation = "parafraseia" if r["label"] == 1 else "nao_parafraseia"
        facts.append(
            {
                "subject": r["sentence1"],
                "relation": relation,
                "object": r["sentence2"],
                "confidence": 0.9,
                "citation": "hf:google-research-datasets/paws-x/en",
                "authority": 96,
                "lang": "en",
                "kind": 1,
                "conflict_seed": False,
                "wall_ms": wall + i,
            }
        )

    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / "facts.jsonl"
    path.write_text(
        "\n".join(json.dumps(f, ensure_ascii=False) for f in facts) + "\n",
        encoding="utf-8",
    )
    n1 = sum(1 for f in facts if f["relation"] == "parafraseia")
    resumo = {
        "ok": True,
        "pairs": len(facts),
        "label1": n1,
        "label0": len(facts) - n1,
        "split": args.split,
        "seed": args.seed,
        "sha256": __import__("hashlib").sha256(path.read_bytes()).hexdigest(),
    }
    print(json.dumps(resumo, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
