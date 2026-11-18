#!/usr/bin/env python3
# B-G0 — três casos no bank real (Qwen3-Embedding) e recall@10 do HV.
# Não grava em data/. Lê uma rodada (--round).

from __future__ import annotations

import argparse
import json
import struct
from pathlib import Path

HEADER = 128
WORDS = 160


def load_f32(path: Path) -> list[list[float]]:
    raw = path.read_bytes()
    n, d = struct.unpack_from("<II", raw, 0)
    need = 8 + n * d * 4
    if len(raw) < need:
        raise SystemExit(f"bank.f32 curto: {len(raw)} < {need}")
    out = []
    off = 8
    for _ in range(n):
        row = list(struct.unpack_from(f"<{d}f", raw, off))
        off += d * 4
        out.append(row)
    return out


def dot(a: list[float], b: list[float]) -> float:
    return sum(x * y for x, y in zip(a, b))


def hamming(a: list[int], b: list[int]) -> int:
    return sum((x ^ y).bit_count() for x, y in zip(a, b))


def load_hv(path: Path) -> list[list[int]]:
    raw = path.read_bytes()
    count = struct.unpack_from("<Q", raw, 28)[0]
    words = struct.unpack_from("<I", raw, 16)[0]
    if words != WORDS:
        raise SystemExit(f"words={words}")
    rows = []
    off = HEADER
    for _ in range(count):
        row = list(struct.unpack_from(f"<{words}Q", raw, off))
        off += words * 8
        rows.append(row)
    return rows


def topk(scores: list[float], k: int) -> list[int]:
    order = sorted(range(len(scores)), key=lambda i: scores[i], reverse=True)
    return order[:k]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--round", type=Path, required=True)
    args = ap.parse_args()
    facts = [
        json.loads(line)
        for line in (args.round / "facts.jsonl").read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    labels = [f"{f['subject']}|{f['relation']}|{f['object']}" for f in facts]
    X = load_f32(args.round / "emb" / "bank.f32")
    if len(X) != len(labels):
        raise SystemExit(f"n emb={len(X)} facts={len(labels)}")

    def by_subject(name: str) -> int:
        for i, lab in enumerate(labels):
            if lab.startswith(name + "|"):
                return i
        raise SystemExit(f"sem rótulo {name}")

    gato = by_subject("gato")
    gata = by_subject("gata")
    para = by_subject("felino doméstico")
    um = by_subject("gat")

    import os

    os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
    from sentence_transformers import SentenceTransformer

    model = SentenceTransformer("Qwen/Qwen3-Embedding-0.6B", truncate_dim=768)
    queries = ["gato", "gata", "felino doméstico", "gat"]
    qv = model.encode(queries, normalize_embeddings=True)

    def ranked(qi: int, k: int, skip: str | None = None) -> list[str]:
        scores = [dot(list(map(float, qv[qi])), row) for row in X]
        order = sorted(range(len(scores)), key=lambda i: scores[i], reverse=True)
        out: list[str] = []
        for i in order:
            if skip and labels[i].startswith(skip):
                continue
            out.append(labels[i])
            if len(out) == k:
                break
        return out

    hit = [ranked(i, 1)[0] for i in range(3)]
    um_top = ranked(3, 10, "gat|")
    print("consulta_gato", hit[0])
    print("consulta_gata", hit[1])
    print("consulta_parafrase", hit[2])
    print("consulta_um_caractere", um_top[0])
    gato_ok = hit[0].startswith("gato|")
    gata_ok = hit[1].startswith("gata|")
    para_ok = hit[2].startswith("gato|") or hit[2].startswith("felino doméstico|")
    um_ok = any(label.startswith("gato|") for label in um_top)
    print("caso_gato_gata", gato_ok and gata_ok)
    print("caso_parafrase", para_ok)
    print("caso_um_caractere", um_ok)
    _ = (gato, gata, para, um)

    hv = args.round / "hv" / "bank.bin"
    if hv.exists():
        bits = load_hv(hv)
        n = min(len(bits), 100)
        hits = 0
        for i in range(n):
            dist = sorted((hamming(bits[i], bits[j]), j) for j in range(n))
            if any(j == i for _, j in dist[:10]):
                hits += 1
        recall = hits / n
        print(f"recall_at_10={recall:.3f}")
        if recall < 0.95:
            return 1
    if not (gato_ok and gata_ok and para_ok and um_ok):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
