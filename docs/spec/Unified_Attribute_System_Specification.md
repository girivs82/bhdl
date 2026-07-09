# Attribute Vocabulary, Resolution, and Consumers

> Implementation-grounded (2026-07). This document is the *system* view of
> attributes: the canonical vocabulary (which attribute names the toolchain
> acts on and what they mean), how an attribute's value is resolved for a given
> instance, and which passes consume which attributes. The *declaration* syntax
> and value/typing rules are in the companion
> [BHDL_Attribute_Type_System.md](BHDL_Attribute_Type_System.md); the language
> summary is [BHDL_Complete_Specification.md](BHDL_Complete_Specification.md) §6.3.
>
> ("Unified" is historical — before this convention, identity attributes were
> used ad-hoc by some passes and ignored by others with no agreed naming. The
> unification is that producers and consumers now share one vocabulary.)

## 1. Two entity granularities

An attribute means different things depending on the kind of entity that
carries it (`bhdl-common::sku` header):

- **Concrete part entity** — names one specific orderable part
  (`NPN_2N3904`, `LM358_DIP8`). It should declare a full SKU: `manufacturer` +
  `mpn` + `physical_package`, ideally with distributor part numbers.
- **Abstract type entity** — names a category (`Res`, `Cap`, `Ind`). The user
  supplies a value at instantiation (`Res(575Ω)`); the entity declares the
  *shape* attributes (`component_class = "resistor"`) but no MPN, and the part
  chooser resolves a concrete part. A board instance may pin a specific part
  with an explicit `attribute mpn = …` override.

## 2. Canonical SKU vocabulary

The identity/manufacturing attributes are defined as constants in
`bhdl-common::sku::attr`, so a misspelling fails to line up at the consumer
boundary rather than silently disappearing:

| Attribute | Meaning |
|-----------|---------|
| `manufacturer` | Manufacturer name as they spell it (e.g. "Yageo"). |
| `mpn` | Canonical manufacturer part number — the orderable identifier (preferred over the historic `part_number`). |
| `physical_package` | Package/case (e.g. `SOT-23-6`). Named thus because `package` is a reserved word. |
| `footprint` | PCB footprint identifier. |
| `kicad_symbol` | Schematic symbol identifier for KiCad export. |
| `datasheet` | Datasheet URL/reference. |
| `tolerance` | Value tolerance (`1%`). |
| `voltage_rating`, `power_rating`, `temp_coeff` | Stress ratings / temperature coefficient. |
| `component_class` | The semantic class — see §3. |
| `refdes_prefix` | Overrides the class-derived reference-designator prefix. |
| `digikey_pn`, `mouser_pn`, `lcsc_pn`, `arrow_pn`, `nexar_pn` | Distributor part numbers (`<distributor>_pn` pattern). |

`manufacturer` + `mpn` + `physical_package` together are the
**order-ready** set — the minimum for the BOM walker to emit an orderable
row (`sku::required_for_order_ready`). A consumer can check which of these is
missing and report it, rather than emit a half-specified row.

## 3. `component_class` — the semantic anchor

`component_class` is the most-read attribute in the toolchain. It states what
a part *is*, and drives:

- **Reference-designator prefix** via `refdes_prefix_for_class` (overridable
  with `refdes_prefix`):

  | Class(es) | Prefix |
  |-----------|--------|
  | `resistor` | `R` |
  | `capacitor` | `C` |
  | `inductor` | `L` |
  | `diode`, `led`, `tvs_diode` | `D` |
  | `fuse` | `F` |
  | `bjt`, `mosfet`, `jfet`, `triode`, … | `Q` |
  | `ic_opamp`, `ic_regulator`, `ic_mcu`, `switching_regulator`, … | `U` |
  | `crystal`, `oscillator` | `Y` |
  | `connector`, `header` | `J` |
  | `test_point` | `TP` |
  | `switch`, `relay` | `SW` |

- **Device-family discovery** — the part chooser and the SPICE model builder
  find candidate parts and select a model by class.
