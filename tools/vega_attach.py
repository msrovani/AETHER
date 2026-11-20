"""F1 — attach do reader gemma-3-1b-it ao vega-U (porto linear, self-supervisionado).

Corpus: data/vega_attach_corpus.txt (1044 linhas do trafego real do projeto).
Sem rótulos e sem retreino: o anchor 0.8B gera os alvos no espaço compartilhado,
o porto do Gemma é ajustado em forma fechada (ridge + refine) contra o engine
congelado. Requer HF_TOKEN no ambiente (gated model) e licença aceita no Hub.

Uso (MSG 0892: motor em .vega/, nunca recriar venv em target/):
  $env:HF_TOKEN = "..." ; .vega/venv/Scripts/python.exe tools/vega_attach.py
"""

from __future__ import annotations

import json
import sys
import time

import vegaml


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default="data/vega_attach_corpus.txt")
    ap.add_argument("--out", default=".vega/models/vega-u-gemma-1b")
    a = ap.parse_args()

    texts = [
        t.strip()
        for t in open(a.corpus, encoding="utf-8")
        if t.strip()
    ]
    print(f"corpus: {len(texts)} linhas ({a.corpus})", flush=True)

    t0 = time.time()
    report = vegaml.attach(
        "google/gemma-3-1b-it",
        texts=texts,
        out_dir=a.out,
    )
    dt = time.time() - t0
    print(f"attach_s: {dt:.0f}", flush=True)
    print(f"ridge_cosines: {json.dumps(report.get('ridge_cosines'), default=str)}", flush=True)
    extra = {k: v for k, v in report.items() if k not in ("ridge_cosines",)}
    print(f"extra: {json.dumps(extra, ensure_ascii=False, default=str)[:400]}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
