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

### 4.1a Selection is a multi-objective cost function, not a fixed cascade

Within the footprint-feasible, in-stock, in-tolerance candidate set, the
pick is **scored**, not lexicographically ranked. Each soft term is
normalized then weighted; lowest total wins:

```
score = w_value·valueErr + w_price·price + w_assembly·asmFee
      + w_stock·(−stock) + w_lead·lead
```

- **value error** is normalized against the *tolerance budget*
  (`|v−target|/tol`, 0 = exact, 1 = at the edge) — an absolute spec metric,
  not min-max, so a wide tolerance band can't dilute it;
- **price** is the unit price at the **build quantity** (the tiered `price`
  JSON is indexed by qty — comparing qty-1 prices would mislead);
- **assembly fee** is the basic(0) < preferred(0.5) < extended(1) proxy for
  JLCPCB's per-part + feeder cost;
- **tolerance** is the *part's* grade (±%) parsed from the description —
  tighter is better; **tempco / drift** is the resistor's `±N ppm/℃` or, for
  ceramic capacitors, the dielectric mapped to a drift proxy (C0G/NP0 ≪ X7R
  ≪ Y5V) — lower is better;
- **stock** rewards headroom; **lead** is modeled but ~0 for the in-stock
  offline DB.

Beyond value, two **hard gates** apply: `tolerance_pct` bounds the *value*
match window, and optional `max_tolerance_pct` bounds the *part grade* — a
feedback/measurement path can require ≤1 % (or ≤0.1 %) parts, and looser
ones become infeasible (a part with no parseable tolerance also fails the
grade gate).

Per-class hard gates round out the quality story (R: tolerance/drift,
C: dielectric, L: current):
- **capacitor `dielectric`** (e.g. `"C0G"`) — a filter/timing/reference cap
  that must be temperature-stable accepts only Class-I parts (C0G≡NP0
  aliased), excluding the cheap X7R/Y5V jellybeans; from a `dielectric`
  instance attribute (`Cap(value, dielectric = "C0G")`).
