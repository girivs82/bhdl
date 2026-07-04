# data/ — local catalogue databases (not in git)

`jlcpcb-components.sqlite3` — the jlcparts offline catalogue the
bhdl-jlcparts-provider reads (in-stock subset, ~1GB, MIT-licensed data from
CDFER/jlcpcb-parts-database, derived from yaqwsx/jlcparts). Re-download:

    curl -L -C - -o data/jlcpcb-components.sqlite3 \
      https://cdfer.github.io/jlcpcb-parts-database/jlcpcb-components.sqlite3

Point the provider at it with `BHDL_JLCPARTS_DB=data/jlcpcb-components.sqlite3`
(the provider also falls back to this path automatically when the env var is
unset).
