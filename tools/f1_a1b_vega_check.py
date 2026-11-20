#!/usr/bin/env python3
# F1 A-1b — validação readonly da flag --vega no chat (MSG CLAIM 0935).
"""Não edita o crate: só classifica presença do contrato A-1b em cli.rs.

Uso:
  python tools/f1_a1b_vega_check.py selftest
  python tools/f1_a1b_vega_check.py check
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "src" / "agent" / "cli.rs"
DOC = ROOT / "docs" / "F1_A1B_VEGA_OPTIN.md"

REQUIRED = [
    (r"--vega", "flag_help"),
    (r"vega:\s*bool", "clap_or_args_field"),
    (r'"--vega"\s*=>\s*args\.vega\s*=\s*true', "argv_parse"),
    (r"fn open_chat_port\(want_vega", "open_chat_port"),
    (r"VegaPort::new", "vega_ctor"),
    (r"Box::new\(StubPort\)", "stub_fallback"),
    (r"default Eixo C", "eixo_c_msg"),
]


def classify(cli_text: str, doc_exists: bool) -> dict:
    hits = {}
    missing = []
    for pat, name in REQUIRED:
        m = re.search(pat, cli_text)
        hits[name] = bool(m)
        if not m:
            missing.append(name)

    ok = len(missing) == 0
    return {
        "schema": "aether.f1_a1b_vega_check/v1",
        "ok": ok,
        "action": "proceed_contract_present" if ok else "require_a1b_wire",
        "cli": str(CLI.relative_to(ROOT)).replace("\\", "/"),
        "doc_present": doc_exists,
        "hits": hits,
        "missing": missing,
        "network": False,
        "wired_to_crate": False,
        "mutated": False,
        "note": "readonly — NAO edita cli.rs; wire/feature decision-pyo3 = BUFFY/OPENCODE",
    }


def run_check() -> dict:
    if not CLI.is_file():
        return {
            "schema": "aether.f1_a1b_vega_check/v1",
            "ok": False,
            "action": "abstain_missing_cli",
            "error": "cli.rs ausente",
            "mutated": False,
        }
    text = CLI.read_text(encoding="utf-8", errors="replace")
    return classify(text, DOC.is_file())


def selftest() -> int:
    # Fixtures sintéticas — sem tocar o repo real além da leitura no check.
    good = """
  --vega                  F1 A-1b
    vega: bool,
            "--vega" => args.vega = true,
fn open_chat_port(want_vega: bool, out: &mut dyn Write) -> Box<dyn DecisionPort> {
        match VegaPort::new(&cfg) {
                Box::new(StubPort)
            \"decisor: port=stub (default Eixo C; use --vega para o engine real)\"
"""
    bad = "let port = StubPort;"
    cases = [
        (classify(good, True), True, "proceed_contract_present"),
        (classify(bad, False), False, "require_a1b_wire"),
    ]
    failed = []
    for i, (rep, expect_ok, expect_action) in enumerate(cases):
        if rep.get("ok") != expect_ok or rep.get("action") != expect_action:
            failed.append({"i": i, "rep": rep})
    # check real tree
    live = run_check()
    out = {
        "ok": len(failed) == 0 and live.get("ok") is True,
        "failed": failed,
        "live": live,
        "n": len(cases) + 1,
        "mutated": False,
        "wired_to_crate": False,
    }
    print(json.dumps(out, ensure_ascii=False, indent=2))
    return 0 if out["ok"] else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("selftest")
    sub.add_parser("check")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    rep = run_check()
    print(json.dumps(rep, ensure_ascii=False, indent=2))
    dest = ROOT / "target" / "f1_a1b_vega_check.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(rep, ensure_ascii=False, indent=2), encoding="utf-8")
    return 0 if rep.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
