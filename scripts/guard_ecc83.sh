#!/usr/bin/env bash
# ecc83 byte-guard: the user-approved ecc83 layout must not change
# without approval. Regenerates with the standing recipe and compares
# byte-for-byte against tests/golden/ecc83_srpp.pour_gnd.seed42.kicad_pcb.
set -u
cd "$(dirname "$0")/.." || exit 1
out=${1:-/tmp/ecc83_guard.kicad_pcb}
BHDL_LIB_PATH=$PWD BHDL_JLCPARTS_DB=/nonexistent BHDL_PNR_POUR_GND=1 \
  ./target/release/bhdl-cli tests/circuits/realistic/ecc83_srpp.bhdl layout \
  -o "$out" --seed 42 >/dev/null 2>&1
if cmp -s "$out" tests/golden/ecc83_srpp.pour_gnd.seed42.kicad_pcb; then
  echo "ecc83: byte-identical to golden"; exit 0
else
  echo "ecc83: DIFFERS from golden ($out)"; exit 1
fi
