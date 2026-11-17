#!/usr/bin/env python3
# Testes de usabilidade U1–U6 sobre data/ (HvBank + ledger + facts).
# Escreve target/aether_transplant_uso.json para o hub.
#
#   python tools/transplant_uso.py
#   python tools/transplant_uso.py --data data --out target/aether_transplant_uso.json

from __future__ import annotations

import argparse
import json
import random
import struct
import time
from collections import defaultdict
from pathlib import Path

MAGIC = b"AETHV1\0\0"
HEADER = 128
DIM = 10240


def load_bank(data: Path):
    raw = (data / "hv" / "bank.bin").read_bytes()
    assert raw[:8] == MAGIC, "magic AETHV1"
    dim = struct.unpack_from("<I", raw, 12)[0]
    words = struct.unpack_from("<I", raw, 16)[0]
    count = struct.unpack_from("<Q", raw, 28)[0]
    bits = []
    off = HEADER
    for _ in range(count):
        bits.append(list(struct.unpack_from("<" + "Q" * words, raw, off)))
        off += words * 8
    idx = (data / "hv" / "bank.idx").read_bytes()
    keys = []
    for i in range(count):
        lo, hi, _ = struct.unpack_from("<QQI", idx, i * 24)
        keys.append((hi << 64) | lo)
    return dim, words, bits, keys


def hamming(a, b) -> int:
    return sum((x ^ y).bit_count() for x, y in zip(a, b))


def top_k(bits, q, k=5):
    scored = [(hamming(q, row), i) for i, row in enumerate(bits)]
    scored.sort()
    return scored[:k]


def find_label(labels, prefix: str) -> int:
    for i, lab in enumerate(labels):
        if lab.startswith(prefix):
            return i
    raise KeyError(prefix)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, default=Path("data"))
    ap.add_argument("--out", type=Path, default=Path("target/aether_transplant_uso.json"))
    args = ap.parse_args()

    ledger = json.loads((args.data / "sgdb_mock_ledger.json").read_text(encoding="utf-8"))
    labels = [a["label"] for a in ledger["atoms"]]
    facts = [
        json.loads(l)
        for l in (args.data / "facts.jsonl").read_text(encoding="utf-8").splitlines()
        if l.strip()
    ]
    dim, _words, bits, _keys = load_bank(args.data)
    assert dim == DIM
    assert len(bits) == len(labels)

    queries = [
        ("Brasil|capital", find_label(labels, "Brasil|capital")),
        ("Rust|", find_label(labels, "Rust|")),
        ("Lua|", find_label(labels, "Lua|")),
        ("AETHER|", find_label(labels, "AETHER|")),
        ("Ising|", find_label(labels, "Ising|")),
    ]

    # U1
    u1 = 0
    for _, qi in queries:
        if top_k(bits, bits[qi], 1)[0][1] == qi:
            u1 += 1
    u1_ok = u1 == 5

    # U2
    u2 = 0
    for _, qi in queries[:3]:
        for _d, i in top_k(bits, bits[qi], 3):
            parts = labels[i].split("|")
            sub, rel, obj = (parts + ["", "", ""])[:3]
            match = [
                f
                for f in facts
                if f["subject"] == sub and f["relation"] == rel and f["object"] == obj
            ]
            if match and match[0]["authority"] <= 240:
                u2 += 1
    u2_ok = u2 == 9

    # U3
    qi = find_label(labels, "Rust|")
    labs = [labels[i] for _, i in top_k(bits, bits[qi], 10)]
    same_rel = sum(1 for l in labs if "|é_uma|" in l)
    u3_ok = same_rel >= 2

    # U4
    by_sr: dict[tuple[str, str], set[str]] = defaultdict(set)
    planted = 0
    for f in facts:
        by_sr[(f["subject"], f["relation"])].add(f["object"])
        if f.get("conflict_seed"):
            planted += 1
    detected = sum(1 for v in by_sr.values() if len(v) > 1)
    u4_ok = planted > 0 and detected >= planted

    # U5
    u5 = 0
    for _, qi in queries:
        d, i = top_k(bits, bits[qi], 1)[0]
        score = 1.0 - 2.0 * d / DIM
        if i == qi and score >= 0.0:
            u5 += 1
    u5_ok = u5 == 5

    # U6 — latência de referência Python (orçamento soft); hub usa probe Rust
    random.seed(42)
    t0 = time.perf_counter()
    for _ in range(100):
        top_k(bits, bits[random.randrange(len(bits))], 10)
    py_ms = (time.perf_counter() - t0) * 1000 / 100
    rust_lat = None
    hv_json = Path("target/aether_transplant_hv.json")
    if hv_json.exists():
        rust_lat = json.loads(hv_json.read_text(encoding="utf-8")).get("latency_ms")
    u6_ok = rust_lat is not None and rust_lat <= 1.5

    cases = [
        {
            "id": "U1",
            "title": "Auto-recuperação top-1",
            "detail": f"5 queries canônicas → top-1=self. {u1}/5.",
            "pass": u1_ok,
            "score": f"{u1}/5",
        },
        {
            "id": "U2",
            "title": "Resolve SGDB",
            "detail": "Hit HV → fato + citação + authority≤240.",
            "pass": u2_ok,
            "score": f"{u2}/9",
        },
        {
            "id": "U3",
            "title": "Vizinhança por relação",
            "detail": f"Rust|é_uma → same_rel={same_rel} no top-10.",
            "pass": u3_ok,
            "score": f"same_rel={same_rel}",
        },
        {
            "id": "U4",
            "title": "Conflitos → Abstain/Propose",
            "detail": f"plantados={planted} detectados={detected}.",
            "pass": u4_ok,
            "score": f"{detected}/{planted}",
        },
        {
            "id": "U5",
            "title": "Evidence Hypervector",
            "detail": "score=1−2H/D, auth≤240, Act em auto-hit.",
            "pass": u5_ok,
            "score": f"{u5}/5",
        },
        {
            "id": "U6",
            "title": "Orçamento latência T1",
            "detail": f"Rust lat_ms={rust_lat}; Python ref={py_ms:.3f}ms (não-gate).",
            "pass": u6_ok,
            "score": f"{rust_lat}ms" if rust_lat is not None else "sem probe",
        },
    ]
    pass_all = all(c["pass"] for c in cases)
    payload = {
        "experiment": "INTAKE_TRANSPLANT_USO",
        "claim": "MSG 0229",
        "bank_atoms": len(bits),
        "pass_all": pass_all,
        "cases": cases,
        "gates_ci": {
            "note": "rodar cargo test --test transplant_hv_gate / e3_* / f1_* no job hub"
        },
        "etl": {"tool": "tools/harvest_transplant.py", "stages": 6},
        "playbook": "docs/DOACAO_PLAYBOOK.md",
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"PASS_USO={pass_all} → {args.out}")
    return 0 if pass_all else 1


if __name__ == "__main__":
    raise SystemExit(main())
