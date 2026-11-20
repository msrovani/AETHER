"""Cache de features de fit no disco (Eixo D, item G1 do plano de fechamento).

O `fit` do vegaml le `model.features(state, question)` por exemplo por
pergunta — "reading the rows is the whole cost" (docs do proprio pacote).
As features sao deterministicas do (estado, pergunta) sobre o backbone
CONGELADO, entao o custo de leitura pode ser pago UMA vez e persistido.

IMPORTANTE (achado do ciclo): o `atexit` do Python NAO roda quando o
processo hospedeiro (Rust/PyO3, `auto-initialize`) sai sem finalizar o
interpreter. Por isso o save nao depende de atexit: o hospedeiro chama
`vega_disk_cache.flush(cache_path)` explicitamente apos o fit. O atexit
fica registrado como cinto de reserva para processos Python puros.

Regras de honestidade:
- o cache SO e valido enquanto o backbone (e a calibracao do prompt) nao
  muda — quem troca de modelo/calibracao aponta outro `cache_path` (o
  chamador usa um tag por checkpoint);
- decisao ao vivo tambem passa por aqui, com teto de entradas para nao
  crescer sem limite.

Uso:
    import vega_disk_cache
    vega_disk_cache.install(model, "<dir>/feat_<tag>.pkl")
    ...fit...
    vega_disk_cache.flush()          # persiste tudo que mudou
"""

from __future__ import annotations

import hashlib
import json
import os
import pickle
import threading

MAX_ENTRIES = 20_000
_lock = threading.Lock()
_registry: dict = {}  # cache_path -> {"data": dict, "dirty": bool, "max": int}
_engine_registry: dict = {}  # idem, para respostas do engine (leitura 2 do fit)


def _norm(x) -> str:
    if isinstance(x, str):
        return x
    try:
        return json.dumps(x, sort_keys=True, ensure_ascii=False, default=str)
    except Exception:
        return repr(x)


def _key(state, question) -> str:
    h = hashlib.sha256()
    h.update(_norm(state).encode("utf-8"))
    h.update(b"\x00")
    h.update(_norm(question).encode("utf-8"))
    return h.hexdigest()


def _load(cache_path: str) -> dict:
    if not os.path.exists(cache_path):
        return {}
    try:
        with open(cache_path, "rb") as f:
            loaded = pickle.load(f)
        return loaded if isinstance(loaded, dict) else {}
    except Exception:
        return {}  # cache corrompido e re-constituivel: descarta, nao aborta


def install(model, cache_path: str, max_entries: int = MAX_ENTRIES):
    """Embrulha `model.features` com cache de disco. Devolve o model."""
    cache_dir = os.path.dirname(cache_path)
    if cache_dir:
        os.makedirs(cache_dir, exist_ok=True)
    with _lock:
        if cache_path not in _registry:
            _registry[cache_path] = {"data": _load(cache_path), "dirty": False, "max": max_entries}
        state = _registry[cache_path]
    data = state["data"]

    original = model.features

    def cached(state_arg, question, *args, **kwargs):
        k = _key(state_arg, question)
        with _lock:
            v = data.get(k)
        if v is not None:
            return v
        v = original(state_arg, question, *args, **kwargs)
        with _lock:
            if len(data) < state["max"]:
                data[k] = v
                state["dirty"] = True
        return v

    model.features = cached
    return model


def install_engine(model, cache_path: str, fingerprint: str, max_entries: int = MAX_ENTRIES):
    """Embrulha `model.engine.decide` com cache de disco (leitura 2 do fit).

    `_engine_accuracy` decide cada exemplo CV de novo — com as features em
    cache, esta SEGUNDA leitura vira o custo dominante do bootstrap. As
    respostas do engine sao deterministicas para o MESMO (estado, pergunta,
    pesos, calibracao), entao o cache vale como memoizacao pura — e o
    `fingerprint` (path+config do checkpoint, decidido pelo hospedeiro)
    invalida tudo se o motor mudar.
    """
    cache_dir = os.path.dirname(cache_path)
    if cache_dir:
        os.makedirs(cache_dir, exist_ok=True)
    with _lock:
        if cache_path not in _engine_registry:
            _engine_registry[cache_path] = {
                "data": _load(cache_path),
                "dirty": False,
                "max": max_entries,
            }
        state = _engine_registry[cache_path]
    data = state["data"]
    fp = fingerprint.encode("utf-8")

    original = model.engine.decide

    def cached_decide(state_arg, questions, *args, **kwargs):
        h = hashlib.sha256()
        h.update(fp)
        h.update(b"\x00")
        h.update(_norm(state_arg).encode("utf-8"))
        h.update(b"\x00")
        h.update(_norm(questions).encode("utf-8"))
        k = h.hexdigest()
        with _lock:
            v = data.get(k)
        if v is not None:
            return v
        v = original(state_arg, questions, *args, **kwargs)
        with _lock:
            if len(data) < state["max"]:
                data[k] = v
                state["dirty"] = True
        return v

    model.engine.decide = cached_decide
    return model


def flush(cache_path: str | None = None) -> int:
    """Persiste os caches alterados. Devolve quantos arquivos foram gravados."""
    saved = 0
    with _lock:
        all_regs = [(_registry, "features"), (_engine_registry, "engine")]
    for reg, _kind in all_regs:
        with _lock:
            targets = [p for p, st in reg.items() if st["dirty"]]
        for p in targets:
            if cache_path is not None and p != cache_path:
                continue
            with _lock:
                st = reg[p]
                if not st["dirty"]:
                    continue
                try:
                    tmp = p + ".tmp"
                    with open(tmp, "wb") as f:
                        pickle.dump(st["data"], f, protocol=4)
                    os.replace(tmp, p)
                    st["dirty"] = False
                    saved += 1
                except Exception:
                    pass  # falha de cache nunca derruba o engine
    return saved


def stats() -> dict:
    """Tamanho dos caches, para o log do bootstrap (evidencia, nao promessa)."""
    with _lock:
        out = {p: len(st["data"]) for p, st in _registry.items()}
    with _lock:
        out.update({p: len(st["data"]) for p, st in _engine_registry.items()})
    return out
