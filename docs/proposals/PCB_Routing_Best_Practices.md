# PCB Routing Best Practices for BHDL PnR Engine

This document codifies industry-standard PCB routing practices that should
guide the bhdl-pnr engine's decisions. Each section maps to a specific PnR
subsystem. Rules are annotated with `[IMPL]` when implemented, `[TODO]` when
planned.

---

## 1. Layer Assignment Strategy

### 2-Layer Board
- Top: components + signal routing + power traces (wider)
- Bottom: ground pour (as continuous as possible) + overflow routing
- All nets routed as traces; no dedicated planes
- Acceptable for designs under ~25 MHz
- `[TODO]` Ground pour on bottom layer (not routed traces)

### 4-Layer Board (Most Common)

**Preferred stackup: SIG – GND – PWR – SIG**
- Layer 1 (F.Cu): Components + high-speed signals + general routing
- Layer 2 (In1): Continuous ground plane (primary reference)
- Layer 3 (In2): Power plane (can be split for multiple rails)
- Layer 4 (B.Cu): Signal routing + power traces as wide traces

**Alternative: SIG – GND – GND – SIG**
- Two ground planes provide better shielding
- Power distributed as routed traces on signal layers
- Preferred for mixed-signal or high-speed designs

**Key rule**: Every signal layer must have an adjacent ground reference plane.
- `[IMPL]` 4-layer stackup with SIG/GND/PWR/SIG
- `[IMPL]` GND net uses ground plane (skip trace routing)
- `[TODO]` Power plane splits for multiple voltage rails

### 6/8-Layer Boards
- 6L: SIG – GND – SIG – PWR – GND – SIG
- 8L: SIG – GND – SIG – PWR – GND – SIG – GND – SIG
- Never place two signal layers adjacent (causes crosstalk)
- Maintain stackup symmetry (prevents warping)
- `[IMPL]` 6-layer and 8-layer stackup presets in `stackup.rs`

---

## 2. Routing Order / Priority

### Recommended Sequence

| Priority | Net Type | Rationale |
|----------|----------|-----------|
| 1 | Power/ground infrastructure | Establish power distribution first |
| 2 | Clock signals | Shortest, most direct paths; sensitive to noise |
| 3 | High-speed differential pairs | USB, Ethernet, PCIe — impedance controlled |
| 4 | Length-matched buses | DDR, parallel buses — timing critical |
| 5 | Sensitive analog signals | ADC/DAC paths, precision circuits |
| 6 | General digital signals | SPI, I2C, UART, GPIO |
| 7 | Non-critical connections | LEDs, test points, mounting |

### Two-Pass Routing Strategy

**Pass 1 — Single-layer routing (no vias)**
- Route as many nets as possible on the primary signal layer (F.Cu)
- Maximizes F.Cu utilization; avoids unnecessary vias
- Signal nets first (short, 2-pin connections succeed easily)
- Power nets attempted but may fail (many pins, wide traces)

**Pass 2 — Via-assisted routing for remaining nets**
- Route power nets first (benefit most from B.Cu — wide traces, many pins)
- Then remaining signal nets (escape congested areas via layer change)
- Reduce F.Cu capacity where pass 1 routes exist

**Rationale**: Matches human PCB designer workflow. Minimizes via count.
Professional designers typically achieve <50 vias for a ~50 component board.

- `[IMPL]` Two-pass routing (pass 1 no vias, pass 2 with vias)
- `[TODO]` Priority ordering within each pass (power first in pass 2)
- `[TODO]` Net weight from intent annotations (high-speed > general signal)

---

## 3. Via Usage

### When to Use Vias
- When a net cannot be completed on a single layer (blocked by other routes)
- For power distribution (connect to power/ground planes)
- For BGA fan-out (inner ball escape)
- For ground stitching (connect ground pours to ground plane)

### When NOT to Use Vias
- High-speed signals: each via adds ~0.5–1 nH inductance, ~0.3–0.5 pF capacitance
- Short signal routes that fit on one layer
- Near impedance-critical trace sections

### Via Types (in order of preference)

| Type | Use Case | Cost |
|------|----------|------|
| Through-hole | Standard layer transitions | Standard |
| Blind | HDI, space-constrained | Higher |
| Buried | Internal routing density | Premium |
| Microvia | Fine-pitch BGA, HDI | Highest |
| Via-in-pad | BGA thermal/signal pads | Premium (requires fill) |

### Via Rules
- Minimum spacing: 0.4mm (15 mil) between vias
- Place return-path ground via near every signal via (within 1mm)
- Via stitching for ground: every λ/10 (e.g., 6mm at 2.4 GHz)
- Avoid vias in impedance-controlled trace runs
- `[IMPL]` Through-via model in PathFinder (any layer to any layer)
- `[TODO]` Return-path via placement
- `[TODO]` Via-in-pad for BGA components

---

## 4. Power Routing

### Trace Width (IPC-2221, 1 oz copper, 10°C rise, external layer)

| Current | Trace Width |
|---------|-------------|
| 0.5A | 0.13mm (5 mil) |
| 1A | 0.25mm (10 mil) |
| 2A | 0.76mm (30 mil) |
| 3A | 1.27mm (50 mil) |
| 5A | 2.80mm (110 mil) |

Internal layers need 2–3× wider traces for the same current.

### Power Distribution
- **Planes** (4+ layers): Lowest impedance; natural inter-plane capacitance (~1nF/in²)
- **Star**: Single point for all grounds; reduces ground loops (mixed analog/digital)
- **Bus**: Parallel traces serving multiple loads; acceptable for low current
- `[IMPL]` IPC-2221 trace width calculation in `stackup::trace_width_for_current()`
- `[IMPL]` Power net trace width from GLACIER current data

