#!/usr/bin/env python3
"""B2 pós-FAIL (MSG 0705) — oversample de k e ADC antes do full 5k.

O smoke n=200 (`target/b0_bits_f32_rerank_n200.json`) deu recall@1 0,745 em bits
e 0,750 depois do rerank. Os dois caminhos previstos em
`docs/GATE_B2_BITS_F32_RERANK.md` são testados aqui no mesmo round e na mesma
verdade (query = `s2` sozinho, doc = `s1|rel|s2`):

  1) **oversample**: k ∈ {32..1024} no primeiro estágio, rerank f32 no top-K;
  2) **ADC**: score = Σ_k Pq[k]·sign(bank[k]) — magnitude da query, não só sinal.

O probe carrega também o **teto**: cosseno float exato contra o bank inteiro.
Se o teto empatar com bits+rerank, o gap não é rerank/top-K/quantização.

  python tools/b2_oversample_adc.py --sample 200 --ks 32,256,1024
"""
from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from b0_bits_f32_rerank import load_f32, load_hv  # noqa: E402
from harvest_transplant import build_P  # noqa: E402


def load_queries_f32(path: Path, n: int, dim: int) -> np.ndarray:
    """`s2_query.f32` com header (8 B) ou cru — `b0_pair_cosines` escreve cru."""
    raw = path.read_bytes()
    if len(raw) == n * dim * 4:
        return np.frombuffer(raw, dtype="<f4").reshape(n, dim)
    got, d = struct.unpack_from("<II", raw, 0)
    if got < n or d != dim:
        raise SystemExit(f"{path}: n={got} dim={d} != {n}/{dim}")
    return np.frombuffer(raw, dtype="<f4", count=n * dim, offset=8).reshape(n, dim)


def bits_to_signs(rows: list[list[int]], dim_bits: int) -> np.ndarray:
    """(n, words) u64 → (n, dim_bits) int8 ±1 (bit=1 → +1; ordem do project_bits)."""
    w = np.asarray(rows, dtype=np.uint64)
    bits = ((w[:, :, None] >> np.arange(64, dtype=np.uint64)) & np.uint64(1)).astype(np.int8)
    return bits.reshape(len(rows), dim_bits) * np.int8(2) - np.int8(1)


def scores(bank_signed: np.ndarray, q_vec: np.ndarray) -> np.ndarray:
    """Score float por doc (o bank é int8 para caber; float32 só por bloco)."""
    out = np.empty(bank_signed.shape[0], dtype=np.float32)
    for i in range(0, bank_signed.shape[0], 512):
        out[i : i + 512] = bank_signed[i : i + 512].astype(np.float32) @ q_vec
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="B2: sweep de k + ADC + teto f32")
    ap.add_argument("--round", type=Path, default=Path("data/rounds/20261117_b0_paws"))
    ap.add_argument("--sample", type=int, default=200, help="0 = todas as queries")
    ap.add_argument("--ks", default="32,256,1024")
    ap.add_argument("--out", type=Path, default=Path("target/b2_oversample_adc.json"))
    args = ap.parse_args()

    manifest = json.loads((args.round / "manifest.json").read_text(encoding="utf-8"))
    dim_in, dim_bits = int(manifest["dim_in"]), int(manifest["dim_hv"])
    seed = int(manifest.get("seed", 0))

    n, words, hv_rows = load_hv(args.round / "hv" / "bank.bin")
    n_doc, dim, doc_f32 = load_f32(args.round / "emb" / "bank.f32")
    if n != n_doc or dim != dim_in or words != dim_bits // 64:
        raise SystemExit(f"desalinhado hv={n}/{words} doc={n_doc}/{dim} manifest={dim_in}/{dim_bits}")

    queries = load_queries_f32(args.round / "s2_query_f32.bin", n, dim)
    sample = n if args.sample <= 0 else min(args.sample, n)
    ks = sorted({int(k) for k in args.ks.split(",") if k.strip()})

    bank_signed = bits_to_signs(hv_rows, dim_bits)
    docs = np.asarray(doc_f32, dtype=np.float32)
    docs /= np.maximum(np.linalg.norm(docs, axis=1, keepdims=True), 1e-12)
    qs = queries[:sample].astype(np.float32)
    qs /= np.maximum(np.linalg.norm(qs, axis=1, keepdims=True), 1e-12)
    proj = qs @ build_P(dim_in, dim_bits, seed).T  # (sample, dim_bits)
    sign_q = np.where(proj >= 0.0, np.int8(1), np.int8(-1))

    # O caminho bits tem de ser o MESMO P que gerou `s2_query_hv.bin`; senão nada aqui vale.
    agree = None
    q_hv = args.round / "s2_query_hv.bin"
    if q_hv.exists():
        raw = np.frombuffer(q_hv.read_bytes(), dtype="<u8")
        if raw.size >= sample * words:
            rows = [list(raw[i * words : (i + 1) * words]) for i in range(sample)]
            agree = round(float((bits_to_signs(rows, dim_bits) == sign_q).mean()), 6)

    # Verdade do probe: doc i é o par da query i (mesma linha do facts.jsonl).
    truth = {"bits": sign_q, "adc": proj}
    hit_stage = {m: 0 for m in truth}
    hit_rerank = {m: {k: 0 for k in ks} for m in truth}
    rank = {m: [] for m in (*truth, "f32")}
    exact_hit = 0
    maxk = min(max(ks), n_doc)

    for i in range(sample):
        exact = qs[i] @ docs.T
        rank["f32"].append(int((exact > exact[i]).sum()))
        exact_hit += int(np.argmax(exact) == i)
        for mode, q_vec in truth.items():
            score = scores(bank_signed, q_vec[i].astype(np.float32))
            order = np.argsort(-score, kind="stable")
            pos = int((score > score[i]).sum())
            rank[mode].append(pos)
            hit_stage[mode] += int(pos == 0)
            # cos do top-K: prefix-max dá o melhor candidato de cada k sem laço extra.
            cos = qs[i] @ docs[order[:maxk]].T
            best = np.maximum.accumulate(cos)
            for k in ks:
                hit_rerank[mode][k] += int(pos < k and cos[pos] >= best[min(k, maxk) - 1])

    by_k = {
        m: {str(k): round(hit_rerank[m][k] / sample, 4) for k in ks} for m in truth
    }
    ceiling = round(exact_hit / sample, 4)
    best_recall = max(v for m in by_k for v in by_k[m].values())
    art = {
        "gate": "B2_oversample_adc",
        "verdict": "PASS" if best_recall >= 0.90 else "FAIL",
        "round": str(args.round).replace("\\", "/"),
        "n": sample,
        "ks": ks,
        "seed": seed,
        "sign_agreement_with_s2_query_hv": agree,
        "recall1_stage": {m: round(hit_stage[m] / sample, 4) for m in truth},
        "recall1_after_rerank_by_k": by_k,
        "ceiling_exact_f32_recall1": ceiling,
        "truth_rank": {
            m: {"median": int(np.median(rank[m])), "p90": int(np.percentile(rank[m], 90))}
            for m in rank
        },
        "gate_recall1": 0.90,
        "ref": "MSG_0705_B2_oversample_ADC",
        "doc": "docs/GATE_B2_BITS_F32_RERANK.md",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, ensure_ascii=False))
    return 0 if art["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
