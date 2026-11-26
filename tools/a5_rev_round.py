#!/usr/bin/env python3
"""A5 — round reverso (`s2|rel|s1`) para o gate medir as duas direções.

O `emb/bank_rev.f32` do round já existe (versionado); este script projeta esse
bank com o **mesmo** `P` do harvest (seed do manifest) e escreve um round que o
`Round::load` do A5 consome em `target/a5_rev_round/` (gitignored, regenerável):

  hv/bank_rev.bin        bits ±1 de `bank_rev.f32` projetado por P (AETHV1)
  hv/bank.bin            mesmos bytes (o nome que o `Round::load` abre)
  hv/bank.idx            cópia do idx do round (mesmas chaves/ordem de fatos)
  emb/bank.f32           cópia de `emb/bank_rev.f32`
  s2_query_{hv,f32}.bin  cópia (a query `s2` é a mesma nas duas direções)

Autoverificação: reconstrói o bank **fwd** com o mesmo código e exige igualdade
bit a bit com o `hv/bank.bin` versionado — se o `P` ou a ordem dos bits divergir,
sai 1 em vez de medir contra um bank falso.

  python tools/a5_rev_round.py
"""
from __future__ import annotations

import hashlib
import json
import shutil
import struct
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from harvest_transplant import build_P  # noqa: E402

MAGIC = b"AETHV1\0\0"
HEADER_SIZE = 128
BASE_ROUND = Path("data/rounds/20261117_b0_paws")
OUT = Path("target/a5_rev_round")


def read_f32(path: Path) -> tuple[int, int, np.ndarray]:
    raw = path.read_bytes()
    n, d = struct.unpack_from("<II", raw, 0)
    if len(raw) != 8 + n * d * 4:
        raise SystemExit(f"{path}: {len(raw)} bytes != {8 + n * d * 4}")
    return n, d, np.frombuffer(raw, dtype="<f4", count=n * d, offset=8).reshape(n, d).copy()


def bank_bits(docs: np.ndarray, P: np.ndarray, words: int) -> np.ndarray:
    """(n, dim) f32 → (n, words*8) u8, bit k = sinal da projeção k (LSB-first)."""
    out = np.zeros((docs.shape[0], words * 8), dtype=np.uint8)
    # gemv linha a linha, como o project_bits do harvest: em gemm o BLAS
    # acumula diferente em ~5 bits de 6,4 M perto de zero e o fwd reconstruido
    # deixaria de ser bit-identico ao bank versionado (a prova do P).
    for i in range(docs.shape[0]):
        out[i] = np.packbits(P @ docs[i] >= 0.0, bitorder="little")
    return out


def bank_bytes(bits: np.ndarray, dim_bits: int, words: int, dim_in: int, seed: int) -> bytes:
    payload = bits.astype(np.uint8).tobytes()
    header = bytearray(HEADER_SIZE)
    header[0:8] = MAGIC
    struct.pack_into("<I", header, 8, 1)
    struct.pack_into("<I", header, 12, dim_bits)
    struct.pack_into("<I", header, 16, words)
    struct.pack_into("<I", header, 20, dim_in)
    struct.pack_into("<I", header, 24, 0)
    struct.pack_into("<Q", header, 28, bits.shape[0])
    struct.pack_into("<Q", header, 36, seed & 0xFFFFFFFFFFFFFFFF)
    header[44:76] = hashlib.sha256(payload).digest()
    return bytes(header) + payload


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    manifest = json.loads((BASE_ROUND / "manifest.json").read_text(encoding="utf-8"))
    dim_in = int(manifest["dim_in"])
    dim_bits = int(manifest["dim_hv"])
    words = int(manifest["words"])
    seed = int(manifest["seed"])

    n_fwd, d_fwd, fwd = read_f32(BASE_ROUND / "emb" / "bank.f32")
    n_rev, d_rev, rev = read_f32(BASE_ROUND / "emb" / "bank_rev.f32")
    if (n_fwd, d_fwd) != (n_rev, d_rev) or d_fwd != dim_in:
        raise SystemExit(f"bank fwd {n_fwd}x{d_fwd} != rev {n_rev}x{d_rev} (dim_in={dim_in})")

    P = build_P(dim_in, dim_bits, seed)
    ref = (BASE_ROUND / "hv" / "bank.bin").read_bytes()
    if bank_bytes(bank_bits(fwd, P, words), dim_bits, words, dim_in, seed) != ref:
        raise SystemExit("P/ordem de bits divergente: bank fwd reconstruido != versionado")
    print(f"fwd: bank reconstruido identico ao versionado ({len(ref)} bytes, sha {hashlib.sha256(ref).hexdigest()[:12]})")

    rev_bank = bank_bytes(bank_bits(rev, P, words), dim_bits, words, dim_in, seed)
    OUT.joinpath("hv").mkdir(parents=True, exist_ok=True)
    OUT.joinpath("emb").mkdir(parents=True, exist_ok=True)
    # `bank_rev.bin` é o artefato pedido; `bank.bin` é o nome que o `Round::load`
    # do A5 abre. Mesmos bytes, duas entradas no mesmo round.
    (OUT / "hv" / "bank_rev.bin").write_bytes(rev_bank)
    (OUT / "hv" / "bank.bin").write_bytes(rev_bank)
    shutil.copyfile(BASE_ROUND / "hv" / "bank.idx", OUT / "hv" / "bank.idx")
    shutil.copyfile(BASE_ROUND / "emb" / "bank_rev.f32", OUT / "emb" / "bank.f32")
    for name in ("s2_query_hv.bin", "s2_query_f32.bin"):
        shutil.copyfile(BASE_ROUND / name, OUT / name)

    prov = {
        "round": "rev s2|rel|s1 (bank_rev.f32 -> HV com o P do manifest)",
        "base_round": str(BASE_ROUND).replace("\\", "/"),
        "out": str(OUT).replace("\\", "/"),
        "n": n_rev,
        "dim_in": dim_in,
        "dim_hv": dim_bits,
        "jl_seed": seed,
        "bank_sha256": hashlib.sha256(rev_bank).hexdigest(),
        "bank_rev_f32_sha256": hashlib.sha256((BASE_ROUND / "emb" / "bank_rev.f32").read_bytes()).hexdigest(),
    }
    (OUT / "provenance.json").write_text(json.dumps(prov, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"rev: {OUT} n={n_rev} bank sha {prov['bank_sha256'][:12]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
