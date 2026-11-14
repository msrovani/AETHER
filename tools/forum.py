#!/usr/bin/env python3
"""
FORUM: protocolo binário append-only para os 3 nós do PROJETO AETHER.

Regras do arquivo (FORUM.txt):
  - uma linha por frame, hexadecimal maiúsculo, sem comentários, sem texto
  - apenas acréscimo (append); nunca editar nem apagar linhas antigas
  - seq estritamente crescente; CRC32 valida cada frame
  - vocabulário compartilhado: o frame DICT (0x02) é a única fonte de verdade dos tokens
    e NUNCA pode ser reescrito; novos tokens só podem ser acrescentados em DICT posteriores

Uso:
  python tools/forum.py init              # semeia o fórum (só se estiver vazio)
  python tools/forum.py verify            # valida magic/CRC/seq do arquivo inteiro
  python tools/forum.py dump              # traduz frames para palavras (leitura humana)
  python tools/forum.py send --src 2 --type vote --payload 00 03 01 19
  python tools/forum.py propose --src 2 --id 4 --tokens m2_wave ack review
"""
from __future__ import annotations

import argparse
import struct
import sys
import zlib
from pathlib import Path

MAGIC = 0xA3
VERSION = 0x01
DEFAULT_FORUM = Path(r"C:\Users\msrov\Desktop\FORUM.txt")

# magic(1) ver(1) seq(2) src(1) dst(1) type(1) ref(2) len(2) = 11 bytes
HDR = struct.Struct(">BBHBBHH")
HDR_LEN = HDR.size
CRC_LEN = 4

# --- tipos de frame -------------------------------------------------------
T_ANNOUNCE = 0x01
T_DICT = 0x02
T_CLAIM = 0x03
T_PROPOSE = 0x04
T_VOTE = 0x05
T_STATUS = 0x06
T_COMMIT = 0x07
T_ALERT = 0x08
T_RULE = 0x09
T_DONE = 0x0A

TYPE_BY_NAME = {
    "announce": T_ANNOUNCE,
    "dict": T_DICT,
    "claim": T_CLAIM,
    "propose": T_PROPOSE,
    "vote": T_VOTE,
    "status": T_STATUS,
    "commit": T_COMMIT,
    "alert": T_ALERT,
    "rule": T_RULE,
    "done": T_DONE,
}
TYPE_NAMES = {v: k for k, v in TYPE_BY_NAME.items()}

# --- missões e estados ----------------------------------------------------
MISSIONS = {
    0: "m0_scaffold",
    1: "m1_thermo",
    2: "m2_wave",
    3: "m3_kuramoto",
    4: "m4_organism",
}
MISSION_BY_NAME = {v: k for k, v in MISSIONS.items()}

STATES = {0: "proposed", 1: "claimed", 2: "in_progress", 3: "review", 4: "done", 5: "blocked"}
STATE_BY_NAME = {v: k for k, v in STATES.items()}

VOTES = {0: "abstain", 1: "yes", 2: "no"}

# --- vocabulário compartilhado -------------------------------------------
# ORDEM IMUTÁVEL: o id é índice+1. Nunca reordene; só acrescente no fim.
LEXICON = [
    "aether", "forum", "protocol", "v1", "append_only", "one_frame_per_line",
    "hex_uppercase", "seq_monotonic", "crc32", "no_edit_delete", "claim_before_code",
    "vote_quorum_2of3", "no_force_push", "tests_release", "mission", "m0_scaffold",
    "m1_thermo", "m2_wave", "m3_kuramoto", "m4_organism", "mission_split", "claim",
    "vote", "yes", "no", "abstain", "propose", "accept", "reject", "status", "ready",
    "blocked", "done", "owner", "lease", "ttl", "seconds", "module", "test",
    "invariant", "benchmark", "latency", "rust", "cargo", "udp", "packet40", "crates",
    "ndarray", "rustfft", "mio", "bytemuck", "node1", "node2", "node3", "buffy",
    "peer_cursor", "peer_opencode", "handoff", "review", "merge", "branch", "conflict",
    "escalate_human", "ack", "ping", "deadline", "start", "stop", "forum_tool",
    "bootstrap", "quorum", "human_arbiter", "binary_only", "no_json", "no_prose",
    "pending_vote", "main", "rename_ok", "crc_validated", "vocabulary",
    "decode_required", "register_node",
]
TOKEN_ID = {w: i + 1 for i, w in enumerate(LEXICON)}
ID_TOKEN = {i + 1: w for i, w in enumerate(LEXICON)}

