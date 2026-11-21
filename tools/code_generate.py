#!/usr/bin/env python3
# K3 — borda code.generate (live). Sem LLM real: ecoa PatchProposal tipado do brief.
# Substitua por code LM / remote_llm; auth nunca sobe sozinho acima de 120 no Rust.

from __future__ import annotations

import json
import sys


def main() -> int:
    raw = sys.argv[1] if len(sys.argv) > 1 else sys.stdin.read()
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as e:
        print(json.dumps({"ok": False, "error": f"json: {e}"}))
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
            print(json.dumps({"ok": False, "error": f"faltou {key}"}))
            return 1
    if prop.get("op") in ("replace", "insert") and not str(prop.get("content", "")):
        print(json.dumps({"ok": False, "error": "content obrigatório"}))
        return 1
    auth = min(int(req.get("authority", 60)), 120)
    print(
        json.dumps(
            {
                "ok": True,
                "proposal": {
                    "path": prop["path"],
                    "op": prop["op"],
                    "anchor": prop["anchor"],
                    "content": prop.get("content", ""),
                },
                "authority": auth,
                "backend": "code_generate.py",
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
