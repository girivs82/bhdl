# BHDL Scalability Enhancement Implementation Plan

**Created:** 2025-10-11
**Status:** Design Phase
**Goal:** Enhance BHDL to scale from small boards (<50 components) to enterprise-scale boards (500+ components)

## Executive Summary

This plan adds four major features to BHDL v2.0 to enable scalability to enterprise-level circuit boards while preserving the intuitive flow-based syntax:

1. **Power Domain Blocks** - Declarative power distribution with automatic decoupling
2. **Component Groups & Wildcards** - Bulk operations on multiple components
3. **Enhanced Interface Bundles** - Single-line bus connections
4. **Template System** - Reusable connection patterns

All enhancements maintain backward compatibility with existing BHDL v2.0 code.

---

## Phase 1: Power Domain Enhancement

### Motivation

Current BHDL requires explicit connection to each power pin:
```bhdl
@VCC -> U1.VDD;
@VCC -> U2.VDD;
@VCC -> U3.VDD;
// ... 97 more lines for 100 ICs
@GND -> U1.GND;
@GND -> U2.GND;
// ... 98 more lines
```

This doesn't scale. Power distribution is a **star topology**, not a flow chain.

### Proposed Syntax

```bhdl
power_domain @VCC_3V3 = 3.3V @ 10A {
    // Power sources
    sources {
        reg1: LDO_3V3().OUT;
        reg2: LDO_3V3().OUT;  // Redundant supply
    }

    // Load distribution
    distribution {
        fpga.VCCO[0..7];        // All 8 I/O banks
        ics[*].VDD;             // All ICs with VDD pin
        connectors[*].power;    // All connector power pins
    }

    // Automatic decoupling network
    decoupling {
        near fpga: [10µF @ 5, 1µF @ 10, 0.1µF @ 30];
        near ics[each]: [10µF @ 1, 0.1µF @ 2];
        distributed: [0.1µF @ 50];
    }

    // Constraints
    constraints {
        max_voltage_drop: 50mV;
        min_trace_width: 0.5mm;
        copper_weight: 2oz;
    }
}
```

### Implementation Details

#### 1.1 Parser Changes

**File:** `bhdl-parser/src/syntax.rs`
```rust
// Add new keywords
POWER_DOMAIN_KW,    // power_domain
SOURCES_KW,         // sources
DISTRIBUTION_KW,    // distribution
DECOUPLING_KW,      // decoupling
NEAR_KW,            // near
EACH_KW,            // each
DISTRIBUTED_KW,     // distributed
CONSTRAINTS_KW,     // constraints

// Add new syntax nodes
POWER_DOMAIN_DEF,       // power_domain @NAME = spec { ... }
SOURCES_BLOCK,          // sources { ... }
DISTRIBUTION_BLOCK,     // distribution { ... }
DECOUPLING_BLOCK,       // decoupling { ... }
DECOUPLING_RULE,        // near fpga: [caps]
CONSTRAINTS_BLOCK,      // constraints { ... }
CONSTRAINT_ITEM,        // name: value
```

**File:** `bhdl-parser/src/top_level.rs`
```rust
// Add power_domain parsing function
pub(crate) fn power_domain(p: &mut Parser) {
    assert!(p.at(POWER_DOMAIN_KW));
    let m = p.start();

    p.bump(POWER_DOMAIN_KW);

    // Expect @ prefix for net name
    p.expect(AT);
    p.expect(IDENT);

    // Power spec: = 3.3V @ 10A
    p.expect(EQ);
    expression(p); // voltage
    p.expect(AT);
    expression(p); // current

    // Body
    p.expect(L_BRACE);

    while !p.at(R_BRACE) && !p.at(EOF) {
        match p.current() {
            SOURCES_KW => sources_block(p),
            DISTRIBUTION_KW => distribution_block(p),
            DECOUPLING_KW => decoupling_block(p),
            CONSTRAINTS_KW => constraints_block(p),
            _ => {
                p.error("Expected sources, distribution, decoupling, or constraints");
                p.bump_any();
            }
        }
    }

    p.expect(R_BRACE);
    m.complete(p, POWER_DOMAIN_DEF);
}
```

#### 1.2 AST Changes

