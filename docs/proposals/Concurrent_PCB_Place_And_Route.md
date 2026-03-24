# Concurrent Semantically-Aware PCB Place & Route

## Summary

A PCB placement and routing engine for BHDL that co-optimizes component placement and trace routing in a unified iterative loop. Unlike sequential approaches (place-then-route) or proxy-based methods (Cypress's net crossing), this system runs actual PathFinder routing on a coarse 3D grid during placement iterations, feeding real congestion and via-count data back as placement forces.

BHDL's unique semantic data — intent annotations, GLACIER operating point, expansion block hierarchy, power domain structure — drives layer assignment, trace width selection, and placement grouping in ways no existing tool can match.

## Motivation

### Why Not Just Use Cypress?

Cypress (ISPD 2025 Best Paper) is state-of-the-art for PCB placement, but has fundamental gaps:

| Limitation | Impact |
|---|---|
| 2-layer only | Can't handle 4/6/8-layer production boards |
| Net crossing as routability proxy | Overcounts crossings resolvable by layer assignment |
| No routing feedback | Placement quality unknown until post-hoc routing |
| Discrete rotation (4 angles) | PCB components can be placed at any angle |
| No semantic awareness | Treats all nets equally regardless of purpose |
| Layer assignment pre-determined | Misses co-optimization opportunity |

### Why Concurrent P&R?

Sequential place-then-route leaves optimization on the table. By routing *during* placement:

1. **Congestion-driven placement**: Components move away from routing bottlenecks
2. **Via-aware placement**: Connected components on different sides get penalized by actual via count
3. **Layer-aware crossing**: Only penalize crossings on the *same routing layer*
4. **Unroutability detection**: PathFinder failure = definitive signal to restructure placement

### Why Semantically-Aware?

BHDL knows things about the circuit that no netlist-only tool can infer:

| BHDL Data Source | P&R Decision |
|---|---|
| `for precision_measurement(accuracy: 0.1%)` | Route adjacent to unbroken ground plane |
| `for input_protection(overvoltage: 6V)` | Keep traces short, outer layer |
| GLACIER: I = 2A through trace | Trace width = 0.5mm (not default 0.2mm) |
| GLACIER: P = 1.2W on regulator | Thermal spacing, copper pour |
| Expansion block: TPS54331 + {L, D, C} | Place children clustered around IC |
| Power domain: V3_3 depends on V5_BUCK | Sequence-aware placement proximity |
| Stage chain: `\|> input_filtering` | Input caps placed at power entry point |

## Architecture

### System Overview

```
                    ┌─────────────────────────────────────────┐
                    │          BHDL Pipeline (existing)        │
                    │                                         │
                    │  Source → Parse → Analyze → Synthesize  │
                    │      → Expand → GLACIER → Physical Sel  │
                    └────────────────┬────────────────────────┘
                                     │
                                     ▼
                    ┌─────────────────────────────────────────┐
                    │        Semantic Preprocessor             │
                    │                                         │
                    │  • Extract net classes + weights         │
                    │  • Build functional groups               │
                    │  • Assign layer constraints              │
                    │  • Compute trace width requirements      │
                    │  • Identify expansion block clusters     │
                    └────────────────┬────────────────────────┘
                                     │
              ┌──────────────────────┼──────────────────────┐
              │                      │                      │
              ▼                      ▼                      ▼
     ┌────────────────┐   ┌──────────────────┐   ┌──────────────────┐
     │   Placement    │   │   3D Routing     │   │   Convergence    │
     │   Engine       │◄──│   Grid           │──►│   Monitor        │
     │                │   │                  │   │                  │
     │ • Analytical   │   │ • PathFinder     │   │ • WL tracking    │
     │   forces (WL,  │   │ • Coarse grid    │   │ • Congestion Δ   │
     │   density)     │   │ • Layer-aware    │   │ • Via count Δ    │
     │ • Congestion   │   │ • Intent-weighted│   │ • Divergence     │
     │   inflation    │   │   capacity       │   │   detection      │
     │ • Via penalty  │   │ • GLACIER trace  │   │ • Rollback       │
     │ • Continuous   │   │   widths         │   │                  │
     │   rotation     │   │                  │   │                  │
     └───────┬────────┘   └──────────────────┘   └──────────────────┘
              │
              ▼
     ┌────────────────┐
     │  Legalization  │
     │                │
     │ • Snap to grid │
     │ • DRC check    │
     │ • Final route  │
     └───────┬────────┘
              │
              ▼
     ┌────────────────┐
     │    Output      │
     │                │
     │ • KiCad PCB    │
     │ • Gerber       │
     │ • 3D preview   │
     └────────────────┘
```

### Crate Structure

New crate: `bhdl-pnr`

```
bhdl-pnr/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Public API: place_and_route()
│   ├── types.rs                  # Board, Component, Net, Layer, Via, Route
│   ├── stackup.rs                # Layer stack presets + auto-inference
│   ├── semantic.rs               # Semantic preprocessor (intent → constraints)
│   ├── placement/
│   │   ├── mod.rs
│   │   ├── analytical.rs         # Wirelength + density forces
│   │   ├── rotation.rs           # Continuous rotation optimizer
│   │   ├── grouping.rs           # Expansion block clustering
│   │   └── optimizer.rs          # Adam/Nesterov update step
│   ├── routing/
│   │   ├── mod.rs
│   │   ├── grid.rs               # 3D routing grid construction
│   │   ├── pathfinder.rs         # Negotiated congestion router
│   │   ├── layer_assign.rs       # Intent-driven layer assignment
│   │   └── trace_width.rs        # GLACIER-driven width selection
│   ├── feedback/
│   │   ├── mod.rs
│   │   ├── congestion.rs         # Congestion map → density inflation
│   │   ├── via_penalty.rs        # Via count → placement force
│   │   └── convergence.rs        # Monitor + rollback
│   ├── legalization/
│   │   ├── mod.rs
│   │   ├── snap.rs               # Grid snapping
│   │   └── drc.rs                # Design rule checking
│   └── output/
│       ├── mod.rs
│       ├── kicad.rs              # KiCad .kicad_pcb export
│       └── visualization.rs     # HTML/Canvas preview
```

## Detailed Design

### 0. Layer Stackup Configuration

Layer stackup is the foundation — it determines routing capacity, impedance, and layer assignment strategy. BHDL provides three ways to specify it:

#### 0.1 Standard Presets (CLI flag)

```
bhdl-cli circuit.bhdl layout --layers 2
bhdl-cli circuit.bhdl layout --layers 4        # default
bhdl-cli circuit.bhdl layout --layers 6
bhdl-cli circuit.bhdl layout --layers 8
```

```rust
pub enum StackupPreset {
    TwoLayer,
    FourLayer,
    SixLayer,
    EightLayer,
}

pub fn stackup_preset(preset: StackupPreset) -> LayerStack {
    match preset {
        StackupPreset::TwoLayer => LayerStack {
            // Simple: Signal / Signal
            // Use case: Hobby, low-complexity, cost-sensitive
            // Routing: All signals share 2 layers, ground pours (no full plane)
            layers: vec![
                Layer { id: 0, name: "F.Cu".into(),  kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
                Layer { id: 1, name: "B.Cu".into(),  kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
            ],
            dielectrics: vec![
                Dielectric { thickness_mm: 1.53, material: "FR4".into(), er: 4.3, loss_tangent: 0.02 },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec { drill_mm: 0.3, pad_mm: 0.6, annular_ring_mm: 0.15 },
        },

        StackupPreset::FourLayer => LayerStack {
            // Standard professional: Signal / Ground / Power / Signal
            // Use case: Most production boards, moderate complexity
            // Routing: Signals on L1+L4, reference planes on L2 (GND) + L3 (PWR)
            // Impedance: 50Ω single-ended achievable with standard geometry
            layers: vec![
                Layer { id: 0, name: "F.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
                Layer { id: 1, name: "In1.Cu".into(), kind: LayerKind::Ground, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 2, name: "In2.Cu".into(), kind: LayerKind::Power,  thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 3, name: "B.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
            ],
            dielectrics: vec![
                Dielectric { thickness_mm: 0.10, material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 1.20, material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.10, material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec { drill_mm: 0.3, pad_mm: 0.6, annular_ring_mm: 0.15 },
        },

        StackupPreset::SixLayer => LayerStack {
            // Complex mixed-signal: Signal / Ground / Signal / Signal / Power / Signal
            // Use case: Mixed analog/digital, moderate high-speed
            // Routing: 4 signal layers, 2 reference planes
            // L1 signals reference L2 (GND), L3/L4 signals reference each other + planes
            layers: vec![
                Layer { id: 0, name: "F.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
                Layer { id: 1, name: "In1.Cu".into(), kind: LayerKind::Ground, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 2, name: "In2.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 3, name: "In3.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 4, name: "In4.Cu".into(), kind: LayerKind::Power,  thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 5, name: "B.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
            ],
            dielectrics: vec![
                Dielectric { thickness_mm: 0.10,  material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.20,  material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.56,  material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.20,  material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.10,  material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec { drill_mm: 0.25, pad_mm: 0.5, annular_ring_mm: 0.125 },
        },

        StackupPreset::EightLayer => LayerStack {
            // High-speed/dense: Sig / GND / Sig / Sig / Sig / Sig / PWR / Sig
            // Use case: DDR, high-pin-count FPGAs, dense designs
            // Routing: 6 signal layers, excellent shielding
            // Every signal layer adjacent to a reference plane
            layers: vec![
                Layer { id: 0, name: "F.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
                Layer { id: 1, name: "In1.Cu".into(), kind: LayerKind::Ground, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 2, name: "In2.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 3, name: "In3.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 4, name: "In4.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 5, name: "In5.Cu".into(), kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.8 },
                Layer { id: 6, name: "In6.Cu".into(), kind: LayerKind::Power,  thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 0.0 },
                Layer { id: 7, name: "B.Cu".into(),   kind: LayerKind::Signal, thickness_mm: 0.035, copper_weight_oz: 1.0, dielectric_constant: 4.3, capacity_factor: 1.0 },
            ],
            dielectrics: vec![
                Dielectric { thickness_mm: 0.075, material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.10,  material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.36,  material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.10,  material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.36,  material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.10,  material: "Core".into(),    er: 4.3, loss_tangent: 0.02 },
                Dielectric { thickness_mm: 0.075, material: "Prepreg".into(), er: 4.2, loss_tangent: 0.02 },
            ],
            total_thickness_mm: 1.6,
            via: ViaSpec { drill_mm: 0.2, pad_mm: 0.45, annular_ring_mm: 0.125 },
        },
    }
}
```

#### 0.2 Automatic Layer Count Inference

When no `--layers` flag is given, infer from circuit complexity:

```rust
pub fn infer_layer_count(netlist: &Netlist, analysis: &AnalysisResult) -> StackupPreset {
    let num_components = count_non_virtual_instances(netlist);
    let num_nets = count_signal_nets(netlist);
    let has_high_speed = has_intent(analysis, "fast_response")
                      || has_intent(analysis, "precision_measurement");
    let max_current = max_glacier_current(analysis);
    let num_power_domains = count_power_domains(analysis);

    // Heuristic thresholds (can be tuned)
    if num_components <= 15 && num_nets <= 20 && !has_high_speed {
        StackupPreset::TwoLayer
    } else if num_components <= 100 && num_power_domains <= 4 && !has_high_speed {
        StackupPreset::FourLayer
    } else if num_components <= 300 || has_high_speed {
        StackupPreset::SixLayer
    } else {
        StackupPreset::EightLayer
    }
}
```

#### 0.3 BHDL Syntax (Future)

The parser already has `LAYER_STACKUP_KW` and `LAYER_STACKUP_BLOCK` syntax kinds. Future work will wire up parsing to allow explicit stackup in BHDL source:

```bhdl
board MyBoard {
    layer_stackup {
        layer F_Cu: signal { copper = 1oz; }
        layer In1:  ground { copper = 1oz; }
        layer In2:  power  { copper = 1oz; }
        layer B_Cu: signal { copper = 1oz; }
    }

    // ... circuit connections
}
```

This is not needed for Phase 1 — presets + auto-inference cover 95% of use cases.

#### 0.4 How Stackup Affects P&R

The layer stack directly configures the 3D routing grid and placement constraints:

| Stackup property | P&R effect |
|---|---|
| Number of signal layers | Grid Z-dimension: more layers = more routing capacity per cell |
| Ground plane layers | `capacity_factor = 0.0`: blocked for routing, available as reference |
| Power plane layers | `capacity_factor = 0.0`: blocked for routing, power distributed via plane |
| Dielectric thickness | Impedance calculation: `Z0 = f(trace_width, dielectric_thickness, εr)` |
| Copper weight | Current capacity: 1oz = 35µm, affects IPC-2221 trace width calculation |
| Via specs | Via cost in PathFinder: smaller vias = less area consumed = lower cost |

**Layer constraint resolution** — how intents map to specific layers given a stackup:

```rust
fn resolve_layer_constraint(
    constraint: &LayerConstraint,
    stack: &LayerStack,
) -> Vec<usize> {
    match constraint {
        LayerConstraint::Any => {
            // All signal layers
            stack.signal_layer_indices()
        }
        LayerConstraint::AdjacentToGround => {
            // Signal layers that have a ground plane as immediate neighbor
            stack.layers_adjacent_to(LayerKind::Ground)
            // 4-layer: [0, 3] (F.Cu next to In1.Cu=GND, B.Cu next to In2.Cu)
            // 6-layer: [0, 2] (F.Cu next to In1.Cu=GND, In2.Cu next to In1.Cu=GND)
        }
        LayerConstraint::InnerOnly => {
            // Inner signal layers (shielded by outer copper)
            stack.inner_signal_layer_indices()
            // 6-layer: [2, 3]
            // 8-layer: [2, 3, 4, 5]
        }
        LayerConstraint::Required(layer_id) => vec![*layer_id],
        LayerConstraint::Preferred(layer_id) => vec![*layer_id], // soft preference
    }
}
```

### 1. Board Outline, Fixed Components, and Keep-Outs

#### 1.1 BHDL Syntax

Board outline, fixed placements, mounting holes, and keep-out zones are specified inside the `board` block using a `physical { }` section. This reuses the existing `attribute` system with a new structured block:

```bhdl
board ComplexPowerTree {
    // === Physical Board Definition ===
    physical {
        // Board outline — rectangle or polygon
        outline = rectangle(80mm, 60mm);
        // outline = polygon((0,0), (80,0), (80,50), (60,60), (0,60));  // irregular

        // Layer stackup (alternative to --layers CLI flag)
        layers = 4;
        // layers = stackup { ... };  // future: full custom stackup

        // Mounting holes — blocked for routing and placement
        mount M1 = hole(3.2mm) at (4mm, 4mm);
        mount M2 = hole(3.2mm) at (76mm, 4mm);
        mount M3 = hole(3.2mm) at (4mm, 56mm);
        mount M4 = hole(3.2mm) at (76mm, 56mm);

        // Keep-out zones — no components or traces
        keepout = rectangle(15mm, 10mm) at (30mm, 0mm);   // connector cutout
        keepout = circle(5mm) at (40mm, 30mm);             // heatsink clearance

        // Fixed component placements — position locked, still participates in routing
        place barrel_jack at (0mm, 30mm) edge(left) rotate(0);
        place usb_conn   at (40mm, 0mm) edge(bottom) rotate(0);
        place debug_hdr   at (70mm, 60mm) edge(top) rotate(180);

        // Placement regions — soft constraints (prefer, don't require)
        region "power" = rectangle(20mm, 40mm) at (5mm, 10mm);
        prefer buck, reg33, reg5aux, reg18 in "power";

        // Thermal constraints from GLACIER
        // (auto-derived: components with P > 0.5W get 3mm thermal spacing)
    }

    // === Electrical (existing syntax, unchanged) ===
    power VIN = 24V @ 5A |> input_filtering(max_ripple: 50mV) |> regulation;
    // ...

    // Fixed components are instantiated normally, just referenced in physical { }
    VIN -> barrel_jack: BarrelJack(5.5mm).VCC;
    barrel_jack.GND -> GND;
    // ...
}
```

**Key principles:**
- `place ... at (x, y)` fixes position and optionally rotation. The component still participates in wirelength/routing optimization — only its position is frozen.
- `edge(left|right|top|bottom)` is sugar for anchoring to a board edge. The coordinate on the edge axis is the offset along that edge.
- `mount` defines non-electrical features (mounting holes) that block placement and routing.
- `keepout` defines zones that block both component placement and trace routing.
- `region` + `prefer` are soft constraints — the placer tries to put components in the named region but won't fail if it can't.

#### 1.2 Attribute-Based Fallback (No Parser Changes)

Until `physical { }` parsing is implemented, the same information can be expressed with attributes on instances, which the pipeline already supports:

```bhdl
board MyBoard {
    // Board dimensions via board-level attributes
    attribute board_width = 80;     // mm
    attribute board_height = 60;    // mm
    attribute board_layers = 4;

    // Fixed placement via instance attributes
    VIN -> jack: BarrelJack(5.5mm).VCC;
    attribute jack.fixed = true;
    attribute jack.x = 0;           // mm from board origin
    attribute jack.y = 30;
    attribute jack.rotation = 0;    // degrees
    attribute jack.side = "top";
    attribute jack.edge = "left";   // hint: placed on left edge

    VIN -> usb: USBTypeC().VBUS;
    attribute usb.fixed = true;
    attribute usb.x = 40;
    attribute usb.y = 0;
    attribute usb.edge = "bottom";

    // Mounting holes as virtual entities
    mount1: MountingHole(3.2mm);
    attribute mount1.fixed = true;
    attribute mount1.x = 4;
    attribute mount1.y = 4;
}
```

This works today — `instance.attributes` is `HashMap<String, String>` and the semantic preprocessor can read `fixed`, `x`, `y`, `rotation`, `side`, `edge` from it.

#### 1.3 Board Model Types

```rust
/// Board configuration provided by user or inferred
pub struct BoardConfig {
    pub outline: BoardOutline,          // Board shape
    pub stackup: StackupSource,         // Layer stack
    pub min_trace_width_mm: f64,        // Minimum trace width (default: 0.15)
    pub min_spacing_mm: f64,            // Minimum clearance (default: 0.15)
    pub edge_clearance_mm: f64,         // Component keep-out from board edge (default: 0.5)
    pub fixed_placements: Vec<FixedPlacement>,  // Mechanically constrained components
    pub mounting_holes: Vec<MountingHole>,       // Non-electrical features
    pub keepout_zones: Vec<KeepoutZone>,         // No-go areas
    pub placement_regions: Vec<PlacementRegion>, // Soft grouping hints
}

/// Board outline — defines the physical boundary
pub enum BoardOutline {
    Rectangle { width_mm: f64, height_mm: f64 },
    Polygon(Vec<(f64, f64)>),       // Arbitrary outline (clockwise vertices)
    AutoSize,                       // Infer from components (40% utilization target)
}

impl BoardOutline {
    pub fn width(&self) -> f64 {
        match self {
            BoardOutline::Rectangle { width_mm, .. } => *width_mm,
            BoardOutline::Polygon(pts) => {
                let max_x = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
                let min_x = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                max_x - min_x
            }
            BoardOutline::AutoSize => 0.0,  // resolved later
        }
    }
    pub fn height(&self) -> f64 { /* analogous */ }
    pub fn contains(&self, x: f64, y: f64) -> bool { /* point-in-polygon */ }
}

/// Component with mechanically fixed position
pub struct FixedPlacement {
    pub instance_name: String,      // References BHDL instance (e.g., "barrel_jack")
    pub x_mm: f64,                  // X from board origin (bottom-left)
    pub y_mm: f64,                  // Y from board origin
    pub rotation_deg: f64,          // Rotation in degrees
    pub side: BoardSide,            // Top or Bottom
    pub edge: Option<BoardEdge>,    // Which edge this is anchored to (display hint)
}

pub enum BoardEdge { Left, Right, Top, Bottom }

/// Mounting hole — blocks placement and routing in its area
pub struct MountingHole {
    pub x_mm: f64,
    pub y_mm: f64,
    pub drill_mm: f64,              // Hole diameter
    pub keepout_mm: f64,            // Clearance ring around hole (default: 2mm)
}

/// Keep-out zone — no components or traces allowed
pub struct KeepoutZone {
    pub shape: ZoneShape,
    pub applies_to: KeepoutTarget,  // Components, routing, or both
}

pub enum ZoneShape {
    Rectangle { x: f64, y: f64, w: f64, h: f64 },
    Circle { x: f64, y: f64, r: f64 },
    Polygon(Vec<(f64, f64)>),
}

pub enum KeepoutTarget {
    All,                // No components AND no traces
    ComponentsOnly,     // No components, traces OK (e.g., under heatsink)
    RoutingOnly,        // Components OK, no traces (e.g., flex zone)
}

/// Soft placement hint — prefer components in a named region
pub struct PlacementRegion {
    pub name: String,               // "power", "analog", "digital"
    pub shape: ZoneShape,
    pub preferred_instances: Vec<String>,  // Instance names to prefer here
    pub weight: f64,                // How strongly to prefer (default: 1.0)
}

impl Default for BoardConfig {
    fn default() -> Self {
        BoardConfig {
            outline: BoardOutline::AutoSize,
            stackup: StackupSource::Auto,
            min_trace_width_mm: 0.15,
            min_spacing_mm: 0.15,
            edge_clearance_mm: 0.5,
            fixed_placements: Vec::new(),
            mounting_holes: Vec::new(),
            keepout_zones: Vec::new(),
            placement_regions: Vec::new(),
        }
    }
}

pub enum StackupSource {
    Preset(StackupPreset),          // --layers 2/4/6/8
    Auto,                           // Infer from circuit complexity
    Custom(LayerStack),             // Explicit layer stack (future: from BHDL syntax)
}

/// Complete layer stack specification
pub struct LayerStack {
    pub layers: Vec<Layer>,
    pub dielectrics: Vec<Dielectric>,
    pub total_thickness_mm: f64,
    pub via: ViaSpec,
}

pub struct Dielectric {
    pub thickness_mm: f64,
    pub material: String,           // "FR4", "Prepreg", "Rogers"
    pub er: f64,                    // Relative permittivity
    pub loss_tangent: f64,          // Dissipation factor
}

pub struct ViaSpec {
    pub drill_mm: f64,              // Drill diameter
    pub pad_mm: f64,                // Pad diameter
    pub annular_ring_mm: f64,       // Ring width
}

impl LayerStack {
    /// Indices of layers available for signal routing
    pub fn signal_layer_indices(&self) -> Vec<usize> {
        self.layers.iter()
            .filter(|l| l.kind == LayerKind::Signal && l.capacity_factor > 0.0)
            .map(|l| l.id)
            .collect()
    }

    /// Signal layers adjacent to a given layer kind
    pub fn layers_adjacent_to(&self, kind: LayerKind) -> Vec<usize> {
        let mut result = Vec::new();
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.kind != LayerKind::Signal { continue; }
            // Check neighbor above
            if i > 0 && self.layers[i - 1].kind == kind { result.push(i); }
            // Check neighbor below
            if i + 1 < self.layers.len() && self.layers[i + 1].kind == kind { result.push(i); }
        }
        result
    }

    /// Inner signal layers (not first or last)
    pub fn inner_signal_layer_indices(&self) -> Vec<usize> {
        self.layers.iter()
            .filter(|l| l.kind == LayerKind::Signal && l.id != 0 && l.id != self.layers.len() - 1)
            .map(|l| l.id)
            .collect()
    }

    /// Via area consumed on each layer (for capacity reduction)
    pub fn via_blockage_mm2(&self) -> f64 {
        std::f64::consts::PI * (self.via.pad_mm / 2.0).powi(2)
    }
}

/// Physical PCB board with layer stack
pub struct Board {
    pub config: BoardConfig,
    pub layer_stack: LayerStack,
    pub components: Vec<Component>,
    pub nets: Vec<PnrNet>,
}

/// Single PCB layer
pub struct Layer {
    pub id: usize,
    pub name: String,
    pub kind: LayerKind,          // Signal, Ground, Power, Mixed
    pub thickness_mm: f64,
    pub copper_weight_oz: f64,    // 1oz = 35µm
    pub dielectric_constant: f64, // εr for impedance calculation
    pub capacity_factor: f64,     // routing capacity relative to full (0.0-1.0)
}

pub enum LayerKind {
    Signal,                       // General routing
    Ground,                       // Ground plane (capacity_factor ≈ 0.0)
    Power,                        // Power plane
    Mixed,                        // Signal + power routing
}

/// Component to be placed
pub struct Component {
    pub id: ComponentId,
    pub name: String,
    pub refdes: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub pins: Vec<PinPosition>,   // pin locations relative to component origin
    pub side: BoardSide,          // Top or Bottom (SMD constraint)
    pub group: Option<GroupId>,   // Expansion block or functional group
    pub thermal_power_w: f64,     // From GLACIER: power dissipation
    pub package: String,          // "0402", "SOT-23", "LQFP48"

    // Placement constraint
    pub placement: PlacementConstraint,

    // Mutable placement state
    pub x: f64,
    pub y: f64,
    pub theta: f64,               // Continuous rotation (radians)
}

pub enum PlacementConstraint {
    /// Fully free — optimizer controls position and rotation
    Free,
    /// Position fixed, rotation fixed — connector, power jack, mounting
    /// Set from `place ... at (x, y) rotate(r)` or `attribute fixed = true`
    Fixed { x: f64, y: f64, theta: f64 },
    /// Position fixed, rotation free — e.g., component must be at location but can rotate
    FixedPosition { x: f64, y: f64 },
    /// Constrained to board edge — position along edge is free
    /// e.g., `place usb at edge(bottom)` — x is free, y = 0
    Edge { edge: BoardEdge, offset: Option<f64> },
    /// Soft preference for a region — penalty if outside, not hard constraint
    PreferRegion { region_name: String },
}

pub struct PinPosition {
    pub pin_id: PinId,
    pub name: String,
    pub dx: f64,                  // Offset from component center
    pub dy: f64,
    pub net: Option<NetId>,
}

/// Net with semantic metadata
pub struct PnrNet {
    pub id: NetId,
    pub name: String,
    pub pins: Vec<(ComponentId, PinId)>,
    pub net_class: PnrNetClass,
    pub weight: f64,              // Placement force multiplier
    pub required_trace_width_mm: f64,  // From GLACIER current
    pub layer_constraint: LayerConstraint,
    pub intent: Option<String>,
}

pub enum PnrNetClass {
    Signal,
    Power { voltage: f64, current: f64 },
    Ground,
    HighSpeed { max_length_mm: f64 },
    DifferentialPair { partner: NetId },
}

pub enum LayerConstraint {
    Any,                          // Route on any signal layer
    Preferred(usize),             // Prefer specific layer
    Required(usize),              // Must use this layer
    AdjacentToGround,             // Must be on layer next to ground plane
    InnerOnly,                    // Inner layers only (shielded)
}
```

### 2. Semantic Preprocessor

Transforms BHDL pipeline output into P&R-ready data structures.

```rust
pub fn preprocess(
    netlist: &Netlist,
    analysis: &AnalysisResult,
    simulation: &SimulationAnnotations,
    board_config: &BoardConfig,
) -> Board {
    // 1. Resolve layer stack from config
    let layer_stack = match &board_config.stackup {
        StackupSource::Preset(preset) => stackup_preset(*preset),
        StackupSource::Auto => {
            let preset = infer_layer_count(netlist, analysis);
            stackup_preset(preset)
        }
        StackupSource::Custom(stack) => stack.clone(),
    };

    // 2. Convert netlist instances → Components with physical dimensions
    //    Package sizes from physical selection: "0402" → 1.0×0.5mm
    let mut components = build_components(netlist, simulation);

    // 3. Apply placement constraints from board config + instance attributes
    apply_placement_constraints(&mut components, board_config, netlist);

    // 4. Resolve board outline
    let outline = resolve_board_outline(&board_config.outline, &components, board_config.edge_clearance_mm);

    // 5. Convert netlist nets → PnrNets with semantic weights
    let mut nets = build_nets(netlist, analysis, simulation);

    // 6. Build functional groups from expansion blocks
    let groups = build_groups(netlist, analysis);

    // 7. Assign layer constraints from intents (using resolved layer_stack)
    assign_layer_constraints(&mut nets, analysis, &layer_stack);

    // 8. Compute trace widths from GLACIER currents + copper weight
    assign_trace_widths(&mut nets, simulation, &layer_stack);

    Board { config: board_config.clone(), layer_stack, components, nets, .. }
}

/// Apply placement constraints from board config and instance attributes
fn apply_placement_constraints(
    components: &mut [Component],
    config: &BoardConfig,
    netlist: &Netlist,
) {
    // Method 1: From BoardConfig.fixed_placements (parsed from `physical { }` block)
    for fp in &config.fixed_placements {
        if let Some(comp) = components.iter_mut().find(|c| c.name == fp.instance_name) {
            comp.placement = PlacementConstraint::Fixed {
                x: fp.x_mm,
                y: fp.y_mm,
                theta: fp.rotation_deg.to_radians(),
            };
            comp.x = fp.x_mm;
            comp.y = fp.y_mm;
            comp.theta = fp.rotation_deg.to_radians();
            comp.side = fp.side;
        }
    }

    // Method 2: From instance attributes (fallback when physical { } not parsed)
    for comp in components.iter_mut() {
        if matches!(comp.placement, PlacementConstraint::Free) {
            if let Some(attrs) = get_instance_attributes(netlist, &comp.name) {
                if attrs.get("fixed").map(|v| v == "true").unwrap_or(false) {
                    let x = attrs.get("x").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    let y = attrs.get("y").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    let rot = attrs.get("rotation").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    let side = match attrs.get("side").map(|s| s.as_str()) {
                        Some("bottom") => BoardSide::Bottom,
                        _ => BoardSide::Top,
                    };

                    comp.placement = PlacementConstraint::Fixed {
                        x, y, theta: rot.to_radians(),
                    };
                    comp.x = x;
                    comp.y = y;
                    comp.theta = rot.to_radians();
                    comp.side = side;
                }

                // Edge constraint: `attribute conn.edge = "left"`
                if let Some(edge_str) = attrs.get("edge") {
                    if matches!(comp.placement, PlacementConstraint::Free) {
                        let edge = match edge_str.as_str() {
                            "left" => BoardEdge::Left,
                            "right" => BoardEdge::Right,
                            "top" => BoardEdge::Top,
                            "bottom" => BoardEdge::Bottom,
                            _ => continue,
                        };
                        let offset = attrs.get("offset").and_then(|v| v.parse().ok());
                        comp.placement = PlacementConstraint::Edge { edge, offset };
                    }
                }
            }
        }
    }

    // Method 3: Region preferences from board config
    for region in &config.placement_regions {
        for inst_name in &region.preferred_instances {
            if let Some(comp) = components.iter_mut().find(|c| c.name == *inst_name) {
                if matches!(comp.placement, PlacementConstraint::Free) {
                    comp.placement = PlacementConstraint::PreferRegion {
                        region_name: region.name.clone(),
                    };
                }
            }
        }
    }
}

/// Resolve board outline — AutoSize computes from components
fn resolve_board_outline(
    outline: &BoardOutline,
    components: &[Component],
    edge_clearance: f64,
) -> BoardOutline {
    match outline {
        BoardOutline::AutoSize => {
            // Only count free components for area estimation;
            // fixed edge components contribute to minimum dimensions instead
            let free_area: f64 = components.iter()
                .filter(|c| matches!(c.placement, PlacementConstraint::Free | PlacementConstraint::PreferRegion { .. }))
                .map(|c| c.width_mm * c.height_mm)
                .sum();

            // Fixed components define minimum board extent
            let fixed_extent_x = components.iter()
                .filter(|c| !matches!(c.placement, PlacementConstraint::Free))
                .map(|c| c.x + c.width_mm / 2.0)
                .fold(0.0_f64, f64::max);
            let fixed_extent_y = components.iter()
                .filter(|c| !matches!(c.placement, PlacementConstraint::Free))
                .map(|c| c.y + c.height_mm / 2.0)
                .fold(0.0_f64, f64::max);

            // Target ~40% utilization for free components
            let auto_side = (free_area / 0.4).sqrt() + 2.0 * edge_clearance;

            // Board must be at least large enough to contain fixed components
            let width = auto_side.max(fixed_extent_x + edge_clearance);
            let height = auto_side.max(fixed_extent_y + edge_clearance);

            BoardOutline::Rectangle { width_mm: width, height_mm: height }
        }
        other => other.clone(),
    }
}
```

**Net weight assignment from intents:**

```rust
fn compute_net_weight(net: &Net, flow_tracker: &FlowTracker) -> f64 {
    let base = 1.0;

    // High-speed intents get higher weight (shorter traces)
    if has_intent(net, "fast_response") { return base * 3.0; }
    if has_intent(net, "precision_measurement") { return base * 2.5; }

    // Protection paths: keep short
    if has_intent(net, "input_protection") { return base * 2.0; }

    // Power: moderate weight (width matters more than length)
    if net.net_class.is_power() { return base * 0.8; }

    // Ground: low weight (plane, not routed)
    if net.net_class.is_ground() { return base * 0.1; }

    base
}
```

**Trace width from GLACIER current (IPC-2221):**

```rust
fn trace_width_for_current(current_a: f64, copper_oz: f64, temp_rise_c: f64) -> f64 {
    // IPC-2221 internal layer formula
    // A = I / (k * ΔT^b)^(1/c)  where k=0.024, b=0.44, c=0.725
    let area_mils2 = (current_a / (0.024 * temp_rise_c.powf(0.44))).powf(1.0 / 0.725);
    let thickness_mils = copper_oz * 1.378;  // 1oz = 1.378 mils
    let width_mils = area_mils2 / thickness_mils;
    width_mils * 0.0254  // convert to mm
}
```

**Layer constraint from intent:**

```rust
fn layer_constraint_for_intent(intent: &str) -> LayerConstraint {
    match intent {
        "precision_measurement" => LayerConstraint::AdjacentToGround,
        "fast_response" => LayerConstraint::AdjacentToGround,
        "input_protection" => LayerConstraint::Required(0), // Top layer, short
        _ => LayerConstraint::Any,
    }
}
```

### 3. Placement Engine

#### 3.1 Objective Function

```
L(x, y, θ) = W_wl · WL(x, y, θ)           // Weighted wirelength
            + λ_D  · D(x, y, θ)            // Density (overlap prevention)
            + λ_C  · C(x, y, θ)            // Congestion inflation (from routing)
            + λ_V  · V(x, y)               // Via penalty (from routing)
            + λ_G  · G(x, y)               // Group cohesion (expansion blocks)
            + λ_T  · T(x, y)               // Thermal spreading
```

vs Cypress: `L = WL + λ_D·D + λ_NC·NC`

Key differences:
- **No net crossing term** — replaced by actual routing congestion feedback (C)
- **Via penalty** (V) — from 3D routing, not estimated
- **Group cohesion** (G) — keeps expansion block children near parent
- **Thermal spreading** (T) — from GLACIER power dissipation data
- **Continuous rotation** (θ) — no Gumbel-Softmax discretization

#### 3.2 Wirelength (WL)

Log-sum-exp smooth approximation of HPWL (same as Cypress/DREAMPlace):

```
WL_net = γ · log(Σ_k exp(x_k/γ)) + γ · log(Σ_k exp(-x_k/γ))
       + γ · log(Σ_k exp(y_k/γ)) + γ · log(Σ_k exp(-y_k/γ))
```

Pin positions depend on rotation (continuous, not discrete):

```
x_k = x_c + dx_k · cos(θ_c) - dy_k · sin(θ_c)
y_k = y_c + dx_k · sin(θ_c) + dy_k · cos(θ_c)
```

Gradients ∂WL/∂x, ∂WL/∂y, ∂WL/∂θ computed analytically via chain rule.

**Per-net weighting**: Unlike Cypress (uniform weights), multiply each net's WL contribution by its semantic weight:

```
WL_total = Σ_net  w_net · WL_net
```

where `w_net` comes from intent classification (Section 2).

#### 3.3 Density (D)

Electrostatic model from ePlace/DREAMPlace:
- Components mapped to charge density on 2D grid
- Poisson's equation solved via 2D FFT
- Gradient = electric field pushes components apart
- **Per-side density**: separate maps for top and bottom component layers (like Cypress)

Component footprint on density grid accounts for rotation:

```rust
fn rotated_bbox(comp: &Component) -> (f64, f64) {
    let w = comp.width_mm;
    let h = comp.height_mm;
    let cos_t = comp.theta.cos().abs();
    let sin_t = comp.theta.sin().abs();
    (w * cos_t + h * sin_t, w * sin_t + h * cos_t)
}
```

**Obstacle injection**: Keepout zones, mounting holes, and board boundary violations are injected as fixed density on the grid. This creates repulsive forces that push movable components away from forbidden areas without special-casing in the optimizer:

```rust
fn inject_obstacles_to_density(
    density_grid: &mut Vec<Vec<f64>>,
    config: &BoardConfig,
    x_coords: &[f64], y_coords: &[f64],
) {
    let high_density = 10.0;  // Much higher than any component — guarantees repulsion

    // Mounting holes → circular obstacles on density grid
    for hole in &config.mounting_holes {
        let radius = hole.drill_mm / 2.0 + hole.keepout_mm;
        fill_circle(density_grid, hole.x_mm, hole.y_mm, radius, high_density, x_coords, y_coords);
    }

    // Keepout zones (component-blocking ones)
    for zone in &config.keepout_zones {
        if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::ComponentsOnly) {
            fill_shape(density_grid, &zone.shape, high_density, x_coords, y_coords);
        }
    }

    // Board boundary — cells outside board outline get high density
    for (r, row) in density_grid.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let cx = (x_coords[c] + x_coords[c + 1]) / 2.0;
            let cy = (y_coords[r] + y_coords[r + 1]) / 2.0;
            if !config.outline.contains(cx, cy) {
                *cell = high_density;
            }
        }
    }
}
```

**Fixed components** also contribute density (they physically occupy space) but their density contribution is constant — never updated by the optimizer.

#### 3.4 Continuous Rotation

Unlike Cypress's Gumbel-Softmax over {0°, 90°, 180°, 270°}, rotation θ is a continuous variable optimized with the same gradient descent:

```
∂L/∂θ_c = Σ_net (∂WL_net/∂θ_c) + λ_D · (∂D/∂θ_c)
```

The wirelength gradient through rotation:

```
∂x_k/∂θ_c = -dx_k · sin(θ_c) - dy_k · cos(θ_c)
∂y_k/∂θ_c =  dx_k · cos(θ_c) - dy_k · sin(θ_c)
```

**Post-optimization snapping**: After convergence, optionally snap θ to nearest 45° (manufacturing preference). Compare objective before/after snap; keep whichever is better.

#### 3.5 Group Cohesion (G)

Expansion blocks should keep children near the parent IC:

```
G = Σ_group Σ_{c ∈ group} ‖pos(c) - centroid(group)‖²
```

This is a simple quadratic penalty that pulls group members toward their center of mass. The centroid moves with the group, so this doesn't anchor components to fixed positions.

**Weight schedule**: Start with high λ_G (keep groups tight), reduce as placement matures (allow spreading if routing demands it).

#### 3.6 Thermal Spreading (T)

From GLACIER power dissipation:

```
T = Σ_{i,j: P_i > threshold, P_j > threshold}  P_i · P_j / ‖pos_i - pos_j‖²
```

High-power components repel each other. Only active for components with P > 100mW (no point repelling passives). Weight λ_T is low — thermal is a soft constraint, not a hard one.

#### 3.7 Region Preference Force (R)

Soft force pulling components toward their preferred placement region:

```
R = Σ_{c with PreferRegion} w_region · ‖pos(c) - centroid(region)‖²
```

Only active if `pos(c)` is outside the region shape. Weight `w_region` comes from `PlacementRegion.weight` (default 1.0). Inside the region, force is zero — no penalty for any position within bounds.

#### 3.8 Fixed Component Handling

Fixed components participate in **loss computation** but are **excluded from gradient updates**:

```rust
fn update_positions(board: &mut Board, forces: &Forces, config: &OptimizerConfig) {
    for (i, comp) in board.components.iter_mut().enumerate() {
        match &comp.placement {
            PlacementConstraint::Fixed { .. } => {
                // Position/rotation frozen — skip gradient update
                // Component still contributes to WL, density, and routing
                continue;
            }
            PlacementConstraint::FixedPosition { x, y } => {
                // Position frozen, rotation can update
                comp.theta += config.rotation_lr * forces.d_theta[i];
                // x, y unchanged
            }
            PlacementConstraint::Edge { edge, offset } => {
                // Constrained to edge — project gradient onto edge axis
                match edge {
                    BoardEdge::Left => {
                        comp.x = 0.0 + board.config.edge_clearance_mm;
                        comp.y += config.position_lr * forces.dy[i];
                        clamp_y(comp, board);
                    }
                    BoardEdge::Right => {
                        comp.x = board.width() - board.config.edge_clearance_mm;
                        comp.y += config.position_lr * forces.dy[i];
                        clamp_y(comp, board);
                    }
                    BoardEdge::Top => {
                        comp.y = board.height() - board.config.edge_clearance_mm;
                        comp.x += config.position_lr * forces.dx[i];
                        clamp_x(comp, board);
                    }
                    BoardEdge::Bottom => {
                        comp.y = 0.0 + board.config.edge_clearance_mm;
                        comp.x += config.position_lr * forces.dx[i];
                        clamp_x(comp, board);
                    }
                }
                comp.theta += config.rotation_lr * forces.d_theta[i];
            }
            PlacementConstraint::Free | PlacementConstraint::PreferRegion { .. } => {
                // Standard Adam update
                comp.x += config.position_lr * forces.dx[i];
                comp.y += config.position_lr * forces.dy[i];
                comp.theta += config.rotation_lr * forces.d_theta[i];
                clamp_to_board(comp, board);
            }
        }
    }
}
```

**Key invariant**: Fixed components never move, but their pins still exert wirelength forces on connected free components. This naturally pulls free components toward fixed connectors/jacks.

**Initialization**: Fixed components start at their constrained position. Free components are initialized to center of board or center of their preferred region.

#### 3.9 Optimizer

Adam optimizer (same as Cypress) with separate learning rates:
- Position (x, y): lr = 1e-3, standard Adam
- Rotation (θ): lr = 1e-4, more conservative (rotation changes are high-impact)

No bilevel optimization needed — θ is updated every iteration alongside position, since it's continuous (no discrete acceptance criterion).

### 4. 3D Routing Grid

#### 4.1 Grid Construction

Build a coarse orthogonal 3D grid from component boundaries:

```rust
pub struct RoutingGrid {
    pub cells: Vec<Vec<Vec<GridCell>>>,  // [layer][row][col]
    pub x_coords: Vec<f64>,             // Column boundaries (mm)
    pub y_coords: Vec<f64>,             // Row boundaries (mm)
    pub num_layers: usize,
    pub via_cost: f64,                  // Cost of layer change
}

pub struct GridCell {
    pub capacity: usize,          // Max routes through this cell
    pub demand: usize,            // Current routes through this cell
    pub history: f64,             // Accumulated congestion history (PathFinder)
    pub present: f64,             // Current iteration congestion penalty
    pub blocked: bool,            // Component occupies this cell on this layer
}
```

**Grid resolution**: Component-pitch cells. For a board with ~50 components at ~5mm pitch, the grid is roughly 30×30×N_layers = 3,600-7,200 cells. Tiny.

**Grid construction algorithm:**

```rust
fn build_grid(board: &Board) -> RoutingGrid {
    // 1. Collect all component edges (left, right, top, bottom)
    let mut x_cuts: Vec<f64> = vec![0.0, board.width_mm];
    let mut y_cuts: Vec<f64> = vec![0.0, board.height_mm];

    for comp in &board.components {
        let (bw, bh) = rotated_bbox(comp);
        x_cuts.push(comp.x - bw/2.0);
        x_cuts.push(comp.x + bw/2.0);
        y_cuts.push(comp.y - bh/2.0);
        y_cuts.push(comp.y + bh/2.0);
    }

    // 2. Sort and deduplicate (merge cuts within 0.1mm)
    x_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    x_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.1);
    // same for y_cuts

    // 3. Create 3D grid — Z dimension = board.layer_stack.layers.len()
    let cols = x_cuts.len() - 1;
    let rows = y_cuts.len() - 1;
    let num_layers = board.layer_stack.layers.len();

    let mut cells = vec![vec![vec![GridCell::default(); cols]; rows]; num_layers];

    // 4. Mark blocked cells (component footprints on their placement layer)
    for comp in &board.components {
        let layer_idx = match comp.side {
            BoardSide::Top => 0,                       // First layer
            BoardSide::Bottom => num_layers - 1,       // Last layer
        };
        // Block cells covered by component on its layer
        mark_blocked(&mut cells[layer_idx], comp, &x_cuts, &y_cuts);
    }

    // 5. Set per-layer capacity from layer_stack (stackup-driven)
    //    Ground/Power planes: capacity = 0 (no routing, used as reference)
    //    Signal layers: capacity scaled by capacity_factor from stackup preset
    for (l, layer) in board.layer_stack.layers.iter().enumerate() {
        let base_capacity = match layer.kind {
            LayerKind::Ground | LayerKind::Power => 0,  // Plane — no routing
            LayerKind::Signal => 4,                     // 4 tracks per cell (tunable)
            LayerKind::Mixed => 2,                      // Reduced capacity
        };
        for row in &mut cells[l] {
            for cell in row.iter_mut() {
                if !cell.blocked {
                    cell.capacity = (base_capacity as f64 * layer.capacity_factor) as usize;
                }
            }
        }
    }

    // 6. Block cells for mounting holes (all layers — through-hole)
    for hole in &board.config.mounting_holes {
        let keepout_radius = hole.drill_mm / 2.0 + hole.keepout_mm;
        for l in 0..num_layers {
            mark_circle_blocked(&mut cells[l], hole.x_mm, hole.y_mm,
                                keepout_radius, &x_cuts, &y_cuts);
        }
    }

    // 7. Block cells for keepout zones
    for zone in &board.config.keepout_zones {
        match zone.applies_to {
            KeepoutTarget::All | KeepoutTarget::RoutingOnly => {
                // Block routing on all layers
                for l in 0..num_layers {
                    mark_shape_blocked(&mut cells[l], &zone.shape, &x_cuts, &y_cuts);
                }
            }
            KeepoutTarget::ComponentsOnly => {
                // Components can't be placed here, but traces can route through
                // (handled in placement — density set to infinity in zone)
            }
        }
    }

    // 8. Via cost from stackup: larger vias consume more area on every layer
    let via_cost = 2.0 + board.layer_stack.via_blockage_mm2() * 10.0;

    // 9. Apply net-specific capacity reservations from intent
    //    (e.g., high-speed nets reserve capacity on adjacent-to-ground layer)

    RoutingGrid { cells, x_coords: x_cuts, y_coords: y_cuts, num_layers: layers, .. }
}
```

#### 4.2 PathFinder Router

Classic negotiated congestion routing adapted for 3D PCB grid:

```rust
pub fn pathfinder_route(
    grid: &mut RoutingGrid,
    nets: &[PnrNet],
    max_iterations: usize,
    history_factor: f64,       // h_fac: how fast history accumulates (typ: 0.5-2.0)
    present_factor: f64,       // p_fac: how much current congestion costs (typ: 1.0)
) -> Vec<Route> {
    let mut routes: Vec<Route> = vec![Route::empty(); nets.len()];

    for iteration in 0..max_iterations {
        // Reset present congestion
        grid.reset_demand();

        // Route each net (order by priority: high-weight first)
        let net_order = priority_sorted_indices(nets);

        for &net_idx in &net_order {
            let net = &nets[net_idx];

            // Rip up previous route
            if !routes[net_idx].is_empty() {
                grid.remove_route(&routes[net_idx]);
            }

            // Find shortest path with congestion-aware cost
            let route = shortest_path_3d(
                grid, net,
                |cell| {
                    let base = 1.0;
                    let history = cell.history * history_factor;
                    let present = if cell.demand >= cell.capacity {
                        present_factor * (cell.demand - cell.capacity + 1) as f64
                    } else {
                        0.0
                    };
                    base + history + present
                },
            );

            // Add route to grid
            grid.add_route(&route);
            routes[net_idx] = route;
        }

        // Update history for overused cells
        for cell in grid.all_cells_mut() {
            if cell.demand > cell.capacity {
                cell.history += (cell.demand - cell.capacity) as f64;
            }
        }

        // Check convergence: no overused cells
        if grid.max_overflow() == 0 {
            break;
        }
    }

    routes
}
```

**Shortest path in 3D grid** (Dijkstra with via cost):

```rust
fn shortest_path_3d(
    grid: &RoutingGrid,
    net: &PnrNet,
    cost_fn: impl Fn(&GridCell) -> f64,
) -> Route {
    // Multi-sink Dijkstra (Steiner tree approximation)
    // Start from source pin's grid cell
    // Expand to neighbors: 4 cardinal directions + 2 vertical (layer change = via)

    let source = pin_to_grid_cell(net.pins[0], grid);
    let sinks: HashSet<_> = net.pins[1..].iter()
        .map(|p| pin_to_grid_cell(*p, grid))
        .collect();

    let mut heap = BinaryHeap::new();
    let mut dist = HashMap::new();
    let mut prev = HashMap::new();

    heap.push(State { cost: 0.0, cell: source });
    dist.insert(source, 0.0);

    let mut reached_sinks = Vec::new();

    while let Some(State { cost, cell }) = heap.pop() {
        if sinks.contains(&cell) {
            reached_sinks.push(cell);
            if reached_sinks.len() == sinks.len() { break; }
            // Add reached sink to source set (Steiner approximation)
        }

        // Expand: 4 planar neighbors
        for neighbor in grid.planar_neighbors(cell) {
            let edge_cost = cost_fn(&grid.get(neighbor));
            // Wider traces need more capacity — scale cost by trace width
            let width_factor = net.required_trace_width_mm / 0.2;  // normalized to default
            let new_cost = cost + edge_cost * width_factor;

            if new_cost < *dist.get(&neighbor).unwrap_or(&f64::INFINITY) {
                dist.insert(neighbor, new_cost);
                prev.insert(neighbor, cell);
                heap.push(State { cost: new_cost, cell: neighbor });
            }
        }

        // Expand: 2 vertical neighbors (via)
        for layer_neighbor in grid.vertical_neighbors(cell) {
            let via_cost = grid.via_cost;
            // Layer constraint check
            if !net.layer_constraint.allows(layer_neighbor.layer) {
                continue;
            }
            let new_cost = cost + via_cost;
            if new_cost < *dist.get(&layer_neighbor).unwrap_or(&f64::INFINITY) {
                dist.insert(layer_neighbor, new_cost);
                prev.insert(layer_neighbor, cell);
                heap.push(State { cost: new_cost, cell: layer_neighbor });
            }
        }
    }

    // Backtrace to build route
    reconstruct_route(&prev, source, &reached_sinks)
}
```

### 5. Routing Feedback Loop

The core innovation: routing results feed back into placement.

#### 5.1 Tiered Routing Schedule

```
Iteration   1-100:   Placement only (WL + density + group cohesion)
                     Cheap. Establish rough placement.

Every 50 iters:      Coarse PathFinder (max 5 negotiation iterations)
                     Build congestion map
                     Estimate via count
                     Update λ_C and λ_V

Iteration 100-400:   Placement + congestion inflation + via penalty
                     Components drift from congested channels

Every 25 iters:      PathFinder (max 10 negotiation iterations)
                     Update congestion map (finer)

Iteration 400-600:   Full forces, decreasing λ_G (loosen groups)
                     Allow spreading if routing demands

Final (600+):        Placement converged
                     Full PathFinder for actual routing (max 50 iterations)
```

#### 5.2 Congestion → Density Inflation

After each PathFinder run, build a congestion map and inflate component sizes in congested regions:

```rust
fn apply_congestion_inflation(
    components: &mut [Component],
    grid: &RoutingGrid,
    inflation_factor: f64,       // α: how aggressively to inflate (typ: 0.3)
) {
    for comp in components.iter_mut() {
        // Average congestion around this component
        let cells = grid.cells_near(comp.x, comp.y, comp.side);
        let avg_overflow: f64 = cells.iter()
            .map(|c| (c.demand as f64 / c.capacity.max(1) as f64 - 1.0).max(0.0))
            .sum::<f64>() / cells.len() as f64;

        // Inflate effective size for density calculation
        comp.density_inflation = 1.0 + inflation_factor * avg_overflow;
        // density term will use: width * density_inflation, height * density_inflation
    }
}
```

This is how VLSI tools (RIPPLE, DREAMPlace 4.0) feed routing back — indirectly through the density term. The beauty is that it's smooth: gradual inflation → gradual spreading. No discrete force extraction needed.

#### 5.3 Via Count → Placement Penalty

```rust
fn compute_via_penalty(
    components: &[Component],
    routes: &[Route],
    nets: &[PnrNet],
) -> Vec<(f64, f64)> {  // (∂V/∂x, ∂V/∂y) per component
    let mut grad = vec![(0.0, 0.0); components.len()];

    for (net_idx, route) in routes.iter().enumerate() {
        let via_count = route.via_count();
        if via_count == 0 { continue; }

        let net = &nets[net_idx];
        // Distribute via penalty to connected components
        // Gradient: push connected components toward same side/position
        for &(comp_id, _pin_id) in &net.pins {
            let comp = &components[comp_id.0];
            let net_centroid = net_centroid(net, components);

            // Force toward centroid (reduces spread → fewer vias)
            let fx = (net_centroid.0 - comp.x) * via_count as f64;
            let fy = (net_centroid.1 - comp.y) * via_count as f64;

            grad[comp_id.0].0 += fx;
            grad[comp_id.0].1 += fy;
        }
    }

    grad
}
```

### 6. Convergence Monitor

```rust
pub struct ConvergenceMonitor {
    wl_history: VecDeque<f64>,        // Last N wirelength values
    congestion_history: VecDeque<f64>, // Last N max-overflow values
    via_history: VecDeque<usize>,      // Last N total via counts
    best_state: Option<PlacementState>,
    best_cost: f64,
    window_size: usize,               // Sliding window (typ: 50)
}

impl ConvergenceMonitor {
    pub fn check(&mut self, state: &PlacementState) -> ConvergenceAction {
        let wl = state.wirelength;
        let overflow = state.max_overflow;
        let vias = state.total_vias;

        self.wl_history.push_back(wl);
        // ...

        // Track best
        let cost = wl + 1000.0 * overflow as f64 + 10.0 * vias as f64;
        if cost < self.best_cost {
            self.best_cost = cost;
            self.best_state = Some(state.clone());
        }

        // Divergence detection: WL increasing over window
        if self.wl_history.len() >= self.window_size {
            let recent_avg = self.wl_history.iter().rev().take(10).sum::<f64>() / 10.0;
            let earlier_avg = self.wl_history.iter().rev().skip(20).take(10).sum::<f64>() / 10.0;
            if recent_avg > earlier_avg * 1.1 {
                return ConvergenceAction::Rollback;
            }
        }

        // Convergence: WL stable and no overflow
        if overflow == 0 && self.wl_stable() {
            return ConvergenceAction::Converged;
        }

        ConvergenceAction::Continue
    }
}
```

### 7. Legalization

After global placement converges:

```rust
pub fn legalize(board: &mut Board) {
    // 0. Verify fixed components haven't drifted (defensive — should be invariant)
    for comp in &board.components {
        if let PlacementConstraint::Fixed { x, y, theta } = &comp.placement {
            assert!((comp.x - x).abs() < 1e-6, "Fixed component {} moved!", comp.name);
            assert!((comp.y - y).abs() < 1e-6, "Fixed component {} moved!", comp.name);
        }
    }

    // 1. Snap to placement grid (only free/edge components)
    for comp in &mut board.components {
        if matches!(comp.placement, PlacementConstraint::Fixed { .. }) {
            continue;  // Fixed components already at exact position
        }
        comp.x = (comp.x / comp.snap_grid).round() * comp.snap_grid;
        comp.y = (comp.y / comp.snap_grid).round() * comp.snap_grid;
    }

    // 2. Snap rotation (optional: nearest 45° or keep continuous)
    for comp in &mut board.components {
        if matches!(comp.placement, PlacementConstraint::Fixed { .. }) {
            continue;  // Fixed rotation is sacred
        }
        if comp.snap_rotation {
            let deg = comp.theta.to_degrees();
            let snapped = (deg / 45.0).round() * 45.0;
            comp.theta = snapped.to_radians();
        }
    }

    // 3. Resolve remaining overlaps (greedy displacement)
    //    Fixed components are immovable obstacles — only free components move
    resolve_overlaps(&mut board.components);

    // 4. Enforce keepout zones — push any component out of keepout areas
    for comp in &mut board.components {
        if matches!(comp.placement, PlacementConstraint::Fixed { .. }) { continue; }
        for zone in &board.config.keepout_zones {
            if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::ComponentsOnly) {
                if zone.shape.contains(comp.x, comp.y) {
                    push_out_of_zone(comp, &zone.shape);
                }
            }
        }
    }

    // 5. Enforce mounting hole clearance
    for comp in &mut board.components {
        if matches!(comp.placement, PlacementConstraint::Fixed { .. }) { continue; }
        for hole in &board.config.mounting_holes {
            let clearance = hole.drill_mm / 2.0 + hole.keepout_mm;
            let dist = ((comp.x - hole.x_mm).powi(2) + (comp.y - hole.y_mm).powi(2)).sqrt();
            if dist < clearance + comp.width_mm.max(comp.height_mm) / 2.0 {
                push_away_from_point(comp, hole.x_mm, hole.y_mm, clearance);
            }
        }
    }

    // 6. Enforce board boundary
    for comp in &mut board.components {
        if matches!(comp.placement, PlacementConstraint::Fixed { .. }) { continue; }
        clamp_to_board(comp, &board.config.outline, board.config.edge_clearance_mm);
    }

    // 7. DRC check
    let violations = check_drc(board);
    // Report violations; attempt auto-fix for minor issues
}
```

### 8. Final Detailed Routing

After legalization, run PathFinder on a **fine** grid for production-quality routes:

```rust
pub fn detailed_route(board: &mut Board) -> Vec<PhysicalRoute> {
    // 1. Build fine grid (0.1mm resolution)
    let fine_grid = build_fine_grid(board, 0.1);

    // 2. Full PathFinder with many iterations (max 100)
    let routes = pathfinder_route(&mut fine_grid, &board.nets, 100, 1.0, 1.0);

    // 3. Convert grid routes to physical traces
    let physical_routes: Vec<PhysicalRoute> = routes.iter()
        .map(|r| grid_route_to_physical(r, &fine_grid))
        .collect();

    // 4. Trace width assignment (from semantic preprocessor)
    for (route, net) in physical_routes.iter_mut().zip(board.nets.iter()) {
        route.width_mm = net.required_trace_width_mm;
    }

    // 5. Via optimization: merge nearby vias, remove redundant layer changes
    optimize_vias(&mut physical_routes);

    physical_routes
}

pub struct PhysicalRoute {
    pub net_id: NetId,
    pub segments: Vec<TraceSegment>,
    pub vias: Vec<Via>,
    pub width_mm: f64,
}

pub struct TraceSegment {
    pub layer: usize,
    pub start: (f64, f64),      // mm
    pub end: (f64, f64),        // mm
}

pub struct Via {
    pub x: f64,
    pub y: f64,
    pub from_layer: usize,
    pub to_layer: usize,
    pub drill_mm: f64,          // typ: 0.3mm
    pub annular_ring_mm: f64,   // typ: 0.15mm
}
```

### 9. Output Formats

#### KiCad PCB Export

```rust
pub fn export_kicad_pcb(board: &Board, routes: &[PhysicalRoute]) -> String {
    // Generate .kicad_pcb s-expression format
    // Components → (footprint ...) with (at x y rotation)
    // Routes → (segment (start x y) (end x y) (width w) (layer F.Cu))
    // Vias → (via (at x y) (size s) (drill d) (layers F.Cu B.Cu))
}
```

#### HTML/Canvas Preview

Reuse existing BHDL schematic viewer architecture: generate JSON, embed in HTML with Canvas renderer.

## Integration with BHDL Pipeline

### Entry Point

```rust
// In bhdl-cli, new command:
// bhdl-cli circuit.bhdl layout [--layers 4] [--width 50] [--height 50]
// bhdl-cli circuit.bhdl layout                  # auto-infer layers + board size
// bhdl-cli circuit.bhdl layout --layers 6 --width 80 --height 60

pub struct PnrConfig {
    pub board: BoardConfig,
    pub placement: PlacementConfig,
    pub routing_schedule: RoutingSchedule,
    pub optimizer: OptimizerConfig,
    pub convergence: ConvergenceConfig,
    pub max_iterations: usize,
}

impl Default for PnrConfig {
    fn default() -> Self {
        PnrConfig {
            board: BoardConfig::default(),   // auto-size, auto-layers
            placement: PlacementConfig {
                position_lr: 1e-3,
                rotation_lr: 1e-4,
                lambda_density: 1.0,
                lambda_group: 5.0,
                lambda_thermal: 0.1,
            },
            routing_schedule: RoutingSchedule {
                first_route_iter: 100,
                coarse_interval: 50,
                fine_interval: 25,
                fine_start_iter: 400,
            },
            optimizer: OptimizerConfig::Adam { beta1: 0.9, beta2: 0.999 },
            convergence: ConvergenceConfig {
                window_size: 50,
                wl_tolerance: 0.01,
                max_rollbacks: 3,
            },
            max_iterations: 800,
        }
    }
}

pub fn place_and_route(
    netlist: &Netlist,
    analysis: &AnalysisResult,
    simulation: &SimulationAnnotations,
    config: PnrConfig,
) -> Result<PnrResult, PnrError> {
    // 1. Semantic preprocessing (resolves stackup, auto-sizes board, assigns weights)
    let board = semantic::preprocess(netlist, analysis, simulation, &config.board);

    // 2. Initial placement
    //    Fixed components → already at their constrained positions (from preprocess)
    //    Edge components → placed on their edge, free axis at midpoint
    //    PreferRegion components → placed at region centroid
    //    Free components → random scatter within board area (avoid keepouts)
    placement::initialize(&mut board, &config.placement);

    // 3. Iterative placement + routing loop
    let mut monitor = ConvergenceMonitor::new(config.convergence);
    let mut grid = routing::build_grid(&board);
    let mut routes = Vec::new();

    for iteration in 0..config.max_iterations {
        // Placement step
        let forces = placement::compute_forces(&board, &routes);
        placement::update_positions(&mut board, &forces, &config.optimizer);

        // Routing feedback (periodic)
        if should_route(iteration, &config.routing_schedule) {
            grid = routing::build_grid(&board);
            routes = routing::pathfinder_route(&mut grid, &board.nets, 10, 0.5, 1.0);
            feedback::apply_congestion_inflation(&mut board.components, &grid, 0.3);
        }

        // Convergence check
        match monitor.check(&board.snapshot()) {
            ConvergenceAction::Converged => break,
            ConvergenceAction::Rollback => board.restore(&monitor.best_state()),
            ConvergenceAction::Continue => {},
        }
    }

    // 4. Legalization
    legalization::legalize(&mut board);

    // 5. Detailed routing
    let final_routes = routing::detailed_route(&mut board);

    // 6. DRC
    let drc_report = legalization::check_drc(&board);

    Ok(PnrResult { board, routes: final_routes, drc: drc_report })
}
```

### Pipeline Position

```
Parse → Analyze → Synthesize → Expand → GLACIER → Physical Selection
    → [NEW] Place & Route → KiCad PCB / Gerber / 3D Preview
```

The P&R module sits at the end of the pipeline, consuming everything upstream.

## Implementation Plan

### Phase 1: Analytical Placement (No Routing Feedback)

**Goal**: Working placement engine with continuous rotation, density prevention, group cohesion.

**Steps**:
1. Create `bhdl-pnr` crate with types (`Board`, `Component`, `PnrNet`, `Layer`, `BoardConfig`, `PlacementConstraint`, etc.)
2. Implement `stackup.rs` — 4 standard presets + auto-inference from circuit complexity
3. Implement `semantic.rs` — netlist → Board conversion:
   - Package→dimensions mapping ("0402" → 1.0×0.5mm, "SOT-23" → 2.9×1.3mm, etc.)
   - Instance attribute extraction for fixed placements, edge constraints, regions
   - Board outline resolution (explicit dimensions, auto-size, or polygon)
   - Keepout zone and mounting hole incorporation
4. Implement constraint-aware initialization:
   - Fixed components placed at exact coordinates (immovable)
   - Edge components placed along their edge
   - Region components placed at region centroid
   - Free components randomly scattered (avoiding keepouts/mounting holes)
5. Implement LSE wirelength with rotation-aware pin positions
6. Implement FFT-based density (use `rustfft` crate)
   - Keepout zones injected as high-density areas on density grid (repels components)
   - Mounting holes as density obstacles
7. Implement group cohesion force (expansion block clustering)
8. Implement region preference force (soft pull toward preferred zone)
9. Implement Adam optimizer with position + rotation
   - Constraint-aware update: skip fixed, project edge, standard for free
10. Implement convergence monitor with rollback
11. Implement legalization with constraint-aware overlap resolution
12. Test on `complex_power_tree.bhdl` (~24 components, add barrel jack as fixed)

**Output**: Component positions and rotations. No routing yet. Layer stack resolved. Fixed components verified in place.

**Dependencies**: `rustfft`, `slotmap` (already in workspace)

### Phase 2: Coarse 3D Routing Grid + PathFinder

**Goal**: Working PathFinder router on coarse grid, standalone.

**Steps**:
1. Implement 3D grid construction from component boundaries
2. Implement PathFinder with negotiated congestion
3. Implement Dijkstra shortest path with via cost
4. Implement layer constraint checking
5. Test routing standalone (fixed placement from Phase 1)

**Output**: Coarse routes through 3D grid. Via count. Congestion map.

### Phase 3: Concurrent P&R Loop

**Goal**: Routing feedback drives placement.

**Steps**:
1. Implement congestion-to-inflation feedback
2. Implement via penalty force
3. Implement tiered routing schedule
4. Tune weight schedules (λ_C, λ_V progression)
5. Compare placement quality: with vs without routing feedback

### Phase 4: Semantic Integration

**Goal**: BHDL-specific advantages.

**Steps**:
1. Intent-driven net weights
2. GLACIER-driven trace widths
3. Intent-driven layer constraints
4. Thermal spreading from GLACIER power data
5. Test on circuits with intents and expansion blocks

### Phase 5: Output + Polish

**Goal**: Production-ready output.

**Steps**:
1. Detailed routing on fine grid
2. KiCad PCB export
3. HTML/Canvas visualization
4. DRC checking
5. CLI integration (`bhdl-cli layout`)

## Existing BHDL Infrastructure Status

### Ready to Consume

| System | Status | What P&R gets |
|---|---|---|
| Netlist (bhdl-netlist) | Production | SlotMap IDs, Instance/Net/Pin/ConnectionPoint |
| GLACIER DC simulation | Production | V, I, P per node → trace widths, thermal |
| Intent system (FlowTracker) | Production | Net weights, layer constraints, grouping |
| Power domains (NetAttribute) | Production | Voltage/current per rail, stage chains |
| Physical selection | Production | Package sizes → component dimensions |
| Expansion blocks | Production | Functional groups → placement clusters |
| Symbol/layout definitions | Production | Pin positions from entity definitions |
| Test circuits | 100+ .bhdl files | `complex_power_tree.bhdl` = primary test case |

### Needs Implementation

| System | Status | What's needed |
|---|---|---|
| Layer stackup (parser) | Keyword registered, no parse rule | Wire up `parse_layer_stackup()` (Phase 2+) |
| Layer stackup (presets) | Not yet | `stackup.rs` with 4 presets + auto-inference |
| Board dimensions | Not in BHDL syntax | CLI flags `--width`/`--height` or auto-size |
| Package → footprint mm | Partial | Mapping table: "0402" → 1.0×0.5mm |
| Pin positions (physical) | Not yet | Need actual pad locations per package |
| KiCad PCB export | Not yet | `.kicad_pcb` s-expression writer |

### Parser Stackup Plumbing (Already Exists)

The BHDL parser has syntax infrastructure for `layer_stackup` that was defined but never fully wired:
- `LAYER_STACKUP_KW` keyword in lexer (`bhdl-parser/src/lexer.rs:49`)
- `LAYER_STACKUP_BLOCK` syntax kind (`bhdl-parser/src/syntax.rs:231`)
- `LAYER_DEF` syntax kind (`bhdl-parser/src/syntax.rs:232`)
- `LayerStackupBlock` AST node (`bhdl-ast/src/blocks.rs:12-31`)
- `LayerDef` AST node (`bhdl-ast/src/blocks.rs:33-47`)
- `Board.layer_stackup_block()` accessor (`bhdl-ast/src/items.rs:55`)

This means parsing BHDL `layer_stackup { }` blocks requires only writing the parser grammar rule — the AST nodes and accessors are ready.

## Key Design Decisions

### Why Not GPU?

For PCB-scale problems (10-1000 components, 50-500 nets), CPU is sufficient:
- FFT density: `rustfft` on 64×64 grid = microseconds
- PathFinder on 3,600-cell grid = milliseconds
- Adam update for 1000 components = microseconds
- Total per iteration: <1ms → 1000 iterations in <1 second

GPU acceleration (Cypress approach) only makes sense for >5000 components. BHDL targets boards where semantic awareness matters more than raw throughput.

### Why Not Cypress's Net Crossing?

Net crossing is a *proxy* for routability. We have *actual routability* from PathFinder. The proxy is unnecessary and in fact misleading on multi-layer boards (penalizes crossings that route on different layers).

### Why Continuous Rotation?

PCB components can physically be placed at any angle. Cypress's 4-angle discretization is a VLSI artifact (standard cell libraries only exist in 4 orientations). For PCB, continuous θ with analytical gradients is strictly superior.

Post-placement, we offer optional snapping to 45° intervals (manufacturing convenience, not mathematical necessity).

### Why Coarse Grid (Not Fine)?

Fine routing during placement iteration is computationally prohibitive. Coarse grid captures:
- Channel congestion (which corridors are overloaded)
- Via necessity (which nets need layer changes)
- Unroutability (PathFinder fails to converge)

It does NOT capture:
- Exact trace geometry (handled in final detailed routing)
- DRC violations at trace level (handled in legalization)

This is the right tradeoff: routing feedback at placement speed.

## Metrics and Evaluation

### Primary Metrics
1. **HPWL** (half-perimeter wirelength): Lower = shorter traces
2. **Routability**: % of nets successfully routed
3. **Via count**: Fewer = cheaper, better SI
4. **Total trace length**: After detailed routing (post-legalization)
5. **DRC violations**: Must be zero for production

### Comparison Baselines
1. **Random placement** + PathFinder routing
2. **BHDL schematic placer** (existing topological sort) + PathFinder
3. **Cypress** (if GPU available) + sequential PathFinder
4. **KiCad autoplacer** (if available)

### Test Circuits
1. `complex_power_tree.bhdl` — 24 components, 4 power domains, expansion blocks
2. Synthetic stress tests — 100, 500, 1000 components
3. Real-world boards — USB hub, sensor board, motor controller (to be created)

## References

1. McMurchie & Ebeling, "PathFinder: Negotiation-Based Performance-Driven FPGA Routing," FPGA 1995
2. Zhang et al., "Cypress: VLSI-Inspired PCB Placement with GPU Acceleration," ISPD 2025
3. Lin et al., "DREAMPlace: Deep Learning Toolkit-Enabled GPU Acceleration for Modern VLSI Placement," DAC 2019
4. Cheng & Kuh, "Module Placement Based on Resistive Network Optimization," IEEE TCAD 1984 (original force-directed)
5. Lu et al., "ePlace: Electrostatics-Based Placement Using Fast Fourier Transform," ACM TODAES 2015
6. Hsu et al., "NTUplace4: Routability-Driven Placement for Mixed-Size Designs," ICCAD 2012
7. IPC-2221B, "Generic Standard on Printed Board Design," 2012 (trace width formulas)
