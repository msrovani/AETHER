#!/usr/bin/env python3
"""Fatia bank AETHV1 em shards ≤ capacidade E1B (0,4×D → 4096 @ 10240).

Uso (um shard):
  python tools/hv_slice_shard.py \\
    --src data/rounds/20261117_b0_paws/hv \\
    --dst data/rounds/b2_shards/4k/hv \\
    --n 4096

Uso (plano completo 25×4k):
  python tools/hv_slice_shard.py \\
    --src data/rounds/b2_100k/hv \\
    --dst-root data/rounds/b2_shards/e1b \\
    --plan-all
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

MAGIC = b"AETHV1\0\0"
HEADER_SIZE = 128
E1B_FRAC = 0.4


def _paths(src: Path) -> tuple[Path, Path]:
    bin_p = src / "bank.bin"
    idx_p = src / "bank.idx"
    if not idx_p.exists():
        alt = bin_p.with_extension("idx")
        if alt.exists():
            idx_p = alt
    return bin_p, idx_p


def e1b_cap(dim: int) -> int:
    return int(E1B_FRAC * dim)


def shard_plan(n_atoms: int, dim: int) -> list[tuple[int, int]]:
    cap = max(1, e1b_cap(dim))
    out: list[tuple[int, int]] = []
    off = 0
    while off < n_atoms:
        n = min(cap, n_atoms - off)
        out.append((off, n))
        off += n
    return out


def slice_bank(src: Path, dst: Path, n: int, *, offset: int = 0) -> dict:
    bin_p, idx_p = _paths(src)
    raw = bin_p.read_bytes()
    idx = idx_p.read_bytes()
    if raw[:8] != MAGIC:
        raise SystemExit(f"magic inválido em {bin_p}")
    dim = struct.unpack_from("<I", raw, 12)[0]
    words = struct.unpack_from("<I", raw, 16)[0]
    count = struct.unpack_from("<Q", raw, 28)[0]
    if offset + n > count:
        raise SystemExit(f"offset+n={offset + n} > count={count}")
    atom_bytes = words * 8
    start = HEADER_SIZE + offset * atom_bytes
    payload = raw[start : start + n * atom_bytes]
    sha = hashlib.sha256(payload).digest()
    header = bytearray(raw[:HEADER_SIZE])
    struct.pack_into("<Q", header, 28, n)
    header[44:76] = sha
    dst.mkdir(parents=True, exist_ok=True)
    (dst / "bank.bin").write_bytes(bytes(header) + payload)
    (dst / "bank.idx").write_bytes(idx[offset * 24 : (offset + n) * 24])
    return {
        "src": str(src),
        "dst": str(dst),
        "dim": dim,
        "words": words,
        "src_count": count,
        "offset": offset,
        "n": n,
        "e1b_cap": e1b_cap(dim),
    }


def plan_all(src: Path, dst_root: Path) -> dict:
    bin_p, _ = _paths(src)
    raw = bin_p.read_bytes()
    if raw[:8] != MAGIC:
        raise SystemExit(f"magic inválido em {bin_p}")
    dim = struct.unpack_from("<I", raw, 12)[0]
    count = struct.unpack_from("<Q", raw, 28)[0]
    plan = shard_plan(count, dim)
    shards = []
    for i, (off, n) in enumerate(plan):
        dst = dst_root / f"shard_{i:02d}_n{n}" / "hv"
        meta = slice_bank(src, dst, n, offset=off)
        shards.append(meta)
    manifest = {
        "src": str(src),
        "dst_root": str(dst_root),
        "dim": dim,
        "src_count": count,
        "e1b_cap": e1b_cap(dim),
        "n_shards": len(shards),
        "shards": [
            {"i": i, "offset": s["offset"], "n": s["n"], "dst": s["dst"]}
            for i, s in enumerate(shards)
        ],
    }
    dst_root.mkdir(parents=True, exist_ok=True)
    (dst_root / "manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return manifest


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--src", type=Path, required=True, help="dir com bank.bin+bank.idx")
    ap.add_argument("--dst", type=Path, help="dir de saída hv/ (modo único)")
    ap.add_argument("--dst-root", type=Path, help="raiz dos shards (modo --plan-all)")
    ap.add_argument("--n", type=int, default=4096, help="átomos (default=4096 E1B)")
    ap.add_argument("--offset", type=int, default=0, help="offset no bank fonte")
    ap.add_argument(
        "--plan-all",
        action="store_true",
        help="emite todos os shards ≤ e1b_cap + manifest.json",
    )
    ap.add_argument(
        "--dry-plan",
        action="store_true",
        help="só imprime o plano (offset,n) sem escrever",
    )
    args = ap.parse_args()

    if args.dry_plan or args.plan_all:
        bin_p, _ = _paths(args.src)
        raw = bin_p.read_bytes()
        dim = struct.unpack_from("<I", raw, 12)[0]
        count = struct.unpack_from("<Q", raw, 28)[0]
        plan = shard_plan(count, dim)
        print({"dim": dim, "count": count, "e1b_cap": e1b_cap(dim), "plan": plan})
        if args.dry_plan:
            return 0
        if not args.dst_root:
            raise SystemExit("--plan-all exige --dst-root")
        manifest = plan_all(args.src, args.dst_root)
        print(manifest)
        return 0

    if not args.dst:
        raise SystemExit("--dst obrigatório no modo único")
    meta = slice_bank(args.src, args.dst, args.n, offset=args.offset)
    print(meta)
    if meta["n"] > meta["e1b_cap"]:
        print(f"AVISO: n={meta['n']} > e1b_cap={meta['e1b_cap']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
