//! Core types for the PCB place & route engine.
//!
//! These mirror the design in `docs/proposals/Concurrent_PCB_Place_And_Route.md`.

use serde::{Deserialize, Serialize};
use slotmap::new_key_type;

// ── Slot map keys ──────────────────────────────────────────────────────

new_key_type! {
    pub struct ComponentId;
    pub struct NetId;
    pub struct PinId;
    pub struct GroupId;
}

// ── Board ──────────────────────────────────────────────────────────────

/// Top-level board with resolved layer stack, components, and nets.
#[derive(Clone)]
pub struct Board {
    pub config: BoardConfig,
    pub layer_stack: LayerStack,
    pub components: Vec<Component>,
    pub nets: Vec<PnrNet>,
    pub groups: Vec<FunctionalGroup>,
    /// Placement recipes from stdlib (vendor datasheet layout recommendations).
    /// The *rigid* placement form: absolute (dx, dy, rotation) offsets copied
    /// from datasheet reference layouts. When present, honored verbatim.
    // BTreeMap: find_recipe_for_block returns the FIRST matching recipe
    // while iterating — HashMap order let hash randomness pick which
    // recipe wins and made placement nondeterministic.
    pub placement_recipes: std::collections::BTreeMap<String, bhdl_common::PlacementRecipe>,
    /// Layout constraints lowered from intent + interface constraints.
    /// The *flexible* placement/routing form: proximity, loop area, length
    /// match, etc. that the optimizer balances against other costs.
    /// Populated by the intent-lowering pass after semantic build.
    /// See `bhdl-pnr/docs/constraint_model_v0.md`.
    pub constraints: Vec<crate::constraint::Constraint>,
    /// DDR speed bin carried by an on-board SDRAM entity (bin label,
    /// tCK ns) — gives the delay/skew grading UI-relative CONTEXT
    /// (spread as % of the bin's unit interval). Report-only; the
    /// declared constraint tolerances keep gating.
    pub ddr_bin: Option<(String, f64)>,
}

// ── Board configuration ────────────────────────────────────────────────

/// Board configuration provided by user or inferred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardConfig {
    pub outline: BoardOutline,
    pub stackup: StackupSource,
    pub min_trace_width_mm: f64,
    /// P4 return-path routing cost (grid si_cost stamp) for THIS
    /// run — the knob-aware trial tier sets it per trial; the
    /// BHDL_PNR_SI_COST env still forces it globally.
    #[serde(default)]
    pub si_return_cost: bool,
    /// Fanout-first: pre-site IC plane-pad stub+vias on the EMPTY
    /// board before signal routing. Cures the late-drop failure
    /// class (a walled-in QFP ground pad) but perturbs every route —
    /// measured costing a previously-perfect seed a sink — so it
    /// runs only as a best-of FALLBACK tier, never by default.
    #[serde(default)]
    pub fanout_first: bool,
    /// Validator amputee choice by REPAIR COST (smaller span subtree
    /// loses) instead of net weight. Cures the whole-net-in-disguise
    /// amputation class (ecc83 strict: one bad corner cost K2 26 of
    /// 27 segments and recovery was walled) but the changed rip
    /// cascade measured 4 unc on other boards — a knife-edge knob
    /// that runs only as a best-of FALLBACK tier under dominance.
    #[serde(default)]
    pub cheap_amputation: bool,
    /// Escape-demand density scale: >0 makes IC pin rows project
    /// fanout-corridor demand into the placer's density map (s99's
    /// conserved congestion debt cleared at 2.0 — the row finally
    /// got its aisle at placement time). Global placement
    /// perturbation, so it runs only as a best-of fallback tier.
    #[serde(default)]
    pub escape_demand: f64,
    pub min_spacing_mm: f64,
    pub edge_clearance_mm: f64,
    pub fixed_placements: Vec<FixedPlacement>,
    pub mounting_holes: Vec<MountingHole>,
    pub keepout_zones: Vec<KeepoutZone>,
    /// Interior board apertures (display windows, shaft slots):
    /// (x0, y0, x1, y1) rects cut out of the board. Emitted as interior
    /// Edge.Cuts, blocked for routing (edge band) and placement, and
    /// punched from plane fills.
    pub cutouts: Vec<(f64, f64, f64, f64)>,
    /// Double-sided assembly declared (`assembly double_sided;`) —
    /// detailed placement may flip free SMD parts to the back.
    pub double_sided: bool,
    /// Laminate selection (`layer_stackup N material <name>;`).
    pub stackup_material: Option<String>,
    /// `route_bias bottom;` — preferred outer signal layer ("bottom" or
    /// "top"). Lateral routing moves on OTHER signal layers cost extra,
    /// so copper collects on the preferred side the way hand-routed THT
    /// boards keep it on the solder side. Vias stay legal — the bias is
    /// a preference, not a layer rule.
    #[serde(default)]
    pub route_bias: Option<String>,
    /// `pour <net>;` — declared pour intents (net names, declaration
    /// order). The engine picks layer + region organically.
    pub pours: Vec<String>,
    /// `route_bias bottom strict;` — signal copper is CONFINED to the
    /// preferred layer (per-net allowed_layers), the way the hand-
    /// routed single-sided demos work: the router detours around
    /// obstacles instead of jumping layers. SMD pad layers are always
    /// relaxed in (pads live where they live).
    #[serde(default)]
    pub route_bias_strict: bool,
    /// `track_width X;` — declared design-rule trace width (mm). This
    /// is a PER-NET width floor, NOT the routing quantum: the grid
    /// pitch stays min_trace + min_spacing, and wide tracks ride the
    /// existing clear-ring machinery. Folding the design width into
    /// min_trace_width_mm instead coarsened the ecc83 lattice to
    /// 1.435mm — few enough corridors that congestion never resolved
    /// (two nets shipped sharing a cell center; validator ripped both).
    #[serde(default)]
    pub design_track_width_mm: Option<f64>,
    pub placement_regions: Vec<PlacementRegion>,
    /// IPC-7351B courtyard excess (per side, mm) added to each
    /// component's pad/body extent to form its keepout boundary. Set from
    /// the density level at board build; consumed by overlap resolution
    /// and available to `KeepAway` clearance. See `footprint_spec_v0.md`
    /// §6.1.
    pub courtyard_excess_mm: f64,
}