### Decoupling Capacitor Placement
- Within 1.3–2.5mm (50–100 mil) of IC power pins
- Smallest value closest to pin (100nF closest, 10µF bulk further)
- Via directly in or adjacent to cap pad (not via a trace run)
- Connection order: IC pin → capacitor → via to plane
- `[IMPL]` Expansion blocks auto-place decoupling caps
- `[TODO]` Placement constraint: caps within 2.5mm of parent IC

### Power Plane Splits
- Split power planes for multiple voltages when needed
- **Never split the ground plane** — keep continuous
- Place stitching capacitors at split boundaries (100nF + 1µF)
- Never route high-speed signals across a plane split
- `[TODO]` Power plane split awareness in routing

---

## 5. Signal Integrity

### Controlled Impedance
- Single-ended: 50Ω (standard)
- Differential: 90–100Ω (100Ω most common: LVDS, USB, Ethernet)
- Manufacturing tolerance: ±10%
- Determined by: trace width, dielectric thickness, copper weight, εr
- `[TODO]` Impedance-controlled net class with width computation

### Differential Pair Routing
- Route both traces simultaneously on same layer
- Maintain uniform spacing along entire length
- Length match to within 5 mil (0.127mm)
- Continuous reference plane beneath both traces
- Pair-to-pair spacing: ≥ 5W (5× trace width)
- `[TODO]` Differential pair routing mode in PathFinder

### Crosstalk Mitigation
- **3W Rule**: Center-to-center trace spacing ≥ 3× trace width (70% reduction)
- **10W Rule**: For critical signals (98% reduction)
- **3H Rule**: Spacing ≥ 3× height above reference plane
- Guard traces with ground vias for sensitive signals
- `[TODO]` 3W spacing enforcement in DRC

### Return Path Continuity
- **Never route signals across plane splits or gaps**
- Return current flows directly beneath the signal on the reference plane
- When changing layers: place ground via near signal via (within 1mm)
- `[TODO]` Plane split crossing detection in DRC

---

## 6. Component Placement Rules

### General
- Connectors at board edges
- High-power components (regulators, MOSFETs) need thermal relief copper
- Temperature-sensitive components away from heat sources
- `[IMPL]` Edge constraint for connectors in PlacementConstraint
- `[IMPL]` Thermal spreading force in placement optimizer

### Decoupling Capacitors
- Within 2.5mm of IC power pins
- Smallest value closest to pin
- `[IMPL]` Functional group cohesion for expansion block children

### Crystal / Oscillator
- Immediately adjacent to host IC (< 10mm trace length)
- Do NOT route other signals underneath
- Guard ring (ground copper) around crystal circuit
- `[TODO]` Crystal placement constraint

---

## 7. DRC Rules

### Clearances (IPC-2221)

| Voltage | Clearance | Creepage |
|---------|-----------|----------|
| 0–50V | 0.6mm | 0.9mm |
| 51–100V | 0.6mm | 1.2mm |
| 101–150V | 0.8mm | 1.6mm |
| 151–300V | 1.6mm | 3.2mm |

### Trace Geometry
- **45° routing**: Industry standard (135° corners)
- **90° bends**: Avoid (impedance discontinuity, acid traps)
- **Arc routing**: Preferred for RF (> 1 GHz)
- `[TODO]` 45° diagonal routing in PathFinder (currently 8-connected grid)

### Manufacturing
- Minimum trace/space: 0.15mm (6 mil) standard; 0.10mm (4 mil) fine-pitch
- Annular ring: ≥ 0.13mm (5 mil) IPC Class 2; ≥ 0.18mm (7 mil) Class 3
- Teardrops at pad-to-trace and via-to-trace junctions
- `[IMPL]` Minimum trace width and spacing in BoardConfig
- `[IMPL]` Component spacing DRC check
- `[TODO]` Teardrop generation

---

## 8. BHDL-Specific Advantages

These are features unique to BHDL's semantically-aware PnR that go beyond
traditional EDA tools:

### Intent-Driven Routing
- `for precision_measurement(accuracy: 0.1%)` → force 3W spacing, guard traces
- `for input_protection(overvoltage: 6V)` → route protection devices close to connector
- `for automotive_safety()` → enforce creepage/clearance per ISO 26262
- `[TODO]` Map intent annotations to routing constraints

### GLACIER-Informed Decisions
- Actual operating current from DC simulation → trace width
- Power dissipation → thermal via placement
- Voltage at each node → clearance requirements
- `[IMPL]` GLACIER current → trace width via IPC-2221
- `[IMPL]` GLACIER power → thermal_power_w for thermal spreading

### Expansion Block Awareness
- Expansion children (L, D, C for buck regulator) cluster near parent IC
- Cap bank members maintain parallel routing
- `[IMPL]` Functional group cohesion force
- `[TODO]` Enforce cap-to-IC distance constraint (< 2.5mm)

---

## Implementation Roadmap

### Phase 1 (Current) ✅
- [x] IPC-7351B footprint generation
- [x] Two-pass routing (single-layer → via-assisted)
- [x] GND plane connection
- [x] IPC-2221 trace width from GLACIER current
- [x] 8-connected diagonal routing
- [x] Functional group cohesion
- [x] Through-via model

### Phase 2 (Next)
- [ ] Routing priority order (power → signals in pass 2)
- [ ] 45° trace angle enforcement
- [ ] 3W spacing DRC rule
- [ ] Decoupling cap distance constraint (< 2.5mm from IC)
- [ ] Return-path via placement

### Phase 3 (Future)
- [ ] Impedance-controlled routing (50Ω/100Ω)
- [ ] Differential pair routing mode
- [ ] Power plane split awareness
- [ ] Intent-driven routing constraints
- [ ] BGA fan-out strategies
- [ ] Teardrop generation
