#!/usr/bin/env python3
# L1b — sweep threads × batch (MSG 0502 OPENCODE).
#   python scripts/embed_threads_batch_sweep.py --facts data/rounds/20261117_b1/facts.jsonl
#
# Grava target/embed_threads_batch_sweep.json. Marca contended se --contended.

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
    load_embedder,
    resolve_device,
    resolve_dtype,
    set_determinism,
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--limit", type=int, default=256)
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--threads", default="8,10,16", help="lista CSV")
    ap.add_argument("--batches", default="16,32", help="lista CSV")
    ap.add_argument("--contended", action="store_true")
    ap.add_argument("--out", type=Path, default=Path("target/embed_threads_batch_sweep.json"))
    args = ap.parse_args()

    try:
        import torch
    except ImportError:
        print("ABSTAIN: torch ausente", file=sys.stderr)
        return 3

    device = resolve_device("cpu")
    if device != "cpu":
        print("sweep L1b é CPU-only", file=sys.stderr)
        return 2
    dtype = resolve_dtype("cpu", "fp32")
    set_determinism(args.seed, throughput_gpu=False)

    labels = []
    for line in args.facts.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        labels.append(f"{f['subject']}|{f['relation']}|{f['object']}")
        if len(labels) >= args.limit:
            break

    threads_list = [int(x) for x in args.threads.split(",") if x.strip()]
    batches_list = [int(x) for x in args.batches.split(",") if x.strip()]

    print(f"carregando modelo device=cpu dtype={dtype}…", flush=True)
    t_load = time.perf_counter()
    model, model_kwargs = load_embedder(
        args.donor, dim=args.dim, device="cpu", dtype=dtype, prefer_fa2=False
    )
    load_s = time.perf_counter() - t_load

    # interop só pode ser setado uma vez por processo.
    try:
        torch.set_num_interop_threads(1)
    except Exception:
        pass

    # Warmup global (1 lote @ batch 16).
    torch.set_num_threads(max(2, threads_list[0]))
    _ = model.encode(labels[:16], batch_size=16, show_progress_bar=False, normalize_embeddings=True)

    rows = []
    best = None
    for th in threads_list:
        torch.set_num_threads(th)
        for bs in batches_list:
            # mini-warmup por célula
            _ = model.encode(
                labels[: min(bs, len(labels))],
                batch_size=bs,
                show_progress_bar=False,
                normalize_embeddings=True,
            )
            t0 = time.perf_counter()
            _ = model.encode(
                labels,
                batch_size=bs,
                show_progress_bar=False,
                normalize_embeddings=True,
            )
            enc = time.perf_counter() - t0
            rate = len(labels) / enc if enc > 0 else 0.0
            row = {
                "threads": th,
                "torch_get_num_threads": torch.get_num_threads(),
                "batch": bs,
                "n": len(labels),
                "encode_s": round(enc, 3),
                "texts_per_s": round(rate, 2),
                "eta_100k_h": round((100_000 / rate) / 3600, 2) if rate > 0 else None,
            }
            rows.append(row)
            print(json.dumps(row), flush=True)
            if best is None or row["texts_per_s"] > best["texts_per_s"]:
                best = row

    baseline = next((r for r in rows if r["threads"] == 8 and r["batch"] == 16), rows[0])
    gain = None
    if best and baseline and baseline["texts_per_s"] > 0:
        gain = round((best["texts_per_s"] / baseline["texts_per_s"] - 1.0) * 100, 1)

    art = {
        "probe": "L1b_threads_batch_0502",
        "donor": args.donor,
        "device": "cpu",
        "dtype": dtype,
        "seed": args.seed,
        "temperature": 0,
        "contended": bool(args.contended),
        "load_s": round(load_s, 3),
        "cpu_count_logical": __import__("os").cpu_count(),
        "model_kwargs": {k: str(v) for k, v in model_kwargs.items()},
        "rows": rows,
        "best": best,
        "baseline_8x16": baseline,
        "best_vs_baseline_pct": gain,
        "promote_if_ge_20pct": bool(gain is not None and gain >= 20.0),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, "best": best, "gain_pct": gain}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
