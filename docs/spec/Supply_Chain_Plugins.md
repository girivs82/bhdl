# Supply-Chain Plugins — real MPNs from distributor/catalog APIs (audit + design)

> **Status:** Audit + design (2026-06). The plugin *protocol* already
> exists (`bhdl-analyzer/src/plugin.rs`, §3); this spec records the
> EDA-supply-chain API landscape, picks the providers worth implementing,
> and defines how a provider turns BHDL's parametric selection into a real,
> orderable part (MPN + manufacturer + stock + price) in the BOM — and how
> that stays reproducible via `bhdl.lock`.

## 1. Why

Rating-aware catalog selection (`value_snap::select_family` +
`glacier_physical_selection::apply_catalog_physical_selection`) already
picks, per passive, the **smallest catalogue part covering value + derated
stress** and writes its package. What it can't do from the bundled stdlib
`part_family` catalogues alone is name a **real, orderable MPN** with live
**stock/price/lifecycle** — those come from a distributor/catalog API. A
*supply-chain plugin* fetches them; the part_family declaration constrains
*what* is acceptable (class, value, E-series, ratings, package), the plugin
resolves *which actual part* to buy.

## 2. API landscape (audited 2026-06)

| Source | Free? | Auth | Parametric search | Returns | Catch |
|---|---|---|---|---|---|
| **DigiKey Product Info v4** | ✅ free | OAuth2 | ✅ (ProductSearch + parameter filters) | MPN, stock, real-time price, datasheet, lifecycle | 120 req/min, 1000/day; ToS prohibits redistributing the dataset + requires removal on termination |
| **Mouser Search API** | ✅ free | API key | ⚠️ weak — keyword/MPN, ≤50 results | MPN, stock, price | not a parametric discovery engine; best as an MPN enricher |
| **Nexar (Octopart) GraphQL** | 🟡 ~1000 parts/mo eval | OAuth2 | ✅ best cross-distributor aggregation | MPN + offers across many distributors | free tier too small for production; paid prices not public |
| **LCSC** | ❌ no public API | — | — | — | scrape-only (~100 req/min) |
| **JLCPCB official API** | 🟡 approval-gated | key | limited | assembly parts | approval based on order history |
| **jlcparts (yaqwsx, open)** | ✅✅ fully free | none | ✅ offline | LCSC/JLC parts: MPN, mfr, package, parametric attrs, stock, price | community-maintained; LCSC/JLC catalogue only; data is a generated snapshot |

Sources: developer.digikey.com (Product Info v4, API user agreement),
api.mouser.com (Search API), nexar.com/api + octopart.com (Nexar/Octopart
transition, free Welcome-1K plan), github.com/yaqwsx/jlcparts.

**Picks (free-first):**
- **jlcparts offline DB** — default zero-config provider: no key, no rate
  limit, queryable offline, parametric, reproducible. Catalogue is the
  LCSC/JLC cost-optimized/assembly sweet spot. Shipped as a zero-dependency
  Rust binary over the full SQLite (§4.1) + a Python CSV reference (§4.2).
- **DigiKey v4** — first online provider: authoritative, broad, real
  parametric search; user supplies their own OAuth creds.
- Mouser/Nexar — secondary enrichers / premium cross-distributor coverage.

## 3. The plugin protocol (already implemented)

`bhdl-analyzer/src/plugin.rs` already defines the boundary — JSON over a
single stdin/stdout exchange, the provider is any executable (Rust/Python/
shell), zero-config default is `bhdl_plugin_default`:

- **In:** a `CandidateBundle` (the parametric requirements — class, value,
  E-series/ranges, ratings, package, per `selections_needed[i]`).
- **Out:** `PluginResponse { protocol_version, selections, warnings }`,
  each `PluginSelection { class_index, mpn, manufacturer, family, vendor,
  vendor_sku, qty, unit_price, currency, stock, lead_time_weeks, note,
  error }`.

So a supply-chain provider is exactly: read requirements on stdin → query
its source → emit `PluginSelection`s with the real `mpn`/`manufacturer`/
`stock`/`unit_price`. No core change to the protocol is needed.

