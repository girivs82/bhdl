#!/bin/sh
# PnR P0 oracle sweep: run `layout` over the realistic corpus and grade
# every board with KiCad's OWN DRC (kicad-cli pcb drc) — the external
# arbiter of fabrication truth. We do not grade our own homework.
#
# Output: per-board violation counts by class + unconnected items.
# P0 gate: every board produces a loadable .kicad_pcb and every
# violation class is understood. P1 gate: zero violations, zero
# unconnected (see docs/spec/PnR_Professional_Architecture.md).
set -u
cd "$(dirname "$0")/.." || exit 1
KICAD_CLI=${KICAD_CLI:-/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli}
if [ ! -x "$KICAD_CLI" ]; then
  echo "kicad-cli not found ($KICAD_CLI) — set KICAD_CLI" >&2
  exit 1
fi
outdir=${1:-/tmp/bhdl_layout_drc}
mkdir -p "$outdir"
total_v=0; total_u=0; boards=0; failed=0
for f in tests/circuits/realistic/*.bhdl; do
  b=$(basename "$f" .bhdl)
  pcb="$outdir/$b.kicad_pcb"
  if ! BHDL_JLCPARTS_DB=/nonexistent RUST_LOG=off timeout 300 \
      ./target/release/bhdl-cli "$f" layout -o "$pcb" >"$outdir/$b.layout.log" 2>&1; then
    echo "$b: LAYOUT-FAILED"
    failed=$((failed+1))
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
echo "── oracle baseline: $boards boards, $total_v violations, $total_u unconnected, $failed failed"
