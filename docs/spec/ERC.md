# ERC — Electrical Rules Above the Netlist

> **Status:** all three tiers are BUILT — T1 batches 1–2 plus ERC019/ERC026,
> T2 part-carried `check {}` rules (ERC025), T3 policy plugins
> (`BHDL_ERC_PLUGINS`), severity gating (`--erc-fail-on`), and reasoned
> waivers (`erc_waive`), the T2 predicate extensions (exists / value-eq /
> same_net), and the ERC024 absence ledger. Remaining specified-only: ERC023
> (needs a measurement-path concept); ERC022 and the ERC019 solved-DC
> upgrade are BUILT.

## 1. Why BHDL can check more than an EDA netlist tool

A traditional ERC sees pins and nets. BHDL's design database also carries:

| Source | Examples | Rules it powers |
|---|---|---|
| Declared pin semantics | `signal in/out/inout`, `power in`, Clock/Analog/Differential types | driver conflicts, diff polarity, TX/RX |
| Rails with budgets | `power VCC = 3.3V @ 500mA` | domain crossing, rail overload |
| Datasheet attributes | `dropout_voltage`, `input_voltage_max`, `i_supply`, `polarized` | dropout, abs-max, reversed electrolytic |
| Solved operating point | GLACIER node voltages, §4 stress values | polarity vs real DC, ripple vs spec |
| Intents & requirements | `for noise_filtering(cutoff:…)`, `supply { ripple_max }` | intent-contradiction, requirement gates |
| The part itself | its `design{}` / `simulation{}` / `check{}` blocks | vendor rules shipped with the part |

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
the ordinary expression grammar plus the netlist predicates
`connected(PIN)` (on a net with at least one OTHER member),
`exists(CHILD)` (support part reachable by local name), and
`same_net(P1, P2)` (pin strapping). `self.<attribute>` resolves through
the entity's datasheet attributes and `<child>.value` through the support
part's snapped value, with `==`/`!=` comparing at engineering tolerance;
multiple conditions are written as multiple requires —
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
required since the part has an internal EN pull-up). The desugared `supply`
circuit emits the full application circuit (S4a,
Power_Supply_Synthesis.md §6) — generated buck supplies satisfy the
TPS54302 BOOT rule with their emitted bootstrap cap, verified end-to-end.

**T3 — policy plugins (JSON over stdio).** Org-wide review policy
(наming conventions, forbidden vendors, creepage classes) as an external
process receiving a serialized design summary and returning findings — the
same plugin protocol discipline as the supply-chain providers. Proprietary
rules never need to live in-tree. — **BUILT.**

Configuration: `BHDL_ERC_PLUGINS` — colon-separated executable paths. Each
plugin receives one DesignSummary (`protocol_version`, `kind:
"erc_policy_check"`, `instances[{refdes, entity, attributes, pins[{name,
direction, net?}]}]`, `nets[{name, class, voltage?, budget_a?, members}]`)
on stdin and replies `{protocol_version, findings[{rule_id, severity,
description, fix?, instance?, net?}], warnings[]}` on stdout. Findings are
anchored back onto the named instance/net (Global fallback) and enter the
report BEFORE the waiver partition — org rule ids gate (`--erc-fail-on`)
and waive (`erc_waive = "NAMING-001: reason"`) exactly like built-ins.
Failure semantics: a plugin that can't spawn, exits non-zero, or replies
malformed JSON becomes ONE visible `ERC-PLUGIN` Warning — a broken policy
gate must be seen, but a tooling failure never fabricates design errors
(Real-Data Policy applied to tooling). Reference implementation:
`scripts/erc-policy-example.py` (refdes prefix convention + unbudgeted-rail
rule); fixture `tests/circuits/erc/erc_policy.bhdl`.

Severity gating (`--erc-fail-on error` fails the build, exit 3) and waivers
(`attribute erc_waive = "ERC016: shared budget with X"` — waives WITH a
recorded reason, surfaced in the report) are BUILT and apply uniformly
across all three tiers.

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
  reversed-across-a-rail error. UPGRADED: when the unified DC analysis
  succeeded, the SOLVED node voltage (by net name) takes precedence over
  the declared class — which also gives signal nets a potential; declared
  potentials remain the fallback, and nets with neither still skip.
- **ERC022** intent contradiction — BUILT (Error): a filtering intent
  (`for noise_filtering(cutoff: X)` / `anti_alias` / `filter`) whose
  declared cutoff the PLACED values contradict by more than one octave,
  with all numbers in the finding ("R=1kΩ × C=100nF gives f_c = 1.59kHz —
  2.7 octaves off"). v1 topology scope, everything else skips per
  Real-Data: anchor on the annotated shunt cap on a SIGNAL net (rail caps
  skip — a load resistor on a rail is not a filter element, and rail
  filtering needs ESR/source-impedance data this pass lacks); exactly one
  series R → RC, exactly one L → LC. Enabled by stamping intent
  attributes during generation (phase 12.5), before the DRC phase.
- **ERC023** precision-path grade mismatch: a `grade`-profile / 1% part fed
  through 5% parts on the same declared measurement path.
- **ERC024** UNCHECKED visibility — BUILT (the absence ledger): every
  sign-off axis that skipped for missing data (NoData verdict or an
  UNCHECKED provenance note) renders as an Info row in a dedicated
  "Unchecked axes" section of the sign-off report, naming exactly which
  datum is missing. Lives at the sign-off render site (the skip facts
  don't exist yet at DRC time) and is deliberately NOT waivable — a
  waived absence is still an absence. The phase-margin section reports
  its own UNCHECKED state separately.
- **ERC025** T2 surface: entity-carried `check {}` blocks — BUILT (see §2),
  including the predicate extensions: `exists(CHILD)` (expansion child /
  S4-stamped sibling / board-level bare name), `<child>.value` comparisons
  against datasheet attributes with engineering equality (`==`/`!=` at
  1e-6 relative tolerance — `c_boot.value == self.bootstrap_capacitor`),
  and `same_net(P1, P2)` pin strapping (`same_net(MODE, GND)`). A
  rail-targeted `connected(PIN, @RAIL)` form was considered and REJECTED:
  a part cannot know board rail names — strapping intent is expressible
  as `same_net` against the part's own pins.
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