**File:** `bhdl-ast/src/items.rs`
```rust
#[derive(Debug, Clone)]
pub struct PowerDomain {
    pub name: String,           // Net name (with @ prefix)
    pub voltage: Expression,
    pub current: Expression,
    pub sources: Vec<SourceDef>,
    pub distribution: Vec<PinRef>,
    pub decoupling: Vec<DecouplingRule>,
    pub constraints: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct SourceDef {
    pub handle: String,
    pub component: ComponentInst,
    pub pin: String,
}

#[derive(Debug, Clone)]
pub struct DecouplingRule {
    pub placement: DecouplingPlacement,
    pub capacitors: Vec<CapSpec>,
}

#[derive(Debug, Clone)]
pub enum DecouplingPlacement {
    Near { component: String },
    NearEach { components: ComponentSelector },
    Distributed,
}

#[derive(Debug, Clone)]
pub struct CapSpec {
    pub value: Expression,
    pub count: usize,
}
```

#### 1.3 Analyzer Changes

**File:** `bhdl-analyzer/src/passes/pass_power_domain.rs` (NEW FILE)
```rust
//! Power Domain Analysis Pass
//!
//! Expands power_domain blocks into explicit connections and component instantiations.

use bhdl_ast::PowerDomain;
use crate::symbol_table::SymbolTable;

pub struct PowerDomainAnalyzer;

impl PowerDomainAnalyzer {
    pub fn analyze(domain: &PowerDomain, symbols: &mut SymbolTable) -> Result<(), Error> {
        // 1. Validate voltage/current specs
        validate_power_spec(domain)?;

        // 2. Expand distribution list
        let loads = expand_distribution(&domain.distribution, symbols)?;

        // 3. Generate decoupling capacitors
        let decoupling_caps = generate_decoupling(&domain.decoupling, &loads)?;

        // 4. Create explicit connections
        for load in loads {
            symbols.add_connection(domain.name.clone(), load);
        }

        // 5. Instantiate decoupling capacitors
        for cap in decoupling_caps {
            symbols.add_component(cap);
        }

        // 6. Validate constraints
        validate_constraints(&domain.constraints, &loads)?;

        Ok(())
    }
}

fn expand_distribution(
    pins: &[PinRef],
    symbols: &SymbolTable
) -> Result<Vec<ExpandedPin>, Error> {
    let mut expanded = Vec::new();

    for pin_ref in pins {
        match pin_ref {
            // fpga.VCCO[0..7] -> expand to 8 pins
            PinRef::Range { component, pin, range } => {
                for i in range.start..=range.end {
                    expanded.push(ExpandedPin {
                        component: component.clone(),
                        pin: format!("{}[{}]", pin, i),
                    });
                }
            }

            // ics[*].VDD -> find all ICs, expand to their VDD pins
            PinRef::Wildcard { selector, pin } => {
                let components = resolve_selector(selector, symbols)?;
                for comp in components {
                    if comp.has_pin(pin) {
                        expanded.push(ExpandedPin {
                            component: comp.name.clone(),
                            pin: pin.clone(),
                        });
                    }
                }
            }

            // Regular pin reference
            PinRef::Simple { component, pin } => {
                expanded.push(ExpandedPin {
                    component: component.clone(),
                    pin: pin.clone(),
                });
            }
        }
    }

    Ok(expanded)
}
```

#### 1.4 Synthesizer Changes

