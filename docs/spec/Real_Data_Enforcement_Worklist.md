# Real-Data Policy — Enforcement Worklist

Audit of every fabricated value in the analysis / selection path (per
`Real_Data_Policy.md`). 31 violations. Status: in progress.

## A. Estimate tables (typical-value lookups)
- [x] `ripple_calculator.rs:39` `typical_esr_mohm(dielectric,package)` — done (sweep 2/N). Table deleted; output- and input-cap banks drop the ESR-sized `mid_freq` tier and size the bulk tier on the capacitive ripple term alone (full ripple budget). ESR ripple is now loudly noted UNACCOUNTED in the tier rationale + estimated ripple. Cap-sizing runs only under `bom --simulate`, not the `freeze` oracle path → oracle 51/51.
- [x] (signoff stability) — removed `typical_esr_mohm` from the stability path; now UNCHECKED when ESR absent.

## B. Defaults / unwrap_or (fabricated when real data absent)
- [x] `signoff.rs:162` ripple_ratio `.unwrap_or(0.3)` — done (sweep 1/N, b23f589). Now `Option`; tps54302 declares `ripple_ratio = 0.35` (K_IND); None → stepping UNCHECKED.
- [x] `signoff.rs:169` loop_ratio `.unwrap_or(0.1)` — done (sweep 1/N). Crossover target now `f_sw·loop_ratio` from the real entity attr.
- [x] `signoff.rs:174` v_ref `.unwrap_or(0.6)` — done (sweep 1/N). Real `feedback_voltage` or stability UNCHECKED.
- [~] `glacier_physical_selection.rs` tolerance `.unwrap_or(2.0)` — **KEPT** (sweep 5/N). This is the value-MATCH WINDOW (how close the catalogue nominal must be), a selection knob the stdlib always declares (`tolerance = 0.05`), NOT a fabricated part measurement. Left as a selection-policy default (allowed, like the E-series grids / derate factors).
- [x] `glacier_physical_selection.rs` resistor current `.unwrap_or(0.0)` + capacitor voltage `.unwrap_or(0.0)` — **HARD-REJECT done (sweep 5/N)**. `select_resistor_physical` / `select_capacitor_physical` now return `None` (→ component left unselected / DNP) when the REAL GLACIER stress entry is *absent*, instead of fabricating zero stress (which would pick the smallest, possibly under-rated part). A real solved zero (`Some(0.0)`) still proceeds. The `P=I²R` / `V=I·R` derivations are KEPT — they're EXACT physics from the real current, not estimates. Measured: connectivity oracle 51/51; buck still resolves 8 MPNs (real GLACIER data); components GLACIER can't solve get no physical selection.
- [x] ~~`spice_extraction.rs` resistor tolerance / LED Vf·If·r_d / diode Vf·Is~~
  — **NOT a live violation: dead code.** 329/401 lines are commented out and
  there are ZERO live callers of any `extract_*` function (verified). Its
  fabricated defaults never execute. Only `parse_unit_value` is live (imported
  by 4 synthesizer files) and is fine. The audit mis-targeted this file; the
  real LED/diode/R defaults live in `model_extractor.rs` (below).
- [x] ~~`lib.rs:813,1089` param parse `.unwrap_or(0.0)`~~ — **NOT a violation** (sweep 6/N): the `.parse().unwrap_or(0.0)` is guarded by `param_value.chars().all(is_digit or '.')`, so the value is real-and-declared; the 0.0 is an unreachable defensive fallback for a malformed-but-numeric edge case, not fabricated-data-when-absent.
- [x] LED current-limit synthesis cluster — **done (sweep 7/N)**. The authoritative path (`component_inference.rs`, the one with the module resolver) read a real `{color}_forward_voltage`/`_forward_current` but fell back to a color→Vf TABLE and a 20mA default when absent. Removed both fallbacks: it now requires the LED's real declared forward voltage AND current (color-keyed or the entity's plain `forward_voltage`/`forward_current`, unit-aware parse), and SKIPS auto-sizing with a loud warning when neither is declared. No fabricated operating point. Oracle 51/51.
  - NOTE (plumbing gap, follow-up): the LED entity declares `forward_voltage = 2.0V` etc., but the module resolver does not currently populate `metadata.electrical_specs` from those attributes — so today auto-sizing skips+warns for most LEDs. Wiring the entity attrs into `electrical_specs` restores real-data LED resistor auto-sizing.
  - The secondary `lib.rs:962-966` color-table path (`get_led_forward_voltage` + 20mA + `supply.unwrap_or(5.0)`) feeds the parallel spice_synthesis resolver; lower priority now that `resolve_voltage_divider` is hard-errored and the authoritative inference path is enforced. Queued.
