#!/bin/sh
# Schematic v4 sweep over the realistic corpus.
# Gates: every board renders with "0 unidiomized, 0 collisions" and exit 0
# (lib_eff_reg exit=1 is a known pre-existing exception).
set -u
cd "$(dirname "$0")/.." || exit 1
dir=${1:-/tmp/schemsweep}
mkdir -p "$dir"
n=0; bad=0; ex=0
for f in tests/circuits/realistic/*.bhdl; do
  b=$(basename "$f" .bhdl)
  o=$(BHDL_JLCPARTS_DB=/nonexistent RUST_LOG=off timeout 120 \
      ./target/release/bhdl-cli "$f" visualize -o "$dir/$b.html" --svg-v4 "$dir/$b.svg" 2>&1)
  rc=$?
  n=$((n+1))
  [ $rc -ne 0 ] && { ex=$((ex+1)); echo "EXIT $f"; }
  c=$(echo "$o" | grep -oE "[0-9]+ unidiomized, [0-9]+ collisions" | tail -1)
  case "$c" in
    "0 unidiomized, 0 collisions"|"") ;;
    *) bad=$((bad+1)); echo "BAD $f: $c" ;;
  esac
done
./scripts/sweep_cleanup.sh
echo "schematic sweep: $n boards, $bad bad, $ex exit"