ESC_U8 = 0xFE   # seguido de 1 byte: número inteiro 0..255
ESC_U16 = 0xFD  # seguido de 2 bytes BE: 256..65535
SEP = 0xFF      # quebra de cláusula


# --- primitivas -----------------------------------------------------------
def build_frame(seq: int, src: int, dst: int, typ: int, payload: bytes = b"", ref: int = 0) -> bytes:
    hdr = HDR.pack(MAGIC, VERSION, seq & 0xFFFF, src & 0xFF, dst & 0xFF, typ & 0xFF, ref & 0xFFFF, len(payload))
    crc = zlib.crc32(hdr[1:] + payload) & 0xFFFFFFFF
    return hdr + payload + struct.pack(">I", crc)


def parse_frame(raw: bytes) -> dict:
    if len(raw) < HDR_LEN + CRC_LEN:
        raise ValueError(f"frame curto: {len(raw)} bytes")
    magic, ver, seq, src, dst, typ, ref, ln = HDR.unpack(raw[:HDR_LEN])
    if magic != MAGIC:
        raise ValueError(f"magic inválido: 0x{magic:02X}")
    if ver != VERSION:
        raise ValueError(f"versão não suportada: {ver}")
    if len(raw) != HDR_LEN + ln + CRC_LEN:
        raise ValueError(f"len inconsistente: header={ln} total={len(raw)}")
    payload = raw[HDR_LEN:HDR_LEN + ln]
    (crc,) = struct.unpack(">I", raw[HDR_LEN + ln:])
    if crc != (zlib.crc32(raw[1:HDR_LEN + ln]) & 0xFFFFFFFF):
        raise ValueError(f"CRC inválido no seq {seq}")
    return {"seq": seq, "ver": ver, "src": src, "dst": dst, "type": typ,
            "ref": ref, "payload": payload, "crc": crc}


def encode_tokens(tokens: list[str]) -> bytes:
    out = bytearray()
    for tok in tokens:
        if tok.isdigit():
            n = int(tok)
            if n <= 0xFF:
                out += bytes([ESC_U8, n])
            elif n <= 0xFFFF:
                out += bytes([ESC_U16]) + struct.pack(">H", n)
            else:
                raise ValueError(f"número fora de faixa: {n}")
        elif tok in ("|", "."):
            out.append(SEP)
        else:
            try:
                out.append(TOKEN_ID[tok])
            except KeyError:
                raise SystemExit(f"token desconhecido: {tok}\nadicione-o ao LEXICON (nunca reordene)")
    return bytes(out)


def decode_tokens(payload: bytes) -> list[str]:
    out: list[str] = []
    i = 0
    while i < len(payload):
        b = payload[i]
        if b == SEP:
            out.append("|")
            i += 1
        elif b == ESC_U8:
            out.append(str(payload[i + 1]))
            i += 2
        elif b == ESC_U16:
            out.append(str(struct.unpack(">H", payload[i + 1:i + 3])[0]))
            i += 3
        else:
            out.append(ID_TOKEN.get(b, f"?{b:02X}"))
            i += 1
    return out


# --- payloads tipados -----------------------------------------------------
def p_announce(name: str, caps: int = 0x0000_000F) -> bytes:
    nb = name.encode("utf-8")[:16]
    return struct.pack(">I", caps) + nb


def p_dict(entries: list[str]) -> bytes:
    """DICT envia o vocabulário in-band: id u8 + len u8 + ascii."""
    out = bytearray()
    for i, w in enumerate(entries, start=1):
        b = w.encode("ascii")[:255]
        out += bytes([i, len(b)]) + b
    return bytes(out)


def p_claim(mission: int, prio: int = 1, eta: int = 900, ttl: int = 900, flags: int = 0) -> bytes:
    return struct.pack(">BBHHB", mission, prio, eta & 0xFFFF, ttl & 0xFFFF, flags)


def p_propose(pid: int, tokens: list[str]) -> bytes:
    return bytes([pid]) + encode_tokens(tokens)


