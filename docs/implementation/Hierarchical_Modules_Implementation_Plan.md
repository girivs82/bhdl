# Hierarchical Modules - Implementation Plan

## Overview

This document provides a detailed implementation plan for adding hierarchical module support to BHDL, including all parser, AST, analyzer, and synthesizer changes required.

## Phase 1: Parser and AST Extensions

### 1.1 Parser Grammar Updates

#### Module Definition with Parameters
```rust
// In bhdl-parser/src/grammar.rs
pub(crate) fn module_def(p: &mut Parser) {
    assert!(p.at(T![module]));
    let m = p.start();
    p.bump(T![module]);
    p.expect(T![ident]);
    
    // NEW: Parse parameter list
    if p.at(T!['(']) {
        param_list(p);
    }
    
    p.expect(T!['{']);
    
    while !p.at(T!['}']) && !p.at(T![EOF]) {
        if p.at(T![pin]) {
            pin_declaration(p);
        } else if p.at(T![attribute]) {
            attribute_declaration(p);
        } else if p.at(T![ident]) && p.nth_at(1, T![:]) {
            // NEW: Module instantiation within module
            instance_declaration(p);
        } else {
            component_or_connection(p);
        }
    }
    
    p.expect(T!['}']);
    m.complete(p, MODULE_DEF);
}

fn param_list(p: &mut Parser) {
    assert!(p.at(T!['(']));
    let m = p.start();
    p.bump(T!['(']);
    
    while !p.at(T![')']) && !p.at(T![EOF]) {
        param_declaration(p);
        if !p.at(T![')']) {
            p.expect(T![,]);
        }
    }
    
    p.expect(T![')']);
    m.complete(p, PARAM_LIST);
}

fn param_declaration(p: &mut Parser) {
    let m = p.start();
    p.expect(T![ident]);  // param name
    p.expect(T![:]);
    type_expr(p);         // param type
    
    if p.at(T![=]) {
        p.bump(T![=]);
        expr(p);          // default value
    }
    
    m.complete(p, PARAM_DECL);
}
```

#### Module Instantiation Updates
```rust
fn instance_declaration(p: &mut Parser) {
    let m = p.start();
    p.expect(T![ident]);  // instance name
    p.expect(T![:]);
    p.expect(T![ident]);  // module name
    
    // NEW: Parse arguments
    if p.at(T!['(']) {
        instance_args(p);
    }
    
    p.expect(T!['{']);
    
    // Parse port mappings with new syntax
    while !p.at(T!['}']) && !p.at(T![EOF]) {
        if p.at(T![attribute]) {
            scoped_attribute(p);
        } else {
            port_mapping(p);
        }
    }
    
    p.expect(T!['}']);
    m.complete(p, INSTANCE_DECL);
}

fn port_mapping(p: &mut Parser) {
    let m = p.start();
    
    // Left side: module pin (no dots!)
    pin_reference(p);
    
    // Connection operator
    if p.at(T![<-]) {
        p.bump(T![<-]);
    } else if p.at(T![->]) {
        p.bump(T![->]);
    } else if p.at(T![<->]) {
        p.bump(T![<->]);
    } else {
        p.error("expected connection operator");
    }
    
    // Right side: signal or qualified pin
    connection_target(p);
    
    p.expect(T![;]);
    m.complete(p, PORT_MAPPING);
}
```

### 1.2 AST Node Definitions

```rust
// In bhdl-ast/src/lib.rs

#[derive(Debug, Clone)]
pub struct ModuleDef {
    pub name: String,
    pub params: Option<ParamList>,
    pub pins: Vec<PinDecl>,
    pub attributes: Vec<AttributeDecl>,
    pub instances: Vec<InstanceDecl>,  // NEW
    pub components: Vec<ComponentDecl>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone)]
pub struct ParamList {
    pub params: Vec<ParamDecl>,
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: String,
    pub param_type: TypeExpr,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct InstanceDecl {
    pub name: String,
    pub module_name: String,
    pub args: Option<InstanceArgs>,      // NEW
    pub port_mappings: Vec<PortMapping>, // NEW syntax
    pub attributes: Vec<ScopedAttribute>, // NEW
}

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub module_pin: PinRef,      // Always on left
    pub direction: ConnOp,       // <-, ->, <->
    pub target: ConnectionTarget, // Signal or qualified pin
}

#[derive(Debug, Clone)]
pub struct ScopedAttribute {
    pub path: Vec<String>,  // e.g., ["controller", "fsw"]
    pub value: Expr,
}
```

