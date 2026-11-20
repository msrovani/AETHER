#!/usr/bin/env python3
"""Embedder semântico local (Eixo C, tomada 3 do plano F2→F3).

Serve o contrato HTTP do trait `Embedder` do neural-sgdb, sem framework:

    GET  /health                            -> {"ok", "model", "dim", "offline", ...}
    POST /embed        {"text": "..."}      -> {"embedding": [...]}
    POST /embed_batch  {"texts": [...]}     -> {"embeddings": [[...], ...]}

Política de rede (plano F2→F3, Eixo C):
    * default = OFFLINE (`HF_HUB_OFFLINE=1`): o servidor NUNCA baixa nada;
    * `AETHER_EMBED_ALLOW_DOWNLOAD=1` habilita a aquisição ÚNICA do modelo
      (setup explícito) — depois disso volte ao default offline;
    * socket fora do inventário (`/health` reporta `offline`) é bug.

Uso:
    .venv-py/Scripts/python.exe tools/embed_server.py [--port 8788] [--model NAME]
    AETHER_EMBED_MODEL / AETHER_EMBED_PORT / AETHER_EMBED_THREADS / AETHER_EMBED_ALLOW_DOWNLOAD
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

DEFAULT_MODEL = "sentence-transformers/all-MiniLM-L6-v2"
DEFAULT_PORT = 8788
BATCH_SIZE = 32


def env_flag(name: str) -> bool:
    return os.environ.get(name, "").strip().lower() in ("1", "true", "yes", "on")


def load_model(name: str, allow_download: bool):
    """Carrega o modelo; offline por default (nenhuma chamada externa)."""
    os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
    if allow_download:
        os.environ["HF_HUB_OFFLINE"] = "0"
    else:
        os.environ.setdefault("HF_HUB_OFFLINE", "1")
    from sentence_transformers import SentenceTransformer

    return SentenceTransformer(name, device="cpu")


class State:
    model = None
    model_name = DEFAULT_MODEL
    offline = True
    dim = 0


def embed(texts: list[str]) -> list[list[float]]:
    vecs = State.model.encode(
        texts,
        normalize_embeddings=True,
        convert_to_numpy=True,
        batch_size=BATCH_SIZE,
    )
    return [[float(x) for x in v] for v in vecs]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):  # silencioso: latência medida pelo cliente
        pass

    def _send(self, code: int, obj) -> None:
        body = json.dumps(obj).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            return self._send(
                200,
                {
                    "ok": State.model is not None,
                    "model": State.model_name,
                    "dim": State.dim,
                    "normalized": True,
                    "offline": State.offline,
                    "backend": "sentence_transformers/cpu",
                },
            )
        return self._send(404, {"error": f"unknown path {self.path}"})

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(n)
        try:
            req = json.loads(raw or b"{}")
        except Exception as e:  # noqa: BLE001
            return self._send(400, {"error": f"bad json: {e}"})
        try:
            if self.path == "/embed":
                return self._send(200, {"embedding": embed([req.get("text", "")])[0]})
            if self.path == "/embed_batch":
                texts = req.get("texts", [])
                if not texts:
                    return self._send(200, {"embeddings": []})
                if not isinstance(texts, list) or not all(isinstance(t, str) for t in texts):
                    return self._send(400, {"error": "texts deve ser lista de strings"})
                return self._send(200, {"embeddings": embed(texts)})
            return self._send(404, {"error": f"unknown path {self.path}"})
        except Exception as e:  # noqa: BLE001
            # Exceção do motor vira erro tipado — nunca derruba o servidor.
            return self._send(500, {"error": str(e)})


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="embed_server AETHER (384d, offline por default)")
    ap.add_argument("--model", default=os.environ.get("AETHER_EMBED_MODEL", DEFAULT_MODEL))
    ap.add_argument("--port", type=int, default=int(os.environ.get("AETHER_EMBED_PORT", DEFAULT_PORT)))
    ap.add_argument(
        "--allow-download",
        action="store_true",
        default=env_flag("AETHER_EMBED_ALLOW_DOWNLOAD"),
        help="aquisição única do modelo (setup explícito); default = offline",
    )
    args = ap.parse_args(argv)

    State.model_name = args.model
    State.offline = not args.allow_download
    print(f"embed_server: loading {args.model} (offline={State.offline}) ...", flush=True)
    State.model = load_model(args.model, args.allow_download)
    State.dim = int(State.model.get_sentence_embedding_dimension())
    print(f"embed_server: model ready dim={State.dim}", flush=True)

    srv = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"embed_server: listening on http://127.0.0.1:{args.port}", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        srv.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