impl Default for BoardConfig {
    fn default() -> Self {
        BoardConfig {
            outline: BoardOutline::AutoSize,
            stackup: StackupSource::Auto,
            min_trace_width_mm: 0.15,
            si_return_cost: false,
            fanout_first: false,
            cheap_amputation: false,
            escape_demand: 0.0,
            min_spacing_mm: 0.15,
            edge_clearance_mm: 0.5,
            fixed_placements: Vec::new(),
            mounting_holes: Vec::new(),
            keepout_zones: Vec::new(),
            cutouts: Vec::new(),
            double_sided: false,
            stackup_material: None,
            route_bias: None,
            pours: Vec::new(),
            route_bias_strict: false,
            design_track_width_mm: None,
            placement_regions: Vec::new(),
            courtyard_excess_mm: 0.25, // IPC nominal
        }
    }
}

impl Component {
    /// Courtyard rectangle extent (w, h) = pad/body extent + 2× the
    /// board's per-side courtyard excess. The manufacturable keepout the
    /// placer must respect between components.
    pub fn courtyard_extent(&self, excess_mm: f64) -> (f64, f64) {
        (
            self.width_mm + 2.0 * excess_mm,
            self.height_mm + 2.0 * excess_mm,
        )
    }
}

// ── Board outline ──────────────────────────────────────────────────────

/// Board outline — defines the physical boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardOutline {
    Rectangle { width_mm: f64, height_mm: f64 },
    Polygon(Vec<(f64, f64)>),
    AutoSize,
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
            BoardOutline::AutoSize => 0.0,
        }
    }

    pub fn height(&self) -> f64 {
        match self {
            BoardOutline::Rectangle { height_mm, .. } => *height_mm,
            BoardOutline::Polygon(pts) => {
                let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                let min_y = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                max_y - min_y
            }
            BoardOutline::AutoSize => 0.0,
        }
    }

    /// Point-in-polygon test (ray casting).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        match self {
            BoardOutline::Rectangle { width_mm, height_mm } => {
                x >= 0.0 && x <= *width_mm && y >= 0.0 && y <= *height_mm
            }
            BoardOutline::Polygon(pts) => {
                // Ray casting algorithm
                let n = pts.len();
                let mut inside = false;
                let mut j = n - 1;
                for i in 0..n {
                    let (xi, yi) = pts[i];
                    let (xj, yj) = pts[j];
                    if ((yi > y) != (yj > y))
                        && (x < (xj - xi) * (y - yi) / (yj - yi) + xi)
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
            BoardOutline::AutoSize => true,
        }
    }
}

