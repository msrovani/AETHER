#!/usr/bin/env python3
# Ponte mínima texto→papéis S/V/O/M (hash) para B-G0′ / E1.
# Não é LanguagePort fino — só materializa o contrato do gate Rust.
#
#   python tools/b0_paws_to_roles.py \
#     --facts data/rounds/20261117_b0_paws/facts.jsonl \
#     --out data/rounds/20261117_b0_paws/roles.jsonl --limit 2000

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

N_ENT, N_VERB, N_MOD = 64, 16, 8


def fnv1a(s: str) -> int:
    h = 0xCBF29CE484222325
    for b in s.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def tokenize(text: str) -> list[str]:
    return [t.lower() for t in re.split(r"[^A-Za-z0-9]+", text) if t]


def roles_ordered(toks: list[str]) -> list[int]:
    if not toks:
        return [0, 0, 0, 0]
    s = fnv1a(toks[0]) % N_ENT
    o = fnv1a(toks[-1]) % N_ENT
    v = fnv1a(toks[len(toks) // 2]) % N_VERB
    m = fnv1a(f"{toks[0]}|{toks[-1]}") % N_MOD
    return [s, v, o, m]


def roles_bag(toks: list[str]) -> list[int]:
    if not toks:
        return [0, 0, 0, 0]
    hs = sorted(fnv1a(t) for t in toks)
    s = hs[0] % N_ENT
    o = hs[-1] % N_ENT
    v = hs[len(hs) // 2] % N_VERB
    acc = 0
    for h in hs:
        acc ^= ((h << 13) | (h >> 51)) & 0xFFFFFFFFFFFFFFFF
    m = acc % N_MOD
    return [s, v, o, m]


def main() -> int:
    ap = argparse.ArgumentParser(description="PAWS facts → role ids (B-G0′)")
    ap.add_argument("--facts", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--limit", type=int, default=2000)
    args = ap.parse_args()

    n = 0
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.facts.open(encoding="utf-8") as fin, args.out.open(
        "w", encoding="utf-8"
    ) as fout:
        for line in fin:
            if n >= args.limit:
                break
            if not line.strip():
                continue
            row = json.loads(line)
            t1 = tokenize(row.get("subject", ""))
            t2 = tokenize(row.get("object", ""))
            out = {
                "i": n,
                "label": row.get("relation"),
                "s1_ord": roles_ordered(t1),
                "s2_ord": roles_ordered(t2),
                "s1_bag": roles_bag(t1),
                "s2_bag": roles_bag(t2),
            }
            fout.write(json.dumps(out, ensure_ascii=False) + "\n")
            n += 1
    print(f"OK roles={n} → {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
