#!/usr/bin/env python3
# Posta MSG no FORUM.txt textual (append-only). Espelho Linux de forum_post.ps1.
"""
Escrita canônica no Linux/Cloud Agent: lock exclusivo + MSG_ID = max+1 + UTF-8.

Path (primeira que existir / for definida):
  1) --forum
  2) env FORUM / FORUM_PATH / AETHER_FORUM
  3) C:\\Users\\msrov\\Desktop\\FORUM.txt (Windows)
  4) ./FORUM.txt (cwd / workspace)

Uso:
  python tools/forum_post.py --from CLOUD-CURSOR --type STATUS --topic AETHER_AGENT \\
    --body "ESTADO=P2_DONE" "PROXIMO=P3_SAFETY"
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import time
from pathlib import Path

MSG_ID_RE = re.compile(r"===MSG\s+(\d+)===")
WIN_DEFAULT = Path(r"C:\Users\msrov\Desktop\FORUM.txt")


def resolve_forum(cli: Path | None) -> Path:
    if cli is not None:
        return cli
    for key in ("FORUM", "FORUM_PATH", "AETHER_FORUM"):
        v = os.environ.get(key)
        if v:
            return Path(v)
    if WIN_DEFAULT.exists():
        return WIN_DEFAULT
    return Path.cwd() / "FORUM.txt"


def acquire_lock(lock_path: Path, timeout_s: float = 60.0):
    """Lock exclusivo via fcntl (POSIX) ou msvcrt (Windows)."""
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    fh = open(lock_path, "a+b")
    deadline = time.monotonic() + timeout_s
    while True:
        try:
            if os.name == "nt":
                import msvcrt

                fh.seek(0)
                msvcrt.locking(fh.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl

                fcntl.flock(fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            return fh
        except OSError:
            if time.monotonic() >= deadline:
                fh.close()
                raise TimeoutError(f"timeout aguardando lock {lock_path}")
            time.sleep(0.1)


def release_lock(fh) -> None:
    try:
        if os.name == "nt":
            import msvcrt

            fh.seek(0)
            msvcrt.locking(fh.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(fh.fileno(), fcntl.LOCK_UN)
    finally:
        fh.close()


def next_msg_id(text: str) -> int:
    max_id = 0
    for m in MSG_ID_RE.finditer(text):
        max_id = max(max_id, int(m.group(1)))
    return max_id + 1


def expand_body(parts: list[str]) -> list[str]:
    out: list[str] = []
    for p in parts:
        out.extend(x.strip() for x in p.split("|") if x.strip())
    return out


def post(
    forum: Path,
    frm: str,
    typ: str,
    topic: str,
    to: str,
    deadline: str,
    body: list[str],
) -> int:
    lock_path = forum.with_suffix(forum.suffix + ".lock")
    lock_fh = acquire_lock(lock_path)
    try:
        raw = forum.read_bytes() if forum.exists() else b""
        if raw.startswith(b"\xef\xbb\xbf"):
            raw = raw[3:]
        text = raw.decode("utf-8") if raw else ""
        msg_id = next_msg_id(text)
        id_str = f"{msg_id:04d}"
        lines = [
            f"===MSG {id_str}===",
            f"FROM: {frm}",
            f"TYPE: {typ}",
            f"TOPIC: {topic}",
            f"TO: {to}",
            f"DEADLINE: {deadline}",
            "DATA:",
        ]
        for item in body:
            lines.append(f"  {item}")
        lines.append(f"===END {id_str}===")
        block = "\n".join(lines) + "\n"
        if not text:
            prefix = ""
        elif text.endswith("\n"):
            prefix = "\n"
        else:
            prefix = "\n\n"
        with open(forum, "ab") as out:
            out.write((prefix + block).encode("utf-8"))
            out.flush()
            os.fsync(out.fileno())
        print(f"MSG {id_str} anexada ({frm}/{typ}/{topic})")
        return msg_id
    finally:
        release_lock(lock_fh)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Append MSG UTF-8 ao FORUM.txt")
    ap.add_argument("--forum", type=Path, default=None)
    ap.add_argument("--from", dest="frm", required=True)
    ap.add_argument("--type", dest="typ", required=True)
    ap.add_argument("--topic", required=True)
    ap.add_argument("--to", default="TODOS")
    ap.add_argument("--deadline", default="-")
    ap.add_argument(
        "--body",
        nargs="*",
        default=[],
        help="linhas CHAVE=valor (também aceita a|b|c)",
    )
    args = ap.parse_args(argv)
    forum = resolve_forum(args.forum)
    if not forum.exists() and forum.parent != Path(".") and not forum.parent.exists():
        print(f"erro: path do fórum inacessível: {forum}", file=sys.stderr)
        print(
            "defina FORUM_PATH ou copie FORUM.txt para o workspace",
            file=sys.stderr,
        )
        return 2
    body = expand_body(list(args.body))
    post(forum, args.frm, args.typ, args.topic, args.to, args.deadline, body)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