- **inductor `current_a`** — a power-path inductor must carry the rail
  current or it saturates/overheats. The part's *conservative* rating (the
  MIN of Irms/Isat parsed from the description, e.g. `31mΩ 3A 4.3A 6.8uH` →
  3 A) must be ≥ the requirement; an unrated part fails the gate. From a
  `rated_current`/`current` instance attribute. Verified: a bare 6.8 µH
  `availability` pick is `FXL0630-6R8-M` (5 A); with `current_a = 5.5` it
  correctly flips to the 8.5 A `FXL1040-6R8-M`.

  Crucially this is **sized by the recipe, not passed by the board author.**
  A board author writes `Ind(value, rated_current = "6A")` directly, but a
  vendor stdlib recipe with a `design { }` block SIZES it from the operating
  point: the TPS54331 buck computes `I_peak = I_out + ΔI_L/2` and emits
  `l_rated_current = I_peak·1.3` (saturation headroom), then its expansion
  instantiates `Ind(l_value, rated_current = l_rated_current)`. The
  expansion interpreter resolves the named-arg value against the design
  block's outputs exactly as it does the positional value — so a recipe can
  *size* any leaf attribute, not just its value. For the default 2 A / 0.3
  ripple operating point this requires ≥ ~3.0 A, which the resolved
  `SWPA6045S6R8MT` (3 A Irms) just clears.

  **GLACIER-refined operating point (`bhdl bom --simulate`).** The closed-form
  `I_peak` is a seed; with `--simulate`, the BOM runs a GLACIER DC solve and
  `stamp_inductor_sim_current` stamps each inductor's `current_rating` from
  the *simulated* branch current (with the buck VOUT-side-net inference for
  inductors that read 0 A at DC, and 80 % saturation derating). The supply
  current gate then prefers that `current_rating` over the recipe/board seed,
  so the inductor is selected against the actual operating point. Package /
  value selection is left to the catalogue pass (identical with or without
  `--simulate`), so enabling the solve never disturbs the footprint. Without
  `--simulate` the BOM uses the recipe seed + declared rail voltages (fast,
  no solve). This realizes the GLACIER half of the stress model (tasks #1/#4)
  for the inductor-current axis.

- **capacitor `voltage_v` / resistor `power_w`** — the V and P analogues of
  the inductor current gate, completing the per-class stress trio at the MPN
  level. The resolver derives the requirement from the *operating* stress
  (not the part's nominal default): cap voltage = the max voltage across the
  part (declared rail voltages in the BOM path, sim node voltages under
  `--simulate`) × 2 derate; resistor power = the simulated dissipation × 2
  derate (so it only gates under `--simulate`). The provider parses the
  part's rated `…V` / `…W` from the description and requires rating ≥
  requirement. So a 10 µF on a 12 V rail demands a ≥ 24 V MLCC even if a
  cheaper 16 V part shares the package the catalogue chose.

  **Over-stress is left UNPOPULATED, not auto-substituted.** If a stress gate
  excludes *every* part at the value/footprint (the component is genuinely
  over-stressed — e.g. a 1.65 Ω carrying 2 A ≈ 6.6 W in an 0603), the
  provider leaves the line unpopulated and emits a loud `OVER-STRESSED …
  LEFT UNPOPULATED` warning naming the unmet requirement. It never
  auto-drops to an under-rated part: a missed warning must not ship a part
  that burns up — the designer fixes the derating / package / topology. (A
  stress gate fires only when its operating value is known: cap voltage from
  declared rails always; resistor power and the sim-refined values only under
  `--simulate`.) (Feeding sim stress into catalogue *package* selection —
  reconciling the glacier↔catalogue passes — remains the larger follow-up.)

**Profiles** are weight presets, selectable at synthesis time:
- `precision` — value-only → exact E-series wins;
- `grade` (a.k.a. `feedback`/`measurement`/`reference`) — a **precision
  path**: exact value AND high part grade (tight tolerance, low drift),
  cost secondary. This is the one to use for a feedback divider or a
  precision measurement chain;
- `cost` — price + assembly dominate; a slightly-off in-tolerance value is
  fine;
- `availability` — max stock / min lead;
- `balanced` — default (a little of everything, incl. mild grade weight).

Explicit weight objects (incl. `tolerance`/`tempco`) override presets.

Verified on the full DB: 10 kΩ/0603 under `cost` → `0603WAF1002T5E` (Thick
Film ±1 % ±100 ppm, basic, cheap), under `grade` → `AR03BTS1002` (Thin Film
±0.1 % ±25 ppm precision part); adding `max_tolerance_pct: 0.5` tightens the
feasible set further. 100 nF/0402 default → `CL05B104KO5NNNC` (Samsung X7R,
basic), with `dielectric: "C0G"` → `GRM31C5C1H104JA01L` (Murata C0G). The
same parameter mechanism that gives a resistor its grade gives a capacitor
its dielectric — one `Cap` type, no `C0G_Cap` variant.

**Per-net / per-part policy.** The BHDL side
(`glacier_physical_selection::apply_supply_chain_mpns` + `SupplyOptions`)
resolves the objective per passive with three-level precedence:
1. the instance's own `supply_profile` / `supply_weights` / `supply_qty`
   attribute (in BHDL source — travels with the design); a per-part
   `max_tolerance` attribute additionally hard-gates the part grade;
2. the policy of a net it connects to (`--supply-net NET=PROFILE` /
   `$BHDL_SUPPLY_NET_PROFILES`, keyed by net name);
3. the global default (`bhdl bom --supply-profile … --supply-qty …` /
   `$BHDL_SUPPLY_PROFILE`, `$BHDL_SUPPLY_QTY`).

The chosen objective + quantity ride in the protocol (top-level default +
optional per-`requirement` override), so any provider honours them.

Demonstrated on `tps54331_test.bhdl`: `--supply-profile cost
--supply-net V3_3=precision` flips only the parts on the `V3_3` rail (R1
121Ω → exact `C327406`, R3 31.6kΩ → exact `C103536`) to precision while the
feedback-bottom R4 (10kΩ, on the internal FB/GND nets) keeps the cost pick
`C25804`. `cargo test` covers the scorer (profile changes the pick) and the
price-tier selector.

**Recipe-driven precision (the stdlib carries the policy).** "This node
needs a precision part" is expressed as **per-instance attributes on the
ordinary `Res`**, not a separate part type — precision is a *spec*, not a
different component. The TPS54331 recipe builds its FB divider with
`Res(r_top_value, tolerance = 1%, supply_profile = "grade")`:
- `tolerance = 1%` → the part's grade spec; the resolver uses it as the
  hard `max_tolerance` gate (the default `0.05` on a load/pull-up → ≤5%,
  which excludes nothing);
- `supply_profile = "grade"` → soft preference for tight tolerance + low
  drift among the feasible parts.

So a bare `bhdl bom` (no flags) resolves the feedback resistors to thin-film
≤1 % low-TC parts (R3 31.6 kΩ → YAGEO `RT0603BRC0731K6L`, R4 10 kΩ → Ever
Ohms `TP0603T10K0P0510Z`) while the LED/load resistors stay on cheap ±1 %
jellybeans — **zero board-designer effort**, one `Res` type.

This relies on constructor **named-arg overrides reaching the leaf
instance**. They previously vanished on the flow-style instantiation recipes
use (`… -> r: Res(x, k = v).pin`): the expansion extractor packs all args
(positional + named) into `params` as raw text, and the interpreter only
consumed `params[0]` as the value. The expansion interpreter now lifts the
`k = v` entries from `params` onto the instance as attributes (overriding
entity defaults, last-write-wins), and the resolver sanitizes raw stamped
values (`tolerance` via a fraction/percent-robust parse; `supply_profile`
trimmed of surrounding quotes/space). Board-level `… -> r: Res(x, k=v) …`
also stamps named args now (synthesizer reads both the `PARAM_LIST` and
`PARAM_ASSIGN_BLOCK` arg shapes). (A separate, narrower bug remains: a
board-level *standalone* `r: Res(v, k=v);` drops the positional value — not
hit by the flow-style recipes/boards.)

> **Per-net source annotation is still partial.** A passive inherits a net
> policy via the `--supply-net`/env map (by net name) or — preferably — via
> per-instance `tolerance`/`supply_profile` attributes set in the recipe.
> A true source-level
> `net V3_3 { supply: precision }` block would need a `Net.attributes` +
> parser extension (scoped as a follow-up); the semantic-part-type route
> covers the common recipe cases without it.

### 4.2 `bhdl_jlcparts_provider.py` (CSV) — the hackable reference

`bhdl-stdlib/plugins/bhdl_jlcparts_provider.py` reads the basic/preferred
**CSV** (`$BHDL_JLCPARTS_CSV`). Tiny, no build step, easy to fork — the
reference implementation of the protocol for other parties. Covers the
assembly subset only (misses odd E96 values the full SQLite carries).

Because the data is a **local snapshot**, both are hermetic and
reproducible: no per-build network call, MIT-licensed (no
redistribution-ToS problem).

## 5. Reproducibility (`bhdl.lock` part pins) — BUILT

Live providers return volatile stock/price, but the **selected MPN is a
stable identifier** — so it's pinned. `bhdl.lock` (the existing library lock,
next to `bhdl.toml`) gained a `[[part]]` section:

```toml
[[part]]
refdes       = "buck_R_top"          # the instance's structural name (stable)
mpn          = "RT0603BRC0731K6L"
manufacturer = "YAGEO"
vendor_sku   = "C860829"
provider     = "bhdl-jlcparts-provider"
```

Lifecycle (cmd_bom, mirrors Cargo):
- **first `bhdl bom`** → resolves via the provider, writes the pins
  (`✓ wrote N part pin(s) to bhdl.lock`);
- **rebuild** → reuses the pins, **does not call the provider**
  (`🔒 N MPN(s) pinned from bhdl.lock`) — reproducible and offline;
- **`--update-lock`** → re-resolves and rewrites;
- **`--locked`** → builds against the committed pins; errors if none exist
  (CI guard).

`apply_supply_chain_mpns` returns the selections; `apply_locked_parts`
applies pins without a provider; `enforce_lockfile` preserves the part
section when it rewrites library locks (shared file). Only the **MPN/SKU
identifier** is stored — never stock or price — so no distributor dataset is
redistributed (satisfies e.g. DigiKey's no-redistribution ToS), the same
contract as source-resolver revision pinning
(`docs/spec/Source_Resolvers.md`).

Verified on a `bhdl.toml` project (copy of `tps54331_test`): first build
wrote 8 pins; a second build with `$BHDL_JLCPARTS_DB` **unset** still
produced the identical BOM from the lock; `--locked` on a pin-less project
errors; `--update-lock` refreshes. Manifest-less circuits (no `bhdl.toml`)
resolve fresh every build, unchanged. Pinning by `refdes` (structural name);
requirement-hash keying is a possible future refinement for rename
stability.

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