- **Schematic symbol** choice and **ERC applicability** — which rules apply to
  a part (a `voltage_regulator` gets dropout/rail checks; a `capacitor` gets
  the polarized-reversal check).

## 4. Resolution and late-binding

The value of an attribute for a given instance is resolved in layers:

- **Entity default** — the attribute as the entity declares it, with
  parameter references (`attribute resistance = value;`) resolved against the
  instance's constructor arguments and any entity-parameter defaults. The
  synthesizer stamps constructor arguments (and defaults) onto the instance so
  `design {}` / `simulation {}` blocks can read them via `self.<param>`.
- **Instance / board override** — a board may attach an attribute to a
  specific instance (e.g. `attribute mpn = "…"` to pin a part), overriding the
  entity default for that instance only.
- **Order-independent late-binding** — attribute defaults for every entity are
  gathered into a global index across *all* imported files, so an entity
  referenced from a sibling file gets its attributes attached regardless of
  import order. A late-binding pass fills in attributes the extraction-time
  overlay missed. This is why import order never changes the result.

## 5. Toolchain-stamped attributes

Beyond the user-authored vocabulary, synthesis *stamps* attributes onto
instances to carry provenance and control. These are read by the toolchain,
not typically hand-authored:

- **Provenance**: `refdes` (the allocated designator — §Handles_And_Refdes),
  `expansion_parent` / `vpin_parent` / `vpin_role` (which parent an expansion
  child belongs to), `auto_created`, `abstract_origin` / `selected_sku` (which
  concrete SKU an abstract entity resolved to), `socketed_in`, `stage_name` /
  `stage_order` / `stage_rail` (supply-tree position).
- **Control**: `do_not_populate` + `dnp_reason` (a DNP part stays in the
  structural netlist but every electrical/BOM consumer skips it —
  [Board_SKU_Variants.md](Board_SKU_Variants.md)), `erc_waive` (a reasoned ERC
  waiver, printed in a separate table — [ERC.md](ERC.md)), `expansion_skipped`.
- **Model surface**: the `spice_*` family (`spice_type`, `spice_model`,
  `spice_is`, `spice_rs`, `spice_vsat_p`, …) — device model parameters the
  DC/transient solver stamps for the active-device model
  ([Vendor_Simulation_Blocks.md](Vendor_Simulation_Blocks.md)).
- **Intent**: the `intent_*` family (`intent_name`, `intent_max_ripple`, …) —
  carried from a flow's `for INTENT(...)` clause (main spec §3.4) for
  synthesis and ERC to read.
- **Supply metadata**: `supply_*` / `i_supply` — stamped by the `supply`
  desugarer and read by sign-off and ERC016; these are exempt from the
  constructor-argument check (main spec §7.3).

## 6. Consumers are read-only

Every consumer reads attributes; none but the synthesis passes that own a
stamp *write* them. The BOM walker reads SKU identity; the part chooser reads
`component_class` + ratings + `tolerance`; ERC reads class + electrical facts +
`erc_waive`; the model builder reads `spice_*`; sign-off reads ratings and
`supply_*`; the schematic reads `component_class`, `refdes`, `kicad_symbol`,
and display-equation strings; refdes allocation reads `component_class` /
`refdes_prefix`; PnR/KiCad export reads `footprint` / `physical_package` /
`socketed_in`. Because attributes feed analysis and manufacturing, an
attribute a consumer needs but cannot find yields UNCHECKED or a loud error —
never a fabricated value (Real-Data Policy).

## 7. Not yet implemented

A *typed* attribute-declaration surface (schemas, capabilities, validation
rules) and a *behavioral* per-timestep attribute surface were designed but are
not built. See [BHDL_Attribute_Type_System.md](BHDL_Attribute_Type_System.md)
§7 and [Behavioral_Models.md](Behavioral_Models.md). Today's attributes are
static bindings, resolved once at synthesis, read by the consumers above.
