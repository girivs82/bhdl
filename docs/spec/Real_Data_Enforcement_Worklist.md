# Real-Data Policy — Enforcement Worklist

Audit of every fabricated value in the analysis / selection path (per
`Real_Data_Policy.md`). 31 violations. Status: in progress.

## A. Estimate tables (typical-value lookups)
- [ ] `ripple_calculator.rs:39` `typical_esr_mohm(dielectric,package)` — whole table. Used by output-cap bank sizing (`ripple_calculator.rs:117`) and input-cap bank sizing (`input_cap_calculator.rs:132`). ESR must be the part's real catalogue value.
- [x] (signoff stability) — removed `typical_esr_mohm` from the stability path; now UNCHECKED when ESR absent.

## B. Defaults / unwrap_or (fabricated when real data absent)
- [ ] `signoff.rs:162` ripple_ratio `.unwrap_or(0.3)` — declare on entity (datasheet K_IND) or stepping UNCHECKED.
- [ ] `signoff.rs:169` loop_ratio `.unwrap_or(0.1)` — entity declares it (tps54302 does); remove default.
- [ ] `signoff.rs:174` v_ref `.unwrap_or(0.6)` — entity declares feedback_voltage; remove default.
- [ ] `glacier_physical_selection.rs:642` tolerance `.unwrap_or(2.0)`.
- [ ] `glacier_physical_selection.rs:1185` current `.unwrap_or(0.0)`; `:1192` power `I²R` proxy; `:1201` voltage `I·R` proxy — UNCHECKED when GLACIER data absent.
- [ ] `spice_extraction.rs:45,178` resistor tolerance `.unwrap_or(5.0)`.
- [ ] `spice_extraction.rs:205,209,214` LED Vf=2.0 / If=0.020 / r_d=10.
- [ ] `spice_extraction.rs:238,244` diode Vf=0.7 / Is=1e-9.
- [ ] `lib.rs:813,1089` param parse `.unwrap_or(0.0)`.
- [ ] `lib.rs:966,1219` supply_voltage `.unwrap_or(5.0)`.
- [ ] `unified_simulation.rs:597` R=1000 / `:616` C=100n / `:621` ESR=0.1 / `:622` ESL=1n.
- [ ] `spice_synthesis.rs:251` divider load_current `.unwrap_or(0.001)`.
- [ ] `netlist_converter.rs` regulator vout `.unwrap_or(5.0)`; read_param defaults (rds_on=0.2, f_sw=500e3, t_sw=80n, i_quiescent=5e-3); 10kΩ dropout.
- [ ] `model_extractor.rs:107-133` default params (resistance=1000, capacitance=1e-9, inductance=1e-6, Vf=0.7/2.0, voltage=5, Koren nominals).

## C. Proxies (a different real value substituted)
- [ ] `signoff.rs:147` `i_out` = regulator rated output_current used as the actual per-rail load. Real load must come from the rail / a declared load.
- [ ] (shared) glacier I²R / I·R above.

## D. Unit/shape assumptions
- [ ] `ripple_calculator.rs` / `input_cap_calculator.rs` hardcoded tier dielectrics (C0G/X7R/X5R) — must reflect a real catalogue part.

## Policy constants (legitimate — NOT violations)
Derate factors (CAP 2×, RES 2×, IND 1.25×), SIGNOFF_MARGIN 1.2, 2π, E-series
grids, entity-declared datasheet constants (loop_crossover_k, feedback_voltage,
switching_frequency, output_current). These are policy/physics/datasheet, allowed.

## The blocker: enforcement is gated on DATA AVAILABILITY
Most of B/C/A cannot become "real value or UNCHECKED" usefully until the
catalogue/datasheets actually carry the data (cap ESR/DF, diode/LED Vf·Is,
regulator rds_on/t_sw/i_q, real per-rail load). Today they don't, so strict
enforcement turns almost every analysis UNCHECKED and (with hard-reject) almost
every BOM un-buildable. Sourcing the data (catalogue enrichment / datasheet
extraction) is therefore the real unblocker and must lead the device-model and
selection stages.
