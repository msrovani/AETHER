#!/usr/bin/env python3
# F2 — digest de certificado ≤320 tokens (Eixo B). Sem payload bruto.
"""Monta o estado que o reader pode ver: campos do atestado, nunca o vetor.

Uso:
  python tools/cert_digest.py selftest
  python tools/cert_digest.py --state "E=-1.0 gap=0.1 viol=0"
  python tools/cert_digest.py --cert-json data/exemplo.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from research_harness import digest_certificate  # noqa: E402

TOKEN_BUDGET = 320
# Chaves que nunca entram no texto do digest (payload bruto / vetor).
_DROP_KEYS = (
    "payload",
    "state",
    "raw",
    "sources",
    "sources_text",
    "embedding",
    "vector",
)


def _num(value: object, *, lo: float | None = None, hi: float | None = None) -> str:
    try:
        number = float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return "nan"
    if number != number or number in (float("inf"), float("-inf")):
        return "nan"
    if lo is not None:
        number = max(lo, number)
    if hi is not None:
        number = min(hi, number)
    return f"{number:.4f}"


def _token(value: object, fallback: str) -> str:
    raw = str(value or fallback).strip().lower().replace(" ", "_")
    cleaned = "".join(ch for ch in raw if ch.isalnum() or ch == "_")[:32]
    return cleaned or fallback


def digest_certificate_fields(cert: dict) -> dict:
    """Allowlist de campos do atestado. Payload/vetor é descartado, não truncado."""
    dropped = any(cert.get(key) not in (None, "", [], {}) for key in _DROP_KEYS)
    try:
        violations = max(0, int(cert.get("violations", 0)))
    except (TypeError, ValueError):
        violations = 0
    try:
        authority = int(cert.get("authority", 0))
    except (TypeError, ValueError):
        authority = 0
    authority = min(255, max(0, authority))
    prefix = [
        "cert",
        f"E={_num(cert.get('energy'))}",
        f"gap={_num(cert.get('gap'), lo=0.0)}",
        f"viol={violations}",
        f"consensus={_num(cert.get('consensus', 0), lo=0.0, hi=1.0)}",
        f"prov={_token(cert.get('provenance'), 'unknown')}",
        f"auth={authority}",
    ]
    question = str(cert.get("question") or cert.get("case_id") or "")
    q_tokens = question.split()
    room = TOKEN_BUDGET - len(prefix)
    kept_q = q_tokens[: max(room, 0)]
    tokens = prefix + kept_q
    return {
        "digest": " ".join(tokens),
        "n_tokens": len(tokens),
        "truncated": len(q_tokens) > len(kept_q),
        "max_tokens": TOKEN_BUDGET,
        "dropped_raw": dropped,
        "ok": len(tokens) <= TOKEN_BUDGET,
    }


def selftest() -> int:
    marker = "RAWPAYLOAD_UNICO"
    cert = {
        "energy": -1.0,
        "gap": -4.0,
        "violations": 0,
        "consensus": 1.4,
        "provenance": "Inferred",
        "authority": 200,
        "question": " ".join(f"q{i}" for i in range(500)),
        "payload": marker + " " + " ".join(["p"] * 500),
        "state": marker,
        "vector": [0.1] * 64,
    }
    digest = digest_certificate_fields(cert)
    failures: list[str] = []
    if digest["n_tokens"] > TOKEN_BUDGET or not digest["ok"]:
        failures.append(f"budget {digest['n_tokens']}")
    if not digest["truncated"] or digest["n_tokens"] != TOKEN_BUDGET:
        failures.append(f"truncate {digest['n_tokens']} trunc={digest['truncated']}")
    if marker in digest["digest"] or "vector" in digest["digest"]:
        failures.append("payload vazou para o digest")
    if not digest["dropped_raw"]:
        failures.append("dropped_raw falso")
    if "E=-1.0000" not in digest["digest"] or "gap=0.0000" not in digest["digest"]:
        failures.append(f"campos {digest['digest'][:80]}")
    if "consensus=1.0000" not in digest["digest"] or "prov=inferred" not in digest["digest"]:
        failures.append("normalizacao")
    if "auth=200" not in digest["digest"]:
        failures.append("authority do atestado")
    # Corte de string (harness) continua valendo para texto já montado.
    long_state = " ".join(f"t{i}" for i in range(400))
    capped = digest_certificate(long_state)
    if capped["n_tokens"] != TOKEN_BUDGET or not capped["truncated"]:
        failures.append("harness truncate")
    report = {"ok": not failures, "failures": failures, "n_tokens": digest["n_tokens"]}
    print(json.dumps(report, ensure_ascii=False))
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Digest de certificado ≤320 tokens")
    parser.add_argument("--state", default=None, help="texto já montado; só corta em 320")
    parser.add_argument("--cert-json", default=None, help="JSON do atestado; descarta payload")
    parser.add_argument("cmd", nargs="?", default=None, choices=["selftest", "digest"])
    args = parser.parse_args()
    if args.cmd == "selftest":
        return selftest()
    if args.cert_json:
        cert = json.loads(Path(args.cert_json).read_text(encoding="utf-8"))
        if not isinstance(cert, dict):
            print(json.dumps({"ok": False, "error": "cert json deve ser objeto"}))
            return 2
        print(json.dumps(digest_certificate_fields(cert), ensure_ascii=False, indent=2))
        return 0
    if not args.state:
        parser.print_help()
        return 2
    print(json.dumps(digest_certificate(args.state), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