**File:** `bhdl-synthesizer/src/power_domain_synthesis.rs` (NEW FILE)
```rust
//! Power Domain Synthesis
//!
//! Converts analyzed power domain into netlist connections and components.

use bhdl_netlist::{Netlist, NetId, InstanceId};
use bhdl_analyzer::PowerDomainData;

pub struct PowerDomainSynthesizer;

impl PowerDomainSynthesizer {
    pub fn synthesize(
        domain: &PowerDomainData,
        netlist: &mut Netlist
    ) -> Result<(), Error> {
        // 1. Create the power net
        let power_net = netlist.create_net(&domain.name);

        // 2. Connect all sources to the net
        for source in &domain.sources {
            let source_pin = netlist.get_pin(source.instance_id, &source.pin_name)?;
            netlist.connect(source_pin, power_net)?;
        }

        // 3. Connect all loads to the net
        for load in &domain.loads {
            let load_pin = netlist.get_pin(load.instance_id, &load.pin_name)?;
            netlist.connect(load_pin, power_net)?;
        }

        // 4. Instantiate decoupling capacitors
        for (idx, cap) in domain.decoupling_caps.iter().enumerate() {
            let cap_id = netlist.add_instance(
                format!("C_DECOUP_{}", idx),
                "Cap",
                vec![("value", cap.value.clone())],
            )?;

            // Connect to power and ground
            let pos_pin = netlist.get_pin(cap_id, "1")?;
            let neg_pin = netlist.get_pin(cap_id, "2")?;
            netlist.connect(pos_pin, power_net)?;
            netlist.connect(neg_pin, domain.ground_net)?;

            // Add placement constraint
            if let Some(near_comp) = &cap.near_component {
                netlist.add_placement_constraint(
                    cap_id,
                    PlacementConstraint::Near {
                        target: near_comp.clone(),
                        max_distance: 5.0, // mm
                    }
                );
            }
        }

        Ok(())
    }
}
```

### Testing Strategy

**Test File:** `tests/circuits/power_domain/fpga_power.bhdl`
```bhdl
board FPGABoard {
    power VIN = 12V @ 15A;
    ground GND;

    // Define multiple power domains
    power_domain @VCC_1V0 = 1.0V @ 5A {
        sources {
            reg_core: TPS54302(1.0V).OUT;
        }
        distribution {
            fpga.VCCINT[0..3];  // 4 core voltage pins
        }
        decoupling {
            near fpga: [10µF @ 4, 1µF @ 8, 0.1µF @ 20];
        }
    }

    power_domain @VCC_3V3 = 3.3V @ 10A {
        sources {
            reg_io: TPS54302(3.3V).OUT;
        }
        distribution {
            fpga.VCCO[0..7];    // 8 I/O bank voltages
        }
        decoupling {
            near fpga: [10µF @ 8, 0.1µF @ 40];
        }
    }

    fpga: XC7A100T() {
        // Power pins handled by power_domain blocks
        // Only need to specify signal pins here
        USER_LED -> led: LED(blue).A;
    }
}
```

**Test Commands:**
```bash
cargo test -p bhdl-parser test_power_domain_parsing
cargo test -p bhdl-analyzer test_power_domain_expansion
cargo test -p bhdl-synthesizer test_power_domain_netlist
```

### Deliverables

- [ ] Parser support for `power_domain` syntax
- [ ] AST representation of power domains
- [ ] Analyzer pass to expand power domains
- [ ] Synthesizer to convert to netlist
- [ ] Unit tests for parsing
- [ ] Integration tests for expansion
- [ ] Example circuits demonstrating usage
- [ ] Documentation update

---

## Phase 2: Component Groups and Wildcards

### Motivation

Need to perform bulk operations on multiple components:
- Connect power to all ICs
- Connect ground to all components
- Apply constraints to component sets

### Proposed Syntax

```bhdl
// Define component groups
group power_ics = [reg1, reg2, reg3, pmic];
group sensors = [temp_sensor, humidity, pressure];
group all_ics = [*];  // All instantiated ICs

// Bulk operations using wildcards
@VCC -> power_ics[*].VIN;     // Connect to VIN of each
@GND -> all_ics[*].GND;        // Connect to GND of all ICs

// Filtered groups
group high_speed = all_ics where has_pin("CLK");
@CLK -> high_speed[*].CLK;
```

### Implementation Details

#### 2.1 Parser Changes

**File:** `bhdl-parser/src/syntax.rs`
```rust
// Add keywords
GROUP_KW,          // group
ALL_KW,            // all (already exists as wildcard)

// Add nodes
GROUP_DEF,         // group name = [components];
COMPONENT_SELECTOR,// [component_list] or [*] or filter
COMPONENT_FILTER,  // where condition
```

