#!/usr/bin/env python3
# A-2c — lock de engine GPU entre agentes (arquivo + protocolo forum).
"""Evita dois engines na mesma 3050 sem coordenacao (docs/F1_AUDIT.md A-2c).

Uso:
  python tools/gpu_engine_lock.py acquire --holder CURSOR --lease-s 1800
  python tools/gpu_engine_lock.py status
  python tools/gpu_engine_lock.py release --holder CURSOR
  python tools/gpu_engine_lock.py selftest
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = Path(os.environ.get("AETHER_GPU_LOCK", str(ROOT / "target" / "gpu_engine.lock.json")))


def now_ts() -> float:
    return time.time()


def read_lock(path: Path) -> dict | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def write_lock(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    tmp.replace(path)


def is_expired(lock: dict, now: float | None = None) -> bool:
    now = now if now is not None else now_ts()
    return float(lock.get("expires_at", 0)) <= now


def acquire(holder: str, lease_s: int, path: Path) -> dict:
    cur = read_lock(path)
    now = now_ts()
    if cur and not is_expired(cur, now) and cur.get("holder") != holder:
        return {
            "ok": False,
            "error": "gpu_busy",
            "holder": cur.get("holder"),
            "expires_at": cur.get("expires_at"),
            "action": "abstain_engine_unavailable",
            "forum_hint": "CLAIM_GPU_ENGINE ou aguardar lease",
        }
    data = {
        "schema": "aether.gpu_engine_lock/v1",
        "holder": holder,
        "acquired_at": now,
        "expires_at": now + lease_s,
        "lease_s": lease_s,
        "acquired_at_utc": datetime.now(timezone.utc).isoformat(),
        "pid": os.getpid(),
    }
    write_lock(path, data)
    return {"ok": True, "lock": data, "path": str(path)}


def release(holder: str, path: Path) -> dict:
    cur = read_lock(path)
    if not cur:
        return {"ok": True, "released": False, "reason": "absent"}
    if cur.get("holder") != holder:
        return {"ok": False, "error": "not_holder", "holder": cur.get("holder")}
    path.unlink(missing_ok=True)
    return {"ok": True, "released": True}


def status(path: Path) -> dict:
    cur = read_lock(path)
    if not cur:
        return {"ok": True, "busy": False}
    expired = is_expired(cur)
    return {
        "ok": True,
        "busy": not expired,
        "expired": expired,
        "lock": cur,
    }


def selftest() -> int:
    path = ROOT / "target" / "gpu_engine_lock_selftest.json"
    path.unlink(missing_ok=True)
    failures: list[str] = []
    a = acquire("CURSOR", 60, path)
    if not a.get("ok"):
        failures.append("acquire CURSOR failed")
    b = acquire("BUFFY", 60, path)
    if b.get("ok") or b.get("error") != "gpu_busy":
        failures.append("second holder should get gpu_busy")
    if b.get("action") != "abstain_engine_unavailable":
        failures.append("busy must map to abstain_engine_unavailable")
    r = release("BUFFY", path)
    if r.get("ok"):
        failures.append("BUFFY cannot release CURSOR lock")
    r2 = release("CURSOR", path)
    if not r2.get("released"):
        failures.append("CURSOR release failed")
    # expired lease
    write_lock(
        path,
        {
            "holder": "OLD",
            "expires_at": now_ts() - 10,
            "acquired_at": now_ts() - 100,
            "lease_s": 1,
        },
    )
    c = acquire("CURSOR", 30, path)
    if not c.get("ok"):
        failures.append("expired lock should be acquirable")
    release("CURSOR", path)
    report = {"ok": not failures, "failures": failures}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p_acq = sub.add_parser("acquire")
    p_acq.add_argument("--holder", required=True)
    p_acq.add_argument("--lease-s", type=int, default=1800)
    p_acq.add_argument("--path", default=str(LOCK_PATH))
    p_rel = sub.add_parser("release")
    p_rel.add_argument("--holder", required=True)
    p_rel.add_argument("--path", default=str(LOCK_PATH))
    p_st = sub.add_parser("status")
    p_st.add_argument("--path", default=str(LOCK_PATH))
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    if args.cmd == "acquire":
        print(json.dumps(acquire(args.holder, args.lease_s, Path(args.path)), ensure_ascii=False, indent=2))
        return 0
    if args.cmd == "release":
        print(json.dumps(release(args.holder, Path(args.path)), ensure_ascii=False, indent=2))
        return 0
    print(json.dumps(status(Path(args.path)), ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
