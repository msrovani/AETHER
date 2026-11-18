#!/usr/bin/env python3
# Mede textos/s do donor (A1) — CPU fp32/batch16 · GPU bf16+FA2/batch64.
# Contrato 0473/0476: seed fixo + temperatura=0. Ver docs/EMBED_THROUGHPUT_ANALYSIS.md.
#
#   python scripts/embed_rate_1k.py --facts data/rounds/20261117_b1/facts.jsonl --limit 1000
#
# ABSTAIN (exit 3) se sentence-transformers ausente. Grava target/embed_rate_1k.json.

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

_ROOT = Path(__file__).resolve().parents[1]
if str(_ROOT / "tools") not in sys.path:
    sys.path.insert(0, str(_ROOT / "tools"))

from embed_runtime import (  # noqa: E402
    default_batch,
    load_embedder,
    resolve_backend,
    resolve_device,
    resolve_dtype,
    set_determinism,
    tune_cpu_threads,
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--limit", type=int, default=1000)
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
        help="auto=cuda se disponível, senão cpu",
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
    ap.add_argument("--out", type=Path, default=Path("target/embed_rate_1k.json"))
    ap.add_argument(
        "--contended",
        action="store_true",
        help="marca artefato como medido sob contenção (não limpo)",
    )
    args = ap.parse_args()

    try:
        import sentence_transformers  # noqa: F401
    except ImportError:
        print("ABSTAIN: sentence-transformers ausente", file=sys.stderr)
        return 3

    device = resolve_device(args.device)
    if device is None:
        print("ABSTAIN: --device cuda sem GPU", file=sys.stderr)
        return 3

    dtype = resolve_dtype(device, args.dtype)
    backend = resolve_backend(args.backend)
    batch = args.batch if args.batch > 0 else default_batch(device)
    threads = tune_cpu_threads() if device == "cpu" else 0
    set_determinism(args.seed, throughput_gpu=(device == "cuda"))

    labels = []
    for line in args.facts.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        labels.append(f"{f['subject']}|{f['relation']}|{f['object']}")
        if len(labels) >= args.limit:
            break

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

    # Warmup: 1º lote fria alocação/kernels (senão p99 do A1 fica pessimista).
    warm_n = min(batch, len(labels))
    if warm_n > 0:
        _ = model.encode(
            labels[:warm_n],
            batch_size=batch,
            show_progress_bar=False,
            normalize_embeddings=True,
        )

    t0 = time.perf_counter()
    _ = model.encode(
        labels,
        batch_size=batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    enc_s = time.perf_counter() - t0
    rate = len(labels) / enc_s if enc_s > 0 else 0.0
    eta_100k_h = (100_000 / rate) / 3600.0 if rate > 0 else None
    art = {
        "donor": args.donor,
        "n": len(labels),
        "batch": batch,
        "dim": args.dim,
        "seed": args.seed,
        "temperature": 0,
        "device": device,
        "dtype": dtype,
        "backend": backend,
        "model_kwargs": {k: str(v) for k, v in model_kwargs.items()},
        "torch_threads": threads,
        "contended": bool(args.contended),
        "load_s": round(load_s, 3),
        "encode_s": round(enc_s, 3),
        "texts_per_s": round(rate, 2),
        "eta_100k_hours_naive": None if eta_100k_h is None else round(eta_100k_h, 2),
        "decision_hint": (
            "CPU ok (<4h @100k)"
            if eta_100k_h is not None and eta_100k_h < 4
            else "considerar batch GPU do donor com fallback CPU"
            if eta_100k_h is not None
            else "sem taxa"
        ),
        "gate_a1": "limpo" if not args.contended else "contendido_abstain_para_decisao",
        "opt": "L1_fp32_cpu_or_L2_bf16_gpu",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
