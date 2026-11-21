#!/usr/bin/env python3
"""Escriba live (opt-in) — reformula claims; NÃO chama vegaml.decide.

Entrada JSON stdin: {style, claims[], rule}.
Sem transformers: devolve prosa ancorada (dry). Com --require-model e falha → exit 3.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def anchored(claims: list, style: str) -> str:
    lines = []
    for c in claims:
        sub = c.get("subject", "")
        rel = c.get("relation", "")
        val = c.get("value", "")
        auth = c.get("authority", 0)
        if not str(val).strip():
            continue
        if style == "compact":
            lines.append(f"{sub} {rel}={val} (auth={auth})")
        else:
            lines.append(
                f"Com base na evidência, {sub} {rel} {val} (autoridade {auth})."
            )
    return " ".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--artifact", default="")
    ap.add_argument("--style", default="factual")
    ap.add_argument("--input", default="", help="JSON file (senão stdin)")
    ap.add_argument("--require-model", action="store_true")
    args = ap.parse_args()
    if args.input:
        raw = Path(args.input).read_text(encoding="utf-8")
    else:
        raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        print("", end="")
        return 2
    claims = payload.get("claims") or []
    style = payload.get("style") or args.style
    # Live model path reservado; por default anchored (CI / sem GPU).
    if args.require_model:
        print("model_not_wired", file=sys.stderr)
        return 3
    text = anchored(claims, style)
    print(text)
    return 0 if text.strip() else 1


if __name__ == "__main__":
    sys.exit(main())
