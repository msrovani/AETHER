#!/usr/bin/env python3
# F5 T2 — stub de allowlist de rede (deny-default, sem HTTP).
"""Política offline para research.query com allow_network.

Uso:
  python tools/f5_t2_allowlist.py selftest
  python tools/f5_t2_allowlist.py check --host example.com
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALLOWLIST = ROOT / "data" / "research_network_allowlist.json"


def load_allowlist(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def normalize_host(value: str) -> str:
    raw = value.strip().lower()
    if "://" in raw:
        host = urlparse(raw).hostname or ""
    else:
        host = raw.split("/")[0].split(":")[0]
    return host.strip(".")


def decide(
    host: str,
    allowlist: dict[str, Any],
    *,
    allow_network: bool = False,
    hitl_ack: str | None = None,
) -> dict[str, Any]:
    """Deny-default. Nunca abre socket — só decide."""
    h = normalize_host(host)
    policy = allowlist.get("default_policy", "deny")
    default_net = bool(allowlist.get("allow_network_default", False))
    hosts = {normalize_host(x) for x in allowlist.get("hosts", []) if x}

    out: dict[str, Any] = {
        "host": h,
        "allowed": False,
        "reason": None,
        "network_attempted": False,
        "policy": policy,
    }

    if not h:
        out["reason"] = "empty_host"
        return out

    if policy != "deny":
        out["reason"] = "unsupported_policy_only_deny_stub"
        return out

    if default_net:
        out["reason"] = "allow_network_default_must_be_false"
        return out

    if not allow_network:
        out["reason"] = "allow_network_false"
        return out

    if hitl_ack != "SIM":
        out["reason"] = "hitl_ack_required"
        return out

    if h not in hosts:
        out["reason"] = "host_not_in_allowlist"
        return out

    out["allowed"] = True
    out["reason"] = "allowlist_hit_offline_only"
    return out


def gate_research_network(
    abstain_reason: str,
    allow_network: bool,
    host: str | None,
    allowlist: dict[str, Any],
) -> dict[str, Any]:
    """Espelho do contrato F5: só unknown; rede só com allowlist+HITL."""
    if abstain_reason != "unknown":
        return {
            "ok": False,
            "error": "pesquisa_so_em_unknown",
            "network": False,
        }
    if not allow_network:
        return {
            "ok": True,
            "mode": "offline_fixture",
            "network": False,
            "reason": "allow_network_false",
        }
    decision = decide(host or "", allowlist, allow_network=True, hitl_ack="SIM")
    if not decision["allowed"]:
        return {
            "ok": False,
            "error": "network_denied",
            "network": False,
            "decision": decision,
        }
    # Stub: mesmo com allow, NÃO faz HTTP neste tier.
    return {
        "ok": False,
        "error": "t2_live_fetch_not_implemented",
        "network": False,
        "decision": decision,
        "note": "allowlist ok; implementação HTTP fica para T2 live sancionado",
    }


def selftest() -> int:
    failures: list[str] = []
    al = load_allowlist(DEFAULT_ALLOWLIST)

    if al.get("default_policy") != "deny":
        failures.append("default_policy deve ser deny")
    if al.get("allow_network_default") is not False:
        failures.append("allow_network_default deve ser false")
    if al.get("hosts"):
        failures.append("stub inicial: hosts deve ser []")

    d0 = decide("example.com", al, allow_network=False)
    if d0["allowed"] or d0["reason"] != "allow_network_false":
        failures.append(f"sem rede deve negar: {d0}")

    d1 = decide("example.com", al, allow_network=True, hitl_ack=None)
    if d1["allowed"] or d1["reason"] != "hitl_ack_required":
        failures.append(f"sem HITL deve negar: {d1}")

    d2 = decide("evil.test", al, allow_network=True, hitl_ack="SIM")
    if d2["allowed"] or d2["reason"] != "host_not_in_allowlist":
        failures.append(f"host fora da lista: {d2}")

    # allowlist temporária com host
    al2 = dict(al)
    al2["hosts"] = ["docs.python.org"]
    d3 = decide("https://docs.python.org/3/", al2, allow_network=True, hitl_ack="SIM")
    if not d3["allowed"]:
        failures.append(f"host listado deveria passar política: {d3}")

    g = gate_research_network("ambiguous", False, None, al)
    if g.get("ok") or g.get("error") != "pesquisa_so_em_unknown":
        failures.append(f"ambiguous não abre: {g}")

    g2 = gate_research_network("unknown", False, None, al)
    if not g2.get("ok") or g2.get("network"):
        failures.append(f"unknown offline deve ok sem rede: {g2}")

    g3 = gate_research_network("unknown", True, "docs.python.org", al2)
    if g3.get("network") or g3.get("error") != "t2_live_fetch_not_implemented":
        failures.append(f"stub não deve HTTP: {g3}")

    # nenhum selftest pode ter tentado rede
    report = {
        "schema": "aether.f5_t2_allowlist/v1",
        "ok": not failures,
        "failures": failures,
        "network_attempted": False,
        "allowlist": str(DEFAULT_ALLOWLIST),
    }
    out = ROOT / "target" / "f5_t2_allowlist.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if not failures else 1


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_check = sub.add_parser("check")
    p_check.add_argument("--host", required=True)
    p_check.add_argument("--allow-network", action="store_true")
    p_check.add_argument("--hitl-ack", default=None)
    p_check.add_argument("--allowlist", default=str(DEFAULT_ALLOWLIST))

    sub.add_parser("selftest")

    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()

    al = load_allowlist(Path(args.allowlist))
    print(
        json.dumps(
            decide(
                args.host,
                al,
                allow_network=args.allow_network,
                hitl_ack=args.hitl_ack,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
