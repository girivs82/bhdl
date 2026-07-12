#!/bin/sh
# BOM --simulate sweep over the realistic corpus.
# Baseline shape: 48 SIGNOFF / 18 OK / 0 EXIT1. Compare runs with:
#   diff <(sort /tmp/bhdl_bom_sweep.txt) <(sort /tmp/bhdl_bom_sweep.prev)
set -u
cd "$(dirname "$0")/.." || exit 1
out=${1:-/tmp/bhdl_bom_sweep.txt}
: > "$out"
for f in tests/circuits/realistic/*.bhdl; do
  o=$(BHDL_JLCPARTS_DB=/nonexistent RUST_LOG=off timeout 90 \
      ./target/release/bhdl-cli "$f" bom --simulate 2>&1)
  rc=$?
  if [ $rc -ne 0 ]; then echo "EXIT1 $f" >> "$out"
  elif echo "$o" | grep -q "Sign-off report"; then echo "SIGNOFF $f" >> "$out"
  else echo "OK $f" >> "$out"
  fi
done
./scripts/sweep_cleanup.sh
awk '{print $1}' "$out" | sort | uniq -c
grep "^EXIT1" "$out" || true
