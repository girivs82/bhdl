# RFC: Adopting SKALP Language Infrastructure for BHDL

**Author:** girivs
**Date:** 2026-02-19
**Status:** Draft
**Priority:** High
**Related:** `docs/comparison/SKALP_Features_Analysis.md`

## Motivation

BHDL and SKALP are sister projects targeting different hardware design domains (board vs. IC). While their domains differ fundamentally, SKALP's language infrastructure — type system, generics, const evaluation, monomorphization, traits, safety annotations — is mature, battle-tested on real designs (DC-DC controllers, motor drives, FP arithmetic), and largely domain-agnostic.

BHDL's current language infrastructure has significant gaps that limit expressiveness and safety:
- **String-based types** with no parameterization or width inference
- **Untyped entity parameters** with no validation at instantiation
- **Limited const evaluation** (i64-only, no struct/enum/unit support)
- **No monomorphization** (cannot specialize generic entities)
- **No trait system** (cannot express behavioral interfaces)
- **Flat diagnostics** with no error classification or recovery

This RFC proposes adopting specific SKALP patterns to close these gaps.

---

## 1. Parameterized Type System

### Current State (BHDL)

`bhdl-analyzer/src/types.rs` defines types as:
```rust
pub struct ResolvedTypeInfo {
    pub base_type_name: String,  // "signal", "power", "voltage"
    pub bounds: Option<(i64, i64)>,  // bus width
}
```

No parametric types, no width parameters, no type constructors. Types are opaque strings.

### SKALP Pattern

`skalp-frontend/src/types.rs` uses a rich algebraic type:
```rust
enum Type {
    Bit(Width), Int(Width), Nat(Width),
    Fixed { integer_bits, fractional_bits },
    Array { element_type, size },
    Struct(StructType), Enum(EnumType),
    TypeParam(String),  // Generic placeholder
    ...
}
enum Width { Fixed(u32), Param(String), Inferred(WidthVar) }
```

### Proposed BHDL Design

```rust
enum BhdlType {
    // Electrical primitives
    Voltage(Option<VoltageSpec>),      // voltage, voltage<3.3, 0.05>
    Current(Option<CurrentSpec>),
    Resistance(Option<ResistanceSpec>),
    Capacitance, Inductance, Impedance,
    Power, Frequency, Temperature, Time,

    // Signal types
    Signal(Option<VoltageDomain>),     // signal, signal<@VCC_3V3>
    Bus(Width),                        // bus[8], bus[N]
    Differential,

    // Composite
    Array { element: Box<BhdlType>, size: ArraySize },
    Struct(StructDef),
    Enum(EnumDef),

    // Parametric
    TypeParam(String),                 // Generic placeholder: T
    ConstParam(String),                // Const generic: N
}

enum Width { Fixed(u32), Param(String), Inferred }
enum ArraySize { Fixed(usize), Param(String) }

struct VoltageSpec { nominal: f64, tolerance: f64 }  // 3.3V +/- 5%
```

### BHDL Syntax

```bhdl
// Parameterized voltage types
type V3V3 = voltage<3.3, 0.05>;    // 3.3V +/- 5%
type V5V0 = voltage<5.0, 0.10>;    // 5.0V +/- 10%

// Parameterized component types
entity ResistorDivider<V_IN: voltage, V_OUT: voltage>
where V_IN > V_OUT
{
    pin input: power<V_IN> in;
    pin output: power<V_OUT> out;
    pin gnd: ground;
}
```

### Files to Modify
- `bhdl-common/src/types.rs` — new `BhdlType` enum
- `bhdl-parser/` — parse parameterized type syntax
- `bhdl-ast/src/` — AST nodes for type parameters
- `bhdl-analyzer/src/types.rs` — type resolution and inference
- `bhdl-analyzer/src/passes/pass2.rs` — type checking with parameters

### Effort: 4-6 weeks

---

## 2. Typed Generics with Constraints

### Current State (BHDL)

`bhdl-ast/src/common.rs` lines 44-57: `ParamDecl` holds only name + optional type name + default:
```rust
pub struct ParamDecl(pub(crate) SyntaxNode<BhdlLanguage>);
// name(), type_ref() -> Option<TypeRef>, default_value() -> Option<Expr>
```

