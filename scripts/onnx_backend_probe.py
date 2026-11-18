#!/usr/bin/env python3
# R5 (L3) — probe do backend ONNX contra torch fp32 no CPU (MSG 0526, gates 0612/0613).
#
#   python scripts/onnx_backend_probe.py --facts data/rounds/20261117_b1/facts.jsonl --n 128
#
# Por que export manual: `SentenceTransformer(backend="onnx")` exige `optimum-onnx`,
# que pina `transformers<4.58`, enquanto o ST 6.x exige `transformers>=5` (conflito
# observado em 2026-11-18, MSG 0632). `transformers.onnx` (legado) foi removido no v5.
# Então: exportamos o encoder com `torch.onnx.export` (opset 17) e rodamos direto no
# onnxruntime — mesmo tokenizer, mesmo pooling, mesmo truncate_dim.
#
# Gates de promoção (contrato 0526/0613):
#   1. taxa_onnx >= taxa_torch_fp32 (mesma janela, mesmo host);
#   2. cosine medio por texto entre as saidas >= 0.99.
# Falhou qualquer um: ONNX continua opt-in (nao promove). ABSTAIN (exit 3) se o
# export/inferencia falhar — nunca derrubar o host.
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

OPSET = 17


def load_texts(path: Path, n: int) -> list[str]:
    texts: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        f = json.loads(line)
        texts.append(f"{f['subject']}|{f['relation']}|{f['object']}")
        if len(texts) >= n:
            break
    return texts


def export_encoder(st_model, out_path: Path) -> None:
    """Exporta o encoder HF (last_hidden_state) para ONNX com eixos dinamicos."""
    import torch

    transformer = st_model[0].auto_model
    transformer.eval()

    class _Enc(torch.nn.Module):
        def __init__(self, m):
            super().__init__()
            self.m = m

        def forward(self, input_ids, attention_mask):
            out = self.m(
                input_ids=input_ids, attention_mask=attention_mask, return_dict=True
            )
            return out.last_hidden_state

    enc = _Enc(transformer)
    dummy_ids = torch.ones((1, 8), dtype=torch.long)
    dummy_attn = torch.ones((1, 8), dtype=torch.long)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    torch.onnx.export(
        enc,
        (dummy_ids, dummy_attn),
        str(out_path),
        input_names=["input_ids", "attention_mask"],
        output_names=["last_hidden_state"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq"},
            "attention_mask": {0: "batch", 1: "seq"},
            "last_hidden_state": {0: "batch", 1: "seq"},
        },
        opset_version=OPSET,
        do_constant_folding=True,
        # torch>=2.9 defaulta no exporter dynamo, que exige `onnxscript` (dependência
        # nova no venv compartilhado). O legado TorchScript exporta o mesmo grafo.
        dynamo=False,
    )


