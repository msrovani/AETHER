#!/usr/bin/env bash
# Verifica a Definição de Pronto da ARCHITECTURE.md §8 em modo headless.
#
# O organismo deve, na mesma execução:
#   * manter R > 0.95;
#   * reduzir a entropia espectral após a interferência (redução < 0);
#   * terminar com H(s*) estritamente abaixo de H(s0).
#
# Uso:
#   scripts/check_organism.sh [duração] [caminho do binário]
#
# Sem o segundo argumento usa `cargo run --release`; no Windows, onde o cargo
# não está no PATH, passe o binário ja compilado (ex.:
# `scripts/check_organism.sh 10s target/release/aether-organism.exe`).
set -uo pipefail

DUR="${1:-10s}"
BIN="${2:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$(mktemp 2>/dev/null || echo "/tmp/aether-dod.$$")"
trap 'rm -f "$LOG"' EXIT

cd "$ROOT" || exit 1

if [[ -n "$BIN" ]]; then
  "$BIN" --duration "$DUR" --plain >"$LOG" 2>&1
else
  cargo run --release --quiet --bin aether-organism -- --duration "$DUR" --plain >"$LOG" 2>&1
fi
rc=$?
if [[ "$rc" -ne 0 ]]; then
  echo "DOD_FAIL exec=$rc" >&2
  tail -n 20 "$LOG" >&2
  exit 1
fi

last_num() { grep -oE -- "$1" "$LOG" | tail -n1 | grep -oE -- '-?[0-9]+(\.[0-9]+)?' | tail -n1; }

R="$(last_num 'R = -?[0-9]+\.[0-9]+')"
RED="$(last_num 'redução = -?[0-9]+\.[0-9]+')"
H_LINE="$(grep -E 'H\(s\*\) = ' "$LOG" | tail -n1)"
HSS="$(printf '%s' "$H_LINE" | grep -oE -- '-?[0-9]+\.[0-9]+' | sed -n 1p)"
H0="$(printf '%s' "$H_LINE" | grep -oE -- '-?[0-9]+\.[0-9]+' | sed -n 2p)"

for pair in "R=$R" "redução=$RED" "H(s*)=$HSS" "H(s0)=$H0"; do
  if [[ "${pair#*=}" == "" ]]; then
    echo "DOD_FAIL metrica_ausente:${pair%%=*}" >&2
    tail -n 20 "$LOG" >&2
    exit 1
  fi
done

veredito="$(awk -v r="$R" -v red="$RED" -v hs="$HSS" -v h0="$H0" 'BEGIN {
  ok = (r > 0.95) && (red < 0) && (hs < h0)
  printf "%s r=%.4f reducao=%.4f H(s*)=%.5f H(s0)=%.5f", (ok ? "DOD_OK" : "DOD_FAIL"), r, red, hs, h0
}')"

echo "$veredito"
[[ "$veredito" == DOD_OK* ]] || exit 1
