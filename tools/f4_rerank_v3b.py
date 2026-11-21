#!/usr/bin/env python3
"""V3-B — um rerank, set v2 congelado, gate inalterado (MSG 1088).

Base: BAAI/bge-m3 denso. Reranker: BAAI/bge-reranker-v2-m3.
Pool 50, max_length 512 e batch 4 ficam fixos antes do placar.
Não altera o dataset nem data/embed_eval_last.json.
"""

from __future__ import annotations

import gc
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SET = ROOT / "data" / "embed_eval_set_v2.jsonl"
SHA = "c422a1a00fc18c9335b437f507f8bf250f8df2a1689f3ef2f2259c7fad0f3397"
OUT = ROOT / "target" / "embed_eval_bge_rerank_v2.json"
EMBED_ID = "BAAI/bge-m3"
RERANK_ID = "BAAI/bge-reranker-v2-m3"
POOL = 50
MAX_LENGTH = 512
BATCH = 4

os.environ["AETHER_EVAL_SET"] = str(SET)
os.environ["AETHER_EMBED_CUDA"] = "1"
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
sys.path.insert(0, str(ROOT / "tools"))

import embed_eval  # noqa: E402
import embed_server  # noqa: E402


def dense_ranks(docs: list[dict], queries: list[dict]) -> tuple[list[list[int]], float]:
    os.environ["HF_HUB_OFFLINE"] = "1"
    import huggingface_hub.constants as hub

    hub.HF_HUB_OFFLINE = True
    t0 = time.time()
    embed_server.State.model = embed_server.load_model(EMBED_ID, allow_download=False)
    texts = [d["text"] for d in docs]
    mat = embed_server.embed(texts)
    qv = embed_server.embed([q["q"] for q in queries])
    elapsed = time.time() - t0
    if len(mat) != len(docs) or len(qv) != len(queries):
        raise SystemExit("embedder devolveu contagem errada")
    ranks = [embed_eval.cosine_rank(q, mat) for q in qv]
    drop_embedder()
    return ranks, elapsed


def drop_embedder() -> None:
    embed_server.State.model = None
    gc.collect()
    try:
        import torch

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass


def allow_download() -> None:
    """HF trata qualquer string não vazia como offline, inclusive '0'."""
    for key in ("HF_HUB_OFFLINE", "TRANSFORMERS_OFFLINE", "HF_DATASETS_OFFLINE"):
        os.environ.pop(key, None)
    import huggingface_hub.constants as hub

    hub.HF_HUB_OFFLINE = False


def rerank(docs: list[dict], queries: list[dict], dense: list[list[int]]) -> tuple[list[list[int]], float]:
    allow_download()
    from sentence_transformers import CrossEncoder

    t0 = time.time()
    ce = CrossEncoder(
        RERANK_ID,
        device="cuda",
        max_length=MAX_LENGTH,
        model_kwargs={"dtype": "bfloat16"},
    )
    out: list[list[int]] = []
    for q, rank in zip(queries, dense):
        pool = rank[:POOL]
        pairs = [(q["q"], docs[i]["text"]) for i in pool]
        scores = ce.predict(pairs, batch_size=BATCH, show_progress_bar=False)
        order = sorted(range(len(pool)), key=lambda j: (-float(scores[j]), pool[j]))
        out.append([pool[j] for j in order])
    elapsed = time.time() - t0
    del ce
    drop_embedder()
    return out, elapsed


def main(argv: list[str] | None = None) -> int:
    global RERANK_ID, OUT
    args = argv if argv is not None else sys.argv[1:]
    if len(args) >= 1:
        RERANK_ID = args[0]
    if len(args) >= 2:
        OUT = Path(args[1])
        if not OUT.is_absolute():
            OUT = ROOT / OUT
    _meta, docs, queries = embed_eval.load_set(SHA)
    print(
        f"set ok {len(docs)} docs {len(queries)} consultas · rerank={RERANK_ID}",
        flush=True,
    )
    dense, t_dense = dense_ranks(docs, queries)
    dense_score = embed_eval.score(queries, dense, docs)
    print(f"denso hit5={dense_score['hit5']} em {t_dense:.1f}s", flush=True)
    reranked, t_rr = rerank(docs, queries, dense)
    rr_score = embed_eval.score(queries, reranked, docs)
    bm25 = embed_eval.BM25(docs)
    lex_score = embed_eval.score(queries, [bm25.rank(q["q"]) for q in queries], docs)
    gate = embed_eval.GATE_HIT5
    # O gate da cadeira é o hit@5 do sistema medido. O denso fica no artefato
    # como base; quem precisa chegar a 0.974 é o rerank.
    passed = rr_score["hit5"] >= gate
    result = {
        "sha256_set": SHA,
        "n_docs": len(docs),
        "n_queries": len(queries),
        "model_embed": EMBED_ID,
        "model_rerank": RERANK_ID,
        "rerank_pool": POOL,
        "max_length": MAX_LENGTH,
        "batch": BATCH,
        "latency": {"dense_s": round(t_dense, 2), "rerank_s": round(t_rr, 2)},
        "systems": {
            "lexical_bm25": lex_score,
            "embed_minilm": dense_score,
            "rerank": rr_score,
        },
        "gate": {
            "hit5_min": gate,
            "embed_pass": dense_score["hit5"] >= gate,
            "rerank_pass": rr_score["hit5"] >= gate,
            "hybrid_pass": False,
            "both_pass": passed,
        },
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(
        f"RERANK hit5={rr_score['hit5']} gate={gate} "
        f"{'PASSOU' if rr_score['hit5'] >= gate else 'REPROVADO'} -> {OUT.name}",
        flush=True,
    )
    return 0 if passed else 2


if __name__ == "__main__":
    sys.exit(main())
