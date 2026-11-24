#!/usr/bin/env python3
"""B2 — probe das duas direções (item 3 de `docs/GATE_B2_BITS_F32_RERANK.md`).

O probe anterior provou que o gap de 24,5 pp não é top-K, nem rerank, nem
quantização: bits+rerank empata com o teto f32 exato (0,755). O suspeito é a
assimetria query=`s2` vs doc=`s1|rel|s2`. Aqui o fato é indexado nas **duas**
direções e a query `s2` consulta as duas:

  fwd: `s1|rel|s2`  (o bank do round, já embutido)
  rev: `s2|rel|s1`  (re-embed com o mesmo donor/seed/dim/temperature)

Mesmo donor e mesmas opções do `donor_map_embed` (Qwen3-Embedding-0.6B, MRL
768, seed 42, temperature 0, sem prompt). O `bank_rev.f32` fica em cache: o
probe é idempotente e o re-embed só roda uma vez.

  python tools/b2_two_directions.py --sample 200 --k 32
"""
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import struct
import sys
import time
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


def embed_reverse(round_dir: Path, donor: str, dim: int, seed: int) -> Path:
    """Embed `s2|rel|s1` com o MESMO runtime/donor do `emb/bank.f32`."""
    from embed_runtime import (
        default_batch,
        load_embedder,
        resolve_device,
        resolve_dtype,
        set_determinism,
        tune_cpu_threads,
    )

    facts = [
        json.loads(l)
        for l in (round_dir / "facts.jsonl").read_text(encoding="utf-8").splitlines()
        if l.strip()
    ]
    labels = [f"{f['object']}|{f['relation']}|{f['subject']}" for f in facts]
    device = resolve_device("cpu")
    set_determinism(seed)
    tune_cpu_threads()
    dtype = resolve_dtype(device, "auto")
    model, _ = load_embedder(donor, dim=dim, device=device, dtype=dtype)
    print(f"embed rev: {len(labels)} textos device={device} dtype={dtype}", flush=True)
    t0 = time.perf_counter()
    emb = model.encode(
        labels,
        batch_size=default_batch(device),
        normalize_embeddings=True,
        show_progress_bar=False,
    )
    out = round_dir / "emb" / "bank_rev.f32"
    raw = struct.pack("<II", len(emb), dim)
    for row in emb:
        raw += struct.pack(f"<{dim}f", *map(float, row))
    out.write_bytes(raw)
    print(f"embed rev: {time.perf_counter() - t0:.5g}s -> {out}", flush=True)
    return out


def signs_from_f32(docs: np.ndarray, P: np.ndarray, dim_bits: int) -> np.ndarray:
    """(n, dim) f32 → (n, dim_bits) int8 ±1 pelo mesmo P do bank."""
    out = np.empty((docs.shape[0], dim_bits), dtype=np.int8)
    for i in range(0, docs.shape[0], 256):
        proj = docs[i : i + 256] @ P.T
        out[i : i + 256] = np.where(proj >= 0.0, np.int8(1), np.int8(-1))
    return out