- [x] `spice_synthesis.rs` divider load_current `.unwrap_or(0.001)` + hardcoded 12 V supply — **done (sweep 6/N)**. `resolve_voltage_divider` fabricated BOTH the load (1 mA) and the supply (12 V) to compute R; it now hard-errors, directing the designer to declare the divider's R values explicitly. Oracle 51/51.
- [x] `model_extractor.rs` default params (resistance=1000, capacitance=1e-9, inductance=1e-6, Vf=0.7/2.0, voltage=5, Koren nominals) — **done (sweep 3/N)**. Decision: **hard-error the sim**. Removed the fabricated default-seed; `extract_from_data` now builds parameters only from entity-declared values and HARD-ERRORS (naming the component + missing param) if the SPICE model's required parameter(s) are absent. Measured: connectivity oracle 51/51 (freeze path untouched); `bom --simulate` sign-off now hard-errors on **30/51** circuits (under-declared LEDs/diodes/triodes/regulators), 20 clean + buck survives. This is the intended "maximally force the data problem" degradation.
- [x] `netlist_converter.rs` regulator vout `.unwrap_or(5.0)` + read_param defaults (rds_on/f_sw/t_sw/i_quiescent) — **done (sweep 4/N)**. `vout_voltage` and `req_param("i_quiescent"/"rds_on"/"f_sw"/"t_sw")` now hard-error (naming the regulator + missing param) instead of fabricating. The 10kΩ dropout is LEFT: it's explicitly an MNA-connectivity-stability constant (not a device measurement), allowed like other policy/physics constants.
- [x] `unified_simulation.rs` R=1000 / C=100n — **done (sweep 4/N)**: hard-error on missing real resistance/capacitance. ESR/ESL: absent ⇒ IDEAL (0), not the fabricated 0.1Ω/1nH (an ideal element makes no measurement claim).
  - Net effect of sweep 3+4: connectivity oracle 51/51; `bom --simulate` hard-errors on 32/51 circuits (under-declared); buck sign-off preserved (analytic, entity-attr-driven).

## C. Proxies (a different real value substituted)
- [x] `signoff.rs:147` `i_out` = regulator rated output_current used as the actual per-rail load — **done (sweep 9/N)**. The actual load is a *design* property (the rail's declared `@ I` budget), not a *device* datasheet attribute (`GLACIER=power-expert, stdlib=datasheet` — the load is neither). Carried it end-to-end: `NetClass::Power(f64)` → `Power { voltage, current: Option<f64> }` (the chosen home; PnR's trace-width path now consumes the REAL rail current too, dropping its `unwrap_or(0.5)` estimate to a fallback). Plumbed `PowerDecl::current()` (already parsed) → analyzer `PowerDomain.declared_current` / `NetAttribute::PowerDomain.declared_current` (distinct from the legacy `max_current` 1.0A estimate) → synth net build → `SwitcherOp.i_out: Option<f64>`. Sign-off now reads the **output rail's** declared current; absent `@ I` ⇒ `None` ⇒ inductor peak-current, ripple-ratio stepping, and input-cap ripple all report **UNCHECKED** (no rated-current proxy). Measured: TPS54302 buck (`VOUT = 5V @ 2A`) now reports `I_pk = 2.0A + ΔI_L/2 = 2.292A` (real 2A load, was the rated 3A); strip the `@ 2A` and it reports `I_pk UNCHECKED`. Connectivity oracle green; synthesizer lib 74/74; no new analyzer-test regressions (the one pre-existing `test_resistor_inference_led_current_limiting` fail is the sweep-7 LED test-harness `electrical_specs` gap, untouched here).
- [ ] (shared) glacier I²R / I·R above.

