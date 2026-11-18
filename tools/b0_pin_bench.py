#!/usr/bin/env python3
# Bench de afinidade — P-cores vs default (pedido do humano, 18/11).
#
# Um processo, modelo carregado UMA vez, três configurações de
# threads/afinidade medindo embed de N textos cada:
#   1. baseline: 8 threads, sem pin (o padrão atual do embed_runtime)
#   2. pin 0xFFF: 12 threads nos primeiros 12 lógicos (P-cores do 240H)
#   3. 16 threads, sem pin (todos os lógicos)
#
# Afinidade via SetProcessAffinityMask (kernel32, ctypes) — mesmo papel do
# core_topology.rs do governador, mas para o sidecar Python.
#
#   python tools/b0_pin_bench.py --n 64
# Saída: JSON por config em stdout.

from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import json
import sys
import time
from pathlib import Path

kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
kernel32.SetProcessAffinityMask.argtypes = [wt.HANDLE, ctypes.c_size_t]
kernel32.SetProcessAffinityMask.restype = wt.BOOL


def cpu_efficiency_masks() -> dict[str, int]:
    """Máscaras P/E via GetLogicalProcessorInformationEx (layout empírico:
    Relationship@0 (Core=0), Size@4, EffClass@9, GroupCount@30, Mask@32/16B —
    igual ao core_topology.rs do governador)."""
    size = wt.ULONG(0)
    kernel32.GetLogicalProcessorInformationEx(0, None, ctypes.byref(size))
    buf = (ctypes.c_ubyte * size.value)()
    p_mask = 0
    e_mask = 0
    if kernel32.GetLogicalProcessorInformationEx(0, buf, ctypes.byref(size)):
        base = ctypes.addressof(buf)
        off = 0
        end = size.value
        while off < end:
            rel = int.from_bytes(buf[off : off + 4], "little")
            sz = int.from_bytes(buf[off + 4 : off + 8], "little")
            if sz == 0:
                break
            if rel == 0:  # RelationProcessorCore
                eff = buf[off + 9]
                groups = int.from_bytes(buf[off + 30 : off + 32], "little")
                logical: list[int] = []
                for g in range(groups):
                    mask = int.from_bytes(buf[off + 32 + 16 * g : off + 40 + 16 * g], "little")
                    for bit in range(64):
                        if (mask >> bit) & 1 == 1:
                            logical.append(bit)
                if eff == 0:
                    e_mask |= sum(1 << l for l in logical)
                else:
                    p_mask |= sum(1 << l for l in logical)
            off += sz
    return {"p": p_mask, "e": e_mask}


def set_affinity(mask: int | None) -> int:
    """Aplica máscara de afinidade ao processo; devolve a máscara efetiva."""
    h = kernel32.GetCurrentProcess()
    if mask is None:
        # limpa o pin: permite todos os lógicos do sistema
        c = wt.ULONG()
        kernel32.GetProcessAffinityMask(h, ctypes.byref(c), ctypes.byref(wt.ULONG()))
        mask = c.value or 0xFFFF
    ok = kernel32.SetProcessAffinityMask(h, ctypes.c_size_t(mask))
    return mask if ok else -1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=64)
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    import torch
    from sentence_transformers import SentenceTransformer

    masks = cpu_efficiency_masks()
    print(
        json.dumps(
            {
                "topologia": {
                    "p_mask": hex(masks["p"]),
                    "e_mask": hex(masks["e"]),
                }
            }
        ),
        flush=True,
    )

    texts = [f"texto de benchmark {i} sobre o tema {i % 7} com valor {i * 3}" for i in range(args.n)]

    resultados = []
    p_mask = masks["p"]
    configs = [
        ("baseline_8_threads", 8, None),
        ("pin_P_cores", min(16, bin(p_mask).count("1") * 2), p_mask),
        ("threads_16_sempin", 16, None),
    ]
    # dtype IMPORTA: bf16 na CPU é ~29x mais lento que fp32 (MSG 0504/0510).
    model = SentenceTransformer(
        args.donor,
        truncate_dim=args.dim,
        device="cpu",
        model_kwargs={"dtype": torch.float32},
    )
    model.warmup() if hasattr(model, "warmup") else None
    _ = model.encode(texts[:16], batch_size=16, normalize_embeddings=True)  # warmup

    for nome, threads, mask in configs:
        torch.set_num_threads(threads)
        eff = set_affinity(mask)
        t0 = time.perf_counter()
        _ = model.encode(texts, batch_size=16, normalize_embeddings=True)
        dt = time.perf_counter() - t0
        rate = args.n / dt
        linha = {
            "config": nome,
            "threads": threads,
            "mask": mask if mask is not None else "none",
            "affinity_efetiva": eff,
            "torch_num_threads": torch.get_num_threads(),
            "s": round(dt, 2),
            "textos_por_s": round(rate, 2),
        }
        resultados.append(linha)
        print(json.dumps(linha), flush=True)

    out = Path("target/b0_pin_bench.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(resultados, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"artefato={out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