## Phase 2: Symbol Table and Name Resolution

### 2.1 Hierarchical Scopes

```rust
// In bhdl-analyzer/src/symbol_table.rs

#[derive(Debug)]
pub struct HierarchicalScope {
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub symbols: HashMap<String, Symbol>,
    pub children: Vec<ScopeId>,
}

#[derive(Debug)]
pub enum ScopeKind {
    Board,
    Module(ModuleInfo),
    Instance(InstanceInfo),
}

#[derive(Debug)]
pub struct ModuleInfo {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub is_definition: bool,  // true for module def, false for instance
}

impl SymbolTable {
    pub fn enter_module_scope(&mut self, module: &ModuleDef) -> ScopeId {
        let scope = HierarchicalScope {
            parent: Some(self.current_scope),
            kind: ScopeKind::Module(ModuleInfo {
                name: module.name.clone(),
                params: self.extract_params(module),
                is_definition: true,
            }),
            symbols: HashMap::new(),
            children: Vec::new(),
        };
        
        let scope_id = self.add_scope(scope);
        self.current_scope = scope_id;
        
        // Add parameters to scope
        for param in &module.params {
            self.add_parameter(param);
        }
        
        scope_id
    }
}
```

### 2.2 Instance Resolution

```rust
// In bhdl-analyzer/src/resolver.rs

impl Resolver {
    fn resolve_instance(&mut self, instance: &InstanceDecl) -> Result<ResolvedInstance> {
        // Look up module definition
        let module_def = self.symbol_table
            .lookup_module(&instance.module_name)
            .ok_or_else(|| format!("Unknown module: {}", instance.module_name))?;
        
        // Create instance scope
        let instance_scope = self.symbol_table.enter_instance_scope(
            &instance.name,
            &module_def,
        );
        
        // Resolve parameter arguments
        let resolved_args = self.resolve_instance_args(
            &instance.args,
            &module_def.params,
        )?;
        
        // Resolve port mappings
        let resolved_mappings = self.resolve_port_mappings(
            &instance.port_mappings,
            &module_def,
        )?;
        
        // Resolve scoped attributes
        let resolved_attrs = self.resolve_scoped_attributes(
            &instance.attributes,
            &module_def,
        )?;
        
        self.symbol_table.exit_scope();
        
        Ok(ResolvedInstance {
            name: instance.name.clone(),
            module_def,
            args: resolved_args,
            mappings: resolved_mappings,
            attributes: resolved_attrs,
            scope_id: instance_scope,
        })
    }
}
```

## Phase 3: Analysis Passes

### 3.1 Module Connectivity Analysis

```rust
// In bhdl-analyzer/src/passes/connectivity.rs

pub struct ConnectivityAnalyzer {
    errors: Vec<AnalysisError>,
    spice_engine: SpiceEngine,
}

impl ConnectivityAnalyzer {
    pub fn analyze_module_connectivity(
        &mut self,
        instance: &ResolvedInstance,
        context: &AnalysisContext,
    ) -> Result<()> {
        // Phase 1: Pin direction checks
        for mapping in &instance.mappings {
            self.check_pin_directions(mapping, context)?;
        }
        
        // Phase 2: Electrical compatibility
        for mapping in &instance.mappings {
            self.check_voltage_levels(mapping, context)?;
            self.check_current_capacity(mapping, context)?;
        }
        
        // Phase 3: Special pin types
        self.check_open_drain_nets(instance, context)?;
        self.check_differential_pairs(instance, context)?;
        
        // Phase 4: SPICE verification
        self.run_spice_verification(instance, context)?;
        
        Ok(())
    }
    
    fn check_pin_directions(
        &mut self,
        mapping: &ResolvedPortMapping,
        context: &AnalysisContext,
    ) -> Result<()> {
        let module_pin_dir = mapping.module_pin.direction;
        let signal_dir = self.infer_signal_direction(&mapping.target, context);
        
        match (module_pin_dir, &mapping.direction, signal_dir) {
            (PinDir::In, ConnOp::LeftArrow, SignalDir::Output) => Ok(()),
            (PinDir::Out, ConnOp::RightArrow, SignalDir::Input) => Ok(()),
            (PinDir::InOut, ConnOp::Bidir, SignalDir::Bidir) => Ok(()),
            (PinDir::OpenDrain, _, SignalDir::OpenDrain) => Ok(()),
            _ => Err(self.direction_mismatch_error(mapping)),
        }
    }
}
```