def scores(signs: np.ndarray, q: np.ndarray) -> np.ndarray:
    out = np.empty(signs.shape[0], dtype=np.float32)
    for i in range(0, signs.shape[0], 512):
        out[i : i + 512] = signs[i : i + 512].astype(np.float32) @ q
    return out


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(description="B2: duas direções (fwd + rev)")
    ap.add_argument("--round", type=Path, default=Path("data/rounds/20261117_b0_paws"))
    ap.add_argument("--sample", type=int, default=200, help="0 = todas as queries")
    ap.add_argument("--k", type=int, default=32, help="candidatos bits antes do rerank")
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--seed", type=int, default=42, help="seed do embed (não do JL)")
    ap.add_argument("--out", type=Path, default=Path("target/b2_two_directions.json"))
    args = ap.parse_args()

    manifest = json.loads((args.round / "manifest.json").read_text(encoding="utf-8"))
    dim, dim_bits = int(manifest["dim_in"]), int(manifest["dim_hv"])
    jl_seed = int(manifest.get("seed", 0))

    n_hv, words, hv_rows = load_hv(args.round / "hv" / "bank.bin")
    n_doc, dim_doc, fwd_rows = load_f32(args.round / "emb" / "bank.f32")
    if n_hv != n_doc or dim_doc != dim or words != dim_bits // 64:
        raise SystemExit(f"desalinhado hv={n_hv}/{words} fwd={n_doc}/{dim_doc}")

    rev_path = args.round / "emb" / "bank_rev.f32"
    if not rev_path.exists():
        rev_path = embed_reverse(args.round, args.donor, dim, args.seed)
    n_rev, dim_rev, rev_rows = load_f32(rev_path)
    if n_rev != n_hv or dim_rev != dim:
        raise SystemExit(f"{rev_path}: n={n_rev} dim={dim_rev} != {n_hv}/{dim}")

    q = load_queries_f32(args.round / "s2_query_f32.bin", n_hv, dim)
    sample = n_hv if args.sample <= 0 else min(args.sample, n_hv)
    k = min(args.k, n_hv)

    P = build_P(dim, dim_bits, jl_seed)  # mesmo P do harvest (seed do manifest)
    docs = {"fwd": np.asarray(fwd_rows, dtype=np.float32), "rev": np.asarray(rev_rows, dtype=np.float32)}
    for d in docs.values():
        d /= np.maximum(np.linalg.norm(d, axis=1, keepdims=True), 1e-12)
    qs = q[:sample].astype(np.float32)
    qs /= np.maximum(np.linalg.norm(qs, axis=1, keepdims=True), 1e-12)

    bits = {"fwd": np.array(hv_rows, dtype=np.uint64)}
    bits["rev"] = signs_from_f32(docs["rev"], P, dim_bits)  # ±1; fwd vem do bank
    # Sanidade do fwd: o bank tem de ser o MESMO P (senão a comparação é falsa).
    fwd_signs = (bits["fwd"][:, :, None] >> np.arange(64, dtype=np.uint64)) & np.uint64(1)
    fwd_signs = fwd_signs.reshape(n_hv, dim_bits).astype(np.int8) * np.int8(2) - np.int8(1)
    agree = float((fwd_signs == signs_from_f32(docs["fwd"], P, dim_bits)).mean())

    q_bits = np.where(qs @ P.T >= 0.0, np.int8(1), np.int8(-1))
    hits = {d: {"stage": 0, "rerank": 0, "exact": 0} for d in docs}

    # Cada direção é medida no seu próprio espaço: cosseno entre direções não é
    # comparável (escalas diferentes), então não há fusão de score aqui.
    for i in range(sample):
        for d in docs:
            score = scores(fwd_signs if d == "fwd" else bits["rev"], q_bits[i].astype(np.float32))
            order = np.argsort(-score, kind="stable")[:k]
            cos = qs[i] @ docs[d][order].T
            exact = qs[i] @ docs[d].T
            hits[d]["stage"] += int(order[0] == i)
            hits[d]["rerank"] += int(order[int(np.argmax(cos))] == i)
            hits[d]["exact"] += int(np.argmax(exact) == i)

    by_dir = {
        d: {
            "recall1_stage": round(hits[d]["stage"] / sample, 4),
            "recall1_after_rerank": round(hits[d]["rerank"] / sample, 4),
            "ceiling_exact_f32": round(hits[d]["exact"] / sample, 4),
        }
        for d in docs
    }
    best = max(v["recall1_after_rerank"] for v in by_dir.values())
    art = {
        "gate": "B2_two_directions",
        "verdict": "PASS" if best >= 0.90 else "FAIL",
        "round": str(args.round).replace("\\", "/"),
        "directions": {"fwd": "s1|rel|s2", "rev": "s2|rel|s1"},
        "donor": args.donor,
        "embed_seed": args.seed,
        "jl_seed": jl_seed,
        "n": sample,
        "k": k,
        "forward_bits_agreement_with_hv_bank": round(agree, 6),
        "by_direction": by_dir,
        "gate_recall1": 0.90,
        "rev_bank_sha256": hashlib.sha256(rev_path.read_bytes()).hexdigest(),
        "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "ref": "MSG_0705_B2_oversample_ADC_item3",
        "doc": "docs/GATE_B2_BITS_F32_RERANK.md",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, ensure_ascii=False))
    return 0 if art["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
