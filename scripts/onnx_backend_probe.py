#!/usr/bin/env python3
# R5 (L3) — probe do backend ONNX vs torch fp32 no CPU (MSG 0526, gate MSG 0502).
#
#   python scripts/onnx_backend_probe.py --facts data/rounds/20261117_b1/facts.jsonl --n 128
#
# Gates de promoção (contrato em tools/embed_runtime.py::load_embedder):
#   1. taxa_onnx >= taxa_torch_fp32 (mesmo host, mesma janela);
#   2. cosine medio por texto entre as duas saidas >= 0.99.
# Se qualquer gate falhar: ONNX continua opt-in (não promove). ABSTAIN (exit 3)
# se o backend onnx não carregar.
#
# Artefato: target/pos_reboot/onnx_backend_probe.json (seed 42, temperatura 0).

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
    tune_cpu_threads,
)


def cosine_rows(a, b) -> list[float]:
    """Cosine por linha entre duas matrizes (numpy-like, sem depender de numpy)."""
    out = []
    for ra, rb in zip(a, b):
        dot = float(sum(x * y for x, y in zip(ra, rb)))
        na = float(sum(x * x for x in ra)) ** 0.5
        nb = float(sum(y * y for y in rb)) ** 0.5
        out.append(dot / (na * nb) if na > 0 and nb > 0 else 0.0)
    return out


def timed_encode(model, texts, batch: int) -> tuple[object, float, object]:
    warm = model.encode(
        texts[: min(batch, len(texts))],
        batch_size=batch,
        show_progress_bar=False,
        normalize_embeddings=True,
    )
    t0 = time.perf_counter()
    emb = model.encode(
        texts,
        batch_size=batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    return emb, time.perf_counter() - t0, warm  # type: ignore[return-value]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--n", type=int, default=128)
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--contended", action="store_true")
    ap.add_argument("--out", type=Path, default=Path("target/pos_reboot/onnx_backend_probe.json"))
    args = ap.parse_args()

    try:
        import torch  # noqa: F401
        import onnxruntime  # noqa: F401
    except ImportError as e:
        print(f"ABSTAIN: dependência ausente ({e}) — pip install onnxruntime", file=sys.stderr)
        return 3

    if not args.facts.exists():
        print(f"ABSTAIN: falta {args.facts}", file=sys.stderr)
        return 3

    device = resolve_device("cpu")
    dtype = resolve_dtype(device, "fp32")
    set_determinism(args.seed, throughput_gpu=False)
    threads = tune_cpu_threads(None)

    texts: list[str] = []
    for line in args.facts.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        texts.append(f"{f['subject']}|{f['relation']}|{f['object']}")
        if len(texts) >= args.n:
            break
    n = len(texts)
    if n < 32:
        print(f"ABSTAIN: poucos textos ({n})", file=sys.stderr)
        return 3

    # --- torch fp32 (referência) ---
    t_load = time.perf_counter()
    torch_model, torch_meta = load_embedder(
        args.donor, dim=args.dim, device=device, dtype=dtype, prefer_fa2=False, backend="torch"
    )
    torch_load_s = time.perf_counter() - t_load
    emb_t, torch_s, _ = timed_encode(torch_model, texts, args.batch)
    torch_rate = n / torch_s if torch_s > 0 else 0.0

    # --- onnx (opt-in) ---
    try:
        t_load = time.perf_counter()
        onnx_model, onnx_meta = load_embedder(
            args.donor, dim=args.dim, device=device, dtype=dtype, prefer_fa2=False, backend="onnx"
        )
        onnx_load_s = time.perf_counter() - t_load
        emb_o, onnx_s, _ = timed_encode(onnx_model, texts, args.batch)
    except Exception as e:  # noqa: BLE001 — contrato: ABSTAIN, nunca derrubar o host
        print(f"ABSTAIN: backend onnx falhou: {e}", file=sys.stderr)
        return 3
    onnx_rate = n / onnx_s if onnx_s > 0 else 0.0

    rows_t = emb_t.tolist() if hasattr(emb_t, "tolist") else [list(r) for r in emb_t]
    rows_o = emb_o.tolist() if hasattr(emb_o, "tolist") else [list(r) for r in emb_o]
    if len(rows_t) != len(rows_o):
        print(
            f"ABSTAIN: contagem divergente torch={len(rows_t)} onnx={len(rows_o)} — nunca zip silencioso",
            file=sys.stderr,
        )
        return 3

    cos = cosine_rows(rows_t, rows_o)
    cos_mean = sum(cos) / len(cos)
    cos_min = min(cos)

    gate_taxa = onnx_rate >= torch_rate
    gate_cos = cos_mean >= 0.99
    promote = gate_taxa and gate_cos

    art = {
        "donor": args.donor,
        "n": n,
        "batch": args.batch,
        "dim": args.dim,
        "seed": args.seed,
        "temperature": 0,
        "device": "cpu",
        "dtype_torch": dtype,
        "torch_threads": threads,
        "contended": bool(args.contended),
        "load_s": {"torch": round(torch_load_s, 3), "onnx": round(onnx_load_s, 3)},
        "encode_s": {"torch": round(torch_s, 3), "onnx": round(onnx_s, 3)},
        "texts_per_s": {"torch": round(torch_rate, 2), "onnx": round(onnx_rate, 2)},
        "ratio_onnx_vs_torch": round(onnx_rate / torch_rate, 3) if torch_rate else None,
        "cosine_mean": round(cos_mean, 5),
        "cosine_min": round(cos_min, 5),
        "gate_taxa_onnx_ge_torch": gate_taxa,
        "gate_cosine_ge_0.99": gate_cos,
        "promote_onnx": promote,
        "torch_meta": {k: str(v) for k, v in torch_meta.items()},
        "onnx_meta": {k: str(v) for k, v in onnx_meta.items()},
        "ref": "MSG_0526_R5",
        "created": "2026-11-18",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(art, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(art, indent=2))
    if not promote:
        print(
            "R5: ONNX NAO promovido (gate taxa ou cosine) — continua opt-in",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
