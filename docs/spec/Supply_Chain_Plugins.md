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
  LCSC/JLC cost-optimized/assembly sweet spot.
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

## 4. jlcparts provider (default, offline)

jlcparts produces per-category JSON + an intermediate `cache.sqlite3`
(components carry LCSC #, manufacturer, MPN, package, category, stock,
price, datasheet, and parametric attributes stored as normalized JSON
dicts). The provider:

1. Locate the DB (`$BHDL_JLCPARTS_DB`, else a cached download path).
2. For each requirement, query `components` filtered by category↔class,
   the parametric value within the part_family's tolerance, the package,
   and `stock > 0`; rank by stock desc / unit-price asc.
3. Emit a `PluginSelection` with `mpn`, `manufacturer`, `vendor="LCSC"`,
   `vendor_sku=<Cxxxx>`, `stock`, `unit_price`, `currency`.

Because the DB is a **local snapshot**, this is hermetic and reproducible —
no per-build network call, no redistribution-ToS problem (it's the open
jlcparts dataset, not a proprietary API response). The exact SQL is written
against the installed `cache.sqlite3` schema (parametric attrs are JSON, so
the query JSON-extracts the primary value per category).

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

## 6. Wiring plan (implementation, not yet built)

The plugin protocol + a default provider exist, but `run_catalog_scan` /
the plugin invocation are **not yet in the live `generate`/`bom`/`spice`
pipeline** (test/demo-only — see `catalog_scan.rs` module doc). To get real
MPNs into the BOM:

1. **Bridge** — build the plugin input from the live netlist's selected
   passives (class + snapped value + ratings + package from
   `apply_catalog_physical_selection`), not the hand-built `InstanceClass`
   list the demos use.
2. **Invoke** — spawn the configured supply-chain plugin (reuse
   `plugin.rs`; default = jlcparts provider), pipe requirements, parse
   `PluginSelection`s. Best-effort: no plugin / no match ⇒ keep the
   catalogue's value + package, leave MPN blank (today's behaviour).
3. **Apply** — write `mpn`/`manufacturer`/`vendor`/`stock`/`unit_price`
   onto the instance (the BOM walker already reads `mpn`/`manufacturer`).
4. **Pin** — record the selected MPN in `bhdl.lock` (§5).
5. **Provider** — ship the jlcparts reference provider; DigiKey as the
   first online provider (BYO OAuth).

Verifiable end-to-end with a **fixture provider** (a tiny script returning
a known MPN for a requirement — same pattern as the source-resolver
fixture test), independent of any real DB/network. The real jlcparts run
needs the user's downloaded `cache.sqlite3`.

## 7. Out of scope

- Credential management — each online provider uses the user's own keys
  (env), never stored in core (same boundary as level-3 source resolvers).
- A package registry — none; providers are external executables on PATH.
- Datasheet/symbol/footprint fetch — separate concern (SnapEDA/Ultra
  Librarian class), not supply-chain selection.
