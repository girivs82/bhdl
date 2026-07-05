# ERC — Electrical Rules Above the Netlist

> **Status:** T1 batch 1 (ERC001–005), batch 2 (ERC006–020, the subset not
> requiring new grammar), severity gating (`--erc-fail-on`), reasoned waivers
> (`erc_waive`), and T2 part-carried `check {}` rules (ERC025) are BUILT.
> T3 (policy plugins) and the remaining batch-3 rules are specified here.

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

**T2 — part-carried rules (HDL, the §4 pattern) — BUILT (ERC025).** A part's
connection requirements are device IP and travel WITH the entity, exactly
like its stress model:

```bhdl
entity LP2985(…) {
    …
    simulation {
        check {
            require connected(EN)
                else "EN (ON/OFF) must not float — tie it to VIN or drive it";
        }
    }
}
```

A `check { }` block sits beside `stress { }` inside `simulation { }` and
holds `require <predicate> else "MSG";` statements. The predicate grammar is
the ordinary expression grammar plus `connected(PIN)`, which the ERC
evaluator substitutes from the netlist (connected = on a net with at least
one OTHER member). `self.<attribute>` resolves through the entity's
datasheet attributes; multiple conditions are written as multiple requires —
each failure is its own finding with its own vendor message (ERC025, Error,
located on the instance; the message doubles as the fix suggestion). A
predicate that cannot be resolved (unknown pin, unresolvable identifier)
SKIPS per the Real-Data Policy. Recipes ride the same extraction/import
plumbing as §4 stress recipes — one vendor-model surface per entity.

Evaluated per instance with net context. Adding a part to the stdlib adds
its rules; no central registry to update. (Same argument that made the
chooser's candidate set "the catalogue is the universe".) First adopters:
LP2985 (`connected(EN)` — floating ON/OFF is undefined) and TPS54302
(`connected(BOOT)` — no bootstrap cap, no switching; EN deliberately NOT
required since the part has an internal EN pull-up). Note the desugared
`supply` circuit currently instantiates no support parts, so the TPS54302
BOOT rule truthfully flags generated buck supplies until supply synthesis
learns to emit the application circuit (S4 follow-up).

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

### Batch 3 — solved-point & intent
- **ERC019** reversed polarized capacitor — BUILT (Critical): `polarized =
  true` part whose `pos` pin sits at a LOWER DC potential than its `neg`
  pin. v1 uses DECLARED potentials (ground = 0V, power rail = its declared
  voltage; signal nets skip per Real-Data) — catches the classic
  reversed-across-a-rail error. Upgrade to GLACIER-solved node voltages
  when the DC solution is plumbed into the DRC phase.
- **ERC022** intent contradiction: `for noise_filtering(cutoff: X)` whose
  RC against the surrounding network misses X by >an octave.
- **ERC023** precision-path grade mismatch: a `grade`-profile / 1% part fed
  through 5% parts on the same declared measurement path.
- **ERC024** UNCHECKED visibility: stress/requirement axes that skipped for
  missing data surfaced as Info findings (the absence ledger).
- **ERC025** T2 surface: entity-carried `check {}` blocks — BUILT (see §2).
  Future predicate extensions: `exists(child)`, child-value comparisons
  (`C_boot.value == 100nF`), `connected(PIN, @RAIL)` rail-targeted form.
- **ERC026** interface completeness — BUILT: I2C half-wired (SDA xor SCL
  connected — Error on the instance, both directions); SPI data pin
  connected without SCK/SCLK (Error) and clock connected with neither
  MOSI nor MISO (Warning). UART deliberately unchecked — TX-only and
  RX-only links are legitimate. v1 matches conventional exact pin names;
  numbered multi-bus parts (SDA0/SDA1) are future scope.
- Diff-pair extensions: pair split across unrelated endpoints; length/skew
  intents once layout lands.
- **Known connectivity quirks — RESOLVED at root** (synthesizer commit
  `2bdcf81`): the hollow-netlist family (same-file entity pins dropping on
  any board with imports, `pin -> @net` indirection losses, apparent fan-out
  fragmentation) was ONE bug — a nested preprocessor-miss branch in
  `add_pins_for_component` returned empty pins instead of cascading to the
  local-entity source. One cosmetic residue remains: net merging can leave a
  vestigial EMPTY duplicate Net object (zero connections). The ERC-side
  defenses stay: `net_members` trusts the `pin_instance.net` back-pointer,
  and ERC008 exempts pins listed in multiple connection lists.
- VIL/VIH-aware domain rule: replace ERC004's 5% rail comparison with pin
  `vih_min`/`vil_max`/`io_tolerant_v` attributes when declared.

## 4. Reporting

Every violation goes to the log AND the `## Design rule check` Markdown table
in the bom/synthesis-report output: rule, severity, finding WITH numbers,
suggested fix. Waived findings (extension) print in a separate table with
their recorded reasons — a waiver hides nothing.
