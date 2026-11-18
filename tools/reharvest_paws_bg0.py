#!/usr/bin/env python3
"""B-G0 — re-harvest PAWS-X com labels upstream (MSG 0458/0461).

Lê `google-research-datasets/paws-x` (config en) e grava facts.jsonl onde:
  relation = "parafraseia"     se label==1
  relation = "nao_parafraseia" se label==0
subject = sentence1, object = sentence2.

Uso:
  python tools/reharvest_paws_bg0.py --pairs 5000 --out data/rounds/bg0_paws
  python tools/reharvest_paws_bg0.py --pairs 5000 --out data/rounds/bg0_paws --match-b1 data/rounds/20261117_b1/facts.jsonl

Sem `datasets`: exit 3 (ABSTAIN honesto). Embed/JL ficam para o sidecar
(`donor_map_embed.py` + `harvest_transplant.py --jl-only`).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_paws(split: str, pairs: int):
    try:
        from datasets import load_dataset
    except ImportError:
        print(
            "datasets ausente. Instale: pip install datasets\n"
            "ABSTAIN: sem label upstream não fabricamos B-G0.",
            file=sys.stderr,
        )
        sys.exit(3)
    ds = load_dataset("google-research-datasets/paws-x", "en", split=split)
    rows = []
    for i, ex in enumerate(ds):
        if len(rows) >= pairs:
            break
        lab = int(ex["label"])
        if lab not in (0, 1):
            continue  # label=-1 (não aplicável) fora do gate
        rows.append(
            {
                "sentence1": ex["sentence1"],
                "sentence2": ex["sentence2"],
                "label": lab,
                "id": ex.get("id", i),
            }
        )
    return rows


def to_facts(rows: list[dict], authority: int = 96) -> list[dict]:
    facts = []
    for i, r in enumerate(rows):
        rel = "parafraseia" if r["label"] == 1 else "nao_parafraseia"
        facts.append(
            {
                "subject": r["sentence1"],
                "relation": rel,
                "object": r["sentence2"],
                "confidence": 0.9 if r["label"] == 1 else 0.85,
                "citation": f"hf:google-research-datasets/paws-x/en#id={r['id']}",
                "authority": authority,
                "lang": "en",
                "kind": 1,
                "conflict_seed": False,
                "paws_label": r["label"],
                "wall_ms": 1_700_000_300_000 + i,
            }
        )
    return facts


def match_b1(rows: list[dict], b1_path: Path) -> tuple[int, int]:
    """Cruza sentence1 com subjects do B1 (diagnóstico)."""
    subjects = set()
    for line in b1_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        subjects.add(json.loads(line)["subject"])
    hit = sum(1 for r in rows if r["sentence1"] in subjects)
    return hit, len(rows)


def main() -> int:
    ap = argparse.ArgumentParser(description="B-G0 re-harvest PAWS-X com labels")
    ap.add_argument("--pairs", type=int, default=5000, help="pares com label 0/1")
    ap.add_argument("--split", default="train")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--match-b1", type=Path, default=None)
    ap.add_argument("--authority", type=int, default=96)
    args = ap.parse_args()

    rows = load_paws(args.split, args.pairs)
    facts = to_facts(rows, authority=args.authority)
    args.out.mkdir(parents=True, exist_ok=True)
    facts_path = args.out / "facts.jsonl"
    with facts_path.open("w", encoding="utf-8") as f:
        for row in facts:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    n1 = sum(1 for r in rows if r["label"] == 1)
    n0 = sum(1 for r in rows if r["label"] == 0)
    meta = {
        "round": "bg0_paws",
        "source": "google-research-datasets/paws-x",
        "config": "en",
        "split": args.split,
        "pairs": len(rows),
        "label_1_parafraseia": n1,
        "label_0_nao_parafraseia": n0,
        "authority_max_transplant": 120,
        "note": "MSG 0458/0461 — labels upstream gravados; embed/JL fora deste script",
        "gate": {
            "recall_at_1_pair": ">=0.90",
            "reject_non_pair": ">=0.90",
        },
    }
    if args.match_b1 and args.match_b1.exists():
        hit, total = match_b1(rows, args.match_b1)
        meta["b1_sentence1_overlap"] = {"hit": hit, "total": total}
    (args.out / "corpus.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(
        f"B-G0 facts={len(facts)} parafraseia={n1} nao_parafraseia={n0} → {facts_path}",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
