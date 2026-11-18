#!/usr/bin/env python3
"""B-G0/B2 opt-in — retrieve em bits (HV) + rerank em float (MSG 0568/0576).

Protocolo:
  1) query = s2 em HV (`s2_query_hv.bin`) → top-K por Hamming no `hv/bank.bin`
  2) rerank = cosine(s2_f32, bank.f32[cand]) — exige `emb/s2_query.f32`

Gera `emb/s2_query.f32` sob demanda (`--embed-s2`) com o mesmo donor/seed.
Sem model e sem arquivo: ABSTAIN honesto (não inventa query float).

  python tools/b0_bits_f32_rerank.py --round data/rounds/20261117_b0_paws --k 32
  python tools/b0_bits_f32_rerank.py --round ... --embed-s2 --device cpu
"""
from __future__ import annotations

import argparse
import json
import math
import struct
import time
from pathlib import Path

HEADER = 128
WORDS = 160


def load_f32(path: Path) -> tuple[int, int, list[list[float]]]:
    raw = path.read_bytes()
    n, d = struct.unpack_from("<II", raw, 0)
    need = 8 + n * d * 4
    if len(raw) < need:
        raise SystemExit(f"{path}: curto {len(raw)} < {need}")
    rows: list[list[float]] = []
    off = 8
    for _ in range(n):
        rows.append(list(struct.unpack_from(f"<{d}f", raw, off)))
        off += d * 4
    return n, d, rows


def write_f32(path: Path, rows: list[list[float]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    n, d = len(rows), len(rows[0])
    raw = struct.pack("<II", n, d)
    for row in rows:
        raw += struct.pack(f"<{d}f", *row)
    path.write_bytes(raw)


def load_hv(path: Path) -> tuple[int, int, list[list[int]]]:
    raw = path.read_bytes()
    count = struct.unpack_from("<Q", raw, 28)[0]
    words = struct.unpack_from("<I", raw, 16)[0]
    rows: list[list[int]] = []
    off = HEADER
    for _ in range(count):
        rows.append(list(struct.unpack_from(f"<{words}Q", raw, off)))
        off += words * 8
    return count, words, rows


def load_queries_hv(path: Path, n: int, words: int) -> list[list[int]]:
    raw = path.read_bytes()
    need = n * words * 8
    if len(raw) != need:
        raise SystemExit(f"{path}: size {len(raw)} != {need}")
    out: list[list[int]] = []
    for q in range(n):
        base = q * words * 8
        out.append(
            [
                int.from_bytes(raw[base + w * 8 : base + (w + 1) * 8], "little")
                for w in range(words)
            ]
        )
    return out


def hamming(a: list[int], b: list[int]) -> int:
    return sum((x ^ y).bit_count() for x, y in zip(a, b))


def cosine(a: list[float], b: list[float]) -> float:
    acc = nx = ny = 0.0
    for x, y in zip(a, b):
        acc += x * y
        nx += x * x
        ny += y * y
    den = math.sqrt(nx) * math.sqrt(ny)
    return acc / den if den > 1e-12 else 0.0


def embed_s2(
    round_dir: Path,
    donor: str,
    dim: int,
    seed: int,
    device: str,
    limit: int = 0,
) -> Path:
    import sys

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from embed_runtime import default_batch, resolve_device, set_determinism, tune_cpu_threads
    from sentence_transformers import SentenceTransformer

    facts = [
        json.loads(l)
        for l in (round_dir / "facts.jsonl").read_text(encoding="utf-8").splitlines()
        if l.strip()
    ]
    if limit > 0:
        facts = facts[:limit]
    s2 = [f["object"] for f in facts]
    set_determinism(seed)
    dev = resolve_device(device) or "cpu"
    if device == "cpu" or dev == "cpu":
        tune_cpu_threads()
    batch = default_batch(dev)
    model = SentenceTransformer(donor, truncate_dim=dim, device=dev)
    emb = model.encode(s2, batch_size=batch, normalize_embeddings=True, show_progress_bar=True)
    rows = [list(map(float, row)) for row in emb]
    # amostra parcial: arquivo dedicado para não corromper o full
    out = (
        round_dir / "emb" / "s2_query.f32"
        if limit <= 0
        else round_dir / "emb" / f"s2_query_n{limit}.f32"
    )
    write_f32(out, rows)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="HV top-K + f32 rerank (MSG 0576)")
    ap.add_argument("--round", type=Path, default=Path("data/rounds/20261117_b0_paws"))
    ap.add_argument("--k", type=int, default=32, help="candidatos HV antes do rerank")
    ap.add_argument("--sample", type=int, default=0, help="0=todos; senão primeiros N")
    ap.add_argument("--embed-s2", action="store_true")
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--out", type=Path, default=Path("target/b0_bits_f32_rerank.json"))
    args = ap.parse_args()

    t0 = time.perf_counter()
    hv_bin = args.round / "hv" / "bank.bin"
    q_hv = args.round / "s2_query_hv.bin"
    doc_f32 = args.round / "emb" / "bank.f32"
    q_f32 = args.round / "emb" / "s2_query.f32"
    if args.sample > 0:
        q_f32_sample = args.round / "emb" / f"s2_query_n{args.sample}.f32"
        if q_f32_sample.exists():
            q_f32 = q_f32_sample

    if args.embed_s2 or not q_f32.exists():
        if not args.embed_s2:
            print(
                json.dumps(
                    {
                        "ok": False,
                        "abstain": True,
                        "reason": "falta emb/s2_query.f32 — rode com --embed-s2",
                    }
                )
            )
            return 2
        print(json.dumps({"phase": "embed_s2", "device": args.device, "sample": args.sample}))
        q_f32 = embed_s2(
            args.round,
            args.donor,
            args.dim,
            args.seed,
            args.device,
            limit=args.sample,
        )

    n_hv, words, bank = load_hv(hv_bin)
    queries = load_queries_hv(q_hv, n_hv, words)
    n_doc, dim, docs = load_f32(doc_f32)
    n_q, dim_q, qs = load_f32(q_f32)
    if dim != dim_q or words != WORDS or n_doc != n_hv:
        raise SystemExit(
            f"desalinhado n_hv={n_hv} n_doc={n_doc} n_q={n_q} dim={dim}/{dim_q} words={words}"
        )

    n = n_hv if args.sample <= 0 else min(args.sample, n_hv, n_q)
    if n_q < n:
        raise SystemExit(f"s2_query.f32 tem n_q={n_q} < sample={n}")
    hit_bits = hit_rerank = 0
    for i in range(n):
        qbits = queries[i]
        # top-K Hamming (menor distância)
        scored = [(hamming(qbits, bank[j]), j) for j in range(n_hv)]
        scored.sort(key=lambda t: t[0])
        top = [j for _, j in scored[: args.k]]
        if top[0] == i:
            hit_bits += 1
        # rerank float
        best_j, best_c = top[0], -2.0
        for j in top:
            c = cosine(qs[i], docs[j])
            if c > best_c:
                best_c, best_j = c, j
        if best_j == i:
            hit_rerank += 1

    r1_bits = hit_bits / n
    r1_rr = hit_rerank / n
    art = {
        "ok": True,
        "round": str(args.round).replace("\\", "/"),
        "n": n,
        "k": args.k,
        "dim": dim,
        "words": words,
        "recall1_bits": round(r1_bits, 4),
        "recall1_bits_f32_rerank": round(r1_rr, 4),
        "delta_pp": round((r1_rr - r1_bits) * 100.0, 2),
        "seed": args.seed,
        "ref": "MSG_0576_0568_0591",
        "elapsed_s": round(time.perf_counter() - t0, 2),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
