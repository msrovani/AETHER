#!/usr/bin/env python3
# FORUM textual UTF-8 (MSG 0001) — verify/dump/next-id; escrita canônica = forum_post.ps1.
"""
FORUM: protocolo textual append-only (PROJETO AETHER).

Verdade operacional (MSG 0001):
  - PATH = C:\\Users\\msrov\\Desktop\\FORUM.txt
  - ENCODING = utf-8
  - append_only; um bloco ===MSG N=== ... ===END N=== por mensagem
  - escrita canônica: tools/forum_post.ps1 (lock + MSG_ID sequencial)

O protocolo binário hex (magic 0xA3) está DEPRECATED. Subcomandos
legacy-binary-* existem só para auditoria de dumps antigos, se houver.

Uso:
  python tools/forum.py verify
  python tools/forum.py dump [--last N]
  python tools/forum.py next-id
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

def _default_forum() -> Path:
    """Path do fórum: env → Desktop Windows → ./FORUM.txt (Cloud/Linux)."""
    import os

    for key in ("FORUM", "FORUM_PATH", "AETHER_FORUM"):
        v = os.environ.get(key)
        if v:
            return Path(v)
    win = Path(r"C:\Users\msrov\Desktop\FORUM.txt")
    if win.exists():
        return win
    local = Path.cwd() / "FORUM.txt"
    if local.exists():
        return local
    # preferência documental no Windows; fallback local para Cloud Agent
    return win if os.name == "nt" else local


DEFAULT_FORUM = _default_forum()

MSG_START = re.compile(r"^===MSG\s+(\d+)===\s*$")
MSG_END = re.compile(r"^===END\s+(\d+)===\s*$")
HEADER_KV = re.compile(r"^(FROM|TYPE|TOPIC|TO|DEADLINE):\s*(.*)$")


@dataclass
class Message:
    msg_id: int
    start_line: int
    end_line: int | None = None
    headers: dict[str, str] = field(default_factory=dict)
    data_lines: list[str] = field(default_factory=list)
    index: int = 0  # ordem de aparição no arquivo (1-based)


def read_forum_text(path: Path) -> tuple[str, list[str]]:
    """Lê o fórum. Preferência UTF-8 (MSG 0001); fallback cp1252/latin-1 se legado."""
    warns: list[str] = []
    if not path.exists():
        return "", warns
    raw = path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf"):
        raw = raw[3:]
        warns.append("BOM UTF-8 removido na leitura")
    try:
        return raw.decode("utf-8"), warns
    except UnicodeDecodeError as e:
        warns.append(
            f"arquivo nao e UTF-8 estrito ({e}); fallback cp1252 — "
            "novos posts devem ser UTF-8 via forum_post.ps1"
        )
        try:
            return raw.decode("cp1252"), warns
        except UnicodeDecodeError:
            warns.append("fallback latin-1 (bytes preservados)")
            return raw.decode("latin-1"), warns


def parse_messages(text: str) -> tuple[list[Message], list[str]]:
    """Parseia blocos MSG. Retorna (mensagens, avisos)."""
    warnings: list[str] = []
    messages: list[Message] = []
    current: Message | None = None
    in_data = False
    appearance = 0

    for n, line in enumerate(text.splitlines(), start=1):
        raw = line.rstrip("\r")
        m_start = MSG_START.match(raw)
        if m_start:
            if current is not None:
                warnings.append(
                    f"linha {n}: novo MSG {int(m_start.group(1)):04d} "
                    f"antes de fechar MSG {current.msg_id:04d} (linha {current.start_line})"
                )
                messages.append(current)
            appearance += 1
            current = Message(
                msg_id=int(m_start.group(1)),
                start_line=n,
                index=appearance,
            )
            in_data = False
            continue

        m_end = MSG_END.match(raw)
        if m_end:
            eid = int(m_end.group(1))
            if current is None:
                warnings.append(f"linha {n}: END {eid:04d} sem START")
                continue
            if eid != current.msg_id:
                warnings.append(
                    f"linha {n}: END {eid:04d} != START {current.msg_id:04d} "
                    f"(linha {current.start_line})"
                )
            current.end_line = n
            messages.append(current)
            current = None
            in_data = False
            continue

        if current is None:
            if raw.strip():
                # prosa/ruído fora de bloco — ignorar com aviso leve só 1x por corrida
                pass
            continue

        if raw.strip() == "DATA:":
            in_data = True
            continue

        hm = HEADER_KV.match(raw)
        if hm and not in_data:
            current.headers[hm.group(1)] = hm.group(2).strip()
            continue

        if in_data:
            if raw.startswith("  ") or raw.startswith("\t"):
                current.data_lines.append(raw.strip())
            elif raw.strip():
                # linha DATA sem indent — ainda captura
                current.data_lines.append(raw.strip())

    if current is not None:
        warnings.append(
            f"MSG {current.msg_id:04d} aberto na linha {current.start_line} sem END"
        )
        messages.append(current)

    return messages, warnings


def find_duplicate_ids(messages: list[Message]) -> dict[int, list[Message]]:
    by_id: dict[int, list[Message]] = {}
    for msg in messages:
        by_id.setdefault(msg.msg_id, []).append(msg)
    return {k: v for k, v in by_id.items() if len(v) > 1}


def cmd_verify(path: Path) -> int:
    text, enc_warns = read_forum_text(path)
    if not text:
        print(f"OK: fórum vazio ou ausente ({path})")
        return 0

    messages, warnings = parse_messages(text)
    warnings = enc_warns + warnings
    dups = find_duplicate_ids(messages)

    for w in warnings:
        print(f"WARN: {w}")

    if dups:
        print(f"WARN: {len(dups)} MSG_ID(s) duplicado(s) (append-only — não reescrever):")
        for mid in sorted(dups):
            parts = []
            for m in dups[mid]:
                topic = m.headers.get("TOPIC", "?")
                frm = m.headers.get("FROM", "?")
                parts.append(f"INDEX={m.index}/L{m.start_line}/{frm}/{topic}")
            print(f"  MSG {mid:04d} x{len(dups[mid])}: " + " | ".join(parts))

    max_id = max((m.msg_id for m in messages), default=0)
    print(
        f"OK: utf8 parseavel, {len(messages)} blocos, "
        f"{len(set(m.msg_id for m in messages))} ids unicos, "
        f"max={max_id:04d}, next={max_id + 1:04d}, "
        f"dups={len(dups)}, warns={len(warnings)}"
    )
    # dups históricos não são erro fatal — append-only
    return 0


def cmd_dump(path: Path, last: int) -> int:
    text, _ = read_forum_text(path)
    messages, _ = parse_messages(text)
    show = messages[-last:] if last > 0 else messages
    for m in show:
        topic = m.headers.get("TOPIC", "?")
        frm = m.headers.get("FROM", "?")
        typ = m.headers.get("TYPE", "?")
        print(
            f"[{m.index}] MSG {m.msg_id:04d} L{m.start_line} "
            f"{frm}/{typ}/{topic} data={len(m.data_lines)}"
        )
    return 0


def cmd_next_id(path: Path) -> int:
    text, _ = read_forum_text(path)
    messages, _ = parse_messages(text)
    max_id = max((m.msg_id for m in messages), default=0)
    print(f"{max_id + 1:04d}")
    return 0


def cmd_legacy_help() -> int:
    print(
        "DEPRECATED: protocolo binário hex removido da verdade operacional.\n"
        "Use tools/forum_post.ps1 para escrever e 'forum.py verify|dump|next-id'.\n"
        "Ver FORUM_PROTOCOL.md e MSG 0001 em FORUM.txt."
    )
    return 2


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(
        description="FORUM textual UTF-8 do PROJETO AETHER (MSG 0001)"
    )
    ap.add_argument("--forum", type=Path, default=DEFAULT_FORUM)
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("verify", help="valida parse UTF-8 e reporta dups")
    d = sub.add_parser("dump", help="lista mensagens")
    d.add_argument("--last", type=int, default=0, help="só as N últimas")
    sub.add_parser("next-id", help="próximo MSG_ID (max+1)")

    # aliases deprecados — não quebram scripts antigos, só orientam
    for name in ("init", "send", "propose"):
        sub.add_parser(name, help="DEPRECATED (binário)")

    args = ap.parse_args(argv)
    path: Path = args.forum

    if args.cmd == "verify":
        return cmd_verify(path)
    if args.cmd == "dump":
        return cmd_dump(path, args.last)
    if args.cmd == "next-id":
        return cmd_next_id(path)
    if args.cmd in ("init", "send", "propose"):
        return cmd_legacy_help()
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
