#!/usr/bin/env python3
# A-2c — tranca da GPU (lock de engine). MSG 1014 / selftest isolado.
"""Serializa o uso do engine por arquivo, com TTL.

Producao: `.vega/engine.lock` (fora de target/). Override: AETHER_ENGINE_LOCK.
Selftest usa `.vega/engine.lock.selftest` — nao disputa tranca viva.

Uso:
  python tools/engine_lock.py status
  python tools/engine_lock.py acquire OPENCODE 1800
  python tools/engine_lock.py wait OPENCODE 1800
  python tools/engine_lock.py release OPENCODE
  python tools/engine_lock.py selftest
"""

from __future__ import annotations

import argparse
import errno
import json
import os
import socket
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK_DIR = ROOT / ".vega"
DEFAULT_LOCK = LOCK_DIR / "engine.lock"
STALE_AFTER = 30


def _lock_path() -> Path:
    return Path(os.environ.get("AETHER_ENGINE_LOCK", str(DEFAULT_LOCK)))


def _now() -> float:
    return time.time()


def _read() -> dict | None:
    lock = _lock_path()
    if not lock.is_file():
        return None
    try:
        with open(lock, "r", encoding="utf-8") as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError):
        return None


def _write(rec: dict) -> None:
    lock = _lock_path()
    lock.parent.mkdir(parents=True, exist_ok=True)
    tmp = lock.with_suffix(".tmp")
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(rec, f, indent=2)
        f.write("\n")
    os.replace(tmp, lock)


def _is_alive(rec: dict | None) -> bool:
    if not rec:
        return False
    ttl = float(rec.get("ttl_s", 0))
    if ttl > 0 and _now() > float(rec.get("expires_at", 0)):
        return False
    if _now() - float(rec.get("at", 0)) > STALE_AFTER and _now() > float(
        rec.get("expires_at", 0)
    ):
        return False
    return True


def status() -> dict:
    rec = _read()
    alive = _is_alive(rec)
    lock = _lock_path()
    try:
        rel = str(lock.resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        rel = str(lock)
    return {
        "schema": "aether.engine_lock/v1",
        "locked": alive,
        "owner": (rec or {}).get("owner"),
        "pid": (rec or {}).get("pid"),
        "expires_at": (rec or {}).get("expires_at"),
        "path": rel,
        "stale_after_s": STALE_AFTER,
    }


def acquire(owner: str, ttl_s: int) -> int:
    lock = _lock_path()
    lock.parent.mkdir(parents=True, exist_ok=True)
    rec = {
        "owner": owner,
        "pid": os.getpid(),
        "host": socket.gethostname(),
        "at": _now(),
        "ttl_s": ttl_s,
        "expires_at": _now() + ttl_s,
    }
    try:
        fd = os.open(str(lock), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except OSError as e:
        if e.errno != errno.EEXIST:
            raise
        cur = _read()
        if _is_alive(cur):
            print(
                f"[engine_lock] ocupado por {cur.get('owner')} "
                f"(expira em {int(cur.get('expires_at', 0) - _now())}s) — "
                f"espere ou faca outra coisa; NAO rode o engine junto.",
                file=sys.stderr,
            )
            return 3
        _write(rec)
        print(
            f"[engine_lock] tranca tomada (lock morto removido) por {owner}",
            file=sys.stderr,
        )
        return 0
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        json.dump(rec, f, indent=2)
        f.write("\n")
    print(f"[engine_lock] tranca tomada por {owner} (ttl {ttl_s}s)", file=sys.stderr)
    return 0


def wait(owner: str, ttl_s: int, timeout_s: int) -> int:
    t0 = _now()
    while True:
        rc = acquire(owner, ttl_s)
        if rc == 0:
            return 0
        if _now() - t0 > timeout_s:
            print("[engine_lock] timeout esperando a tranca", file=sys.stderr)
            return 3
        time.sleep(5)


def release(owner: str) -> int:
    rec = _read()
    if rec and rec.get("owner") != owner:
        print(
            f"[engine_lock] a tranca e de {rec.get('owner')}, nao de {owner}",
            file=sys.stderr,
        )
        return 3
    lock = _lock_path()
    if lock.is_file():
        lock.unlink()
    print(f"[engine_lock] tranca devolvida por {owner}", file=sys.stderr)
    return 0


def selftest() -> int:
    """Isolado: nao disputa `.vega/engine.lock` vivo."""
    failures: list[str] = []
    prev = os.environ.get("AETHER_ENGINE_LOCK")
    os.environ.pop("AETHER_ENGINE_LOCK", None)
    prod = status()
    if prod.get("path") != ".vega/engine.lock":
        failures.append(f"prod_path want=.vega/engine.lock got={prod.get('path')}")

    test_lock = LOCK_DIR / "engine.lock.selftest"
    os.environ["AETHER_ENGINE_LOCK"] = str(test_lock)
    try:
        if test_lock.is_file():
            test_lock.unlink()
        st = status()
        if "engine.lock.selftest" not in str(st.get("path", "")):
            failures.append(f"path_isolado got={st.get('path')}")
        rc1 = acquire("CURSOR_SELFTEST", 60)
        if rc1 != 0:
            failures.append(f"acquire_owner exit={rc1}")
        rc2 = acquire("CURSOR_OTHER", 60)
        if rc2 != 3:
            failures.append(f"segundo_holder want_exit=3 got={rc2}")
            release("CURSOR_OTHER")
        rc3 = release("CURSOR_SELFTEST")
        if rc3 != 0:
            failures.append(f"release exit={rc3}")
        if status().get("locked"):
            failures.append("ainda_locked_apos_release")
    finally:
        if test_lock.is_file():
            try:
                test_lock.unlink()
            except OSError:
                pass
        if prev is None:
            os.environ.pop("AETHER_ENGINE_LOCK", None)
        else:
            os.environ["AETHER_ENGINE_LOCK"] = prev

    out = {
        "ok": len(failures) == 0,
        "failures": failures,
        "canonical_path": ".vega/engine.lock",
        "prod_locked": prod.get("locked"),
        "prod_owner": prod.get("owner"),
        "network": False,
        "ref": "MSG_1014_A2C_SELFTEST_ISOLADO",
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "mode",
        choices=["status", "acquire", "wait", "release", "selftest"],
    )
    ap.add_argument("owner", nargs="?", default=os.environ.get("USER", "anon"))
    ap.add_argument("ttl_s", nargs="?", type=int, default=1800)
    ap.add_argument("--timeout", type=int, default=900)
    a = ap.parse_args()

    if a.mode == "selftest":
        return selftest()
    if a.mode == "status":
        print(json.dumps(status(), indent=2, ensure_ascii=False))
        return 0
    if a.mode == "acquire":
        return acquire(a.owner, a.ttl_s)
    if a.mode == "wait":
        return wait(a.owner, a.ttl_s, a.timeout)
    if a.mode == "release":
        return release(a.owner)
    return 2


if __name__ == "__main__":
    sys.exit(main())
