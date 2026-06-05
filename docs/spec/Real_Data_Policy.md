# Real-Data Policy — no fabricated values in analysis

> **Status:** Policy (binding). Applies to every value consumed by the
> sign-off / GLACIER analyses and by part selection.

## The rule

**Every quantity used in an analysis must come from real datasheet / catalogue
data for the actual part.** No fallbacks, no defaults, no typical-value tables,
no dielectric/package estimates. If a required value is not available for a
part, that value is **not synthesised** — the analysis that needs it reports
**UNCHECKED** (naming the missing data), and a part that cannot supply data we
require is **not preferred** over one that can.

The rationale is a quality bar: a sign-off built on guessed numbers is a guess,
not a sign-off. And it puts the pressure where it belongs — on the catalogue
and the vendor. No one wants to design with parts/vendors that won't publish the
data their part needs to be used correctly.

## What this forbids (examples of violations to remove)

- `typical_esr_mohm(dielectric, package)` and any "typical X for this
  dielectric/package" table.
- `…​.unwrap_or(0.3)` / `.unwrap_or(0.1)` style defaults for ripple ratio,
  crossover ratio, V_ref, etc. — these must be **declared** by the device
  (datasheet) or the dependent check is UNCHECKED.
- Using a *requirement* as if it were a *measurement* (e.g. `sim_max_esr`, the
  max-ESR a part must beat, used as the part's actual ESR).
- A load-current *proxy* (the regulator's rated I_out) standing in for the
  real per-rail load.

## What this requires

- **Device constants** (loop crossover K, V_ref, f_sw, ripple targets) are
  declared on the stdlib entity *from the datasheet* — the datasheet principle
  (`Vendor_Simulation_Blocks.md` §1A). These are real and allowed.
- **Part-instance values** (ESR, DF, impedance, tempco-derated capacitance)
  come from the catalogue/provider's real per-MPN data. Where the catalogue is
  incomplete (e.g. ceramic ESR — ceramics are specified by dissipation factor /
  impedance curves, and the jlcparts catalogue today carries neither), the
  honest state is **UNCHECKED until the data is sourced** (datasheet extraction
  or a vendor/catalogue that publishes it). Enriching the catalogue with the
  missing fields, or preferring parts/vendors that publish them, is the fix —
  never a fabricated stand-in.

## Effect on the control-loop stability check (first application)

Loop phase margin depends on the output-cap ESR zero. The catalogue provides no
ESR (or DF) for ceramics, so the check reports:

> **stability UNCHECKED — output caps <refdes…> provide no ESR data**

and stops there (the crossover `f_co`, which needs only the real `C_out`, is
still reported). The previous `typical_esr_mohm` estimate is removed. Real-data
stability returns once ESR/DF reaches the catalogue.