No type checking on parameters, no constraints, no validation at instantiation.

### SKALP Pattern

```rust
pub struct HirGeneric {
    pub name: String,
    pub param_type: HirGenericType,  // Type, TypeWithBounds{bounds}, Const(type)
    pub default_value: Option<HirExpression>,
}
```

Constraints validated at instantiation via monomorphization engine.

### Proposed BHDL Design

```bhdl
// Typed parameters with constraints
entity BuckConverter<
    V_IN: voltage where V_IN >= 4.5V && V_IN <= 40V,
    V_OUT: voltage where V_OUT < V_IN,
    I_MAX: current where I_MAX <= 3A
>() {
    // Body can use V_IN, V_OUT, I_MAX as compile-time constants
    const R_HIGH: resistance = 100kOhm;
    const R_LOW: resistance = R_HIGH * V_OUT / (V_IN - V_OUT);
    const F_SW: frequency = 500kHz;
}

// Instantiation — compiler validates constraints
main_reg: BuckConverter<12V, 3.3V, 2A>();   // OK
bad_reg: BuckConverter<3V, 5V, 2A>();        // ERROR: V_OUT < V_IN violated
```

### Key Data Structures

```rust
pub struct GenericParam {
    pub name: String,
    pub param_type: GenericParamType,
    pub constraints: Vec<Constraint>,
    pub default: Option<ConstValue>,
}

pub enum GenericParamType {
    Type,                           // T (any type)
    TypeBounded(Vec<String>),       // T: SpiPeripheral
    Const(BhdlType),                // N: nat, V: voltage
}

pub enum Constraint {
    GreaterThan(Expr, Expr),
    LessThan(Expr, Expr),
    Equal(Expr, Expr),
    InRange(Expr, Expr, Expr),
    TraitBound(String, String),     // T: TraitName
}
```

### Files to Modify
- `bhdl-parser/src/top_level.rs` — parse `<V_IN: voltage where ...>` syntax
- `bhdl-ast/src/common.rs` — `GenericParam` with constraints
- `bhdl-analyzer/src/passes/pass2.rs` — validate constraints at instantiation
- `bhdl-analyzer/src/passes/pass3.rs` — substitute parameters during const eval

### Effort: 3-4 weeks (after type system)

---

## 3. Rich Const Evaluator

### Current State (BHDL)

`bhdl-analyzer/src/passes/pass3.rs` line 19: `evaluate_const_expr_as_i64` — evaluates to `i64` only, supports `+`, `-`, `*`, `/`. No structs, enums, booleans, strings, floats, built-in functions.

### SKALP Pattern

`skalp-frontend/src/const_eval.rs`:
```rust
pub enum ConstValue {
    Nat(usize), Int(i64), Bool(bool), String(String), Float(f64),
    FloatFormat(FloatFormatValue),
    Struct(IndexMap<String, ConstValue>),
}
```
- 20+ built-in functions (clog2, pow2, max, min, gcd, etc.)
- User-defined const functions with 100-level recursion guard
- Stack overflow prevention via `stacker::maybe_grow(256KB, 8MB)`

### Proposed BHDL Design

```rust
pub enum ConstValue {
    Integer(i64),
    Float(f64),
    Bool(bool),
    String(String),

    // Physical quantities with units
    Voltage(f64),       // Volts
    Current(f64),       // Amps
    Resistance(f64),    // Ohms
    Capacitance(f64),   // Farads
    Inductance(f64),    // Henries
    Power(f64),         // Watts
    Frequency(f64),     // Hertz
    Temperature(f64),   // Celsius
    Time(f64),          // Seconds

    // Composite
    Struct(IndexMap<String, ConstValue>),
    Array(Vec<ConstValue>),
    Enum { variant: String, payload: Option<Box<ConstValue>> },
}
```

### Built-in Functions for Board Design