def p_vote(target: int, vote: int, reason: str = "ack") -> bytes:
    return struct.pack(">HBB", target & 0xFFFF, vote, TOKEN_ID.get(reason, 64))


def p_status(mission: int, state: int, pct: int) -> bytes:
    return struct.pack(">BBB", mission, state, pct)


def p_commit(mission: int, sha: str) -> bytes:
    return bytes([mission]) + bytes.fromhex(sha)[:8].ljust(8, b"\x00")


# --- arquivo --------------------------------------------------------------
def read_frames(path: Path) -> list[dict]:
    if not path.exists():
        return []
    frames = []
    for n, line in enumerate(path.read_text(encoding="ascii").splitlines(), start=1):
        line = line.strip().replace(" ", "")
        if not line:
            continue
        if line.upper() != line or any(c not in "0123456789ABCDEF" for c in line):
            raise SystemExit(f"linha {n}: esperado hex maiúsculo, obtido {line[:40]!r}")
        try:
            frames.append(parse_frame(bytes.fromhex(line)))
        except ValueError as e:
            raise SystemExit(f"linha {n}: {e}")
    return frames


def next_seq(frames: list[dict]) -> int:
    return (max(f["seq"] for f in frames) + 1) if frames else 1


def append_frames(path: Path, frames: list[bytes]) -> None:
    with path.open("a", encoding="ascii", newline="\n") as fh:
        for fr in frames:
            fh.write(fr.hex().upper() + "\n")


# --- comandos -------------------------------------------------------------
def cmd_verify(path: Path) -> int:
    frames = read_frames(path)
    last = 0
    for f in frames:
        if f["seq"] <= last:
            raise SystemExit(f"seq não monotônico: {f['seq']} após {last}")
        last = f["seq"]
    print(f"OK: {len(frames)} frames, CRC válido, seq 1..{last}")
    return 0


def cmd_dump(path: Path) -> int:
    frames = read_frames(path)
    vocab: dict[int, str] = {}
    for f in frames:
        t = f["type"]
        p = f["payload"]
        head = f"#{f['seq']:03d} n{f['src']}->{'ALL' if f['dst'] == 0 else 'n' + str(f['dst'])}"
        if t == T_ANNOUNCE:
            caps = struct.unpack(">I", p[:4])[0]
            print(f"{head} ANNOUNCE caps=0x{caps:08X} name={p[4:].decode('utf-8', 'replace')}")
        elif t == T_DICT:
            i = 0
            n = 0
            while i < len(p):
                tid, ln = p[i], p[i + 1]
                vocab[tid] = p[i + 2:i + 2 + ln].decode("ascii", "replace")
                i += 2 + ln
                n += 1
            print(f"{head} DICT {n} tokens (último={vocab.get(max(vocab), '?')})")
        elif t == T_PROPOSE:
            toks = decode_tokens(p[1:])
            print(f"{head} PROPOSE p{p[0]} ref={f['ref']}: {' '.join(toks)}")
        elif t == T_VOTE:
            target, vote, reason = struct.unpack(">HBB", p[:4])
            print(f"{head} VOTE p_target=#{target} vote={VOTES.get(vote, vote)} reason={ID_TOKEN.get(reason, reason)}")
        elif t == T_CLAIM:
            m, prio, eta, ttl, flags = struct.unpack(">BBHHB", p[:7])
            print(f"{head} CLAIM {MISSIONS.get(m, m)} prio={prio} eta={eta}s ttl={ttl}s flags=0x{flags:02X}")
        elif t == T_STATUS:
            m, st, pct = struct.unpack(">BBB", p[:3])
            print(f"{head} STATUS {MISSIONS.get(m, m)} {STATES.get(st, st)} {pct}%")
        elif t in (T_COMMIT, T_DONE):
            print(f"{head} {'COMMIT' if t == T_COMMIT else 'DONE'} {MISSIONS.get(p[0], p[0])} {p[1:9].hex()}")
        elif t == T_ALERT:
            code, m = struct.unpack(">HB", p[:3])
            print(f"{head} ALERT code={code} mission={MISSIONS.get(m, m)}")
        elif t == T_RULE:
            print(f"{head} RULE {p[0]} params={p[1:].hex()}")
        else:
            print(f"{head} T=0x{t:02X} payload={p.hex()}")
    return 0