// ── Fixed placements & constraints ─────────────────────────────────────

/// Component with mechanically fixed position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixedPlacement {
    pub instance_name: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub rotation_deg: f64,
    pub side: BoardSide,
    pub edge: Option<BoardEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardSide {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Mounting hole — blocks placement and routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountingHole {
    pub x_mm: f64,
    pub y_mm: f64,
    pub drill_mm: f64,
    pub keepout_mm: f64,
}

impl MountingHole {
    pub fn new(x_mm: f64, y_mm: f64, drill_mm: f64) -> Self {
        MountingHole {
            x_mm,
            y_mm,
            drill_mm,
            keepout_mm: 2.0,
        }
    }
}

/// Keep-out zone — no components or traces allowed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepoutZone {
    pub shape: ZoneShape,
    pub applies_to: KeepoutTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZoneShape {
    Rectangle { x: f64, y: f64, w: f64, h: f64 },
    Circle { x: f64, y: f64, r: f64 },
    Polygon(Vec<(f64, f64)>),
}

impl ZoneShape {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        match self {
            ZoneShape::Rectangle { x, y, w, h } => {
                px >= *x && px <= x + w && py >= *y && py <= y + h
            }
            ZoneShape::Circle { x, y, r } => {
                let dx = px - x;
                let dy = py - y;
                dx * dx + dy * dy <= r * r
            }
            ZoneShape::Polygon(pts) => {
                let n = pts.len();
                let mut inside = false;
                let mut j = n - 1;
                for i in 0..n {
                    let (xi, yi) = pts[i];
                    let (xj, yj) = pts[j];
                    if ((yi > py) != (yj > py))
                        && (px < (xj - xi) * (py - yi) / (yj - yi) + xi)
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeepoutTarget {
    All,
    ComponentsOnly,
    RoutingOnly,
}

/// Soft placement hint — prefer components in a named region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRegion {
    pub name: String,
    pub shape: ZoneShape,
    pub preferred_instances: Vec<String>,
    pub weight: f64,
}

// ── Layer stack ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StackupSource {
    Preset(StackupPreset),
    Auto,
    Custom(LayerStack),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackupPreset {
    TwoLayer,
    FourLayer,
    SixLayer,
    EightLayer,
}

/// Complete layer stack specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerStack {
    pub layers: Vec<Layer>,
    pub dielectrics: Vec<Dielectric>,
    pub total_thickness_mm: f64,
    pub via: ViaSpec,
}

impl LayerStack {
    /// Indices of layers available for signal routing.
    pub fn signal_layer_indices(&self) -> Vec<usize> {
        self.layers
            .iter()
            .filter(|l| l.kind == LayerKind::Signal && l.capacity_factor > 0.0)
            .map(|l| l.id)
            .collect()
    }

    /// Signal layers adjacent to a given layer kind.
    pub fn layers_adjacent_to(&self, kind: LayerKind) -> Vec<usize> {
        let mut result = Vec::new();
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.kind != LayerKind::Signal {
                continue;
            }
            if i > 0 && self.layers[i - 1].kind == kind {
                result.push(i);
            }
            if i + 1 < self.layers.len() && self.layers[i + 1].kind == kind {
                result.push(i);
            }
        }
        result.sort();
        result.dedup();
        result
    }

    /// Inner signal layers (not first or last).
    pub fn inner_signal_layer_indices(&self) -> Vec<usize> {
        let last = self.layers.len().saturating_sub(1);
        self.layers
            .iter()
            .filter(|l| l.kind == LayerKind::Signal && l.id != 0 && l.id != last)
            .map(|l| l.id)
            .collect()
    }

    /// Via area consumed on each layer (for capacity reduction).
    pub fn via_blockage_mm2(&self) -> f64 {
        std::f64::consts::PI * (self.via.pad_mm / 2.0).powi(2)
    }
}

/// Single PCB layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: usize,
    pub name: String,
    pub kind: LayerKind,
    pub thickness_mm: f64,
    pub copper_weight_oz: f64,
    pub dielectric_constant: f64,
    pub capacity_factor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerKind {
    Signal,
    Ground,
    Power,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dielectric {
    pub thickness_mm: f64,
    pub material: String,
    pub er: f64,
    pub loss_tangent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViaSpec {
    pub drill_mm: f64,
    pub pad_mm: f64,
    pub annular_ring_mm: f64,
}

// ── Component ──────────────────────────────────────────────────────────

/// Component to be placed on the board.
#[derive(Clone)]
pub struct Component {
    pub id: ComponentId,
    pub name: String,
    pub refdes: String,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Copper-envelope center offset from the component origin, in
    /// LOCAL (unrotated) coordinates. Zero for symmetric packages.
    pub bbox_dx: f64,
    pub bbox_dy: f64,
    pub pins: Vec<PinPosition>,
    pub side: BoardSide,
    pub group: Option<GroupId>,
    pub thermal_power_w: f64,
    /// GLACIER-solved branch current through this instance (A), when a
    /// DC solve ran. Feeds the power-tree flow analysis with REAL
    /// per-sink draws instead of the equal-share approximation.
    pub solved_current_a: Option<f64>,
    pub package: String,

    // Placement constraint
    pub placement: PlacementConstraint,

    // Mutable placement state
    pub x: f64,
    pub y: f64,
    pub theta: f64,

    // Congestion inflation factor (from routing feedback)
    pub density_inflation: f64,

    /// Typed layout intents attached to this component (e.g. a decoupling
    /// cap carrying `high_freq_bypass`). Read from the netlist instance's
    /// `layout_intents` by the semantic preprocessor; lowered to
    /// `Board.constraints` by the intent-lowering pass. Empty for most
    /// components.
    pub layout_intents: Vec<bhdl_common::intent::vocabulary::LayoutIntent>,
}

impl Component {
    /// Axis-aligned bounding box after rotation.
    pub fn rotated_bbox(&self) -> (f64, f64) {
        let cos_t = self.theta.cos().abs();
        let sin_t = self.theta.sin().abs();
        (
            self.width_mm * cos_t + self.height_mm * sin_t,
            self.width_mm * sin_t + self.height_mm * cos_t,
        )
    }

    /// Copper envelope in GLOBAL coordinates: (center_x, center_y,
    /// half_w, half_h). Asymmetric packages (offset tabs, one-sided
    /// pads) have an envelope center displaced from the component
    /// origin — clamping/overlap tests that assume a centered bbox
    /// under-protect the far side (the pad-at-board-edge family).
    /// Can these two components' bodies conflict in XY? Opposite-side
    /// SMD parts occupy different surfaces and may share coordinates;
    /// a through-hole pin pierces both sides, so a THT part conflicts
    /// with everything.
    pub fn shares_surface(&self, other: &Component) -> bool {
        if self.side == other.side {
            return true;
        }
        let tht = |c: &Component| {
            c.pins
                .iter()
                .any(|p| p.pad.as_ref().and_then(|pd| pd.drill_mm).is_some())
        };
        tht(self) || tht(other)
    }

    pub fn envelope(&self) -> (f64, f64, f64, f64) {
        let cos_t = self.theta.cos();
        let sin_t = self.theta.sin();
        let cx = self.x + self.bbox_dx * cos_t - self.bbox_dy * sin_t;
        let cy = self.y + self.bbox_dx * sin_t + self.bbox_dy * cos_t;
        let (rw, rh) = self.rotated_bbox();
        (cx, cy, rw / 2.0, rh / 2.0)
    }
}

/// Placement constraint for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlacementConstraint {
    /// Fully free — optimizer controls position and rotation.
    Free,
    /// Position and rotation fixed (connector, power jack).
    Fixed { x: f64, y: f64, theta: f64 },
    /// Position fixed, rotation free.
    FixedPosition { x: f64, y: f64 },
    /// Constrained to board edge — free along the edge axis.
    Edge {
        edge: BoardEdge,
        offset: Option<f64>,
    },
    /// Soft preference for a named region.
    PreferRegion { region_name: String },
}

impl PlacementConstraint {
    pub fn is_fixed(&self) -> bool {
        matches!(self, PlacementConstraint::Fixed { .. })
    }

    pub fn is_free(&self) -> bool {
        matches!(
            self,
            PlacementConstraint::Free | PlacementConstraint::PreferRegion { .. }
        )
    }
}

/// Pin location relative to component origin.
#[derive(Clone)]
pub struct PinPosition {
    pub pin_id: PinId,
    pub name: String,
    pub dx: f64,
    pub dy: f64,
    pub net: Option<NetId>,
    /// Real pad geometry from the footprint source (IPC-7351 generator
    /// or imported KiCad footprint). None only on the estimated-pin
    /// fallback path — the exporter then emits a visibly-default pad.
    pub pad: Option<PadGeom>,
    /// True when the footprint had no pad slot for this pin (entity
    /// declares more pins than the package has pads). Unplaced pins emit
    /// NO copper — stacked default pads at the footprint origin read as
    /// shorting_items to the oracle; an honest unconnected beats an
    /// invisible short.
    pub unplaced: bool,
}

/// Pad geometry carried through to fabrication output.
#[derive(Debug, Clone)]
pub struct PadGeom {
    pub width_mm: f64,
    pub height_mm: f64,
    pub shape: PadShapeKind,
    /// Some(drill) = through-hole pad on all copper layers.
    pub drill_mm: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadShapeKind {
    Rect,
    RoundRect,
    Circle,
    Oval,
}

// ── Net ────────────────────────────────────────────────────────────────

/// Net with semantic metadata from BHDL analysis.
#[derive(Clone)]
pub struct PnrNet {
    pub id: NetId,
    pub name: String,
    pub pins: Vec<(ComponentId, PinId)>,
    pub net_class: PnrNetClass,
    pub weight: f64,
    pub required_trace_width_mm: f64,
    pub layer_constraint: LayerConstraint,
    /// Plane layer index this net is assigned to (GND plane, power
    /// plane). Assigned at board construction after stackup resolution;
    /// the router skips assigned nets (their copper is the emitted zone
    /// FILL) and surface pads get via drops.
    pub plane_layer: Option<usize>,
    /// SPLIT-PLANE region (x0, y0, x1, y1): several rails share one
    /// Power layer, each owning a band. None = whole layer (single-net
    /// planes, all Ground planes). Computed AFTER placement from rail
    /// pin centroids; fills clip to it, drops must land inside it.
    pub plane_region: Option<(f64, f64, f64, f64)>,
    /// PLACEMENT-AWARE region shape: union of rects (pin-cluster
    /// boxes + MST corridors). Empty = the region IS plane_region's
    /// rect (single dense pin cloud, split-plane rails, legacy).
    /// Non-empty = plane_region holds the union's BBOX (coarse
    /// consumers prefilter on it) and fills/claims/aprons clip to
    /// the union — a scattered consumer cloud must NOT blanket the
    /// board with pour priority (free-placement vbias covered
    /// 80x70mm and swallowed foreign SMD pockets).
    pub plane_region_rects: Vec<(f64, f64, f64, f64)>,
    /// Declared-pour net whose split REGION is the pin cloud — the
    /// bbox is computed AFTER placement (free parts sit at garbage
    /// init positions at semantic time).
    pub pour_region_pending: bool,
    /// Layer rule: copper restricted to these layer indices (None =
    /// any signal layer). Pad layers are auto-included with a warning
    /// when a rule conflicts with where the pads physically are.
    pub allowed_layers: Option<Vec<usize>>,
    /// SOLVED DC voltage at this net (from the simulation
    /// annotations) — the rail level IR-drop percentages divide by.
    pub solved_voltage_v: Option<f64>,
    /// MEASURED edge swing (V) from a solved IBIS transient trace on
    /// this net — the aggressor amplitude crosstalk volts scale by.
    pub edge_swing_v: Option<f64>,
    /// Legacy string-form intent (from the older `intent_routing_constraints`
    /// path). Retained during transition; superseded by typed constraints
    /// in `Board.constraints`.
    pub intent: Option<String>,
    /// Typed layout intents attached at board level to this net
    /// (e.g. `@USB_DP for ...`). Most nets carry none; net/signal routing
    /// constraints generally arrive via the interface-constraint boundary
    /// (`intf_const__*`) rather than here.
    pub layout_intents: Vec<bhdl_common::intent::vocabulary::LayoutIntent>,
}

impl PnrNet {
    /// Whether this net connects via a dedicated copper plane rather than routed traces.
    ///
    /// Only GND gets a plane (when a dedicated ground layer exists in the stackup).
    /// Power nets are routed as traces since a 4-layer board has at most 1 power
    /// plane but typically multiple power rails (VIN, 5V, 3.3V, 1.8V, etc.).
    /// On 1-layer or 2-layer boards (no dedicated ground layer), even GND is routed.
    pub fn is_plane_connected(&self, _stack: &LayerStack) -> bool {
        // True only for nets ASSIGNED a plane layer at board
        // construction: the exporter now emits real filled_polygon
        // geometry for those planes (headless KiCad DRC uses saved
        // fills), so "the plane connects it" is backed by copper the
        // oracle can see. Surface pads get via drops; through-hole
        // barrels pierce the plane directly.
        self.plane_layer.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PnrNetClass {
    Signal,
    Power { voltage: f64, current: f64 },
    Ground,
    HighSpeed { max_length_mm: f64 },
    DifferentialPair { partner_net_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayerConstraint {
    Any,
    Preferred(usize),
    Required(usize),
    AdjacentToGround,
    InnerOnly,
}

impl LayerConstraint {
    /// Check if a given layer index is allowed by this constraint.
    pub fn allows(&self, layer: usize, stack: &LayerStack) -> bool {
        match self {
            LayerConstraint::Any => {
                stack.layers.get(layer).map_or(false, |l| l.kind == LayerKind::Signal)
            }
            LayerConstraint::Preferred(_) => {
                // Soft constraint — any signal layer is technically allowed
                stack.layers.get(layer).map_or(false, |l| l.kind == LayerKind::Signal)
            }
            LayerConstraint::Required(id) => layer == *id,
            LayerConstraint::AdjacentToGround => {
                stack.layers_adjacent_to(LayerKind::Ground).contains(&layer)
            }
            LayerConstraint::InnerOnly => {
                stack.inner_signal_layer_indices().contains(&layer)
            }
        }
    }
}

// ── Functional groups ──────────────────────────────────────────────────

/// Functional group (e.g., expansion block children).
#[derive(Clone)]
pub struct FunctionalGroup {
    pub id: GroupId,
    pub name: String,
    pub members: Vec<ComponentId>,
    pub parent: Option<ComponentId>,
}

// ── Route (output) ─────────────────────────────────────────────────────

/// Routed path through the 3D grid (coarse or final).
#[derive(Debug, Clone)]
pub struct Route {
    pub net_id: NetId,
    pub segments: Vec<RouteSegment>,
    pub vias: Vec<RouteVia>,
    /// Segment spans per Steiner path (start index, length): each span
    /// connects ONE sink to the tree. Lets the geometric validator
    /// amputate a single offending branch instead of ripping the whole
    /// net (one bad segment was converting a 36/37-complete power tree
    /// into 37 unconnected pins). Empty = no structure known; rip whole.
    pub path_spans: Vec<(usize, usize)>,
    /// Parent span index per span (Steiner attachment): amputating a
    /// span must take its whole SUBTREE — children attach mid-path, and
    /// cutting only the parent strands them as dangling copper (the
    /// oracle's track_dangling). None = attached at the source pin (or
    /// a trunk stub).
    pub path_parents: Vec<Option<usize>>,
    /// Via ranges per span (start index into `vias`, length): amputation
    /// must take a branch's vias with its segments or they orphan as
    /// via_dangling.
    pub via_spans: Vec<(usize, usize)>,
}

impl Route {
    pub fn empty(net_id: NetId) -> Self {
        Route {
            net_id,
            segments: Vec::new(),
            vias: Vec::new(),
            path_spans: Vec::new(),
            path_parents: Vec::new(),
            via_spans: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn via_count(&self) -> usize {
        self.vias.len()
    }

    pub fn total_length(&self) -> f64 {
        self.segments.iter().map(|s| s.length()).sum()
    }
}

#[derive(Debug, Clone)]
pub struct RouteSegment {
    pub layer: usize,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub width_mm: f64,
}

impl RouteSegment {
    pub fn length(&self) -> f64 {
        let dx = self.end.0 - self.start.0;
        let dy = self.end.1 - self.start.1;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Debug, Clone)]
pub struct RouteVia {
    pub x: f64,
    pub y: f64,
    pub from_layer: usize,
    pub to_layer: usize,
}

// ── P&R configuration ──────────────────────────────────────────────────

/// Top-level configuration for the place & route engine.
#[derive(Clone)]
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
            board: BoardConfig::default(),
            placement: PlacementConfig::default(),
            routing_schedule: RoutingSchedule::default(),
            optimizer: OptimizerConfig::default(),
            convergence: ConvergenceConfig::default(),
            max_iterations: 800,
        }
    }
}

#[derive(Clone)]
pub struct PlacementConfig {
    pub position_lr: f64,
    pub rotation_lr: f64,
    pub lambda_density: f64,
    pub lambda_group: f64,
    pub lambda_thermal: f64,
    pub lambda_congestion: f64,
    pub lambda_via: f64,
    pub lambda_region: f64,
    /// Weight on intent-derived proximity/keep-away forces (soft base).
    /// Hard proximity is additionally scaled by a ramping Lagrangian λ.
    pub lambda_proximity: f64,
    /// Weight on intent-derived loop-area minimization forces.
    pub lambda_loop_area: f64,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        PlacementConfig {
            position_lr: 2.0,   // mm per Adam step — components need to move significantly
            rotation_lr: 0.02,  // radians per Adam step — ~1° per step
            lambda_density: 2.0, // strong density to spread components
            lambda_group: 10.0,
            lambda_thermal: 0.1,
            lambda_congestion: 0.0, // starts at 0, grows after routing begins
            lambda_via: 0.0,
            lambda_region: 1.0,
            lambda_proximity: 4.0,
            lambda_loop_area: 1.0,
        }
    }
}

#[derive(Clone)]
pub struct RoutingSchedule {
    pub first_route_iter: usize,
    pub coarse_interval: usize,
    pub fine_interval: usize,
    pub fine_start_iter: usize,
}

impl Default for RoutingSchedule {
    fn default() -> Self {
        RoutingSchedule {
            first_route_iter: 100,
            coarse_interval: 50,
            fine_interval: 25,
            fine_start_iter: 400,
        }
    }
}

impl RoutingSchedule {
    /// Should we route at this iteration?
    pub fn should_route(&self, iteration: usize) -> bool {
        if iteration < self.first_route_iter {
            return false;
        }
        if iteration >= self.fine_start_iter {
            (iteration - self.fine_start_iter) % self.fine_interval == 0
        } else {
            (iteration - self.first_route_iter) % self.coarse_interval == 0
        }
    }

    /// How many PathFinder iterations to run at this stage.
    pub fn pathfinder_iterations(&self, iteration: usize) -> usize {
        if iteration >= self.fine_start_iter {
            10
        } else {
            5
        }
    }
}

#[derive(Clone)]
pub struct OptimizerConfig {
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        OptimizerConfig {
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
        }
    }
}

#[derive(Clone)]
pub struct ConvergenceConfig {
    pub window_size: usize,
    pub wl_tolerance: f64,
    pub max_rollbacks: usize,
}

impl Default for ConvergenceConfig {
    fn default() -> Self {
        ConvergenceConfig {
            window_size: 50,
            wl_tolerance: 0.01,
            max_rollbacks: 3,
        }
    }
}

// ── P&R result ─────────────────────────────────────────────────────────

/// Result of place & route.
#[derive(Clone)]
pub struct PnrResult {
    pub board: Board,
    pub routes: Vec<Route>,
    pub metrics: PnrMetrics,
    pub drc_violations: Vec<DrcViolation>,
}

#[derive(Clone)]
pub struct PnrMetrics {
    /// Pins whose pad is geometrically touched by their net's copper —
    /// the honest connectivity currency for trial selection.
    pub connected_sinks: usize,
    pub hpwl_mm: f64,
    pub total_routed_length_mm: f64,
    pub via_count: usize,
    pub max_congestion: f64,
    pub routability_pct: f64,
    pub iterations: usize,
    /// Pour (signal-layer plane) defects the sink counter can't see:
    /// off-side SMD plane pads with no live drop via + pour islands
    /// the stitch pass could not bridge. Always 0 with the pour
    /// experiment off — trial selection stays byte-identical there.
    pub pour_defects: usize,
    /// DETOUR ENVELOPE: per-net routed-length / pad-MST percentiles.
    /// The 14-board demo survey measured the professional bar as
    /// SCALE-FREE — p50 1.05-1.28, p90 1.36-1.85 from a 15fp tube amp
    /// to a 1508fp 12-layer VME board. Read-only metric (no trial
    /// influence yet); 0.0 when no net qualifies.
    pub detour_p50: f64,
    pub detour_p90: f64,
    /// Nets in the detour sample (>=2 pads, non-plane, MST > 1mm).
    pub detour_nets: usize,
}

#[derive(Clone, Debug)]
pub struct DrcViolation {
    pub kind: DrcViolationKind,
    pub location: (f64, f64),
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum DrcViolationKind {
    Spacing,
    Clearance,
    UnroutedNet,
    TraceWidthBelowMin,
    ViaInKeepout,
}
