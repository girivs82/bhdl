#!/bin/sh
# PnR P0 oracle sweep: run `layout` over the realistic corpus and grade
# every board with KiCad's OWN DRC (kicad-cli pcb drc) — the external
# arbiter of fabrication truth. We do not grade our own homework.
#
# Output: per-board violation counts by class + unconnected items.
# P0 gate: every board produces a loadable .kicad_pcb and every
# violation class is understood. P1 gate: zero violations, zero
# unconnected (see docs/spec/PnR_Professional_Architecture.md).
#
# Usage: sweep_layout_drc.sh [outdir] [seed]
# With no seed the layout runs at the engine default (the "@42"
# baseline) and behavior is unchanged. A seed (2nd arg, or
# BHDL_SWEEP_SEED) is passed to `layout --seed` for the multi-seed
# audits, and the DEFAULT outdir moves to /tmp/bhdl_layout_drc_s<seed>
# so audit artifacts never clobber the baseline's.
set -u
cd "$(dirname "$0")/.." || exit 1
KICAD_CLI=${KICAD_CLI:-/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli}
if [ ! -x "$KICAD_CLI" ]; then
  echo "kicad-cli not found ($KICAD_CLI) — set KICAD_CLI" >&2
  exit 1
fi
seed=${2:-${BHDL_SWEEP_SEED:-}}
if [ -n "$seed" ]; then
  outdir=${1:-/tmp/bhdl_layout_drc_s$seed}
else
  outdir=${1:-/tmp/bhdl_layout_drc}
fi
mkdir -p "$outdir"
total_v=0; total_u=0; boards=0; failed=0
for f in tests/circuits/realistic/*.bhdl; do
  b=$(basename "$f" .bhdl)
  # Fixtures that exist for the SAFETY engine (fault campaign, FIT
  # demos) are not layout targets — they opt out of the layout oracle
  # with an explicit marker instead of polluting the 0-failed baseline.
  if grep -q "bhdl-sweep: skip" "$f" 2>/dev/null; then
    echo "$b: SKIPPED (bhdl-sweep: skip)"
    continue
  fi
  pcb="$outdir/$b.kicad_pcb"
  # 900s ceiling: the LQFP-100 board (test_stm32_vb_panel) is the
  # slowest at ~6.5 min after the conflict-scan perf arc (was 18 min
  # before the epoch-stamp dedupe + bbox pre-reject in geom.rs).
  if ! BHDL_JLCPARTS_DB=/nonexistent RUST_LOG=off timeout 1800 \
      ./target/release/bhdl-cli "$f" layout -o "$pcb" ${seed:+--seed "$seed"} \
      >"$outdir/$b.layout.log" 2>&1; then
    echo "$b: LAYOUT-FAILED"
    failed=$((failed+1))
    continue
  fi
  # Boards with nothing to place skip cleanly (library files, pure
  # syntax fixtures) — not failures, and there is no .kicad_pcb to DRC.
  if grep -q "nothing to lay out" "$outdir/$b.layout.log" 2>/dev/null; then
    echo "$b: SKIPPED (nothing to lay out)"
    continue
  fi
  if ! "$KICAD_CLI" pcb drc --format json --output "$outdir/$b.drc.json" "$pcb" \
      >/dev/null 2>&1; then
    echo "$b: ORACLE-REJECTED (file did not load)"
    failed=$((failed+1))
    continue
  fi
  summary=$(python3 - "$outdir/$b.drc.json" <<'PYEOF'
import json, sys
from collections import Counter
d = json.load(open(sys.argv[1]))
c = Counter(v["type"] for v in d.get("violations", []))
u = len(d.get("unconnected_items", []))
v = sum(c.values())
cls = ",".join(f"{k}:{n}" for k, n in sorted(c.items())) or "-"
print(f"{v} {u} {cls}")
PYEOF
)
  v=$(echo "$summary" | cut -d' ' -f1)
  u=$(echo "$summary" | cut -d' ' -f2)
  cls=$(echo "$summary" | cut -d' ' -f3)
  total_v=$((total_v+v)); total_u=$((total_u+u)); boards=$((boards+1))
  printf '%-40s v=%-4s unc=%-4s %s\n' "$b" "$v" "$u" "$cls"
done
./scripts/sweep_cleanup.sh
echo "── oracle baseline${seed:+ (seed $seed)}: $boards boards, $total_v violations, $total_u unconnected, $failed failed"