**File:** `bhdl-parser/src/top_level.rs`
```rust
pub(crate) fn group_def(p: &mut Parser) {
    assert!(p.at(GROUP_KW));
    let m = p.start();

    p.bump(GROUP_KW);
    p.expect(IDENT);  // group name
    p.expect(EQ);

    // Component list: [comp1, comp2, ...] or [*]
    p.expect(L_BRACKET);

    if p.at(STAR) {
        p.bump(STAR);  // Wildcard for all components
    } else {
        // List of component names
        while !p.at(R_BRACKET) && !p.at(EOF) {
            p.expect(IDENT);
            if p.at(COMMA) {
                p.bump(COMMA);
            }
        }
    }

    p.expect(R_BRACKET);

    // Optional filter: where condition
    if p.at(WHERE_KW) {
        p.bump(WHERE_KW);
        expression(p);  // Filter condition
    }

    p.expect(SEMI);
    m.complete(p, GROUP_DEF);
}
```

#### 2.2 AST Changes

**File:** `bhdl-ast/src/items.rs`
```rust
#[derive(Debug, Clone)]
pub struct ComponentGroup {
    pub name: String,
    pub selector: ComponentSelector,
    pub filter: Option<Expression>,
}

#[derive(Debug, Clone)]
pub enum ComponentSelector {
    Explicit(Vec<String>),    // [comp1, comp2, comp3]
    All,                      // [*]
}

// Extend PinRef to support wildcards
#[derive(Debug, Clone)]
pub enum PinRef {
    Simple {
        component: String,
        pin: String,
    },
    Range {
        component: String,
        pin: String,
        range: Range,
    },
    Wildcard {
        selector: ComponentSelector,  // NEW
        pin: String,
    },
    GroupRef {
        group: String,               // NEW
        pin: String,
    },
}
```

#### 2.3 Analyzer Changes

**File:** `bhdl-analyzer/src/component_groups.rs` (NEW FILE)
```rust
//! Component Group Resolution

use bhdl_ast::ComponentGroup;
use crate::symbol_table::SymbolTable;

pub struct ComponentGroupResolver;

impl ComponentGroupResolver {
    pub fn resolve(
        group: &ComponentGroup,
        symbols: &SymbolTable
    ) -> Result<Vec<String>, Error> {
        match &group.selector {
            ComponentSelector::Explicit(names) => {
                // Verify all components exist
                for name in names {
                    if !symbols.has_component(name) {
                        return Err(Error::UndefinedComponent(name.clone()));
                    }
                }
                Ok(names.clone())
            }

            ComponentSelector::All => {
                // Get all component handles
                let all = symbols.all_components();

                // Apply filter if present
                if let Some(filter) = &group.filter {
                    Ok(apply_filter(all, filter, symbols)?)
                } else {
                    Ok(all)
                }
            }
        }
    }
}

fn apply_filter(
    components: Vec<String>,
    filter: &Expression,
    symbols: &SymbolTable
) -> Result<Vec<String>, Error> {
    let mut filtered = Vec::new();

    for comp in components {
        if evaluate_filter(filter, &comp, symbols)? {
            filtered.push(comp);
        }
    }

    Ok(filtered)
}

fn evaluate_filter(
    filter: &Expression,
    component: &str,
    symbols: &SymbolTable
) -> Result<bool, Error> {
    match filter {
        Expression::FunctionCall { name, args } => {
            match name.as_str() {
                "has_pin" => {
                    let pin_name = evaluate_string_arg(&args[0])?;
                    let comp_def = symbols.get_component(component)?;
                    Ok(comp_def.has_pin(&pin_name))
                }
                "type_is" => {
                    let type_name = evaluate_string_arg(&args[0])?;
                    let comp_def = symbols.get_component(component)?;
                    Ok(comp_def.component_type == type_name)
                }
                _ => Err(Error::UnknownFilterFunction(name.clone()))
            }
        }
        _ => Err(Error::InvalidFilterExpression)
    }
}
```

### Testing Strategy

**Test File:** `tests/circuits/component_groups/multi_ic_board.bhdl`
```bhdl
board MultiICBoard {
    power VCC = 5V @ 5A;
    ground GND;

    // Instantiate multiple ICs
    cpu: STM32F103() { /* ... */ }
    memory: IS62C256() { /* ... */ }
    flash: W25Q32() { /* ... */ }
    eth: ENC28J60() { /* ... */ }

    // Define groups
    group all_memory = [memory, flash];
    group peripherals = [eth];
    group all_ics = [*];

    // Bulk power connections
    @VCC -> all_ics[*].VDD;
    @GND -> all_ics[*].GND;

    // Group-specific constraints
    with routing(max_length = 50mm) {
        cpu.ADDR[0..15] -> memory.A[0..15];
    }
}
```

