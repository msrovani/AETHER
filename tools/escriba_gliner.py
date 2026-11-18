#!/usr/bin/env python3
# Escriba-0 — extrator schema (GLiNER2.5 JointIE) → facts.jsonl (quarentena 60–120).
# Sidecar fora do crate. Sem pesos no núcleo. REBEL/ICKG proibidos (NC).
#
#   pip install 'gliner2>=0.1'   # ou o pacote fastino atual
#   python tools/escriba_gliner.py --corpus path/textos --out data/rounds/<rodada>
#
# Sem dependência / sem CUDA opcional: exit 3 ABSTAIN (honesto).
# Próximo passo opcional: Qwen2.5-7B para relações fora do schema (não neste stub).

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

AUTH_MIN = 60
AUTH_MAX = 120
DEFAULT_MODEL = "fastino/gliner2.5-base-v1"


def write_meta(out: Path, **kw) -> None:
    out.mkdir(parents=True, exist_ok=True)
    path = out / "escriba_meta.json"
    path.write_text(json.dumps(kw, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def abstain(out: Path, reason: str, code: int = 3) -> int:
    write_meta(
        out,
        status="ABSTAIN",
        reason=reason,
        model=DEFAULT_MODEL,
        authority_range=[AUTH_MIN, AUTH_MAX],
        created=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )
    print(f"ABSTAIN: {reason}", file=sys.stderr)
    return code


def main() -> int:
    ap = argparse.ArgumentParser(description="Escriba-0: GLiNER2.5 → facts.jsonl")
    ap.add_argument("--out", type=Path, required=True, help="dir da rodada (facts.jsonl)")
    ap.add_argument("--corpus", type=Path, help="dir/arquivo de textos (.txt/.jsonl)")
    ap.add_argument("--model", default=DEFAULT_MODEL)
    ap.add_argument("--authority", type=int, default=96, help="60–120")
    ap.add_argument("--limit", type=int, default=0, help="0=todos")
    ap.add_argument("--dry-schema", action="store_true", help="só valida schema vazio")
    args = ap.parse_args()

    auth = max(AUTH_MIN, min(AUTH_MAX, args.authority))
    args.out.mkdir(parents=True, exist_ok=True)

    if args.dry_schema:
        facts_path = args.out / "facts.jsonl"
        facts_path.write_text("", encoding="utf-8")
        write_meta(
            args.out,
            status="DRY_SCHEMA",
            facts=0,
            model=args.model,
            authority=auth,
            note="scaffold; rode sem --dry-schema com gliner2 instalado",
            created=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        )
        print(f"DRY_SCHEMA → {facts_path}")
        return 0

    if args.corpus is None or not args.corpus.exists():
        return abstain(args.out, "falta --corpus existente (ou use --dry-schema)")

    try:
        # Import tardio: ausência = ABSTAIN, não crash no CI.
        from gliner2 import AutoExtractor  # type: ignore
    except Exception as e:  # noqa: BLE001
        return abstain(
            args.out,
            f"deps GLiNER2 ausentes ({e!s}); pip install gliner2 — ver docs/DONOR_CATALOG.md",
        )

    # Scaffold: carrega modelo e extrai com schema mínimo person/org/loc.
    # JointIE completo fica para a iteração seguinte (B4).
    try:
        extractor = AutoExtractor.from_pretrained(args.model)
    except Exception as e:  # noqa: BLE001
        return abstain(args.out, f"falha ao carregar {args.model}: {e}")

    texts: list[str] = []
    if args.corpus.is_file():
        raw = args.corpus.read_text(encoding="utf-8", errors="replace")
        if args.corpus.suffix == ".jsonl":
            for line in raw.splitlines():
                if not line.strip():
                    continue
                obj = json.loads(line)
                texts.append(obj.get("text") or obj.get("passage") or str(obj))
        else:
            texts = [p.strip() for p in raw.split("\n\n") if p.strip()]
    else:
        for p in sorted(args.corpus.rglob("*.txt")):
            texts.append(p.read_text(encoding="utf-8", errors="replace").strip())

    if args.limit > 0:
        texts = texts[: args.limit]
    if not texts:
        return abstain(args.out, "corpus vazio")

    schema_ents = ["person", "organization", "location"]
    facts: list[dict] = []
    for i, text in enumerate(texts):
        try:
            result = extractor.extract(text, schema_ents)
        except Exception as e:  # noqa: BLE001
            print(f"skip doc {i}: {e}", file=sys.stderr)
            continue
        ents = (result or {}).get("entities") or {}
        for rel, vals in ents.items():
            for val in vals if isinstance(vals, list) else [vals]:
                facts.append(
                    {
                        "subject": str(val),
                        "relation": f"is_a_{rel}",
                        "object": rel,
                        "confidence": 0.7,
                        "authority": auth,
                        "provenance": "Transplanted",
                        "source": "escriba_gliner",
                        "model": args.model,
                        "lang": "und",
                        "citation": f"doc:{i}",
                    }
                )

    facts_path = args.out / "facts.jsonl"
    with facts_path.open("w", encoding="utf-8") as f:
        for row in facts:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    write_meta(
        args.out,
        status="OK",
        facts=len(facts),
        docs=len(texts),
        model=args.model,
        authority=auth,
        created=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )
    print(f"OK facts={len(facts)} → {facts_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
