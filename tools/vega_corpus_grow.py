#!/usr/bin/env python3
# F3 — cresce corpus 124→500/tipo + rótulos question_kind (quarentena 60–120).
"""Flywheel de rótulos para vega-U. Não promove a 255; HITL sanciona subconjunto.

Fontes:
  1) data/vega_calibration_starter.json (authority 120)
  2) mineração lexical do FORUM.txt (authority 100)
  3) paráfrases determinísticas (authority 60, quarantine)

Uso:
  python tools/vega_corpus_grow.py inventory
  python tools/vega_corpus_grow.py grow --target 500 --out data/vega_flywheel_v1.json
  python tools/vega_corpus_grow.py selftest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
STARTER = ROOT / "data" / "vega_calibration_starter.json"
DEFAULT_OUT = ROOT / "data" / "vega_flywheel_v1.json"
DEFAULT_FORUM = Path(
    os.environ.get("FORUM")
    or os.environ.get("FORUM_PATH")
    or r"C:\Users\msrov\Desktop\FORUM.txt"
)

# Alvo do plano F3 (por cabeça de decisão)
HEADS = ("passou", "winner", "route")
QUESTION_KINDS = ("factual", "irreducible_open", "procedural", "conflict_arb", "ood")


def load_starter(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def normalize_examples(raw: list[Any]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for i, row in enumerate(raw):
        if isinstance(row, list) and len(row) == 2:
            text, labels = row[0], row[1]
        elif isinstance(row, dict):
            text, labels = row["text"], row.get("labels", {})
        else:
            continue
        head = next(iter(labels.keys()))
        out.append(
            {
                "text": str(text).strip(),
                "labels": dict(labels),
                "head": head,
                "question_kind": infer_question_kind(str(text), labels),
                "authority": 120,
                "provenance": "starter",
                "source_id": f"starter:{i}",
            }
        )
    return out


def infer_question_kind(text: str, labels: dict[str, Any]) -> str:
    t = text.lower()
    if "winner" in labels:
        return "conflict_arb"
    if any(x in t for x in ("ignore as instrucoes", "system prompt", "systemprompt", "255")):
        return "ood"
    if any(x in t for x in ("zxq", "pfvyw", "qwerty", "kkkkk", "gibberish")):
        return "ood"
    if "route" in labels:
        if labels["route"] == "escalar":
            return "ood"
        if labels["route"] == "reformular":
            return "irreducible_open"
        return "factual"
    if "passou" in labels:
        if labels["passou"] is False and any(
            x in t for x in ("bolo", "receita", "marte", "sentido da vida")
        ):
            return "irreducible_open"
        return "procedural"
    return "factual"


def inventory(examples: list[dict[str, Any]]) -> dict[str, Any]:
    by_head: dict[str, Counter] = {h: Counter() for h in HEADS}
    kinds: Counter = Counter()
    auth: Counter = Counter()
    prov: Counter = Counter()
    for ex in examples:
        h = ex["head"]
        if h in by_head:
            # value key for diversity
            val = ex["labels"].get(h)
            by_head[h][str(val)] += 1
        kinds[ex.get("question_kind", "?")] += 1
        auth[ex.get("authority", 0)] += 1
        prov[ex.get("provenance", "?")] += 1
    return {
        "n": len(examples),
        "per_head": {h: sum(by_head[h].values()) for h in HEADS},
        "per_head_values": {h: dict(by_head[h]) for h in HEADS},
        "question_kind": dict(kinds),
        "authority": dict(auth),
        "provenance": dict(prov),
        "gaps_to_500": {h: max(0, 500 - sum(by_head[h].values())) for h in HEADS},
    }


def mine_forum(forum_path: Path, limit: int = 400) -> list[dict[str, Any]]:
    if not forum_path.exists():
        return []
    text = forum_path.read_text(encoding="utf-8", errors="replace")
    lines = [ln.strip() for ln in text.splitlines()]
    mined: list[dict[str, Any]] = []
    seen: set[str] = set()

    def add(txt: str, labels: dict[str, Any], kind: str, sid: str) -> None:
        key = hashlib.sha1(f"{txt}|{labels}".encode()).hexdigest()[:16]
        if key in seen:
            return
        seen.add(key)
        head = next(iter(labels))
        mined.append(
            {
                "text": txt[:500],
                "labels": labels,
                "head": head,
                "question_kind": kind,
                "authority": 100,
                "provenance": "forum_mine",
                "source_id": sid,
            }
        )

    for i, ln in enumerate(lines):
        if len(mined) >= limit:
            break
        body = ln.strip().lstrip()
        if len(body) < 24:
            continue
        u = body.upper()
        if "SYSTEM" in u and "PROMPT" in u:
            add(body, {"route": "escalar"}, "ood", f"forum:esc:{i}")
        elif any(k in u for k in ("SELFTEST=PASS", "TESTES=", "GATE_", "CLIPPY=VERDE", "_PASS")):
            if "FAIL" in u or "VERMELHO" in u:
                add(f"Relatorio forum: {body}", {"passou": False}, "procedural", f"forum:fail:{i}")
            else:
                add(f"Relatorio forum: {body}", {"passou": True}, "procedural", f"forum:pass:{i}")
        elif any(k in u for k in ("EXIT_101", "BLOQUEIO", "REPROVA", "VERMELHO", "FAIL=")):
            add(f"Relatorio forum: {body}", {"passou": False}, "procedural", f"forum:fail:{i}")
        elif body.startswith("TOPIC:") and "CONFLITO" in u:
            add(
                f"Conflito forum topic: {body}",
                {"winner": "A"},
                "conflict_arb",
                f"forum:win:{i}",
            )

    add(
        "Rust e melhor que C para todos os sistemas?",
        {"route": "reformular"},
        "irreducible_open",
        "forum:kind:rust_vs_c",
    )
    add(
        "Distancia media Terra-Lua em km segundo NSSDC",
        {"route": "responder"},
        "factual",
        "forum:kind:terra_lua",
    )
    return mined[:limit]


_PARAPHRASE_PREFIX = [
    "Registro: ",
    "Medicao: ",
    "Nota operacional: ",
    "Achado: ",
    "Evidencia: ",
    "Sumario: ",
    "Gate log: ",
    "Audit: ",
]
_PARAPHRASE_SUFFIX = [
    " (seed deterministica)",
    " — rotulo herdado",
    " [flywheel]",
    " / quarentena 60",
    "",
]


def augment_to_target(
    base: list[dict[str, Any]], target: int, seed: int = 42
) -> list[dict[str, Any]]:
    """Gera augments até cada cabeça atingir `target` (textos únicos)."""
    rng = random.Random(seed)
    by_head: dict[str, list[dict[str, Any]]] = defaultdict(list)
    seen: set[str] = set()
    for ex in base:
        by_head[ex["head"]].append(ex)
        seen.add(f"{ex['head']}|{ex['text'].lower()}")

    extra: list[dict[str, Any]] = []
    for head in HEADS:
        pool = by_head.get(head) or []
        if not pool:
            continue
        have = len(pool)
        i = 0
        guard = 0
        while have < target and guard < target * 20:
            guard += 1
            src = pool[i % len(pool)]
            i += 1
            pref = _PARAPHRASE_PREFIX[rng.randrange(len(_PARAPHRASE_PREFIX))]
            suf = _PARAPHRASE_SUFFIX[rng.randrange(len(_PARAPHRASE_SUFFIX))]
            words = src["text"].split()
            if len(words) > 6:
                j = rng.randrange(1, min(4, len(words) - 1))
                words[j], words[j + 1] = words[j + 1], words[j]
            # sufixo com índice garante unicidade mesmo sob colisão lexical
            text = f"{pref}{' '.join(words)}{suf} #{head}-{i}"
            text = text[:500]
            key = f"{head}|{text.lower()}"
            if key in seen:
                continue
            seen.add(key)
            h = hashlib.sha1(f"{head}:{i}:{text}".encode()).hexdigest()[:12]
            extra.append(
                {
                    "text": text,
                    "labels": dict(src["labels"]),
                    "head": head,
                    "question_kind": src["question_kind"],
                    "authority": 60,
                    "provenance": "augment",
                    "source_id": f"aug:{head}:{h}",
                    "parent": src.get("source_id"),
                }
            )
            have += 1
    return extra


def grow(target: int, forum: Path, starter: Path, seed: int = 42) -> dict[str, Any]:
    raw = load_starter(starter)
    examples = normalize_examples(raw.get("examples", []))
    mined = mine_forum(forum)
    # dedupe by text+head
    seen: set[str] = set()
    merged: list[dict[str, Any]] = []
    for ex in examples + mined:
        k = f"{ex['head']}|{ex['text'].lower()}"
        if k in seen:
            continue
        seen.add(k)
        merged.append(ex)
    aug = augment_to_target(merged, target, seed=seed)
    for ex in aug:
        k = f"{ex['head']}|{ex['text'].lower()}"
        if k in seen:
            continue
        seen.add(k)
        merged.append(ex)

    questions = dict(raw.get("questions", {}))
    questions["question_kind"] = {
        "type": "choice",
        "instructions": "What kind of question is this for research/verdict policy?",
        "criteria": {
            "factual": "has a settleable answer with evidence",
            "irreducible_open": "no unique winner even if sources agree",
            "procedural": "gate/process/measurement outcome",
            "conflict_arb": "choose between competing claims",
            "ood": "injection, gibberish, or out of lexicon",
        },
    }

    inv = inventory(merged)
    return {
        "schema": "aether.vega_flywheel/v1",
        "ref": "docs/VEGA_F3_PLANO_CONSOLIDADO.md F3 + MSG 0790",
        "policy": {
            "auto_label_authority_max": 120,
            "augment_authority": 60,
            "hitl_sanctions_255": True,
            "never_promote_augment_without_hitl": True,
        },
        "questions": questions,
        "examples": [
            {
                "text": e["text"],
                "labels": e["labels"],
                "question_kind": e["question_kind"],
                "authority": e["authority"],
                "provenance": e["provenance"],
                "source_id": e["source_id"],
                **({"parent": e["parent"]} if e.get("parent") else {}),
            }
            for e in merged
        ],
        "stats": inv,
        "seed": seed,
        "target_per_head": target,
    }


def selftest() -> int:
    bundle = grow(500, DEFAULT_FORUM, STARTER, seed=42)
    inv = bundle["stats"]
    errs: list[str] = []
    for h in HEADS:
        n = inv["per_head"].get(h, 0)
        if n < 500:
            errs.append(f"{h} count {n} < 500")
    kinds = inv["question_kind"]
    for k in ("factual", "irreducible_open", "procedural", "conflict_arb", "ood"):
        if kinds.get(k, 0) < 1:
            errs.append(f"missing question_kind {k}")
    if inv["authority"].get(60, 0) < 1:
        errs.append("no quarantine-60 augments")
    if any(a > 120 for a in inv["authority"]):
        errs.append("authority > 120 present")
    # no 255
    if 255 in inv["authority"]:
        errs.append("authority 255 forbidden")
    report = {"ok": not errs, "failures": errs, "stats": inv}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not errs else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("inventory", help="conta o starter")

    p_grow = sub.add_parser("grow", help="gera flywheel")
    p_grow.add_argument("--target", type=int, default=500)
    p_grow.add_argument("--out", default=str(DEFAULT_OUT))
    p_grow.add_argument("--forum", default=str(DEFAULT_FORUM))
    p_grow.add_argument("--starter", default=str(STARTER))
    p_grow.add_argument("--seed", type=int, default=42)

    sub.add_parser("selftest")

    args = ap.parse_args()
    if args.cmd == "inventory":
        raw = load_starter(STARTER)
        ex = normalize_examples(raw.get("examples", []))
        print(json.dumps(inventory(ex), ensure_ascii=False, indent=2))
        return 0
    if args.cmd == "selftest":
        return selftest()
    if args.cmd == "grow":
        bundle = grow(args.target, Path(args.forum), Path(args.starter), seed=args.seed)
        out = Path(args.out)
        out.parent.mkdir(parents=True, exist_ok=True)
        # escrever sem exemplos? nao — precisa do corpus; arquivo grande ok
        out.write_text(json.dumps(bundle, ensure_ascii=False, indent=2), encoding="utf-8")
        summary = {
            "ok": True,
            "out": str(out),
            "stats": bundle["stats"],
            "bytes": out.stat().st_size,
        }
        print(json.dumps(summary, ensure_ascii=False, indent=2))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