### Deliverables

- [ ] Parser support for `group` definitions
- [ ] Wildcard `[*]` in pin references
- [ ] Filter expressions (`where` clause)
- [ ] Group resolution in analyzer
- [ ] Expansion to explicit connections
- [ ] Unit tests
- [ ] Integration tests
- [ ] Documentation

---

## Phase 3: Enhanced Interface Bundles

### Motivation

Complex buses require many explicit connections:
```bhdl
// Current: 96 lines for DDR3 interface
cpu.DDR_DQ_0 -> memory.DQ0;
cpu.DDR_DQ_1 -> memory.DQ1;
// ... 94 more lines
```

Need: Single-line bus connections.

### Proposed Syntax

```bhdl
// Enhanced interface with full signal definition
interface DDR3(width: nat = 64) {
    DQ[width]: bidirectional;
    DQS[width/8]: differential bidirectional;
    A[16]: output;
    BA[3]: output;
    CAS_N: output;
    RAS_N: output;
    WE_N: output;
    CK_P: output;
    CK_N: output;
    CS_N: output;
}

// Component with interface
cpu: ARM_Cortex {
    ddr_bus: interface DDR3(width=64);
}

memory: DDR3_SODIMM {
    interface: DDR3(width=64);
}

// Single-line connection!
cpu.ddr_bus <=> memory.interface;
```

### Implementation Details

#### 3.1 Parser Changes

Interface definitions already exist in BHDL, but need enhancement for:
- Array notation in interface signals
- Differential pair notation
- Computed sizes (width/8)

**File:** `bhdl-parser/src/top_level.rs`
```rust
pub(crate) fn interface_signal(p: &mut Parser) {
    let m = p.start();

    // Signal name
    p.expect(IDENT);

    // Optional array suffix: [width] or [computed_expr]
    if p.at(L_BRACKET) {
        p.bump(L_BRACKET);
        expression(p);  // Array size expression
        p.expect(R_BRACKET);
    }

    p.expect(COLON);

    // Optional modifiers
    if p.at(DIFFERENTIAL_KW) {
        p.bump(DIFFERENTIAL_KW);
    }

    // Direction
    direction(p);

    p.expect(SEMI);
    m.complete(p, INTERFACE_SIGNAL);
}
```

#### 3.2 AST Changes

**File:** `bhdl-ast/src/interfaces.rs`
```rust
#[derive(Debug, Clone)]
pub struct InterfaceSignal {
    pub name: String,
    pub array_size: Option<Expression>,  // NEW: support computed sizes
    pub is_differential: bool,           // NEW
    pub direction: Direction,
}

#[derive(Debug, Clone)]
pub struct InterfaceConnection {
    pub lhs: InterfaceRef,
    pub rhs: InterfaceRef,
}

#[derive(Debug, Clone)]
pub struct InterfaceRef {
    pub component: String,
    pub interface_name: String,
}
```

#### 3.3 Analyzer Changes

**File:** `bhdl-analyzer/src/interface_analysis.rs`
```rust
//! Interface Connection Analysis

pub struct InterfaceAnalyzer;

impl InterfaceAnalyzer {
    pub fn expand_interface_connection(
        conn: &InterfaceConnection,
        symbols: &SymbolTable
    ) -> Result<Vec<PinConnection>, Error> {
        // 1. Resolve both interface references
        let lhs_iface = resolve_interface(&conn.lhs, symbols)?;
        let rhs_iface = resolve_interface(&conn.rhs, symbols)?;

        // 2. Verify compatibility
        verify_interface_compatibility(&lhs_iface, &rhs_iface)?;

        // 3. Expand to individual pin connections
        let mut pin_connections = Vec::new();

        for signal in lhs_iface.signals {
            match signal.array_size {
                Some(size) => {
                    // Expand array signals
                    let count = evaluate_const_expr(&size, symbols)?;
                    for i in 0..count {
                        pin_connections.push(PinConnection {
                            source: format!("{}.{}[{}]", conn.lhs.component, signal.name, i),
                            dest: format!("{}.{}[{}]", conn.rhs.component, signal.name, i),
                            bidirectional: signal.direction == Direction::Inout,
                        });
                    }
                }
                None => {
                    // Simple signal
                    pin_connections.push(PinConnection {
                        source: format!("{}.{}", conn.lhs.component, signal.name),
                        dest: format!("{}.{}", conn.rhs.component, signal.name),
                        bidirectional: signal.direction == Direction::Inout,
                    });
                }
            }
        }

        Ok(pin_connections)
    }
}
```