def make_onnx_embedder(onnx_path: Path, st_model, dim: int, batch: int):
    """Fecha uma funcao texts -> embeddings L2-normalizadas (truncate_dim).

    O pooling NAO e reimplementado: usamos o modulo Pooling do proprio
    SentenceTransformer (Qwen3-Embedding = last-token, ver 1_Pooling/config.json),
    senao o cosine vs torch cai para ~0.70 (smoke 2026-11-18).
    """
    import numpy as np
    import onnxruntime as ort
    import torch

    tokenizer = st_model[0].tokenizer
    pooling = next(
        (m for m in st_model if m.__class__.__name__ == "Pooling"), None
    )
    if pooling is None:
        raise RuntimeError("SentenceTransformer sem modulo Pooling — ABSTAIN")

    # Mesmo orçamento de threads do caminho torch (tune_cpu_threads = n//2):
    # ORT default usa todos os lógicos e empurra para E-cores neste host híbrido.
    import os

    n_logical = os.cpu_count() or 4
    want_threads = max(2, n_logical // 2)
    env_threads = os.environ.get("AETHER_EMBED_THREADS", "").strip()
    if env_threads:
        want_threads = max(1, int(env_threads))
    so = ort.SessionOptions()
    so.intra_op_num_threads = want_threads

    sess = ort.InferenceSession(
        str(onnx_path), sess_options=so, providers=["CPUExecutionProvider"]
    )

    def embed(texts: list[str]) -> "object":
        outs = []
        for i in range(0, len(texts), batch):
            chunk = texts[i : i + batch]
            tok = tokenizer(
                chunk,
                padding=True,
                truncation=True,
                return_tensors="np",
            )
            hs = sess.run(
                ["last_hidden_state"],
                {
                    "input_ids": tok["input_ids"].astype("int64"),
                    "attention_mask": tok["attention_mask"].astype("int64"),
                },
            )[0]
            feats = {
                "token_embeddings": torch.from_numpy(hs),
                "attention_mask": torch.from_numpy(
                    tok["attention_mask"].astype("int64")
                ),
            }
            pooled = pooling(feats)["sentence_embedding"].numpy()
            pooled = pooled[:, :dim]  # truncate_dim (MRL), igual ao caminho ST
            norms = np.linalg.norm(pooled, axis=1, keepdims=True)
            outs.append(pooled / np.maximum(norms, 1e-12))
        return np.concatenate(outs, axis=0)

    return embed, sess


def cosine_rows(a, b) -> list[float]:
    import numpy as np

    a = np.asarray(a, dtype="float64")
    b = np.asarray(b, dtype="float64")
    if a.shape != b.shape:
        raise ValueError(f"shape divergente {a.shape} vs {b.shape} — nunca comparar")
    num = (a * b).sum(axis=1)
    den = np.maximum(
        np.linalg.norm(a, axis=1) * np.linalg.norm(b, axis=1), 1e-12
    )
    return list(num / den)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--n", type=int, default=128)
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--dim", type=int, default=768)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument(
        "--onnx-cache",
        type=Path,
        default=Path("target/pos_reboot/qwen3_encoder_768.onnx"),
    )
    ap.add_argument("--reexport", action="store_true")
    ap.add_argument("--contended", action="store_true")
    ap.add_argument("--out", type=Path, default=Path("target/pos_reboot/onnx_backend_probe.json"))
    args = ap.parse_args()

    try:
        import numpy as np  # noqa: F401
        import onnxruntime  # noqa: F401
        import torch  # noqa: F401
    except ImportError as e:
        print(f"ABSTAIN: dependencia ausente ({e})", file=sys.stderr)
        return 3

    if not args.facts.exists():
        print(f"ABSTAIN: falta {args.facts}", file=sys.stderr)
        return 3

    device = resolve_device("cpu")
    dtype = resolve_dtype(device, "fp32")
    set_determinism(args.seed, throughput_gpu=False)
    threads = tune_cpu_threads(None)

    texts = load_texts(args.facts, args.n)
    n = len(texts)
    if n < 32:
        print(f"ABSTAIN: poucos textos ({n})", file=sys.stderr)
        return 3

    # --- referencia: torch fp32 (mesmo caminho do A1/R3) ---
    t_load = time.perf_counter()
    torch_model, torch_meta = load_embedder(
        args.donor, dim=args.dim, device=device, dtype=dtype, prefer_fa2=False, backend="torch"
    )
    torch_load_s = time.perf_counter() - t_load

    _ = torch_model.encode(
        texts[: min(args.batch, n)],
        batch_size=args.batch,
        show_progress_bar=False,
        normalize_embeddings=True,
    )
    t0 = time.perf_counter()
    emb_t = torch_model.encode(
        texts,
        batch_size=args.batch,
        show_progress_bar=True,
        normalize_embeddings=True,
    )
    torch_s = time.perf_counter() - t0
    torch_rate = n / torch_s if torch_s > 0 else 0.0

    # --- ONNX: export (cacheado) + inferencia ---
    try:
        if args.reexport or not args.onnx_cache.exists():
            print(f"exportando ONNX -> {args.onnx_cache} (opset {OPSET})...", flush=True)
            export_encoder(torch_model, args.onnx_cache)
        onnx_embed, sess = make_onnx_embedder(
            args.onnx_cache, torch_model, args.dim, args.batch
        )
        _ = onnx_embed(texts[: min(args.batch, n)])  # warmup
        t0 = time.perf_counter()
        emb_o = onnx_embed(texts)
        onnx_s = time.perf_counter() - t0
    except Exception as e:  # noqa: BLE001 — contrato: ABSTAIN, nunca derrubar o host
        import traceback

        traceback.print_exc()
        print(f"ABSTAIN: export/inferencia ONNX falhou: {type(e).__name__}: {e}", file=sys.stderr)
        return 3
    onnx_rate = n / onnx_s if onnx_s > 0 else 0.0

    rows_t = emb_t.tolist() if hasattr(emb_t, "tolist") else [list(r) for r in emb_t]
    rows_o = emb_o.tolist() if hasattr(emb_o, "tolist") else [list(r) for r in emb_o]
    if len(rows_t) != len(rows_o):
        print(
            f"ABSTAIN: contagem divergente torch={len(rows_t)} onnx={len(rows_o)}",
            file=sys.stderr,
        )
        return 3

    cos = cosine_rows(rows_t, rows_o)
    cos_mean = float(sum(cos) / len(cos))
    cos_min = float(min(cos))

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
        "load_s": {"torch": round(torch_load_s, 3)},
        "encode_s": {"torch": round(torch_s, 3), "onnx": round(onnx_s, 3)},
        "texts_per_s": {"torch": round(torch_rate, 2), "onnx": round(onnx_rate, 2)},
        "ratio_onnx_vs_torch": round(onnx_rate / torch_rate, 3) if torch_rate else None,
        "cosine_mean": round(cos_mean, 5),
        "cosine_min": round(cos_min, 5),
        "gate_taxa_onnx_ge_torch": gate_taxa,
        "gate_cosine_ge_0.99": gate_cos,
        "promote_onnx": promote,
        "onnx_path": str(args.onnx_cache),
        "onnx_opset": OPSET,
        "onnx_providers": sess.get_providers(),
        "torch_meta": {k: str(v) for k, v in torch_meta.items()},
        "metodo": "torch.onnx.export_manual_sem_optimum",
        "ref": "MSG_0526_R5_0612_0613",
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
