#!/bin/sh
# Clean up refdes sidecars minted as a side effect of sweep runs, WITHOUT
# touching committed allocation LUTs:
#   - untracked *.bhdl.refdes  -> removed (sweep artifacts)
#   - tracked *.bhdl.refdes    -> restored to committed state if modified
# A blanket `find -delete` here once destroyed committed sidecars — the
# refdes sidecar is the durable allocation record (see refdes doctrine).
set -u
cd "$(dirname "$0")/.." || exit 1
git ls-files -o --exclude-standard -z -- 'tests/circuits/**/*.bhdl.refdes' \
  | xargs -0 rm -f --
git checkout --quiet -- 'tests/circuits/**/*.bhdl.refdes' 2>/dev/null || true