### Testing Strategy

**Test File:** `tests/circuits/interfaces/ddr3_system.bhdl`
```bhdl
interface DDR3(width: nat = 64) {
    DQ[width]: bidirectional;
    DQS[width/8]: differential bidirectional;
    A[16]: output;
    BA[3]: output;
    CAS_N: output;
    RAS_N: output;
    WE_N: output;
}

board DDR3System {
    cpu: ARM_SoC {
        ddr: interface DDR3(width=64);
    }

    memory: DDR3_SODIMM {
        interface: DDR3(width=64);
    }

    // Single line connects 96 signals!
    cpu.ddr <=> memory.interface;
}
```

### Deliverables

- [ ] Enhanced interface signal parsing (arrays, differential)
- [ ] Interface connection operator `<=>`
- [ ] Interface expansion in analyzer
- [ ] Compatibility checking
- [ ] Unit tests
- [ ] Integration tests
- [ ] Documentation

---

## Phase 4: Template System

### Motivation

Common connection patterns are repeated throughout designs:
- IC power/decoupling pattern
- Pull-up resistor pattern
- Crystal oscillator circuit
- Test point addition

Templates allow these to be defined once and reused.

### Proposed Syntax

```bhdl
// Define template
template ICPowerNetwork(ic, vcc, gnd) {
    vcc -> bypass1: Cap(10µF) near ic -> gnd;
    vcc -> bypass2: Cap(0.1µF) near ic -> gnd;
    vcc -> ic.VDD;
    gnd -> ic.GND;
}

// Define template with parameters
template PullUpResistor(signal, vcc, value: resistance = 10k) {
    vcc -> pullup: Res(value).1 -> signal;
}

// Use templates
board MainBoard {
    power VCC = 3.3V @ 5A;
    ground GND;

    cpu: STM32F103() { /* ... */ }
    memory: IS62C256() { /* ... */ }
    eth: ENC28J60() { /* ... */ }

    // Apply template to each IC
    apply ICPowerNetwork(cpu, @VCC, @GND);
    apply ICPowerNetwork(memory, @VCC, @GND);
    apply ICPowerNetwork(eth, @VCC, @GND);

    // Or bulk apply
    for ic in [cpu, memory, eth] {
        apply ICPowerNetwork(ic, @VCC, @GND);
    }

    // Parameterized template
    apply PullUpResistor(cpu.RESET, @VCC, value: 4.7k);
}
```

### Implementation Details

#### 4.1 Parser Changes

**File:** `bhdl-parser/src/syntax.rs`
```rust
// Add keywords
TEMPLATE_KW,      // template
APPLY_KW,         // apply

// Add nodes
TEMPLATE_DEF,     // template Name(params) { body }
TEMPLATE_PARAM,   // Parameter with optional type and default
APPLY_STMT,       // apply TemplateName(args);
```

**File:** `bhdl-parser/src/top_level.rs`
```rust
pub(crate) fn template_def(p: &mut Parser) {
    assert!(p.at(TEMPLATE_KW));
    let m = p.start();

    p.bump(TEMPLATE_KW);
    p.expect(IDENT);  // Template name

    // Parameters
    p.expect(L_PAREN);
    while !p.at(R_PAREN) && !p.at(EOF) {
        template_param(p);
        if p.at(COMMA) {
            p.bump(COMMA);
        }
    }
    p.expect(R_PAREN);

    // Body
    p.expect(L_BRACE);
    while !p.at(R_BRACE) && !p.at(EOF) {
        connection_stmt(p);  // Standard connection statements
    }
    p.expect(R_BRACE);

    m.complete(p, TEMPLATE_DEF);
}

pub(crate) fn template_param(p: &mut Parser) {
    let m = p.start();

    p.expect(IDENT);  // Parameter name

    // Optional type annotation
    if p.at(COLON) {
        p.bump(COLON);
        type_ref(p);
    }

    // Optional default value
    if p.at(EQ) {
        p.bump(EQ);
        expression(p);
    }

    m.complete(p, TEMPLATE_PARAM);
}
```

