# ERC — Electrical Rules Above the Netlist

> **Status:** T1 batch 1 (ERC001–005) and batch 2 (ERC006–020, the subset not
> requiring new grammar) are BUILT. T2 (part-carried `check {}` rules) and T3
> (policy plugins) are specified here and land with the extensions milestone,
> together with severity gating and waivers.

## 1. Why BHDL can check more than an EDA netlist tool

A traditional ERC sees pins and nets. BHDL's design database also carries:

| Source | Examples | Rules it powers |
|---|---|---|
| Declared pin semantics | `signal in/out/inout`, `power in`, Clock/Analog/Differential types | driver conflicts, diff polarity, TX/RX |
| Rails with budgets | `power VCC = 3.3V @ 500mA` | domain crossing, rail overload |
| Datasheet attributes | `dropout_voltage`, `input_voltage_max`, `i_supply`, `polarized` | dropout, abs-max, reversed electrolytic |
| Solved operating point | GLACIER node voltages, §4 stress values | polarity vs real DC, ripple vs spec |
| Intents & requirements | `for noise_filtering(cutoff:…)`, `supply { ripple_max }` | intent-contradiction, requirement gates |
| The part itself | its `design{}` / `simulation{}` / (future) `check{}` blocks | vendor rules shipped with the part |

The design-engineer stance: a rule should fire with the NUMBERS (both domain
voltages, the summed draw vs the budget), and a rule that cannot resolve its
inputs SKIPS — absence of a violation is never manufactured from absent data
(Real-Data Policy).

## 2. Architecture: three tiers, by where the knowledge lives

**T1 — core rules (in-tree Rust, the current registry).** Universal physics
and topology: anything derivable from the netlist + analysis with no
part-specific or org-specific knowledge. Individually enable/disable-able;
thresholds configurable via `configurable_params`. Hardcoding these is
CORRECT: they are the semantics of electricity, not policy.

**T2 — part-carried rules (HDL, the §4 pattern).** A part's connection
requirements are device IP and travel WITH the entity, exactly like its
stress model:

```bhdl
entity TPS54331(…) {
    …
    simulation {
        check {
            require connected(EN)
                else "EN must not float — tie to VIN or drive it";
            require exists(C_boot) && C_boot.value == 100nF
                else "BOOT needs the datasheet 100nF bootstrap cap";
        }
    }
}
```

Evaluated per instance with net context. Adding a part to the stdlib adds its
rules; no central registry to update. (Same argument that made the chooser's
candidate set "the catalogue is the universe".)

**T3 — policy plugins (JSON over stdio).** Org-wide review policy
(наming conventions, forbidden vendors, creepage classes) as an external
process receiving a serialized design summary and returning findings — the
same plugin protocol discipline as the supply-chain providers. Proprietary
rules never need to live in-tree.

Severity gating (`--erc-level error` fails the build) and waivers
(`attribute erc_waive = "ERC016: shared budget with X"` — waives WITH a
recorded reason, surfaced in the report) are extension-milestone items that
apply uniformly across tiers.

## 3. Rule catalog

### Batch 1 — pin semantics (BUILT)
- **ERC001** driver conflicts: ≥2 push-pull outputs (Error); input-only nets
  (Warning).
- **ERC002** differential polarity: P and N of a pair on one net.
- **ERC003** UART not crossed: TX↔TX / RX↔RX between devices.
- **ERC004** cross-domain signal without level shifter (supply resolved by
  following power pins to rails; `component_class = "level_shifter"` exempts).
- **ERC005** I2C pull-ups missing / pulled to the wrong rail.

### Batch 2 — connectivity + datasheet + budgets (BUILT)
- **ERC006** floating input: a declared `signal in` pin on a placed instance
  with no net (Warning — floating CMOS inputs oscillate/draw).
- **ERC007** unpowered part: a `power in` pin with no net (Error — the part
  is dead; every other check on it is moot).
- **ERC008** single-pin net: exactly one member — almost always a typo'd net
  name (Warning).
- **ERC009** rail shorted to ground: a Power-class net containing a
  ground-direction pin (Error).
- **ERC011** orphan passive: a passive with any pin unconnected — a resistor
  going nowhere does nothing (Warning).
- **ERC016** rail budget overload: Σ declared instance draws (`i_supply` /
  `supply_current` / `i_quiescent` fallback) vs the rail's declared `@ I`
  budget; fires only when at least one draw is declared, reports the number
  of instances with UNDECLARED draw alongside (Error when over).
- **ERC017** regulator below dropout: input rail `V < output_voltage +
  dropout_voltage` — the LDO cannot regulate (Error, both numbers shown).
- **ERC018** absolute-maximum input: supply rail above the part's declared
  `input_voltage_max` (Error) — the IC-level generalization of the cap
  voltage sign-off.
- **ERC020** missing decoupling: an active part whose supply rail carries no
  capacitor at all (Info — datasheet-habit check; the part-specific version
  belongs to T2).

### Batch 3 — solved-point & intent (needs analysis plumbing; extensions)
- **ERC019** reversed polarized capacitor: `polarized = true` part whose
  positive pin sits at a LOWER solved DC voltage than its negative pin —
  uses GLACIER node voltages, beyond any netlist-only tool.
- **ERC022** intent contradiction: `for noise_filtering(cutoff: X)` whose
  RC against the surrounding network misses X by >an octave.
- **ERC023** precision-path grade mismatch: a `grade`-profile / 1% part fed
  through 5% parts on the same declared measurement path.
- **ERC024** UNCHECKED visibility: stress/requirement axes that skipped for
  missing data surfaced as Info findings (the absence ledger).
- **ERC025** T2 surface: entity-carried `check {}` blocks (grammar +
  extractor + per-instance evaluator, reusing the §4 machinery).
- **ERC026** interface completeness: declared I2C/SPI/UART interface with
  unconnected member signals.
- Diff-pair extensions: pair split across unrelated endpoints; length/skew
  intents once layout lands.
- **Known connectivity quirks the checks currently work around** (root-cause
  fixes belong to the synthesizer): (a) an electrically-merged node can be
  left split across two Net objects (ERC008 exempts pins listed in multiple
  connection lists); (b) `pin -> @named-net` indirection drops same-file
  entity pin instances entirely — direct pin-to-pin wiring works (the ERC
  fixtures use it; fixing the indirection un-blinds every net-based rule for
  that wiring style).
- VIL/VIH-aware domain rule: replace ERC004's 5% rail comparison with pin
  `vih_min`/`vil_max`/`io_tolerant_v` attributes when declared.

## 4. Reporting

Every violation goes to the log AND the `## Design rule check` Markdown table
in the bom/synthesis-report output: rule, severity, finding WITH numbers,
suggested fix. Waived findings (extension) print in a separate table with
their recorded reasons — a waiver hides nothing.