## 4. jlcparts providers (default, offline) — BUILT + VERIFIED

Data artifact is **CDFER/jlcpcb-parts-database** (MIT) — the in-stock
JLCPCB catalogue derived from yaqwsx/jlcparts:
- **full SQLite (~1.6 GB): `…/jlcpcb-components.sqlite3`** — the whole
  catalogue (not just basic/preferred), so odd E96 values and specialised
  parts resolve (e.g. 1.65Ω, 31.6kΩ).
- basic+preferred CSV (~3.6 MB): `…/jlcpcb-components-basic-preferred.csv`
  — the no-extra-fee assembly subset.

Two providers ship, sharing the JSON stdin/stdout protocol:

### 4.1 `bhdl-jlcparts-provider` (Rust, SQLite) — the default

A workspace crate (`bhdl-jlcparts-provider`) querying the **full SQLite**.
`rusqlite`'s `bundled` feature statically links SQLite into the binary, so
the provider has **zero runtime dependencies** — no Python interpreter, no
system `libsqlite3`, a single self-contained executable consistent with the
Rust workspace. DB path from `$BHDL_JLCPARTS_DB` (or argv[1]).

It is the **zero-config default**: when `$BHDL_SUPPLY_PROVIDER` is unset but
`$BHDL_JLCPARTS_DB` points at a catalogue, the live `bom`/`visualize` path
auto-resolves `bhdl-jlcparts-provider` (next to the running executable, else
on `PATH`) — see `glacier_physical_selection::default_provider_spec`.

Resolution per requirement: resolve category↔class once to the indexed
`category_id`s; parse the parametric value out of the `description` text
(`510kΩ`→510e3, `100nF`→100e-9, **`6.8uH`→6.8e-6** — inductors use ASCII
`u`, and a Henry match rejects `…Hz` frequencies); among in-stock
value-matching rows, **prefer the closest value** (so exact E96 121Ω wins
over an in-tolerance 120Ω), tiebroken by rank basic > preferred > stock.

**Footprint cascade** — the provider actively translates between BHDL's
package codes and jlcparts' notation rather than giving up:
1. **strict** package-string equality (R/C EIA codes — `0603`, `1206` —
   match jlcparts verbatim);
2. **code token** — the size code as a substring of the package string *or
   the MPN*. This is the inductor translator: a `6045` request matches
   `SWPA6045S6R8MT` / `SRN6045-…` even though jlcparts labels the package
   `SMD,6x6mm` — in SRN**6045** the `45` is the 4.5 mm *height*, not the
   width, so naive L×W translation is wrong, but the size code lives in the
   part number;
3. **value-only** last resort, no package constraint, with a warning that
   the footprint could not be confirmed.

**Verified end-to-end** (zero-config, only `$BHDL_JLCPARTS_DB` set) on
`tps54331_test.bhdl` — all 8 passives resolve, exact values, no warnings:
`121Ω→C22867 0603WAF1210T5E`, `1.65Ω→C25189`, `31.6kΩ→C25967`,
`10µF→C13585`, `4.7µF→C29823`, `6.8µH/6045→C57254 SWPA6045S6R8MT`,
`10kΩ→C25804`. Warm query ~0.12 s (cold ~1.9 s is pure page-in of the 1.6 GB
DB — I/O-bound, identical in any language). `cargo test -p
bhdl-jlcparts-provider` covers the value parser (ASCII-µ, Hz rejection).

### 4.2 `bhdl_jlcparts_provider.py` (CSV) — the hackable reference

`bhdl-stdlib/plugins/bhdl_jlcparts_provider.py` reads the basic/preferred
**CSV** (`$BHDL_JLCPARTS_CSV`). Tiny, no build step, easy to fork — the
reference implementation of the protocol for other parties. Covers the
assembly subset only (misses odd E96 values the full SQLite carries).

Because the data is a **local snapshot**, both are hermetic and
reproducible: no per-build network call, MIT-licensed (no
redistribution-ToS problem).

## 5. Reproducibility (ties to `bhdl.lock`)