#### 4.2 AST Changes

**File:** `bhdl-ast/src/items.rs`
```rust
#[derive(Debug, Clone)]
pub struct TemplateDef {
    pub name: String,
    pub params: Vec<TemplateParam>,
    pub body: Vec<ConnectionStmt>,
}

#[derive(Debug, Clone)]
pub struct TemplateParam {
    pub name: String,
    pub type_hint: Option<TypeRef>,
    pub default_value: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct ApplyStmt {
    pub template_name: String,
    pub args: Vec<Expression>,
}
```

#### 4.3 Analyzer Changes

**File:** `bhdl-analyzer/src/template_expansion.rs` (NEW FILE)
```rust
//! Template Expansion
//!
//! Expands template applications into explicit connections.

use bhdl_ast::{TemplateDef, ApplyStmt};
use crate::symbol_table::SymbolTable;

pub struct TemplateExpander;

impl TemplateExpander {
    pub fn expand(
        apply: &ApplyStmt,
        templates: &HashMap<String, TemplateDef>,
        symbols: &SymbolTable
    ) -> Result<Vec<ConnectionStmt>, Error> {
        // 1. Find template definition
        let template = templates.get(&apply.template_name)
            .ok_or(Error::UndefinedTemplate(apply.template_name.clone()))?;

        // 2. Bind arguments to parameters
        let bindings = bind_arguments(template, apply)?;

        // 3. Substitute parameters in template body
        let expanded = substitute_params(&template.body, &bindings)?;

        // 4. Validate expanded connections
        validate_connections(&expanded, symbols)?;

        Ok(expanded)
    }
}

fn bind_arguments(
    template: &TemplateDef,
    apply: &ApplyStmt
) -> Result<HashMap<String, Expression>, Error> {
    let mut bindings = HashMap::new();

    // Positional arguments
    for (i, arg) in apply.args.iter().enumerate() {
        if i >= template.params.len() {
            return Err(Error::TooManyArguments);
        }

        let param = &template.params[i];
        bindings.insert(param.name.clone(), arg.clone());
    }

    // Fill in defaults for missing arguments
    for param in template.params.iter().skip(apply.args.len()) {
        if let Some(default) = &param.default_value {
            bindings.insert(param.name.clone(), default.clone());
        } else {
            return Err(Error::MissingRequiredArgument(param.name.clone()));
        }
    }

    Ok(bindings)
}

fn substitute_params(
    body: &[ConnectionStmt],
    bindings: &HashMap<String, Expression>
) -> Result<Vec<ConnectionStmt>, Error> {
    let mut substituted = Vec::new();

    for stmt in body {
        substituted.push(substitute_stmt(stmt, bindings)?);
    }

    Ok(substituted)
}
```

### Testing Strategy

**Test File:** `tests/circuits/templates/template_system.bhdl`
```bhdl
// Define reusable templates
template StandardICPower(ic, vcc, gnd) {
    vcc -> decoup1: Cap(10µF) near ic -> gnd;
    vcc -> decoup2: Cap(0.1µF) near ic -> gnd;
    vcc -> ic.VDD;
    gnd -> ic.GND;
}

template I2CPullUp(sda, scl, vcc, value: resistance = 4.7k) {
    vcc -> r_sda: Res(value).1 -> sda;
    vcc -> r_scl: Res(value).1 -> scl;
}

board TemplateDemo {
    power VCC = 3.3V @ 2A;
    ground GND;

    cpu: STM32F103() { /* ... */ }
    sensor: BME280() { /* ... */ }

    // Apply templates
    apply StandardICPower(cpu, @VCC, @GND);
    apply StandardICPower(sensor, @VCC, @GND);
    apply I2CPullUp(cpu.SDA, cpu.SCL, @VCC);
}
```

### Deliverables

- [ ] Parser support for `template` and `apply`
- [ ] AST representation
- [ ] Template expansion in analyzer
- [ ] Parameter binding and substitution
- [ ] Validation of expanded connections
- [ ] Unit tests
- [ ] Integration tests
- [ ] Standard library of common templates
- [ ] Documentation

---

## Implementation Timeline

