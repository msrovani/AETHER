#!/usr/bin/env python3
"""B2 pós-FAIL (MSG 0705) — oversample de k e ADC antes do full 5k.

Hipótese do smoke n=200 (`target/b0_bits_f32_rerank_n200.json`): com k=32 o doc
certo nem entra no top-K Hamming (recall@1 bits 0,745 → f32 0,750). Dois
caminhos documentados em `docs/GATE_B2_BITS_F32_RERANK.md`:

  1) **oversample**: `k` ∈ {32..1024} no primeiro estágio, rerank f32 no top-K;
  2) **ADC** (asymmetric distance computation): score = Σ_k Pq[k]·sign(bank[k]),
     usando a magnitude do `s2_query.f32` projetado — não só o sinal.

Mesmo teto: `recall1_bits_f32_rerank ≥ 0,90` (não baixa).

  python tools/b2_oversample_adc.py --sample 200 --ks 32,128,512,1024
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from b0_bits_f32_rerank import load_f32, load_hv  # noqa: E402
from harvest_transplant import build_P  # noqa: E402

ROUND = Path("data/rounds/20261117_b0_paws")


def load_queries_f32(path: Path, n: int, dim: int) -> np.ndarray:
    """`s2_query.f32` com header (8 B) ou cru (b0_pair_cosines escreve cru)."""
    raw = path.read_bytes()
    if len(raw) == n * dim * 4:
        return np.frombuffer(raw, dtype="<f4").reshape(n, dim)
    got, d = struct.unpack_from("<II", raw, 0)
    if got < n or d != dim:
        raise SystemExit(f"{path}: n={got} dim={d} != {n}/{dim}")
    return np.frombuffer(raw, dtype="<f4", count=n * dim, offset=8).reshape(n, dim)


def bits_to_signs(rows: list[list[int]], dim_bits: int) -> np.ndarray:
    """(n, words) u64 → (n, dim_bits) int8 ±1 (bit=1 → +1, ordem do project_bits)."""
    w = np.asarray(rows, dtype=np.uint64)
    shifts = np.arange(64, dtype=np.uint64)
    bits = ((w[:, :, None] >> shifts) & np.uint64(1)).astype(np.int8)
    return bits.reshape(len(rows), dim_bits) * np.int8(2) - np.int8(1)


def stage_scores(bank_signed: np.ndarray, q_vec: np.ndarray, chunk: int) -> np.ndarray:
    """Score float por doc em blocos (evita 5000×10240 float32 vivo de uma vez)."""
    out = np.empty(bank_signed.shape[0], dtype=np.float32)
    for i in range(0, bank_signed.shape[0], chunk):
        blk = bank_signed[i : i + chunk].astype(np.float32)
        out[i : i + chunk] = blk @ q_vec
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="B2: sweep de k + ADC")
    ap.add_argument("--round", type=Path, default=ROUND)
    ap.add_argument("--sample", type=int, default=200, help="0 = todas as queries")
    ap.add_argument("--ks", default="32,64,128,256,512,1024")
    ap.add_argument("--chunk", type=int, default=512)
    ap.add_argument(
        "--out", type=Path, default=Path("target/b2_oversample_adc.json")
    )
    args = ap.parse_args()
    t0 = time.perf_counter()

    manifest = json.loads((args.round / "manifest.json").read_text(encoding="utf-8"))
    dim_in, dim_bits = int(manifest["dim_in"]), int(manifest["dim_hv"])
    seed = int(manifest.get("seed", 0))

    n_hv, words, hv_rows = load_hv(args.round / "hv" / "bank.bin")
    n_doc, dim, doc_f32 = load_f32(args.round / "emb" / "bank.f32")
    if n_hv != n_doc or dim != dim_in or words != dim_bits // 64:
        raise SystemExit(
            f"desalinhado hv={n_hv}/{words} doc={n_doc}/{dim} manifest={dim_in}/{dim_bits}"
        )

    q_f32 = load_queries_f32(args.round / "s2_query_f32.bin", n_hv, dim)
    n = n_hv if args.sample <= 0 else min(args.sample, n_hv)
    ks = sorted({int(k) for k in args.ks.split(",") if k.strip()} | {1})

    P = build_P(dim_in, dim_bits, seed)  # float32 (dim_bits, dim_in)
    bank_signed = bits_to_signs(hv_rows, dim_bits)
    docs = np.asarray(doc_f32, dtype=np.float32)
    docs /= np.maximum(np.linalg.norm(docs, axis=1, keepdims=True), 1e-12)
    qs = q_f32[:n].astype(np.float32)
    qs /= np.maximum(np.linalg.norm(qs, axis=1, keepdims=True), 1e-12)

    # Sanidade: sinal(Pq) tem de reproduzir os bits s2_query_hv do MESMO P.
    proj = qs @ P.T  # (n, dim_bits)
    sign_q = np.where(proj >= 0.0, np.int8(1), np.int8(-1))
    agree = None
    q_hv_path = args.round / "s2_query_hv.bin"
    if q_hv_path.exists():
        raw = q_hv_path.read_bytes()
        qh = np.frombuffer(raw, dtype="<u8")
        if qh.size >= n * words:
            q_rows = [list(qh[i * words : (i + 1) * words]) for i in range(n)]
            agree = float((bits_to_signs(q_rows, dim_bits) == sign_q).mean())

    modes = ("bits", "adc")
    hit_stage = {m: {k: 0 for k in ks} for m in modes}
    hit_rerank = {m: {k: 0 for k in ks} for m in modes}
    stage_us = {m: [] for m in modes}
    ranks = {m: [] for m in modes}
    ranks["f32"] = []
    exact_hit = 0
    maxk = min(max(ks), n_doc)

    for i in range(n):
        # Teto: cosseno float exato contra o bank inteiro (sem quantização).
        exact = qs[i] @ docs.T
        ranks["f32"].append(int((exact > exact[i]).sum()))
        if int(np.argmax(exact)) == i:
            exact_hit += 1
        for mode in modes:
            q_vec = sign_q[i].astype(np.float32) if mode == "bits" else proj[i]
            t = time.perf_counter()
            score = stage_scores(bank_signed, q_vec, args.chunk)
            stage_us[mode].append((time.perf_counter() - t) * 1e6)
            order = np.argsort(-score, kind="stable")
            ranks[mode].append(int((score > score[i]).sum()))
            for k in ks:
                cand = order[: min(k, maxk)]
                if cand[0] == i:
                    hit_stage[mode][k] += 1
                # rerank f32: o doc certo tem de vencer o cosseno dentro do top-K
                if cand[int(np.argmax(qs[i] @ docs[cand].T))] == i:
                    hit_rerank[mode][k] += 1

    by_k = {}
    for mode in modes:
        by_k[mode] = {
            str(k): {
                "recall1_stage": round(hit_stage[mode][k] / n, 4),
                "recall1_bits_f32_rerank": round(hit_rerank[mode][k] / n, 4),
            }
            for k in ks
        }
    p99 = {
        m: round(float(np.percentile(stage_us[m], 99)), 1) for m in modes
    }
    rank_stats = {
        m: {
            "median": int(np.median(ranks[m])),
            "p90": int(np.percentile(ranks[m], 90)),
            "hit_rank1": round(sum(1 for r in ranks[m] if r == 0) / n, 4),
        }
        for m in (*modes, "f32")
    }
    best = max(
        (
            (by_k[m][str(k)]["recall1_bits_f32_rerank"], m, k)
            for m in modes
            for k in ks
        )
    )
    passed = bool(best[0] >= 0.90)
    art = {
        "ok": True,
        "gate": "B2_oversample_adc",
        "verdict": "PASS" if passed else "FAIL",
        "round": str(args.round).replace("\\", "/"),
        "n": n,
        "ks": ks,
        "dim": dim,
        "dim_bits": dim_bits,
        "seed": seed,
        "sign_agreement_with_s2_query_hv": None if agree is None else round(agree, 6),
        "by_k": by_k,
        "best": {"recall1": best[0], "mode": best[1], "k": best[2]},
        "gate_recall1": 0.90,
        "gate_pass": passed,
        "stage_p99_us": p99,
        "ceiling_exact_f32_recall1": round(exact_hit / n, 4),
        "truth_rank": rank_stats,
        "elapsed_s": round(time.perf_counter() - t0, 2),
        "ref": "MSG_0705_B2_oversample_ADC",
        "doc": "docs/GATE_B2_BITS_F32_RERANK.md",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, ensure_ascii=False))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