### 3.2 Parameter and Attribute Validation

```rust
// In bhdl-analyzer/src/passes/parameters.rs

impl ParameterValidator {
    fn validate_instance_parameters(
        &self,
        instance: &ResolvedInstance,
    ) -> Result<()> {
        let module_params = &instance.module_def.params;
        let provided_args = &instance.args;
        
        // Check all required parameters are provided
        for param in module_params {
            if param.default.is_none() {
                if !provided_args.contains_key(&param.name) {
                    return Err(format!(
                        "Missing required parameter '{}' for module '{}'",
                        param.name, instance.module_def.name
                    ));
                }
            }
        }
        
        // Validate parameter types and constraints
        for (name, value) in provided_args {
            let param_def = module_params.iter()
                .find(|p| p.name == *name)
                .ok_or_else(|| format!("Unknown parameter: {}", name))?;
            
            self.check_type_compatibility(&value, &param_def.param_type)?;
            self.check_constraints(&value, &param_def.constraints)?;
        }
        
        Ok(())
    }
}
```

## Phase 4: Netlist Synthesis

### 4.1 Hierarchical Netlist Generation

```rust
// In bhdl-synthesizer/src/hierarchical.rs

pub struct HierarchicalSynthesizer {
    netlist: Netlist,
    module_cache: HashMap<ModuleSignature, ModuleDefinitionId>,
}

impl HierarchicalSynthesizer {
    pub fn synthesize_instance(
        &mut self,
        instance: &ResolvedInstance,
        parent_module: ModuleDefinitionId,
    ) -> Result<InstanceId> {
        // Check if identical module already synthesized
        let signature = self.compute_module_signature(instance);
        
        let module_def_id = if let Some(&cached) = self.module_cache.get(&signature) {
            cached
        } else {
            // Synthesize new module definition
            let new_def = self.synthesize_module_definition(
                &instance.module_def,
                &instance.args,
            )?;
            self.module_cache.insert(signature, new_def);
            new_def
        };
        
        // Create instance
        let instance_id = self.netlist.add_instance(
            parent_module,
            Instance {
                name: instance.name.clone(),
                module: module_def_id,
                attributes: instance.attributes.clone(),
            },
        );
        
        // Connect pins according to mappings
        for mapping in &instance.mappings {
            self.connect_instance_pin(instance_id, mapping)?;
        }
        
        Ok(instance_id)
    }
    
    fn compute_module_signature(&self, instance: &ResolvedInstance) -> ModuleSignature {
        ModuleSignature {
            module_name: instance.module_def.name.clone(),
            parameters: instance.args.clone(),
            // Don't include instance-specific attributes
        }
    }
}
```

### 4.2 Reference Designator Generation

```rust
// In bhdl-synthesizer/src/refdes.rs

pub struct RefDesGenerator {
    counters: HashMap<String, usize>,
    instance_path: Vec<String>,
}

impl RefDesGenerator {
    pub fn generate_hierarchical_refdes(
        &mut self,
        component_type: &str,
        instance_path: &[String],
    ) -> String {
        if instance_path.is_empty() {
            // Top level: R1, R2, etc.
            self.generate_simple_refdes(component_type)
        } else {
            // Hierarchical: R1_1, R1_2 for components in instance 1
            let instance_index = self.get_instance_index(&instance_path);
            let component_index = self.next_component_index(component_type);
            format!("{}{}_{}", component_type, instance_index, component_index)
        }
    }
}
```

## Phase 5: Testing Strategy

### 5.1 Unit Tests

```rust
// In bhdl-parser/src/tests/hierarchical.rs
#[test]
fn test_parse_module_with_params() {
    let input = r#"
        module VoltageRegulator(vout: voltage = 3.3V, imax: current) {
            pin VIN: power in;
            pin VOUT: power out;
        }
    "#;
    
    let parsed = parse_module(input);
    assert_eq!(parsed.params.len(), 2);
    assert_eq!(parsed.params[0].name, "vout");
    assert!(parsed.params[0].default.is_some());
}

#[test]
fn test_parse_instance_with_port_mapping() {
    let input = r#"
        reg: VoltageRegulator(vout=5V) {
            VIN <- input_power;
            VOUT -> output_rail;
        }
    "#;
    
    let parsed = parse_instance(input);
    assert_eq!(parsed.port_mappings[0].direction, ConnOp::LeftArrow);
}
```

### 5.2 Integration Tests