### Sprint 1 (2 weeks): Phase 1 - Power Domains
- Week 1: Parser + AST
- Week 2: Analyzer + Synthesizer + Tests

### Sprint 2 (1.5 weeks): Phase 2 - Component Groups
- Week 3: Parser + AST + Analyzer
- Week 4: Tests + Documentation (partial)

### Sprint 3 (1.5 weeks): Phase 3 - Enhanced Interfaces
- Week 5: Parser enhancements + AST
- Week 6: Analyzer expansion + Tests (partial)

### Sprint 4 (2 weeks): Phase 4 - Templates
- Week 7: Parser + AST + Analyzer
- Week 8: Template library + Tests + Final documentation

**Total Duration: 8 weeks**

---

## Success Criteria

1. **Scalability Target**: 500+ component board expressed in <500 lines of BHDL
2. **Power Domain**: 100 ICs powered by 1 power domain block (not 200 lines)
3. **Interface**: DDR3 interface connected in 1 line (not 96 lines)
4. **Templates**: Common patterns defined once, reused throughout
5. **Backward Compatibility**: All existing BHDL v2.0 code continues to work
6. **Test Coverage**: >90% coverage for new features
7. **Documentation**: Complete specification and examples

---

## Risk Mitigation

### Risk 1: Parser Complexity
**Mitigation:** Use existing parser patterns, add incrementally, extensive unit tests

### Risk 2: Analyzer Performance
**Mitigation:** Profile expansion passes, optimize hot paths, consider caching

### Risk 3: Template Substitution Bugs
**Mitigation:** Thorough validation, type checking, clear error messages

### Risk 4: Breaking Changes
**Mitigation:** Maintain backward compatibility, add new features alongside old syntax

---

## Future Enhancements (Post Phase 4)

1. **Constraint Propagation**: Constraints flow through interfaces
2. **Layout Templates**: Physical placement patterns
3. **Parametric Templates**: Templates that adapt based on parameters
4. **Template Composition**: Templates that use other templates
5. **Standard Template Library**: Curated collection of proven templates

---

## Appendix: Complete Example

**Before (BHDL v2.0 - Verbose):**
```bhdl
board LargeSystem {
    power VCC = 3.3V @ 10A;
    ground GND;

    // 100 ICs with explicit power connections = 200 lines
    @VCC -> U1.VDD;
    @GND -> U1.GND;
    @VCC -> U2.VDD;
    @GND -> U2.GND;
    // ... 196 more lines

    // DDR3 interface = 96 lines
    cpu.DDR_DQ_0 -> memory.DQ0;
    cpu.DDR_DQ_1 -> memory.DQ1;
    // ... 94 more lines
}
```

**After (BHDL v3.0 with Enhancements - Concise):**
```bhdl
board LargeSystem {
    power VIN = 12V @ 15A;
    ground GND;

    // Power domains - handles all 100 ICs automatically
    power_domain @VCC = 3.3V @ 10A {
        sources { reg: TPS54302(3.3V).OUT; }
        distribution { all_ics[*].VDD; }
        decoupling { distributed: [0.1µF @ 100]; }
    }

    power_domain @GND {
        distribution { all_ics[*].GND; }
    }

    // Component group
    group all_ics = [*];

    // Complex IC with interface
    cpu: ARM_Cortex { ddr: interface DDR3(width=64); }
    memory: DDR3_SODIMM { interface: DDR3(width=64); }

    // Single-line bus connection (96 signals)
    cpu.ddr <=> memory.interface;

    // Template for repetitive patterns
    template ICPower(ic) {
        @VCC -> bypass1: Cap(10µF) near ic -> @GND;
        @VCC -> bypass2: Cap(0.1µF) near ic -> @GND;
    }

    // Apply to specific ICs needing extra decoupling
    apply ICPower(cpu);
}
```

**Line Count:**
- Before: ~300 lines for 100 ICs + DDR3
- After: ~30 lines (10x reduction!)

---

## Conclusion

These enhancements maintain BHDL's intuitive flow-based syntax while adding the scalability needed for enterprise-level boards. The implementation is phased to deliver value incrementally, with each phase building on the previous one.

**Next Steps:**
1. Review and approve this plan
2. Begin Sprint 1: Power Domain implementation
3. Create example boards demonstrating each feature
4. Update specification documents
5. Implement test suite
