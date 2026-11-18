#!/usr/bin/env python3
# Runtime compartilhado do sidecar embed (CPU/GPU) — docs/EMBED_THROUGHPUT_ANALYSIS.md
# CPU: float32 + batch 16 (bf16 no CPU é ~6–9× mais lento).
# GPU: bfloat16 + FlashAttention-2 se disponível + batch 64.

from __future__ import annotations

import os
import random
from typing import Any


def set_determinism(seed: int, *, throughput_gpu: bool = False) -> None:
    """Seed + T=0. Em GPU throughput, libera cudnn.benchmark (ainda com seed)."""
    os.environ.setdefault("PYTHONHASHSEED", str(seed))
    os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    random.seed(seed)
    try:
        import numpy as np

        np.random.seed(seed % (2**32 - 1))
    except ImportError:
        pass
    try:
        import torch

        torch.manual_seed(seed)
        if torch.cuda.is_available():
            torch.cuda.manual_seed_all(seed)
        if throughput_gpu and torch.cuda.is_available():
            torch.backends.cudnn.deterministic = False
            torch.backends.cudnn.benchmark = True
        else:
            try:
                torch.use_deterministic_algorithms(True, warn_only=True)
            except Exception:
                pass
            torch.backends.cudnn.deterministic = True
            torch.backends.cudnn.benchmark = False
    except ImportError:
        pass


def resolve_device(want: str) -> str | None:
    if want == "cpu":
        return "cpu"
    try:
        import torch
    except ImportError:
        return None if want == "cuda" else "cpu"
    if want == "cuda":
        return "cuda" if torch.cuda.is_available() else None
    return "cuda" if torch.cuda.is_available() else "cpu"


def default_batch(device: str) -> int:
    return 64 if device == "cuda" else 16


def tune_cpu_threads() -> int:
    """Usa núcleos físicos quando possível; evita oversubscribe."""
    try:
        import torch

        n = os.cpu_count() or 4
        # Heurística: metade dos lógicos ≈ físicos em HT; mín 2.
        threads = max(2, n // 2)
        torch.set_num_threads(threads)
        try:
            torch.set_num_interop_threads(1)
        except Exception:
            pass
        return threads
    except ImportError:
        return 0


def resolve_dtype(device: str, want: str) -> str:
    """want: auto|fp32|bf16|fp16 → dtype canônico."""
    want = want.lower()
    if want == "auto":
        return "bf16" if device == "cuda" else "fp32"
    if want in ("fp32", "float32"):
        return "fp32"
    if want in ("bf16", "bfloat16"):
        return "bf16"
    if want in ("fp16", "float16", "half"):
        return "fp16"
    raise ValueError(f"dtype desconhecido: {want}")


def torch_dtype(dtype: str):
    import torch

    return {
        "fp32": torch.float32,
        "bf16": torch.bfloat16,
        "fp16": torch.float16,
    }[dtype]


def flash_attn_available() -> bool:
    try:
        import flash_attn  # noqa: F401

        return True
    except ImportError:
        pass
    try:
        import kernels  # noqa: F401

        return True
    except ImportError:
        return False


def build_model_kwargs(device: str, dtype: str, *, prefer_fa2: bool = True) -> dict[str, Any]:
    import torch

    kwargs: dict[str, Any] = {"torch_dtype": torch_dtype(dtype)}
    if device == "cuda" and prefer_fa2 and flash_attn_available():
        kwargs["attn_implementation"] = "flash_attention_2"
    return kwargs


def resolve_backend(want: str) -> str:
    """want: auto|torch|onnx → canônico. auto=torch (ONNX é opt-in medido)."""
    want = (want or "auto").lower()
    if want in ("auto", "torch", "pt", "pytorch"):
        return "torch"
    if want == "onnx":
        return "onnx"
    raise ValueError(f"backend desconhecido: {want}")


def load_embedder(
    donor: str,
    *,
    dim: int,
    device: str,
    dtype: str,
    prefer_fa2: bool = True,
    backend: str = "torch",
):
    """Carrega SentenceTransformer com kwargs otimizados por device/backend.

    backend=onnx (L3): requer `pip install 'sentence-transformers[onnx]'`.
    Gate: só promover se taxa ≥ torch fp32 e cosine médio ≥0,99 no probe.
    """
    from sentence_transformers import SentenceTransformer

    backend = resolve_backend(backend)
    meta: dict[str, Any] = {"backend": backend}
    if backend == "onnx":
        # ONNX Runtime: dtype via provider; model_kwargs torch_dtype não se aplica.
        provider = "CUDAExecutionProvider" if device == "cuda" else "CPUExecutionProvider"
        model = SentenceTransformer(
            donor,
            truncate_dim=dim,
            device=device,
            backend="onnx",
            model_kwargs={"provider": provider},
        )
        meta["provider"] = provider
        meta["dtype"] = "onnx_runtime"
        return model, meta

    model_kwargs = build_model_kwargs(device, dtype, prefer_fa2=prefer_fa2)
    meta.update({k: str(v) for k, v in model_kwargs.items()})
    model = SentenceTransformer(
        donor,
        truncate_dim=dim,
        device=device,
        model_kwargs=model_kwargs,
    )
    return model, model_kwargs