Live APIs return volatile stock/price, but the **selected MPN is a stable
identifier** — pin it. On a build, the plugin resolves a requirement to an
MPN; `bhdl.lock` records `(refdes-or-requirement-hash → mpn, vendor_sku,
provider, snapshot-id)`. A rebuild reuses the pinned MPN; `--offline`/
`--locked` never calls the provider. This mirrors the source-resolver
revision pinning (`docs/spec/Source_Resolvers.md`): the provider fetches,
the lock makes it reproducible and verifiable. DigiKey's no-redistribution
ToS is satisfied because only the MPN (an identifier the user selected),
not the distributor's dataset, is stored.

## 6. Wiring — BUILT (live `bom`/`visualize` pipeline)

`glacier_physical_selection::apply_supply_chain_mpns(netlist)` is now
invoked from the live `bhdl bom` and `bhdl visualize` paths, right after
catalogue physical selection. Steps 1–3 below are implemented; pinning
(step 4) is the remaining piece.

1. **Bridge** — ✅ builds the requirement list from the netlist's selected
   passives (`classify_component` + `parse_value_string` of the snapped
   `value` + `physical_package`), tracking `class_index → InstanceId`.
2. **Invoke** — ✅ spawns `$BHDL_SUPPLY_PROVIDER` (default unset ⇒ no-op),
   pipes the requirements JSON to its stdin, parses the reply with the
   shared `PluginResponse` type. Best-effort: no provider / unparseable
   reply / no match ⇒ keep the catalogue value + package, leave MPN blank.
   (Note: this path spawns the provider directly with the leaner
   *requirements* payload rather than `plugin.rs::run_plugin`, which is
   hardcoded to a `CandidateBundle` input.)
3. **Apply** — ✅ writes `mpn`/`manufacturer`/`lcsc_pn`/`stock` onto the
   instance; the BOM walker reads `mpn`/`manufacturer`/`lcsc_pn`.
4. **Pin** — ⏳ record the selected MPN in `bhdl.lock` (§5). Not yet built.
5. **Provider** — ✅ both jlcparts providers shipped; the Rust SQLite binary
   is the zero-config default (§4.1), the Python CSV is the reference
   (§4.2). DigiKey online provider (BYO OAuth) still TODO.

**Verified end-to-end** — zero-config (only `$BHDL_JLCPARTS_DB` set, no
`$BHDL_SUPPLY_PROVIDER`), full SQLite, on `tps54331_test.bhdl`:
```
$ BHDL_JLCPARTS_DB=/path/jlcpcb-components.sqlite3  bhdl-cli tps54331_test.bhdl bom
  ✓ supply chain: 8 real MPN(s) resolved
| R1 | 1 | 121Ω  | UNI-ROYAL          | 0603WAF1210T5E | 0603 | lcsc=C22867 |
| R2 | 1 | 1.65Ω | UNI-ROYAL          | 0603WAF165KT5E | 0603 | lcsc=C25189 |
| L1 | 1 | 6.8µH | Sunlord            | SWPA6045S6R8MT | 6045 | lcsc=C57254 |
| C1 | 1 | 10µF  | Samsung Electro-M. | CL31A106KBHNNNE| 1206 | lcsc=C13585 |
…
```
All 8 passives resolve with exact values and confirmed footprints (the full
SQLite carries the odd E96 values 1.65Ω/31.6kΩ the CSV subset omits, and the
code-token cascade translates the `6045` inductor footprint). The protocol
field is `protocol_version: "1"` (a string), matching
`plugin.rs::PluginResponse`. A remaining miss now means the part is
genuinely out of stock catalogue-wide (or the value/footprint has no
equivalent), not a coverage gap of the subset — and the build stays
MPN-less for that line rather than erroring.

## 7. Out of scope

- Credential management — each online provider uses the user's own keys
  (env), never stored in core (same boundary as level-3 source resolvers).
- A package registry — none; providers are external executables on PATH.
- Datasheet/symbol/footprint fetch — separate concern (SnapEDA/Ultra
  Librarian class), not supply-chain selection.
