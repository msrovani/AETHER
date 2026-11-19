#!/usr/bin/env python3
# Debug temporário D1 — replica o run completo (n pequeno configurável) com
# estatísticas de distribuição e verificação de alinhamento score/label.

from __future__ import annotations

import json
import random
from pathlib import Path

PAIRS = 200
OUT = Path("target/d1_debug.json")


def main() -> int:
    facts = [
        json.loads(l)
        for l in Path("data/rounds/20261117_b0_paws/facts.jsonl").read_text(encoding="utf-8").splitlines()
        if l.strip()
    ][:PAIRS]
    s1 = [f["subject"] for f in facts]
    s2 = [f["object"] for f in facts]
    labels = [1 if f["relation"] == "parafraseia" else 0 for f in facts]
    print("labels: primeiros 10 =", labels[:10])
    print("balance:", sum(labels), len(labels) - sum(labels))

    import os
    os.environ.setdefault("PYTHONHASHSEED", "42")
    random.seed(42)
    import torch
    torch.manual_seed(42)

    from sentence_transformers import MultiVectorEncoder

    m = MultiVectorEncoder("antoinelouis/colbert-xm", device="cpu", model_kwargs={"dtype": torch.float32})
    q_tok = m.encode_query(s1, batch_size=32, show_progress_bar=False)
    d_tok = m.encode_document(s2, batch_size=32, show_progress_bar=False)
    print("len q:", len(q_tok), "len d:", len(d_tok))

    scores = []
    with torch.no_grad():
        for i in range(len(s1)):
            q = torch.as_tensor(q_tok[i]).float()
            d = torch.as_tensor(d_tok[i]).float()
            qn = q / (q.norm(dim=-1, keepdim=True) + 1e-12)
            dn = d / (d.norm(dim=-1, keepdim=True) + 1e-12)
            sim = qn @ dn.T
            scores.append(float(sim.max(dim=1).values.mean()))

    import statistics

    print(f"scores: min={min(scores):.4f} max={max(scores):.4f} std={statistics.pstdev(scores):.4f}")
    pos = [s for s, l in zip(scores, labels, strict=True) if l == 1]
    neg = [s for s, l in zip(scores, labels, strict=True) if l == 0]
    print(f"pos: min={min(pos):.4f} max={max(pos):.4f} media={statistics.fmean(pos):.4f}")
    print(f"neg: min={min(neg):.4f} max={max(neg):.4f} media={statistics.fmean(neg):.4f}")
    for i in range(6):
        print(f"  i={i} label={labels[i]} score={scores[i]:.4f} s1={s1[i][:40]!r} s2={s2[i][:40]!r}")

    # AUC igual ao do script
    def auc(scores, labels):
        pos = sorted(s for s, l in zip(scores, labels, strict=True) if l == 1)
        neg = sorted(s for s, l in zip(scores, labels, strict=True) if l == 0)
        i = j = wins = ties = 0
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

    a = auc(scores, labels)
    print("AUC:", a)
    OUT.write_text(json.dumps({"n": len(scores), "auc": a, "score_std": statistics.pstdev(scores)}, ensure_ascii=False), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
