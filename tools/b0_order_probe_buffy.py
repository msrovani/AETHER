#!/usr/bin/env python3
# BUFFY — SONDA DE ORDEM (MSG 0521). Pergunta: a AUC 0.4945 do B-G0 vem do
# donor, ou do POOLING? Mesmo donor (Qwen3-Embedding-0.6B, Apache-2.0, cache),
# quatro codificações, mesmos pares PAWS-x (seed 7, prefixo do round b0_paws):
#
#   A_mean  : media dos vetores por token            (bag-of-words explicito)
#   A_last  : pool do donor (ultimo token)           (reproduz o gate A3)
#   B_pos   : sum_t roll(x_t, k*t)                   (bind de posicao VSA)
#   C_bigram: sum_t roll(x_t,1) * roll(x_{t+1},2)    (ordem local / 2-grama)
#
# B e C sao COMPOSICAO — passam a geometria do donor por uma operacao
# nao-comutativa, exatamente o papel da E1 (fase/linguagem). Custo: AUC e
# invariante a carga -> roda em CPU fp32 (nao ocupa a GPU do B-G0 e evita a
# armadilha bf16-CPU 29x da MSG 0513).
#
#   python tools/b0_order_probe_buffy.py --pairs 1600 --out target/b0_order_probe.json

from __future__ import annotations

import argparse
import hashlib
import json
import random
import time
from pathlib import Path

DONOR = "Qwen/Qwen3-Embedding-0.6B"


def build_pairs(n: int, seed: int = 7, split: str = "train") -> list[dict]:
    from datasets import load_dataset

    ds = load_dataset("google-research-datasets/paws-x", "en", split=split)
    ones = [r for r in ds if r["label"] == 1]
    zeros = [r for r in ds if r["label"] == 0]
    rnd = random.Random(seed)
    rnd.shuffle(ones)
    rnd.shuffle(zeros)
    half = n // 2
    chosen = ones[:half] + zeros[: n - half]
    rnd.shuffle(chosen)
    return chosen


def auc(cos: list[float], lab: list[int]) -> float:
    """Mann-Whitney U normalizado, com postos medios para empates."""
    order = sorted(range(len(cos)), key=lambda i: cos[i])
    n1 = sum(lab)
    n0 = len(lab) - n1
    if n1 == 0 or n0 == 0:
        return 0.5
    s = 0.0
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and cos[order[j + 1]] == cos[order[i]]:
            j += 1
        rank = (i + j + 2) / 2.0
        for k in range(i, j + 1):
            if lab[order[k]]:
                s += rank
        i = j + 1
    return (s - n1 * (n1 + 1) / 2.0) / (n1 * n0)


def main() -> int:
    ap = argparse.ArgumentParser(description="sonda de ordem: donor vs composicao")
    ap.add_argument("--pairs", type=int, default=1600)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--step", type=int, default=13, help="passo do roll de posicao")
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--max-len", type=int, default=64)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--out", type=Path, default=Path("target/b0_order_probe.json"))
    args = ap.parse_args()

    import torch
    from transformers import AutoModel, AutoTokenizer

    t_load = time.time()
    tok = AutoTokenizer.from_pretrained(DONOR)
    model = AutoModel.from_pretrained(DONOR, torch_dtype=torch.float32).eval().to(args.device)
    load_s = time.time() - t_load

    rows = build_pairs(args.pairs, args.seed)
    s1 = [r["sentence1"] for r in rows]
    s2 = [r["sentence2"] for r in rows]
    lab = [1 if r["label"] == 1 else 0 for r in rows]

    def encode(texts: list[str]) -> dict[str, "torch.Tensor"]:
        arms = {k: [] for k in ("A_mean", "A_last", "B_pos", "C_bigram")}
        for i in range(0, len(texts), args.batch):
            chunk = texts[i : i + args.batch]
            enc = tok(
                chunk,
                return_tensors="pt",
                padding=True,
                truncation=True,
                max_length=args.max_len,
            ).to(args.device)
            with torch.no_grad():
                out = model(**enc, output_hidden_states=True)
            H = out.hidden_states[-1]
            mask = enc["attention_mask"].bool()
            for b in range(len(chunk)):
                h = H[b][mask[b]].float()
                h = torch.nn.functional.normalize(h, dim=-1)
                d = h.shape[-1]
                arms["A_mean"].append(torch.nn.functional.normalize(h.mean(0), dim=-1))
                arms["A_last"].append(h[-1])
                acc = torch.zeros(d)
                for t in range(h.shape[0]):
                    acc = acc + torch.roll(h[t], (t * args.step) % d, 0)
                arms["B_pos"].append(torch.nn.functional.normalize(acc, dim=-1))
                acc = torch.zeros(d)
                if h.shape[0] >= 2:
                    for t in range(h.shape[0] - 1):
                        acc = acc + torch.roll(h[t], 1, 0) * torch.roll(h[t + 1], 2, 0)
                else:
                    acc = torch.roll(h[0], 1, 0)
                arms["C_bigram"].append(torch.nn.functional.normalize(acc, dim=-1))
        return {k: torch.stack(v) for k, v in arms.items()}

    t_enc = time.time()
    e1 = encode(s1)
    e2 = encode(s2)
    enc_s = time.time() - t_enc

    res = {}
    for name in e1:
        cos = (e1[name] * e2[name]).sum(-1).tolist()
        a = auc(cos, lab)
        if len(cos) != len(lab):
            raise SystemExit(f"order_probe zip: cos={len(cos)} lab={len(lab)}")
        m1 = sum(c for c, l in zip(cos, lab, strict=True) if l) / max(1, sum(lab))
        m0 = sum(c for c, l in zip(cos, lab, strict=True) if not l) / max(
            1, len(lab) - sum(lab)
        )
        res[name] = {
            "auc": round(a, 4),
            "cos_parafrase": round(m1, 4),
            "cos_nao_parafrase": round(m0, 4),
            "margem": round(m1 - m0, 4),
        }

    art = {
        "ok": True,
        "probe": "b0_order_probe_buffy",
        "donor": DONOR,
        "pairs": len(rows),
        "label1": sum(lab),
        "seed": args.seed,
        "position_step": args.step,
        "device": args.device,
        "dtype": "float32",
        "load_s": round(load_s, 1),
        "encode_s": round(enc_s, 1),
        "arms": res,
        "baseline_gate_a3_auc": 0.4945,
        "note": "AUC invariante a carga; pares = prefixo deterministico do round b0_paws",
    }
    raw = json.dumps(art, indent=2, sort_keys=True).encode()
    art["sha256"] = hashlib.sha256(raw).hexdigest()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, indent=2, sort_keys=True), encoding="utf-8")
    print(json.dumps(art, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
