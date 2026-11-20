#!/usr/bin/env python3
# Alias — canônico é tools/f5_t1_agent_loop.py (MSG 0817/0820).
"""Evita segunda implementação; delega ao harness T1 aprovado."""

from __future__ import annotations

import runpy
import sys
from pathlib import Path

sys.argv[0] = str(Path(__file__).resolve().parent / "f5_t1_agent_loop.py")
if len(sys.argv) == 1:
    sys.argv.append("selftest")
runpy.run_path(str(Path(__file__).resolve().parent / "f5_t1_agent_loop.py"), run_name="__main__")
