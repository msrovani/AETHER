#!/usr/bin/env python3
"""ETL offline INTAKE_TRANSPLANT: texto → whitening → JL → bank.bin + idx + manifest.

Dry-run (--dry-run): embeddings sintéticos determinísticos (sem HF/rede).
--jl-only: lê emb/bank.f32 (+ facts) já gerados por sidecar HF; só projeta JL.
Doador documentado: Qwen/Qwen3-Embedding-0.6B (Apache-2.0, MRL→768).

Uso:
  python tools/harvest_transplant.py --limit 200 --dry-run --out data
  python tools/harvest_transplant.py --limit 200 --dry-run --out data --no-whiten
  python tools/harvest_transplant.py --jl-only --out data --seed 0xDEADBEEF --revision <hf>
  python tools/donor_map_embed.py --out data  # sidecar HF → emb/bank.f32 (opcional)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
import unicodedata
from pathlib import Path

try:
    import numpy as np
except ImportError:  # pragma: no cover
    np = None  # type: ignore

MAGIC = b"AETHV1\0\0"
HEADER_SIZE = 128
DIM_BITS = 10240
WORDS = 160
DEFAULT_SEED = 0xDEADBEEF
DIM_IN = 768


def make_prng(seed: int):
    state = seed & 0xFFFFFFFFFFFFFFFF
    if state == 0:
        state = 0x9E3779B97F4A7C15

    def next_u64() -> int:
        nonlocal state
        state ^= (state >> 12) & 0xFFFFFFFFFFFFFFFF
        state ^= (state << 25) & 0xFFFFFFFFFFFFFFFF
        state ^= (state >> 27) & 0xFFFFFFFFFFFFFFFF
        state &= 0xFFFFFFFFFFFFFFFF
        return (state * 0x2545F4914F6CDD1D) & 0xFFFFFFFFFFFFFFFF

    def next_gauss() -> float:
        u1 = (next_u64() + 1) / 2**64
        u2 = (next_u64() + 1) / 2**64
        return math.sqrt(-2.0 * math.log(u1)) * math.cos(2.0 * math.pi * u2)

    return next_u64, next_gauss


def nfc(s: str) -> str:
    return unicodedata.normalize("NFC", s)


def key_u128(label: str) -> int:
    h = hashlib.sha256(nfc(label).encode("utf-8")).digest()
    return int.from_bytes(h[:16], "little")


def synthetic_corpus(n: int) -> list[dict]:
    bases = [
        ("Brasil", "capital", "Brasília"),
        ("São Paulo", "é_um", "estado"),
        ("Rio de Janeiro", "é_um", "estado"),
        ("água", "é", "H2O"),
        ("sol", "é_uma", "estrela"),
        ("Lua", "orbita", "Terra"),
        ("Python", "é_uma", "linguagem"),
        ("Rust", "é_uma", "linguagem"),
        ("AETHER", "usa", "fase"),
        ("Ising", "minimiza", "energia"),
    ]
    rows: list[dict] = []
    for i in range(n):
        s, r, o = bases[i % len(bases)]
        subj = f"{s}_{i // len(bases)}" if i >= len(bases) else s
        obj = f"{o}_{i // len(bases)}" if i >= len(bases) else o
        rows.append(
            {
                "subject": subj,
                "relation": r,
                "object": obj,
                "confidence": 0.85,
                "citation": f"synth://doc/{i}",
                "authority": 100,
                "lang": "pt",
                "kind": 1,
                "conflict_seed": False,
                "wall_ms": 1_700_000_000_000 + i,
            }
        )
        # Planta contradição: mesmo (s,r), objeto diferente
        if i % 37 == 0 and i > 0:
            rows.append(
                {
                    "subject": subj,
                    "relation": r,
                    "object": f"NÃO_{obj}",
                    "confidence": 0.5,
                    "citation": f"synth://doc/{i}/conflict",
                    "authority": 80,
                    "lang": "pt",
                    "kind": 1,
                    "conflict_seed": True,
                    "wall_ms": 1_700_000_000_000 + i + 1,
                }
            )
    return rows


def unique_labels(facts: list[dict]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for f in facts:
        for lab in (f["subject"], f["relation"], f["object"]):
            lab = nfc(lab)
            if lab not in seen:
                seen.add(lab)
                out.append(lab)
    return out


def embed_synthetic(labels: list[str], dim: int, seed: int) -> list[list[float]]:
    """Embeddings estilo E3: combinação linear de átomos + ruído (Spearman JL)."""
    n_base = min(64, max(16, len(labels) // 5 or 16))
    _, g0 = make_prng(seed ^ 0xC1A55E)
    bases = []
    for _ in range(n_base):
        c = [g0() for _ in range(dim)]
        nrm = math.sqrt(sum(x * x for x in c)) or 1.0
        bases.append([x / nrm for x in c])
    mats: list[list[float]] = []
    for lab in labels:
        h = int.from_bytes(hashlib.sha256(lab.encode("utf-8")).digest()[:8], "little")
        _, g = make_prng(seed ^ h ^ 0xE11B)
        i0, i1, i2 = h % n_base, (h >> 8) % n_base, (h >> 16) % n_base
        v = [
            0.55 * bases[i0][j] + 0.35 * bases[i1][j] + 0.25 * bases[i2][j] + g() * 0.2
            for j in range(dim)
        ]
        nrm = math.sqrt(sum(x * x for x in v)) or 1.0
        mats.append([x / nrm for x in v])
    return mats


def zca_fit(X: list[list[float]], eps: float = 1e-5):
    n = len(X)
    d = len(X[0])
    mu = [sum(X[i][j] for i in range(n)) / n for j in range(d)]
    C = [[0.0] * d for _ in range(d)]
    for row in X:
        for a in range(d):
            da = row[a] - mu[a]
            for b in range(d):
                C[a][b] += da * (row[b] - mu[b])
    for a in range(d):
        for b in range(d):
            C[a][b] /= max(n - 1, 1)
            if a == b:
                C[a][b] += eps
    # Diagonal whitening (CI-fast); full ZCA O(d³) fica para escala offline.
    inv_sqrt = [1.0 / math.sqrt(C[j][j]) for j in range(d)]
    W = [[0.0] * d for _ in range(d)]
    for j in range(d):
        W[j][j] = inv_sqrt[j]
    return mu, W


def apply_whiten(x: list[float], mu: list[float], W: list[list[float]]) -> list[float]:
    d = len(x)
    centered = [x[j] - mu[j] for j in range(d)]
    y = [sum(W[i][j] * centered[j] for j in range(d)) for i in range(d)]
    nrm = math.sqrt(sum(v * v for v in y)) or 1.0
    return [v / nrm for v in y]


def build_P(dim_in: int, dim_bits: int, seed: int):
    scale = 1.0 / math.sqrt(dim_bits)
    if np is not None:
        rng = np.random.default_rng(seed & 0xFFFFFFFF)
        return (rng.standard_normal((dim_bits, dim_in), dtype=np.float64) * scale).astype(
            np.float32
        )
    _, gauss = make_prng(seed)
    return [gauss() * scale for _ in range(dim_bits * dim_in)]


def project_bits(P, x: list[float], dim_bits: int, dim_in: int) -> list[int]:
    words = [0] * (dim_bits // 64)
    if np is not None and isinstance(P, np.ndarray):
        xv = np.asarray(x, dtype=np.float32)
        y = P @ xv  # (dim_bits,)
        for k, acc in enumerate(y):
            if acc >= 0.0:
                words[k // 64] |= 1 << (k % 64)
        return words
    for k in range(dim_bits):
        base = k * dim_in
        acc = 0.0
        for j in range(dim_in):
            acc += P[base + j] * x[j]
        if acc >= 0.0:
            words[k // 64] |= 1 << (k % 64)
    return words


def write_embeds(path: Path, X: list[list[float]]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    n = len(X)
    d = len(X[0])
    raw = struct.pack("<II", n, d)
    for row in X:
        raw += struct.pack(f"<{d}f", *row)
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


def read_embeds(path: Path) -> tuple[int, int, list[list[float]], str]:
    raw = path.read_bytes()
    if len(raw) < 8:
        raise ValueError(f"bank.f32 curto: {path}")
    n, d = struct.unpack_from("<II", raw, 0)
    need = 8 + n * d * 4
    if len(raw) != need:
        raise ValueError(f"bank.f32 size mismatch: got {len(raw)} need {need}")
    X: list[list[float]] = []
    off = 8
    for _ in range(n):
        row = list(struct.unpack_from(f"<{d}f", raw, off))
        X.append(row)
        off += d * 4
    return n, d, X, hashlib.sha256(raw).hexdigest()


def load_facts_jsonl(path: Path) -> list[dict]:
    rows: list[dict] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def write_whitening(path: Path, mu: list[float], W: list[list[float]]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    d = len(mu)
    raw = struct.pack("<I", d)
    raw += struct.pack(f"<{d}f", *mu)
    flat = [W[i][j] for i in range(d) for j in range(d)]
    raw += struct.pack(f"<{d * d}f", *flat)
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


def write_bank(
    out_dir: Path,
    bits_rows: list[list[int]],
    keys: list[int],
    kinds: list[int],
    langs: list[int],
    dim_in: int,
    seed: int,
    whitened: bool,
) -> tuple[str, int]:
    hv = out_dir / "hv"
    hv.mkdir(parents=True, exist_ok=True)
    count = len(bits_rows)
    payload = bytearray()
    for row in bits_rows:
        for w in row:
            payload += struct.pack("<Q", w & 0xFFFFFFFFFFFFFFFF)
    sha = hashlib.sha256(payload).digest()
    header = bytearray(HEADER_SIZE)
    header[0:8] = MAGIC
    struct.pack_into("<I", header, 8, 1)
    struct.pack_into("<I", header, 12, DIM_BITS)
    struct.pack_into("<I", header, 16, WORDS)
    struct.pack_into("<I", header, 20, dim_in)
    struct.pack_into("<I", header, 24, 1 if whitened else 0)
    struct.pack_into("<Q", header, 28, count)
    struct.pack_into("<Q", header, 36, seed & 0xFFFFFFFFFFFFFFFF)
    header[44:76] = sha
    (hv / "bank.bin").write_bytes(bytes(header) + bytes(payload))

    idx = bytearray()
    for i, k in enumerate(keys):
        key_lo = k & 0xFFFFFFFFFFFFFFFF
        key_hi = (k >> 64) & 0xFFFFFFFFFFFFFFFF
        idx += struct.pack("<QQ", key_lo, key_hi)
        idx += struct.pack("<BB", kinds[i] & 0xFF, langs[i] & 0xFF)
        idx += b"\0" * 6
    (hv / "bank.idx").write_bytes(idx)
    return hashlib.sha256(bytes(header) + bytes(payload)).hexdigest(), count


def detect_conflicts(facts: list[dict]) -> tuple[int, int]:
    planted = sum(1 for f in facts if f.get("conflict_seed"))
    by_sr: dict[tuple[str, str], set[str]] = {}
    for f in facts:
        key = (nfc(f["subject"]), nfc(f["relation"]))
        by_sr.setdefault(key, set()).add(nfc(f["object"]))
    detected = sum(1 for objs in by_sr.values() if len(objs) > 1)
    return detected, planted


def pack_round(
    out: Path,
    donor: str,
    revision: str,
    dim_in: int,
    seed: int,
    labels: list[str],
    facts: list[dict],
    X: list[list[float]],
    emb_sha: str,
    whitened: bool,
    whiten_hash: str | None,
    claim: str = "MSG 0229 INTAKE_TRANSPLANT",
) -> dict:
    # Guarda anti-deslocamento (MSG 0567/0599): labels/X/facts alinhados.
    if not (len(labels) == len(X) == len(facts)):
        raise SystemExit(
            f"harvest len mismatch labels={len(labels)} X={len(X)} facts={len(facts)}"
        )
    P = build_P(dim_in, DIM_BITS, seed)
    bits_rows: list[list[int]] = []
    keys: list[int] = []
    kinds: list[int] = []
    langs: list[int] = []
    for lab, x, f in zip(labels, X, facts, strict=True):
        bits_rows.append(project_bits(P, x, DIM_BITS, dim_in))
        keys.append(key_u128(lab))
        kinds.append(int(f.get("kind", 1)))
        langs.append(1)

    bank_sha, count = write_bank(
        out, bits_rows, keys, kinds, langs, dim_in, seed, whitened
    )
    detected, planted = detect_conflicts(facts)
    by_rel: dict[str, list] = {}
    for row in facts:
        by_rel.setdefault(row["relation"], []).append(row)
    shard_meta = []
    for rel, rs in by_rel.items():
        for i in range(0, len(rs), 48):
            shard_meta.append(
                {"relation": rel, "offset": i, "count": min(48, len(rs) - i)}
            )
    ledger = {
        "atoms": [
            {"key": hex(k), "label": lab}
            for k, lab in zip(keys, labels, strict=True)
        ],
        "facts": facts,
        "shards": shard_meta,
    }
    (out / "sgdb_mock_ledger.json").write_text(
        json.dumps(ledger, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    from datetime import date

    manifest = {
        "donor": donor,
        "revision": revision,
        "dim_in": dim_in,
        "dim_hv": DIM_BITS,
        "words": WORDS,
        "seed": seed,
        "count": count,
        "labels": len(labels),
        "facts": len(facts),
        "whitened": whitened,
        "sha256_whitening": whiten_hash,
        "sha256_embeds": emb_sha,
        "sha256_bank": bank_sha,
        "shards": shard_meta,
        "conflicts_planted": planted,
        "conflicts_detected": detected,
        # Doutrina MSG 0254/0258: transplantado nasce em quarentena 60-120.
        # 200 exige VerifiedFact do solver; 255 é HITL. O cap de geração
        # substitui o 240 anterior (QUERY 0434 — fora da faixa da doutrina).
        "authority_max_transplant": 120,
        "created": date.today().isoformat(),
        "claim": claim,
    }
    (out / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return {
        "ok": True,
        "count": count,
        "labels": len(labels),
        "sha256_bank": bank_sha,
        "whitened": whitened,
        "conflicts_detected": detected,
        "conflicts_planted": planted,
        "revision": revision,
        "donor": donor,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=200)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument(
        "--jl-only",
        action="store_true",
        help="Só JL+sign a partir de emb/bank.f32 + facts.jsonl (sidecar HF).",
    )
    ap.add_argument("--out", type=Path, default=Path("data"))
    ap.add_argument(
        "--seed",
        type=lambda s: int(s, 0),
        default=DEFAULT_SEED,
        help="u64 (aceita 0x…)",
    )
    ap.add_argument("--dim-in", type=int, default=DIM_IN)
    ap.add_argument("--no-whiten", action="store_true")
    ap.add_argument("--donor", default="Qwen/Qwen3-Embedding-0.6B")
    ap.add_argument("--revision", default="", help="revision HF / git (manifest)")
    args = ap.parse_args()

    if args.jl_only:
        emb_path = args.out / "emb" / "bank.f32"
        facts_path = args.out / "facts.jsonl"
        if not emb_path.exists():
            raise SystemExit(f"--jl-only: falta {emb_path} (rode donor_map_embed.py)")
        if not facts_path.exists():
            raise SystemExit(f"--jl-only: falta {facts_path}")
        n, d, X, emb_sha = read_embeds(emb_path)
        if d != args.dim_in:
            raise SystemExit(f"--jl-only: dim emb={d} != --dim-in {args.dim_in}")
        facts = load_facts_jsonl(facts_path)
        labels = [
            nfc(f"{f['subject']}|{f['relation']}|{f['object']}") for f in facts
        ]
        if len(labels) != n:
            raise SystemExit(f"--jl-only: facts={len(labels)} != embeds n={n}")
        whitened = False
        whiten_hash = None
        whiten_path = args.out / "emb" / "whitening.bin"
        if whiten_path.exists() and not args.no_whiten:
            # Já branqueado offline pelo sidecar: não reaplica.
            whitened = True
            whiten_hash = hashlib.sha256(whiten_path.read_bytes()).hexdigest()
        rev = args.revision or "jl-only"
        donor = args.donor if not args.donor.endswith("#dry-run-synthetic") else args.donor
        summary = pack_round(
            args.out,
            donor=donor,
            revision=rev,
            dim_in=args.dim_in,
            seed=args.seed,
            labels=labels,
            facts=facts,
            X=X,
            emb_sha=emb_sha,
            whitened=whitened,
            whiten_hash=whiten_hash,
        )
        print(json.dumps(summary, ensure_ascii=False))
        return

    if not args.dry_run:
        print(
            "aviso: embedder HF não embutido no harvest — use "
            "tools/donor_map_embed.py + --jl-only, ou --dry-run. Forçando --dry-run."
        )
        args.dry_run = True

    facts = synthetic_corpus(args.limit)
    atom_labels = [
        nfc(f"{f['subject']}|{f['relation']}|{f['object']}") for f in facts
    ]
    seen: set[str] = set()
    uniq_facts: list[dict] = []
    uniq_labels: list[str] = []
    if len(facts) != len(atom_labels):
        raise SystemExit(
            f"facts={len(facts)} atom_labels={len(atom_labels)} antes do dedupe"
        )
    for f, lab in zip(facts, atom_labels, strict=True):
        if lab in seen:
            continue
        seen.add(lab)
        uniq_facts.append(f)
        uniq_labels.append(lab)
    facts = uniq_facts
    labels = uniq_labels
    X = embed_synthetic(labels, args.dim_in, args.seed)
    whitened = not args.no_whiten
    whiten_hash = None
    if whitened:
        mu, W = zca_fit(X)
        whiten_hash = write_whitening(args.out / "emb" / "whitening.bin", mu, W)
        X = [apply_whiten(x, mu, W) for x in X]
    else:
        (args.out / "emb").mkdir(parents=True, exist_ok=True)

    emb_sha = write_embeds(args.out / "emb" / "bank.f32", X)
    facts_path = args.out / "facts.jsonl"
    with facts_path.open("w", encoding="utf-8") as f:
        for row in facts:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    summary = pack_round(
        args.out,
        donor=f"{args.donor}#dry-run-synthetic",
        revision=args.revision or "dry-run",
        dim_in=args.dim_in,
        seed=args.seed,
        labels=labels,
        facts=facts,
        X=X,
        emb_sha=emb_sha,
        whitened=whitened,
        whiten_hash=whiten_hash,
    )
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
