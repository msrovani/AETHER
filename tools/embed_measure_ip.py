#!/usr/bin/env python3
"""Mediação atômica de um embedder no set congelado (2026-11-20, G8-V3-A).

Estratégia de venenidiço: TODO em um processo —
  * servidor `embed_server.Handler` em thread (em-processo) sobre a porta 8790
    (você decide); sem órfãos de HTTP separado, sem SOQUEJOS que morrem;
  * o eval `embed_eval.py --run` como SUBPROCESSO com URL apontando ao thread;
  * after: só imprime o placar das 3 sistemas + gate.

Uso: python tools/embed_measure_ip.py <MODEL_ID> <TAG> [TIMEOUT_S]
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
from http.server import ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))


def main() -> int:
    model_id = sys.argv[1]
    tag = sys.argv[2]
    timeout_s = sys.argv[3] if len(sys.argv) > 3 else "2400"

    # kill órfãos (inútil hoje, segurança a longo prazo)
    subprocess.run(
        ["taskkill", "/F", "/IM", "python.exe", "/T", "/FI", "WindowTITLE", "embed_server*"],
        capture_output=True,
        check=False,
    )

    import embed_server

    # V3-A: medição no GPU bf16 (condição única para os 3 modelos; CPU fp32
    # foi medido inviável: 413s/batch -> ~4h de eval).
    os.environ.setdefault("AETHER_EMBED_CUDA", "1")
    port = 8790
    embed_server.State.model_name = model_id
    embed_server.State.offline = False
    print(f"carregando {model_id} (cuda bf16) ...", flush=True)
    t0 = time.time()
    embed_server.State.model = embed_server.load_model(model_id, allow_download=True)
    embed_server.State.dim = int(embed_server.State.model.get_embedding_dimension())
    print(f"carregado em {time.time() - t0:.0f}s | dim={embed_server.State.dim}", flush=True)

    srv = ThreadingHTTPServer(("127.0.0.1", port), embed_server.Handler)
    th = threading.Thread(target=srv.serve_forever, daemon=True)
    th.start()

    # health em loop
    import urllib.request

    for _ in range(24):
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{port}/health", timeout=10
            ) as r:
                print("health:", r.read().decode()[:150], flush=True)
                break
        except Exception:
            time.sleep(1)
    else:
        print("falhou no health")
        srv.shutdown()
        return 1

    py = ROOT / ".venv-py" / "Scripts" / "python.exe"
    cmd = [
        str(py),
        str(ROOT / "tools" / "embed_eval.py"),
        "--run",
        "--out",
        str(ROOT / "target" / f"embed_eval_{tag}.json"),
        "--sha256",
        "c422a1a00fc18c9335b437f507f8bf250f8df2a1689f3ef2f2259c7fad0f3397",
    ]
    env = dict(os.environ)
    env["AETHER_EVAL_SET"] = str(ROOT / "data" / "embed_eval_set_v2.jsonl")
    env["AETHER_EMBED_URL"] = f"http://127.0.0.1:{port}"
    env["AETHER_EVAL_TIMEOUT_S"] = timeout_s
    print("=== eval", tag, model_id, "===")
    p = subprocess.run(cmd, cwd=ROOT, env=env)

    art = ROOT / "target" / f"embed_eval_{tag}.json"
    if art.exists():
        d = json.loads(art.read_text(encoding="utf-8"))
        s = d["systems"]
        print("PLACAR", tag)
        print("  bm25       ", s["lexical_bm25"]["hit5"])
        print("  embed      ", s["embed_minilm"]["hit5"], "| dim", d["dim"], "| latency", d["latency"])
    srv.shutdown()
    srv.server_close()
    return p.returncode


if __name__ == "__main__":
    sys.exit(main())