```rust
// In tests/hierarchical_integration.rs
#[test]
fn test_hierarchical_power_supply() {
    let bhdl_code = std::fs::read_to_string(
        "tests/circuits/hierarchical/power_supply.bhdl"
    ).unwrap();
    
    // Parse
    let ast = parse(&bhdl_code).unwrap();
    
    // Analyze
    let analysis = analyze(&ast).unwrap();
    assert!(analysis.errors.is_empty());
    
    // Check hierarchy
    assert_eq!(analysis.module_instances.len(), 2);
    assert_eq!(analysis.get_instance("buck_5v").unwrap().module_name, "BuckConverter");
    
    // Synthesize
    let netlist = synthesize(&analysis).unwrap();
    
    // Verify deduplication
    let buck_modules: Vec<_> = netlist.modules()
        .filter(|m| m.name.starts_with("BuckConverter"))
        .collect();
    assert_eq!(buck_modules.len(), 1); // Only one definition despite 2 instances
}

#[test]
fn test_voltage_level_mismatch_detection() {
    let bhdl_code = r#"
        board MixedVoltage {
            mcu_5v: MCU_5V {
                GPIO -> fpga_3v3.INPUT;  // Should error
            }
            
            fpga_3v3: FPGA_3V3 {
                INPUT <- signal;
            }
        }
    "#;
    
    let ast = parse(&bhdl_code).unwrap();
    let analysis = analyze(&ast);
    
    assert!(analysis.is_err());
    let errors = analysis.unwrap_err();
    assert!(errors[0].message.contains("Voltage level mismatch"));
    assert!(errors[0].suggestion.contains("level shifter"));
}
```

### 5.3 Example Circuits

Create comprehensive test circuits:

```bhdl
// tests/circuits/hierarchical/multi_level.bhdl
module PowerStage(voltage: voltage) {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Nested module
    controller: PWMController {
        VCC <- VIN;
        PWM -> gate_driver.IN;
    }
    
    gate_driver: GateDriver {
        IN <- controller.PWM;
        GATE -> mosfet.G;
    }
}

board System {
    stage1: PowerStage(voltage=12V) {
        VIN <- INPUT_24V;
        VOUT -> RAIL_12V;
    }
    
    stage2: PowerStage(voltage=5V) {
        VIN <- RAIL_12V;
        VOUT -> RAIL_5V;
    }
}
```

## Phase 6: Documentation and Migration

### 6.1 Update BHDL Specification

Add hierarchical module syntax to `docs/spec/BHDL_Complete_Specification.md`:
- Module definition with parameters
- Instance declaration with arguments
- Port mapping syntax (left-right convention)
- Scoped attribute syntax
- Examples

### 6.2 Migration Guide

Create `docs/migration/hierarchical_modules.md`:
- Benefits of hierarchical design
- Converting flat designs to hierarchical
- Best practices for module organization
- Parameter vs attribute guidelines

### 6.3 Update Examples

Convert existing examples to use hierarchical modules where appropriate:
- Power supply examples with reusable regulator modules
- Sensor systems with parameterized interfaces
- Communication systems with protocol modules

## Implementation Schedule

### Week 1-2: Parser and AST
- Implement grammar changes
- Add AST nodes
- Unit tests for parsing

### Week 3-4: Analysis Infrastructure  
- Symbol table extensions
- Scope management
- Name resolution

### Week 5-6: Analysis Passes
- Connectivity validation
- Parameter checking
- Electrical analysis integration

### Week 7-8: Synthesis
- Hierarchical netlist generation
- Module deduplication
- Reference designator intelligence

### Week 9-10: Testing and Documentation
- Integration tests
- Example circuits
- Documentation updates

## Success Criteria

1. **Parsing**: All hierarchical syntax parses correctly
2. **Analysis**: Catches all connectivity errors with helpful messages
3. **Synthesis**: Generates correct netlists with deduplication
4. **Performance**: No significant slowdown for flat designs
5. **Compatibility**: Existing designs continue to work
6. **Documentation**: Clear examples and migration guide

## Risks and Mitigations

1. **Risk**: Breaking existing designs
   - **Mitigation**: Extensive test suite, careful parser changes

2. **Risk**: Performance impact from deep hierarchies
   - **Mitigation**: Caching, lazy evaluation, profiling

3. **Risk**: Complex error messages
   - **Mitigation**: Hierarchical error reporting with context

4. **Risk**: SPICE analysis complexity
   - **Mitigation**: Incremental analysis, module-level caching