```bhdl
// E-series resistor snapping
const fn nearest_e96(r: resistance) -> resistance;
const fn nearest_e24(r: resistance) -> resistance;

// Electrical calculations
const fn parallel(r1: resistance, r2: resistance) -> resistance;
const fn divider_ratio(r_high: resistance, r_low: resistance) -> float;
const fn rc_cutoff(r: resistance, c: capacitance) -> frequency;
const fn lc_resonance(l: inductance, c: capacitance) -> frequency;
const fn power_dissipation(v: voltage, i: current) -> power;

// Thermal calculations
const fn thermal_rise(power: power, theta_ja: float) -> temperature;

// Unit conversions
const fn to_milli(v: float) -> float;
const fn to_micro(v: float) -> float;
const fn to_kilo(v: float) -> float;
```

### Dimensional Analysis

The const evaluator should enforce unit consistency:
```bhdl
const x = 3.3V * 100mA;     // OK: voltage * current = power (0.33W)
const y = 3.3V + 100mA;     // ERROR: cannot add voltage and current
const z = 3.3V / 1kOhm;     // OK: voltage / resistance = current (3.3mA)
```

### Stack Safety (from SKALP)

```rust
use stacker;
const STACK_RED_ZONE: usize = 256 * 1024;  // 256 KB
const STACK_GROW_SIZE: usize = 8 * 1024 * 1024;  // 8 MB

fn evaluate(&mut self, expr: &Expr) -> Result<ConstValue, EvalError> {
    stacker::maybe_grow(STACK_RED_ZONE, STACK_GROW_SIZE, || {
        self.evaluate_impl(expr)
    })
}
```

### Files to Modify
- `bhdl-analyzer/src/passes/pass3.rs` — rewrite with `ConstValue` enum
- `bhdl-common/src/` — new `const_eval.rs` module (shared across crates)
- `bhdl-analyzer/src/expression_evaluator.rs` — integrate with const eval
- Add `stacker` to workspace `Cargo.toml`

### Effort: 2-3 weeks

---

## 4. Monomorphization Pipeline

### Current State (BHDL)

No monomorphization. Module instantiation in Pass 2 (`pass2.rs` lines 26-150) just looks up module name in symbol table. Parameters are never substituted into module bodies.

### SKALP Pattern

`skalp-frontend/src/monomorphization/engine.rs`: Iterative fixed-point algorithm:
1. **Collect**: Scan all instantiations, extract generic arguments
2. **Specialize**: Clone module, substitute params, reassign IDs
3. **Remap**: Port/signal IDs from generic to specialized versions
4. **Deduplicate**: Identical specializations share a single copy
5. **Iterate**: Until no new instantiations discovered

### Proposed BHDL Design

```
Pass 2.5: Monomorphization (new pass)

Input: AST with generic module declarations + instantiations
Output: Expanded AST with all generics resolved to concrete types

Algorithm:
1. Build generic registry: { module_name -> GenericModuleDef }
2. Scan all instantiations for generic modules
3. For each (module, concrete_params):
   a. Generate mangled name: "BuckConverter_12V_3V3_2A"
   b. Clone module body
   c. Substitute all param references with concrete values
   d. Run const evaluation on substituted body
   e. Register specialized module
4. Replace generic instantiations with specialized references
5. Repeat until fixpoint
```

### Deduplication Key

```rust
#[derive(Hash, Eq, PartialEq)]
struct SpecializationKey {
    module_name: String,
    params: BTreeMap<String, ConstValue>,  // BTreeMap for determinism
}
```

### Files to Modify
- New: `bhdl-analyzer/src/passes/monomorphization.rs`
- `bhdl-analyzer/src/passes/mod.rs` — integrate as Pass 2.5
- `bhdl-ast/src/` — mangled name support
- `bhdl-synthesizer/src/lib.rs` — consume specialized modules

### Effort: 4-6 weeks

---

## 5. Enum Types and Match Expressions

### Current State (BHDL)

No enum types. No match expressions. Power states, connector types, error conditions are all stringly-typed or implicit.

### SKALP Pattern

```rust
pub struct HirEnumType {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}
pub struct EnumVariant {
    pub name: String,
    pub payload: Option<Type>,
}
pub enum HirPattern {
    Literal(HirLiteral),
    Variable(String),
    Wildcard,
    Path(String, String),  // enum_name::variant_name
}
```

