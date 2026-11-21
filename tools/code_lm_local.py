#!/usr/bin/env python3
# K6 — small code LM local (offline). Sem rede. Ecoa PatchProposal tipado.
# Troque o corpo por ollama/llama.cpp local; declare ram_mb honestamente.

from __future__ import annotations

import argparse
import json
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("brief", nargs="?", default="")
    ap.add_argument("--ram-budget", type=int, default=2048)
    args = ap.parse_args()
    raw = args.brief or sys.stdin.read()
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as e:
        print(json.dumps({"ok": False, "error": f"json: {e}", "offline": True}))
        return 1

    # K6: nunca abre rede neste fixture.
    if req.get("allow_net") is True:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": "rede recusada (K6 offline)",
                    "offline": False,
                }
            )
        )
        return 1

    prop = req.get("proposal")
    if not isinstance(prop, dict):
        prop = {
            k: req[k]
            for k in ("path", "op", "anchor", "content")
            if k in req
        }
    for key in ("path", "op", "anchor"):
        if key not in prop or not str(prop.get(key, "")).strip():
            print(json.dumps({"ok": False, "error": f"faltou {key}", "offline": True}))
            return 1
    if prop.get("op") in ("replace", "insert") and not str(prop.get("content", "")):
        print(json.dumps({"ok": False, "error": "content obrigatório", "offline": True}))
        return 1

    # Simula footprint local pequeno (fixture); peso real declararia mais.
    ram_mb = 64 + int(req.get("_seed", 0)) % 8
    if ram_mb > args.ram_budget:
        print(
            json.dumps(
                {
                    "ok": False,
                    "error": f"ram {ram_mb}>{args.ram_budget}",
                    "offline": True,
                    "ram_mb": ram_mb,
                }
            )
        )
        return 1

    auth = min(int(req.get("authority", 60)), 120)
    print(
        json.dumps(
            {
                "ok": True,
                "offline": True,
                "ram_mb": ram_mb,
                "proposal": {
                    "path": prop["path"],
                    "op": prop["op"],
                    "anchor": prop["anchor"],
                    "content": prop.get("content", ""),
                },
                "authority": auth,
                "backend": "small_lm_local",
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
