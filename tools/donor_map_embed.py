#!/usr/bin/env python3
# Sidecar B0 — mapa HF fora do crate Rust (non-goal: sem pesos no núcleo).
# A2 + L1/L2: CPU fp32/batch16 · GPU bf16+FA2/batch64 · seed+T=0.
#
#   pip install 'sentence-transformers>=2.7' 'transformers>=4.51'
#   python tools/donor_map_embed.py --out data --donor Qwen/Qwen3-Embedding-0.6B --dim 768
#   python tools/donor_map_embed.py --out data/rounds/bg0_paws --device cuda --seed 42
#
# Depois:
#   python tools/harvest_transplant.py --jl-only --out data --revision <hf_rev> --no-whiten

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import time
from pathlib import Path

_TOOLS = Path(__file__).resolve().parent
if str(_TOOLS) not in sys.path:
    sys.path.insert(0, str(_TOOLS))

from embed_runtime import (  # noqa: E402
    default_batch,
    load_embedder,
    resolve_backend,
    resolve_device,
    resolve_dtype,
    set_determinism,
    tune_cpu_threads,
)


def nfc_facts(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def write_f32(path: Path, X: list[list[float]]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    n, d = len(X), len(X[0])
    raw = struct.pack("<II", n, d)
    for row in X:
        raw += struct.pack(f"<{d}f", *row)
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser(description="Sidecar embedder Qwen3 MRL→dim (Apache-2.0)")
    ap.add_argument("--out", type=Path, default=Path("data"))
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument(
        "--batch",
        type=int,
        default=0,
        help="0=auto (CPU 16 / GPU 64)",
    )
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument(
        "--dtype",
        default="auto",
        choices=("auto", "fp32", "bf16", "fp16"),
        help="auto=fp32@CPU / bf16@CUDA",
    )
    ap.add_argument(
        "--device",
        default="auto",
        choices=("auto", "cpu", "cuda"),
        help="auto=cuda se houver; cuda sem GPU → exit 3 ABSTAIN",
    )
    ap.add_argument(
        "--no-fa2",
        action="store_true",
        help="desliga tentativa de flash_attention_2 no GPU",
    )
    ap.add_argument(
        "--backend",
        default="torch",
        choices=("auto", "torch", "onnx"),
        help="L3: onnx opt-in (medir vs torch antes de promover)",
    )
    ap.add_argument(
        "--facts",
        type=Path,
        default=None,
        help="facts.jsonl (padrão: --out/facts.jsonl)",
    )
    ap.add_argument(
        "--limit",
        type=int,
        default=0,
        help="se >0, só embute os primeiros N (probe A2)",
    )
    ap.add_argument(
        "--rate-out",
        type=Path,
        default=None,
        help="opcional: grava JSON de taxa (probe GPU/CPU)",
    )
    args = ap.parse_args()
    facts_path = args.facts or (args.out / "facts.jsonl")
    if not facts_path.exists():
        print(f"falta {facts_path} — gere com harvest --dry-run ou escriba", file=sys.stderr)
        return 2

    try:
        import sentence_transformers  # noqa: F401
    except ImportError:
        print(
            "sentence-transformers ausente. Instale:\n"
            "  pip install 'sentence-transformers>=2.7' 'transformers>=4.51'\n"
            "Pesos ficam no ambiente Python — nunca no crate AETHER.",
            file=sys.stderr,
        )
        return 3

    device = resolve_device(args.device)
    if device is None:
        print("ABSTAIN: --device cuda sem GPU neste host", file=sys.stderr)
        return 3

    dtype = resolve_dtype(device, args.dtype)
    backend = resolve_backend(args.backend)
    batch = args.batch if args.batch > 0 else default_batch(device)
    threads = tune_cpu_threads() if device == "cpu" else 0
    set_determinism(args.seed, throughput_gpu=(device == "cuda"))
    temperature = 0

    facts = nfc_facts(facts_path)
    if args.limit and args.limit > 0:
        facts = facts[: args.limit]
    labels = [f"{f['subject']}|{f['relation']}|{f['object']}" for f in facts]
    print(
        f"carregando {args.donor} truncate_dim={args.dim} device={device} "
        f"dtype={dtype} backend={backend} batch={batch} seed={args.seed} "
        f"temperature={temperature}…",
        flush=True,
    )
    t_load = time.perf_counter()
    try:
        model, model_kwargs = load_embedder(
            args.donor,
            dim=args.dim,
            device=device,
            dtype=dtype,
            prefer_fa2=not args.no_fa2,
            backend=backend,
        )
    except Exception as e:
        if backend == "onnx":
            print(f"ABSTAIN: backend onnx falhou: {e}", file=sys.stderr)
            return 3
        raise
    load_s = time.perf_counter() - t_load
    warm_n = min(batch, len(labels))
    if warm_n > 0:
        _ = model.encode(
            labels[:warm_n],
            batch_size=batch,
            show_progress_bar=False,
            normalize_embeddings=True,
        )
    t0 = time.perf_counter()
    emb = model.encode(
        labels,
        batch_size=batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    enc_s = time.perf_counter() - t0
    X = [list(map(float, row)) for row in emb]
    if len(X[0]) != args.dim:
        print(f"dim inesperada {len(X[0])} != {args.dim}", file=sys.stderr)
        return 4
    sha = write_f32(args.out / "emb" / "bank.f32", X)
    rate = len(X) / enc_s if enc_s > 0 else 0.0
    meta = {
        "donor": args.donor,
        "dim": args.dim,
        "count": len(X),
        "sha256_embeds": sha,
        "mrl": True,
        "license": "Apache-2.0",
        "seed": args.seed,
        "temperature": temperature,
        "device": device,
        "dtype": dtype,
        "backend": backend,
        "batch": batch,
        "model_kwargs": {k: str(v) for k, v in model_kwargs.items()},
        "torch_threads": threads,
        "load_s": round(load_s, 3),
        "encode_s": round(enc_s, 3),
        "texts_per_s": round(rate, 2),
        "opt": "L1_fp32_cpu_or_L2_bf16_gpu",
        "note": "sidecar B0 — harvest --jl-only; seed+T0=0473; ver EMBED_THROUGHPUT_ANALYSIS",
    }
    (args.out / "emb" / "donor_meta.json").write_text(
        json.dumps(meta, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    if args.rate_out:
        args.rate_out.parent.mkdir(parents=True, exist_ok=True)
        args.rate_out.write_text(json.dumps(meta, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, **meta}, ensure_ascii=False))
    print("próximo: python tools/harvest_transplant.py --jl-only --out", args.out, "--no-whiten")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
