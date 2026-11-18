#!/usr/bin/env python3
# D1 — probe per-token MaxSim com ColBERT-XM (MIT, 128d/token, 81 línguas).
#
# Pergunta (forum 0580): MaxSim com embeddings POR TOKEN reais separa o PAWS
# melhor que o donor de frase (AUC 0.6909) e a sonda bigram (0.7347)?
#
#   python tools/b0_maxsim_colbert.py --pairs 5000 --device cuda
#
# Método: MaxSim (ColBERT) = para cada token de s1, máx similaridade de cosseno
# contra os tokens de s2; score do par = média. AUC + bootstrap estratificado
# B=1000 (IC95). Seed 42, temperature 0. Artefato: target/d1_maxsim.json.

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--round", default="data/rounds/20261117_b0_paws")
    ap.add_argument("--donor", default="antoinelouis/colbert-xm")
    ap.add_argument("--device", default="cuda")
    ap.add_argument("--pairs", type=int, default=5000)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--bootstrap", type=int, default=1000)
    args = ap.parse_args()

    facts_path = Path(args.round) / "facts.jsonl"
    facts = [
        json.loads(l)
        for l in facts_path.read_text(encoding="utf-8").splitlines()
        if l.strip()
    ][: args.pairs]
    s1 = [f["subject"] for f in facts]
    s2 = [f["object"] for f in facts]
    labels = [1 if f["relation"] == "parafraseia" else 0 for f in facts]

    import os
    import random

    os.environ.setdefault("PYTHONHASHSEED", str(args.seed))
    random.seed(args.seed)
    import numpy as np

    np.random.seed(args.seed % (2**32 - 1))
    import torch

    torch.manual_seed(args.seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(args.seed)

    from sentence_transformers import SentenceTransformer

    model = SentenceTransformer(
        args.donor,
        device=args.device,
        model_kwargs={"dtype": torch.float32},
    )
    emb1 = model.encode(s1, batch_size=args.batch, show_progress_bar=True)  # (n, T, 128)
    emb2 = model.encode(s2, batch_size=args.batch, show_progress_bar=True)

    # Guarda anti-deslocamento (MSG 0567/0599): encode s1/s2 e labels alinhados.
    if not (len(emb1) == len(emb2) == len(s1) == len(labels)):
        raise SystemExit(
            f"len mismatch emb1={len(emb1)} emb2={len(emb2)} "
            f"s1={len(s1)} labels={len(labels)}"
        )

    scores = []
    with torch.no_grad():
        for i in range(len(s1)):
            a = torch.tensor(emb1[i], device=args.device)
            b = torch.tensor(emb2[i], device=args.device)
            an = a / (a.norm(dim=-1, keepdim=True) + 1e-12)
            bn = b / (b.norm(dim=-1, keepdim=True) + 1e-12)
            sim = an @ bn.T
            scores.append(float(sim.max(dim=1).values.mean()))
    torch.cuda.empty_cache() if torch.cuda.is_available() else None
    if len(scores) != len(labels):
        raise SystemExit(f"scores={len(scores)} labels={len(labels)}")

    def auc(scores: list[float], labels: list[int]) -> float:
        if len(scores) != len(labels):
            raise ValueError(f"auc zip: scores={len(scores)} labels={len(labels)}")
        pos = sorted(s for s, l in zip(scores, labels, strict=True) if l == 1)
        neg = sorted(s for s, l in zip(scores, labels, strict=True) if l == 0)
        i = j = 0
        wins = ties = 0
        while i < len(pos) and j < len(neg):
            if pos[i] > neg[j]:
                wins += len(neg) - j
                i += 1
            elif pos[i] < neg[j]:
                j += 1
            else:
                ties += 1
                j += 1
        n1, n0 = len(pos), len(neg)
        return (wins + ties / 2) / (n1 * n0) if n1 and n0 else 0.5

    base_auc = auc(scores, labels)

    # Bootstrap estratificado B=1000 (IC95 do AUC).
    rnd = random.Random(args.seed)
    idx1 = [i for i, l in enumerate(labels) if l == 1]
    idx0 = [i for i, l in enumerate(labels) if l == 0]
    aucs = []
    for _ in range(args.bootstrap):
        sample = [rnd.choice(idx1) for _ in idx1] + [rnd.choice(idx0) for _ in idx0]
        aucs.append(auc([scores[i] for i in sample], [labels[i] for i in sample]))
    aucs.sort()
    lo, hi = aucs[int(0.025 * args.bootstrap)], aucs[int(0.975 * args.bootstrap) - 1]

    resultado = {
        "ok": True,
        "probe": "D1_per_token_MaxSim_colbert_xm",
        "n": len(scores),
        "auc": round(base_auc, 4),
        "ic95": [round(lo, 4), round(hi, 4)],
        "bootstrap": args.bootstrap,
        "device": args.device,
        "seed": args.seed,
        "regua": {
            "donor_frase": 0.6909,
            "bigram_sonda_buffy": 0.7347,
            "e1_roles": 0.5983,
        },
        "created": "2026-11-18",
    }
    out = Path("target/d1_maxsim.json")
    out.write_text(json.dumps(resultado, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(resultado, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
