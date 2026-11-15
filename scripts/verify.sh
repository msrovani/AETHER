#!/usr/bin/env bash
# Eco-Tokens v3 verify - stack=rust (gerado por eco-tokens-mcp.py)
# smart_trim: se >20 linhas -> 5 head + 15 tail; strip ANSI ECMA-48.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1
STACK="rust"
LOG="$(mktemp -t eco-verify.XXXXXX 2>/dev/null || mktemp)"
trap 'rm -f "$LOG"' EXIT

strip_ansi() { sed -E 's/\x1B(@|[\[\\]-_]|\[[0-?]*[ -/]*[@-~])//g'; }

smart_trim_fail() {
  local clean n head tail
  clean="$(strip_ansi <"$LOG" | tr -d '\r')"
  n="$(printf '%s\n' "$clean" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')"
  if [[ "${n:-0}" -le 20 ]]; then
    printf '%s\n' "$clean" | sed '/^[[:space:]]*$/d' >&2
    return
  fi
  head="$(printf '%s\n' "$clean" | sed '/^[[:space:]]*$/d' | head -n 5)"
  tail="$(printf '%s\n' "$clean" | sed '/^[[:space:]]*$/d' | tail -n 15)"
  echo "----- smart_trim (5 head + 15 tail) -----" >&2
  printf '%s\n' "$head" >&2
  echo "..." >&2
  printf '%s\n' "$tail" >&2
}

# Git Bash: nao deixar link do Git sombrear link.exe MSVC
case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW*|MSYS*|CYGWIN*)
    CLEAN=""
    IFS=':'
    for p in $PATH; do
      case "$p" in */Git/usr/bin|*/Git/usr/bin/|*/git/usr/bin|*/git/usr/bin/) continue ;; esac
      CLEAN="${CLEAN:+$CLEAN:}$p"
    done
    unset IFS
    export PATH="$CLEAN"
    ;;
esac

if [[ -n "${ECO_VERIFY_CMD:-}" ]]; then
  CMD="$ECO_VERIFY_CMD"
else
  CMD='cargo test --workspace --lib -- --quiet 2>/dev/null || cargo test --lib -- --quiet'
fi

rc=0
eval "$CMD" >"$LOG" 2>&1 || rc=$?
if [[ "$rc" -eq 0 ]]; then
  echo "VERIFY_OK stack=$STACK"
  exit 0
fi
echo "VERIFY_FAIL stack=$STACK exit=$rc" >&2
smart_trim_fail
exit 1