def cmd_init(path: Path, force: bool) -> int:
    if path.exists() and path.stat().st_size > 0 and not force:
        raise SystemExit("fórum não está vazio; init recusado (use --force para ignorar)")
    frames: list[bytes] = []

    def add(src, dst, typ, payload, ref=0):
        frames.append(build_frame(len(frames) + 1, src, dst, typ, payload, ref))

    # 1) anúncio do nó 1
    add(1, 0, T_ANNOUNCE, p_announce("buffy/freebuff", 0x0000_001F))
    # 2) vocabulário in-band (fonte de verdade dos tokens)
    add(1, 0, T_DICT, p_dict(LEXICON))
    # 3) regras do fórum
    add(1, 0, T_PROPOSE, p_propose(1, [
        "forum", "append_only", "one_frame_per_line", "hex_uppercase", "seq_monotonic",
        "crc32", "no_edit_delete", "binary_only", "no_json", "no_prose",
        "claim_before_code", "vote_quorum_2of3", "no_force_push", "tests_release",
        "human_arbiter", "escalate_human"]))
    # 4) divisão de missões
    add(1, 0, T_PROPOSE, p_propose(2, [
        "mission_split", "m0_scaffold", "m1_thermo", "m2_wave", "m3_kuramoto",
        "m4_organism", "owner", "review", "merge", "branch"]))
    # 5) leases, quórum, registro de nós
    add(1, 0, T_PROPOSE, p_propose(3, [
        "v1", "protocol", "lease", "ttl", "900", "seconds", "quorum", "2", "3",
        "vocabulary", "decode_required", "register_node", "forum_tool", "handoff"]))
    # 6) reserva do módulo 1, condicionada à votação
    add(1, 0, T_CLAIM, p_claim(MISSION_BY_NAME["m1_thermo"], prio=1, eta=1800, ttl=900, flags=0x01))
    # 7-9) voto do nó 1 nas três propostas
    for target in (3, 4, 5):
        add(1, 0, T_VOTE, p_vote(target, 1, "yes"))

    append_frames(path, frames)
    print(f"semeado: {len(frames)} frames em {path}")
    return 0


def cmd_send(path: Path, src: int, typ_name: str, payload_hex: list[str], dst: int, ref: int) -> int:
    frames = read_frames(path)
    payload = bytes(int(h, 16) for h in payload_hex) if payload_hex else b""
    fr = build_frame(next_seq(frames), src, dst, TYPE_BY_NAME[typ_name], payload, ref)
    append_frames(path, [fr])
    print(f"anexado seq={next_seq(frames)} type={typ_name} {fr.hex().upper()}")
    return 0


def cmd_propose(path: Path, src: int, pid: int, tokens: list[str]) -> int:
    frames = read_frames(path)
    fr = build_frame(next_seq(frames), src, 0, T_PROPOSE, p_propose(pid, tokens))
    append_frames(path, [fr])
    print(f"anexado seq={next_seq(frames)} PROPOSE p{pid}")
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="FORUM binário do PROJETO AETHER")
    ap.add_argument("--forum", type=Path, default=DEFAULT_FORUM)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("init")
    s.add_argument("--force", action="store_true")
    sub.add_parser("verify")
    sub.add_parser("dump")
    s = sub.add_parser("send")
    s.add_argument("--src", type=int, required=True)
    s.add_argument("--type", required=True, choices=sorted(TYPE_BY_NAME))
    s.add_argument("--payload", nargs="*", default=[])
    s.add_argument("--dst", type=int, default=0)
    s.add_argument("--ref", type=int, default=0)
    s = sub.add_parser("propose")
    s.add_argument("--src", type=int, required=True)
    s.add_argument("--id", type=int, required=True)
    s.add_argument("--tokens", nargs="+", required=True)
    args = ap.parse_args(argv)
    path: Path = args.forum

    if args.cmd == "init":
        return cmd_init(path, args.force)
    if args.cmd == "verify":
        return cmd_verify(path)
    if args.cmd == "dump":
        return cmd_dump(path)
    if args.cmd == "send":
        return cmd_send(path, args.src, args.type, args.payload, args.dst, args.ref)
    if args.cmd == "propose":
        return cmd_propose(path, args.src, args.id, args.tokens)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