### Proposed BHDL Design

```bhdl
enum ConnectorType {
    USB_C,
    USB_A,
    HDMI,
    DisplayPort,
    RJ45,
    BarrelJack(voltage, current),  // With payload
}

enum PowerState {
    Off,
    Standby,
    Active,
    Fault(FaultKind),
}

enum FaultKind {
    Overcurrent,
    Overvoltage,
    Overtemperature,
    ShortCircuit,
}

// Match expression for power sequencing
sequence startup(state: PowerState) {
    match state {
        PowerState::Off => {
            enable VCC_STANDBY;
            wait VCC_STANDBY.stable(1ms);
            transition Standby;
        }
        PowerState::Standby => {
            enable VCC_CORE;
            enable VCC_IO;
            wait all_stable(5ms);
            transition Active;
        }
        PowerState::Fault(kind) => match kind {
            FaultKind::Overcurrent => emergency_shutdown();
            FaultKind::Overtemperature => thermal_throttle();
            _ => graceful_shutdown();
        }
    }
}
```

### Files to Modify
- `bhdl-parser/src/` — parse `enum` declarations and `match` expressions
- `bhdl-ast/src/` — `EnumDef`, `MatchExpr`, `Pattern` AST nodes
- `bhdl-analyzer/src/passes/pass1.rs` — register enums in symbol table
- `bhdl-analyzer/src/passes/pass3.rs` — evaluate enum variants as constants
- `bhdl-analyzer/src/passes/pass2.rs` — exhaustiveness checking for match

### Effort: 2-3 weeks

---

## 6. Trait System for Component Interfaces

### Current State (BHDL)

No trait system. Interface compliance is manual. Intent functions (`bhdl-common/src/intent.rs`) are hardcoded, not resolved through traits.

### SKALP Pattern

```rust
pub struct HirTraitDefinition {
    pub name: String,
    pub methods: Vec<HirTraitMethod>,
    pub associated_types: Vec<HirTraitAssociatedType>,
    pub associated_constants: Vec<HirTraitAssociatedConst>,
}
```

### Proposed BHDL Design

```bhdl
// Standard interface traits
trait SpiPeripheral {
    pin MOSI: signal in;
    pin MISO: signal out;
    pin SCK: signal in;
    pin CS: signal in active_low;
    const MAX_FREQ: frequency;
}

trait I2cDevice {
    pin SDA: signal inout;
    pin SCL: signal in;
    const ADDRESS: nat[7];
}

trait PowerRegulator {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    pin EN: signal in;
    const DROPOUT: voltage;
    const MAX_CURRENT: current;
}

// Component implements traits
component LM7805 impl PowerRegulator {
    const DROPOUT = 2.0V;
    const MAX_CURRENT = 1.5A;
    // pins declared automatically from trait
}

component BME280 impl SpiPeripheral, I2cDevice {
    const MAX_FREQ = 10MHz;
    const ADDRESS = 0x76;
}

// Generic entities constrained by traits
entity SpiBus<P: SpiPeripheral, N: nat>() {
    master: SpiMaster();
    devices: P[N];

    // Compiler knows P has MOSI, MISO, SCK, CS
    for i in 0..N {
        master.MOSI -> devices[i].MOSI;
        master.MISO <- devices[i].MISO;
        master.SCK -> devices[i].SCK;
    }
}

// Generic power entity constrained by trait
entity RegulatedSupply<R: PowerRegulator>(vin: voltage, vout: voltage)
where vin - vout > R::DROPOUT
{
    reg: R();
    vin -> reg.VIN;
    reg.VOUT -> vout;
}
```

### Protocol Direction Flipping (from SKALP)

```bhdl
trait SpiMasterPort {
    pin MOSI: signal out;
    pin MISO: signal in;
    pin SCK: signal out;
    pin CS: signal out active_low;
}

// ~ operator flips all directions
entity SpiFlash impl ~SpiMasterPort {
    // MOSI becomes in, MISO becomes out, SCK becomes in, CS becomes in
}
```

