#!/usr/bin/env python3
# D1 — probe per-token MaxSim com API CANÔNICA do ST6 (MultiVectorEncoder).
#
# Responde ao ALERTA 0655 (D1 sem processo/sem artefato) usando a lit de
# 0617/0643: colbert-xm é MultiVectorEncoder; a API simétrica `model.encode()`
# pode invalidar a medição — usar `encode_query` / `encode_document`
# assimétricos e registrar o api_shape no artefato (pedido 0617).
#
# Scores (ambos registrados):
#   meanmaxsim = média dos máximos por token de query (bounded 0..1, AUC
#                comparável com as réguas donor/bigram/E1)
#   maxsim_sum = soma dos máximos (ColBERT clássico)
# AUC + Youden (t_mid) + bootstrap estratificado B=1000 (IC95). Seed 42, T=0.
#
#   python tools/d1_maxsim_st6.py --pairs 5000 --device cuda
#
# Artefato: target/d1_maxsim_st6.json · Não toca b0_maxsim_colbert.py (claim 0581).

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description="D1 MaxSim ST6 (MultiVectorEncoder, meanmaxsim)")
    ap.add_argument("--round", default="data/rounds/20261117_b0_paws")
    ap.add_argument("--donor", default="antoinelouis/colbert-xm")
    ap.add_argument("--device", default="cuda")
    ap.add_argument("--pairs", type=int, default=5000)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--bootstrap", type=int, default=1000)
    ap.add_argument("--out", type=Path, default=Path("target/d1_maxsim_st6.json"))
    ap.add_argument(
        "--dry-check",
        action="store_true",
        help="só valida facts/balance/alinhamento; não carrega modelo nem GPU",
    )
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
    # Guarda anti-deslocamento (lição 0567): n alinhado ANTES de qualquer zip.
    n = len(facts)
    balance = sum(labels)
    if not (n == len(s1) == len(s2) == len(labels)):
        raise SystemExit(f"desalinhamento: n={n} s1={len(s1)} s2={len(s2)} labels={len(labels)}")
    if args.dry_check:
        meta = {
            "ok": True,
            "dry_check": True,
            "n": n,
            "balance": [balance, n - balance],
            "round": str(args.round),
            "donor": args.donor,
            "out": str(args.out),
            "regua": {
                "donor_frase": 0.6909,
                "bigram_sonda_buffy": 0.7347,
                "e1_roles": 0.5983,
                "piso_abstain": 0.60,
            },
            "note": "pronto para encode; claim D1 permanece OPENCODE",
        }
        print(json.dumps(meta, ensure_ascii=False, indent=2))
        return 0

    import os
    import random as rnd_mod

    os.environ.setdefault("PYTHONHASHSEED", str(args.seed))
    rnd_mod.seed(args.seed)
    import numpy as np

    np.random.seed(args.seed % (2**32 - 1))
    import torch

    torch.manual_seed(args.seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(args.seed)

    # API canônica ST6 (MSG 0617/0643): MultiVectorEncoder assimétrico.
    from sentence_transformers import MultiVectorEncoder

    model = MultiVectorEncoder(
        args.donor, device=args.device, model_kwargs={"dtype": torch.float32}
    )

    # encode_query / encode_document são chamadas DISTINTAS por desenho.
    q_tok = model.encode_query(s1, batch_size=args.batch, show_progress_bar=True)  # (n, Tq, d)
    d_tok = model.encode_document(s2, batch_size=args.batch, show_progress_bar=True)  # (n, Td, d)
    if not (len(q_tok) == len(d_tok) == n):
        raise SystemExit(f"encode desalinhado: q={len(q_tok)} d={len(d_tok)} n={n}")

    meanmax: list[float] = []
    summax: list[float] = []
    with torch.no_grad():
        for i in range(n):
            q = torch.as_tensor(q_tok[i], device=args.device).float()
            d = torch.as_tensor(d_tok[i], device=args.device).float()
            qn = q / (q.norm(dim=-1, keepdim=True) + 1e-12)
            dn = d / (d.norm(dim=-1, keepdim=True) + 1e-12)
            sim = qn @ dn.T  # (Tq, Td) — cosine por par de tokens
            per_q_max = sim.max(dim=1).values  # (Tq,)
            meanmax.append(float(per_q_max.mean()))
            summax.append(float(per_q_max.sum()))
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    if not (len(meanmax) == len(summax) == len(labels)):
        raise SystemExit(f"scores desalinhados: meanmax={len(meanmax)} labels={len(labels)}")

    def auc(scores: list[float], labels: list[int]) -> float:
        if len(scores) != len(labels):
            raise ValueError(f"auc zip: scores={len(scores)} labels={len(labels)}")
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

    def bootstrap_ic95(scores: list[float], labels: list[int], b: int, seed: int) -> tuple[float, float]:
        rnd = random.Random(seed)
        idx1 = [i for i, l in enumerate(labels) if l == 1]
        idx0 = [i for i, l in enumerate(labels) if l == 0]
        aucs = []
        for _ in range(b):
            sample = [rnd.choice(idx1) for _ in idx1] + [rnd.choice(idx0) for _ in idx0]
            aucs.append(auc([scores[i] for i in sample], [labels[i] for i in sample]))
        aucs.sort()
        return aucs[int(0.025 * b)], aucs[int(0.975 * b) - 1]

    def youden(scores: list[float], labels: list[int]) -> float:
        """Limiar de Youden: maximiza TPR-FPR (Jorden)."""
        pairs = sorted(zip(scores, labels, strict=True))
        best, best_t = -1.0, 0.0
        n1, n0 = sum(labels), len(labels) - sum(labels)
        for i in range(len(pairs)):
            if i and pairs[i][0] == pairs[i - 1][0]:
                continue
            t = pairs[i][0]
            tp = sum(1 for s, l in pairs if l == 1 and s >= t)
            tn = sum(1 for s, l in pairs if l == 0 and s < t)
            j = (tp / n1 if n1 else 0.0) + (tn / n0 if n0 else 0.0) - 1.0
            if j > best:
                best, best_t = j, t
        return best_t

    base_auc = auc(meanmax, labels)
    lo, hi = bootstrap_ic95(meanmax, labels, args.bootstrap, args.seed)
    t_mid = youden(meanmax, labels)
    auc_sum = auc(summax, labels)

    resultado = {
        "ok": True,
        "probe": "D1_per_token_MaxSim_ST6_canonical",
        "api_shape": "MultiVectorEncoder.encode_query+encode_document (assimétrico, MSG 0617/0643)",
        "metrica_principal": "meanmaxsim_media_dos_max_por_token_de_query",
        "metrica_secundaria": "maxsim_sum_colbert_classico",
        "n": n,
        "balance": [balance, n - balance],
        "auc_meanmaxsim": round(base_auc, 4),
        "auc_maxsim_sum": round(auc_sum, 4),
        "ic95": [round(lo, 4), round(hi, 4)],
        "bootstrap": args.bootstrap,
        "t_mid_youden": round(t_mid, 4),
        "device": args.device,
        "seed": args.seed,
        "regua": {
            "donor_frase": 0.6909,
            "bigram_sonda_buffy": 0.7347,
            "e1_roles": 0.5983,
            "piso_abstain": 0.60,
        },
        "created": "2026-11-18",
    }
    args.out.write_text(json.dumps(resultado, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(resultado, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
