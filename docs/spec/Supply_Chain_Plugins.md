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

## 4. jlcparts provider (default, offline) — BUILT + VERIFIED

Shipped: `bhdl-stdlib/plugins/bhdl_jlcparts_provider.py`. Data artifact is
**CDFER/jlcpcb-parts-database** (MIT) — the in-stock JLCPCB catalogue
derived from yaqwsx/jlcparts:
- full SQLite (~1 GB): `…/jlcpcb-components.sqlite3`
- **basic+preferred CSV (~3.6 MB):** `…/jlcpcb-components-basic-preferred.csv`

The CSV (basic + preferred = the no-extra-fee, in-stock assembly parts) is
the right zero-config default for cost-optimized assembly, small enough to
cache/pin, and avoids the 1 GB download. The provider reads it via
`$BHDL_JLCPARTS_CSV` (or argv[1]); a sibling provider can query the full
SQLite for wider coverage.

Provider logic: per requirement, filter rows by category↔class
(Resistors/Capacitors/Inductors), parse the parametric value out of the
`description` text (`510kΩ`→510e3, `100nF`→100e-9, `10µH`→10e-6) and match
it within tolerance, match `package`, require `stock>0`; rank basic >
preferred > most-stock > cheapest; emit a `PluginSelection` with `mpn`
(= the catalogue's `mfr`), `manufacturer`, `vendor="LCSC"`,
`vendor_sku="C"+lcsc`, `stock`, `unit_price`.

**Verified** against the real CSV — e.g. 10kΩ/0402 → C25744
`0402WGF1002TCE`; 100nF/0402 → C1525 `CL05B104KO5NNNC` (the canonical
JLCPCB basic 100nF); 10µH → C1046 `SDFL2012S100KTF`. All correct, in-stock,
orderable parts.

Because the data is a **local snapshot**, this is hermetic and reproducible:
no per-build network call, MIT-licensed (no redistribution-ToS problem).

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
5. **Provider** — ✅ jlcparts reference provider shipped; DigiKey online
   provider (BYO OAuth) still TODO.

**Verified end-to-end** against the real basic/preferred CSV on
`tps54331_test.bhdl`:
```
$ BHDL_SUPPLY_PROVIDER="python3 bhdl-stdlib/plugins/bhdl_jlcparts_provider.py" \
  BHDL_JLCPARTS_CSV=/tmp/jlc_bp.csv  bhdl-cli tps54331_test.bhdl bom
  ✓ supply chain: 5 real MPN(s) resolved
| R4 | 1 | 10kΩ | UNI-ROYAL(Uniroyal Elec) | 0603WAF1002T5E | 0603 | lcsc=C25804 |
| C1 | 1 | 10µF | Samsung Electro-Mechanics | CL31A106KBHNNNE | 1206 | lcsc=C13585 |
…
```
The protocol field is `protocol_version: "1"` (a string), matching
`plugin.rs::PluginResponse`. Misses (e.g. odd E96 values like 1.65Ω/31.6kΩ,
or a `6045` inductor package code absent from the dataset's notation) are
basic/preferred-subset coverage gaps, resolved by pointing at the full
SQLite catalogue — not pipeline bugs.

## 7. Out of scope

- Credential management — each online provider uses the user's own keys
  (env), never stored in core (same boundary as level-3 source resolvers).
- A package registry — none; providers are external executables on PATH.
- Datasheet/symbol/footprint fetch — separate concern (SnapEDA/Ultra
  Librarian class), not supply-chain selection.