### Files to Modify
- `bhdl-parser/src/` — parse `trait`, `impl`, `~` syntax
- `bhdl-ast/src/` — `TraitDef`, `TraitImpl`, `TraitBound` nodes
- `bhdl-analyzer/src/passes/pass1.rs` — register traits in symbol table
- `bhdl-analyzer/src/passes/pass2.rs` — trait resolution and compliance checking
- `bhdl-synthesizer/src/` — use trait info for connection validation

### Effort: 4-5 weeks

---

## 7. Safety Annotations and Fault Injection

### Current State (BHDL)

`bhdl-safety/src/` has numeric-only safety analysis: overvoltage, overcurrent, thermal stress checks on computed values. No compile-time safety guarantees.

### SKALP Pattern

```rust
pub struct SafetyMechanismConfig {
    pub mechanism_type: Option<String>,
    pub dc: Option<f64>,       // Diagnostic Coverage
    pub lc: Option<f64>,       // Latent Coverage
}
pub struct SeoocConfig {
    pub target_asil: String,   // "A"-"D" or "QM"
    pub assumed_mechanisms: Vec<AssumedMechanismConfig>,
}
pub enum DetectionMode { Continuous, Boot, Periodic, OnDemand }
```

### Proposed BHDL Design

```bhdl
// Safety goal definition
safety_goal SG_OVP {
    id: "SG-001";
    title: "Prevent output overvoltage";
    asil: B;
    ftti: 10ms;
}

// Safety mechanism on a module
#[safety_mechanism(type: ovp_monitor, dc: 99%, implements: SG_OVP)]
entity OutputProtection {
    sense: VoltageDivider<12V, 2.5V>();
    comparator: LM339();

    // Protection path
    reg.VOUT -> sense.input;
    sense.output -> comparator.IN_PLUS;
    ref_voltage -> comparator.IN_MINUS;
    comparator.OUT -> reg.EN;   // Shuts down regulator on OV
}

// Fault injection tests
fault_inject short(reg.VOUT, VIN) -> verify {
    assert comparator.OUT == low within 100us;
    assert reg.EN == disabled;
}

fault_inject open(R_SENSE) -> verify {
    // Detect loss of voltage sensing
    assert system_state == PowerState::Fault(FaultKind::SensorFailure);
}

// Component-level derating annotations
#[safety(derating: voltage=80%, current=70%, temperature=85C_max)]
component CriticalMOSFET: IRFZ44N();

// Redundancy annotations
#[safety(redundant, voting: 2_of_3)]
entity TripleCurrentSense {
    sense_a: CurrentSense();
    sense_b: CurrentSense();
    sense_c: CurrentSense();
}
```

### ISO 26262 / IEC 61508 Integration

```rust
pub enum AsilLevel { QM, A, B, C, D }
pub enum SilLevel { SIL1, SIL2, SIL3, SIL4 }

pub struct SafetyGoal {
    pub id: String,
    pub title: String,
    pub asil: AsilLevel,
    pub ftti_ms: Option<f64>,
    pub description: Option<String>,
}

pub struct SafetyMechanism {
    pub mechanism_type: String,
    pub diagnostic_coverage: f64,     // 0.0 - 1.0
    pub latent_coverage: Option<f64>,
    pub detection_mode: DetectionMode,
    pub response_time_us: Option<f64>,
    pub implements: Vec<String>,       // Safety goal IDs
}

pub struct FaultInjection {
    pub fault_type: FaultType,
    pub target: FaultTarget,
    pub assertions: Vec<SafetyAssertion>,
}

pub enum FaultType { Short(String, String), Open(String), Drift(String, f64) }
```

### Files to Modify
- `bhdl-safety/src/` — expand with ASIL/SIL types, fault injection framework
- `bhdl-parser/src/` — parse `safety_goal`, `fault_inject`, `#[safety]`
- `bhdl-analyzer/src/` — new safety analysis pass
- `bhdl-synthesizer/src/` — generate safety reports, FMEA tables

### Effort: 4-5 weeks

---

## 8. Structured Error Reporting

### Current State (BHDL)