## D. Unit/shape assumptions
- [x] `ripple_calculator.rs` / `input_cap_calculator.rs` hardcoded tier dielectrics (C0G/X7R/X5R) — **done (sweep 10/N)**. Resolution: the hardcoded tier dielectric is a `dielectric_hint` — a *sourcing recommendation* (which dielectric to look for, like the E-series grids / derate factors), NOT a measured property of a chosen part, so it is **legitimate selection policy** (moved to the list below) **provided it never feeds a verdict**. The actual violation was the FALLBACK in `signoff.rs::cap_is_ceramic`, which read the real `dielectric` (stamped by glacier from the selected MPN) *or else* the `dielectric_hint` — so an un-selected cap carrying only a hint got a real `CeramicStructural` ESR-zero phase-margin verdict from a fabricated dielectric. Removed the `.or_else(dielectric_hint)`: the ceramic determination now uses only the real selected-part `dielectric`; a cap with only a hint is treated as unknown ⇒ UNCHECKED (the three-way split's `Unchecked` arm). Measured: TPS54302 buck unchanged (its output caps had no real `dielectric` ⇒ already UNCHECKED; the hint fallback was a latent footgun, not firing here); connectivity oracle 51/51; synthesizer lib 74/74. The hardcoded hint strings stay as sourcing policy.

## Policy constants (legitimate — NOT violations)
Derate factors (CAP 2×, RES 2×, IND 1.25×), SIGNOFF_MARGIN 1.2, 2π, E-series
grids, entity-declared datasheet constants (loop_crossover_k, feedback_voltage,
switching_frequency, output_current), and the cap-tier `dielectric_hint`
strings (a sourcing recommendation, like the E-series grids — legitimate so
long as they never feed a verdict; see D). These are policy/physics/datasheet,
allowed.

## Data source: DigiKey provider (lands real ESR for non-ceramics)
`bhdl-digikey-provider` (DigiKey Product Information API v4, OAuth2
client-credentials, self-contained ureq+rustls — no curl/fetch) is built and
live-verified. It emits real per-MPN `esr_ohms` (+ `esr_test_freq_hz`) for
**electrolytic / tantalum / polymer** caps (e.g. Vishay T55A107M007C0070 →
0.07 Ω @ 100 kHz). **Finding (measured live): DigiKey carries neither ESR nor
Dissipation Factor for ceramics (MLCCs)** — the ceramic Parameters array has
only Temperature Coefficient / Tolerance / Voltage / Capacitance. So **ceramic
ESR stays UNCHECKED even with DigiKey**; the earlier "DF → derive ceramic ESR"
premise does not hold on this catalogue. Ceramic ESR needs manufacturer
impedance curves (datasheet extraction), not a vendor parameter.
- [x] WIRE-UP: `PluginSelection` carries `esr_ohms`/`esr_test_freq_hz`/
  `dielectric`; both providers emit them; glacier writes them onto the netlist
  cap `esr`/`dielectric` attributes (+ pinned in `bhdl.lock`). Sign-off
  stability now splits the ESR zero three ways (Real-Data Policy):
  **Real** (numeric ESR zero from a part's published ESR — electrolytic/
  tantalum/polymer), **CeramicStructural** (ceramic identified by dielectric ⇒
  ESR zero provably ≫ crossover since f_z/f_co = V_out/(2π·ESR·K) ≫1 and
  C_out-independent ⇒ no phase boost ⇒ real verdict from a structural
  inequality, not a number), and **Unchecked** (ESR and type both unknown).
  The TPS54302 buck now reports a real **LOW PHASE MARGIN** (was UNCHECKED) with
  the C_ff fix. Oracle 51/51.

## The blocker: enforcement is gated on DATA AVAILABILITY
Most of B/C/A cannot become "real value or UNCHECKED" usefully until the
catalogue/datasheets actually carry the data (cap ESR/DF, diode/LED Vf·Is,
regulator rds_on/t_sw/i_q, real per-rail load). Today they don't, so strict
enforcement turns almost every analysis UNCHECKED and (with hard-reject) almost
every BOM un-buildable. Sourcing the data (catalogue enrichment / datasheet
extraction) is therefore the real unblocker and must lead the device-model and
selection stages.