`bhdl-analyzer/src/types.rs` line 320: `Diagnostic { message: String, range: TextRange }`. No error codes, no classification, no suggestions.

### SKALP Pattern

Structured error enums with specific variants, position tracking, recovery strategies.

### Proposed BHDL Design

```rust
pub enum DiagnosticKind {
    // Type errors
    TypeMismatch { expected: BhdlType, found: BhdlType },
    VoltageDomainMismatch { from_domain: String, to_domain: String },
    UnitMismatch { expected_unit: String, found_unit: String },

    // Constraint errors
    ConstraintViolation { constraint: String, value: String },
    ParameterOutOfRange { param: String, range: String, value: String },

    // Safety errors
    UnprotectedDomainCrossing { from: String, to: String },
    MissingSafetyMechanism { goal: String },
    InsufficientDiagnosticCoverage { required: f64, achieved: f64 },

    // Resolution errors
    UndefinedSymbol { name: String, suggestions: Vec<String> },
    AmbiguousReference { name: String, candidates: Vec<String> },
    TraitNotImplemented { component: String, trait_name: String },

    // Electrical errors
    ExceededCurrentRating { component: String, rating: f64, actual: f64 },
    ExceededVoltageRating { component: String, rating: f64, actual: f64 },
    ThermalViolation { component: String, max_temp: f64, estimated_temp: f64 },
}

pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub severity: Severity,       // Error, Warning, Info, Hint
    pub range: TextRange,
    pub message: String,          // Human-readable summary
    pub code: Option<String>,     // E.g., "E0042", "W0015"
    pub hints: Vec<DiagnosticHint>,
    pub related: Vec<RelatedInfo>,
}

pub struct DiagnosticHint {
    pub message: String,
    pub range: Option<TextRange>,
    pub fix: Option<SuggestedFix>,
}
```

### Files to Modify
- `bhdl-analyzer/src/types.rs` — new `DiagnosticKind` enum
- All analysis passes — use structured diagnostics
- `bhdl-lsp/src/` — map diagnostics to LSP protocol
- `bhdl-cli/src/` — format diagnostics for terminal output

### Effort: 2-3 weeks

---

## 9. Lexical Scope Chain (Fix Existing Pain Point)

### Current State (BHDL)

`bhdl-analyzer/src/symbol_table.rs`: Flat HashMap lookup with `children: Vec<SymbolTable>` for nesting, but no scope chain during lookup. `Pass3Context` manually maintains `current_scope_stack` as a fragile workaround.

### SKALP Pattern

Scoped symbol table with parent chain, automatic lookup through enclosing scopes, proper shadowing rules.

### Proposed BHDL Design

```rust
pub struct ScopeChain {
    scopes: Vec<Scope>,
    current: ScopeId,
}

pub struct Scope {
    id: ScopeId,
    parent: Option<ScopeId>,
    symbols: IndexMap<String, Symbol>,
    kind: ScopeKind,
}

pub enum ScopeKind {
    Global,
    Board,
    Module,
    PowerDomain,
    GenerateBlock,
    ConditionalBlock,
}

impl ScopeChain {
    /// Look up symbol, traversing parent scopes
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        let mut scope_id = Some(self.current);
        while let Some(id) = scope_id {
            let scope = &self.scopes[id.0];
            if let Some(sym) = scope.symbols.get(name) {
                return Some(sym);
            }
            scope_id = scope.parent;
        }
        None
    }
}
```

### Files to Modify
- `bhdl-analyzer/src/symbol_table.rs` — replace with `ScopeChain`
- `bhdl-analyzer/src/passes/pass1.rs` — build scope chain
- `bhdl-analyzer/src/passes/pass2.rs` — use scope chain for resolution
- `bhdl-analyzer/src/passes/pass3.rs` — remove manual `current_scope_stack` workaround

### Effort: 2-3 weeks

---

## 10. Unit-Aware Dimensional Analysis (BHDL-Specific Extension)

This feature has no SKALP equivalent but is critical for board design. SKALP operates on dimensionless bit vectors; BHDL operates on physical quantities.

### Proposed Design

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct Dimension {
    pub voltage: i8,     // V
    pub current: i8,     // A
    pub resistance: i8,  // Ω (= V/A)
    pub time: i8,        // s
    pub temperature: i8, // °C
    pub length: i8,      // m
}

impl Dimension {
    pub const VOLTAGE: Self     = Dimension { voltage: 1, current: 0, resistance: 0, time: 0, temperature: 0, length: 0 };
    pub const CURRENT: Self     = Dimension { voltage: 0, current: 1, resistance: 0, time: 0, temperature: 0, length: 0 };
    pub const RESISTANCE: Self  = Dimension { voltage: 1, current: -1, resistance: 0, time: 0, temperature: 0, length: 0 };
    pub const POWER: Self       = Dimension { voltage: 1, current: 1, resistance: 0, time: 0, temperature: 0, length: 0 };
    pub const CAPACITANCE: Self = Dimension { voltage: -1, current: 1, resistance: 0, time: 1, temperature: 0, length: 0 };
    pub const FREQUENCY: Self   = Dimension { voltage: 0, current: 0, resistance: 0, time: -1, temperature: 0, length: 0 };
    pub const DIMENSIONLESS: Self = Dimension { voltage: 0, current: 0, resistance: 0, time: 0, temperature: 0, length: 0 };
}
```

Compile-time validation:
```bhdl
const p = 3.3V * 100mA;          // OK: [V^1][A^1] = power
const r = 3.3V / 100mA;          // OK: [V^1][A^-1] = resistance
const bad = 3.3V + 100mA;        // ERROR: [V^1] + [A^1] dimension mismatch
const tau = 10kOhm * 100nF;      // OK: [V^1][A^-1] * [V^-1][A^1][s^1] = [s^1] = time
const f = 1 / (2 * pi * tau);    // OK: [s^-1] = frequency
```

### Effort: 2-3 weeks

---

## Implementation Roadmap

### Phase 1: Foundation (Weeks 1-5)
| Week | Feature | Depends On |
|------|---------|-----------|
| 1-2 | Lexical scope chain (Section 9) | — |
| 2-3 | Rich const evaluator (Section 3) | — |
| 3-5 | Parameterized type system (Section 1) | Scope chain |

### Phase 2: Generics (Weeks 5-11)
| Week | Feature | Depends On |
|------|---------|-----------|
| 5-7 | Typed generics with constraints (Section 2) | Type system |
| 7-8 | Enum types and match (Section 5) | Type system |
| 8-11 | Monomorphization pipeline (Section 4) | Generics, const eval |

### Phase 3: Interfaces & Safety (Weeks 11-18)
| Week | Feature | Depends On |
|------|---------|-----------|
| 11-13 | Dimensional analysis (Section 10) | Const eval |
| 13-15 | Trait system (Section 6) | Generics, type system |
| 15-17 | Safety annotations + FI (Section 7) | Traits, enums |
| 17-18 | Structured diagnostics (Section 8) | All passes |

### Total: ~18 weeks (4.5 months) for full implementation

---

## What NOT to Port from SKALP

Some SKALP features are IC-specific and should not be adopted:

- **Clock domains / CDC analysis** — boards don't have clock domain crossings
- **Sequential process semantics** (`on(clk.rise)`) — boards are declarative
- **Gate-level synthesis** (AIG, technology mapping) — not applicable
- **Formal equivalence checking** (SAT/BMC) — wrong abstraction level
- **Pipeline annotations** — no pipeline concept in board design
- **Non-blocking assignment semantics** — no register model

---

## References

- `docs/comparison/SKALP_Features_Analysis.md` — Prior analysis of high-level SKALP features
- `/Users/girivs/src/hw/hls/crates/skalp-frontend/src/` — SKALP frontend implementation
- `/Users/girivs/src/hw/hls/crates/skalp-frontend/src/monomorphization/` — Monomorphization engine
- `/Users/girivs/src/hw/hls/crates/skalp-frontend/src/const_eval.rs` — Const evaluator
- `/Users/girivs/src/hw/hls/crates/skalp-frontend/src/safety_attributes.rs` — Safety framework
