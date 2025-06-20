# Hierarchical Modules Implementation Plan

## Overview

Enable modules to contain component instances, module instances, and connections - creating true hierarchical designs. This is a fundamental feature that must be implemented before behavioral modeling.

## Design Goals

1. **Full Hierarchy**: Modules can contain components, other modules, and connections
2. **Scoped Names**: Proper instance paths like `board.power.regulator.controller`
3. **Reusability**: Create library modules that can be instantiated multiple times
4. **Clean Syntax**: Natural extension of existing BHDL v2.0 flow syntax

## Syntax Design

### Current (Limited) Module Syntax
```bhdl
module BuckController {
    pin VIN: power in;
    pin VOUT: power out;
    pin FB: analog in;
    pin EN: digital in;
    
    // That's it - no internal structure!
}
```

### Proposed Hierarchical Module Syntax
```bhdl
module BuckConverter(vout: voltage = 3.3V) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    
    // Power and ground declarations (module-scoped)
    power VCC = VIN;  // Internal power net
    ground GND;
    
    // Component instances
    controller: TPS54302 {
        VCC -> .VIN;
        .EN -> EN;
        .FB -> feedback_point;
        .SW -> switch_node;
    }
    
    // Passive components
    L1: Inductor(10uH) {
        switch_node -> .1;
        VOUT -> .2;
    }
    
    D1: SchottkyDiode(SS34) {
        GND -> .A;
        switch_node -> .K;
    }
    
    // Feedback network
    R1: Res(10k) {
        VOUT -> .1;
        feedback_point -> .2;
    }
    
    R2: Res(3.3k) {
        feedback_point -> .1;
        GND -> .2;
    }
    
    // Output filtering
    C1: Cap(100uF) {
        VOUT -> .1;
        GND -> .2;
    }
    
    // Direct connections
    VCC -> C_in: Cap(10uF).1 -> GND;
}
```

### Nested Module Example
```bhdl
module PowerSupply {
    pin VIN: power in;
    pin V3V3, V5V0, V1V2: power out;
    pin PGOOD: digital out;
    
    ground GND;
    
    // Input protection
    input_stage: InputProtection {
        VIN -> .IN;
        .OUT -> protected_vin;
        .GND -> GND;
    }
    
    // Multiple regulators
    buck_3v3: BuckConverter(vout=3.3V) {
        protected_vin -> .VIN;
        .VOUT -> V3V3;
        .EN -> enable_3v3;
    }
    
    buck_5v0: BuckConverter(vout=5.0V) {
        protected_vin -> .VIN;
        .VOUT -> V5V0;
        .EN -> enable_5v0;
    }
    
    // LDO for low-noise 1.2V
    ldo_1v2: LDO_TPS7A47 {
        V3V3 -> .VIN;
        .VOUT -> V1V2;
        .EN -> enable_1v2;
    }
    
    // Power sequencing
    sequencer: PowerSequencer {
        .EN_IN -> main_enable;
        .EN0 -> enable_3v3;
        .EN1 -> enable_5v0;
        .EN2 -> enable_1v2;
        .ALL_GOOD -> PGOOD;
    }
}
```

## Pipeline Changes Required

### 1. Parser (`bhdl-parser`)

**Files to modify:**
- `src/grammar.rs`
- `src/syntax_kind.rs`

**Changes needed:**
```rust
// In grammar.rs - extend parse_module_body()
fn parse_module_body(p: &mut Parser) {
    p.expect(T!['{']);
    
    while !p.at(T!['}']) && !p.at(EOF) {
        match p.current() {
            T![pin] => parse_pin_decl(p),
            T![power] => parse_power_decl(p),      // NEW
            T![ground] => parse_ground_decl(p),     // NEW
            T![attribute] => parse_attribute(p),     // NEW
            T![signal] => parse_signal_decl(p),     // NEW
            T![ident] => {
                // Could be:
                // - instance: Type
                // - instance: Type()
                // - instance: Type {}
                // - net -> component
                parse_instance_or_connection(p);     // NEW
            }
            _ => {
                // Connection statements
                parse_connection_stmt(p);            // NEW
            }
        }
    }
    
    p.expect(T!['}']);
}

// New parsing functions
fn parse_instance_or_connection(p: &mut Parser) {
    // Look ahead to determine if this is:
    // 1. "name: Type" - instance declaration
    // 2. "net -> ..." - connection
    
    let checkpoint = p.checkpoint();
    p.expect(T![ident]);
    
    if p.at(T![:]) {
        // Instance declaration
        parse_instance_decl(p, checkpoint);
    } else if p.at(T![->]) || p.at(T![<->]) {
        // Connection starting with identifier
        parse_connection_from_checkpoint(p, checkpoint);
    }
}
```

### 2. AST (`bhdl-ast`)

**Files to modify:**
- `src/items.rs` - Update Module struct
- `src/common.rs` - May need updates for module instances

**Changes needed:**
```rust
// In items.rs - extend Module implementation
impl Module {
    // Existing
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> { ... }
    pub fn attributes(&self) -> impl Iterator<Item = AttributeDecl> { ... }
    
    // NEW - Add these methods
    pub fn power_decls(&self) -> impl Iterator<Item = PowerDecl> {
        self.0.children().filter_map(PowerDecl::cast)
    }
    
    pub fn ground_decls(&self) -> impl Iterator<Item = GroundDecl> {
        self.0.children().filter_map(GroundDecl::cast)
    }
    
    pub fn signal_decls(&self) -> impl Iterator<Item = SignalDecl> {
        self.0.children().filter_map(SignalDecl::cast)
    }
    
    pub fn instances(&self) -> impl Iterator<Item = InstanceDecl> {
        self.0.children().filter_map(InstanceDecl::cast)
    }
    
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
}

// NEW - Instance declaration node
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceDecl(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InstanceDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { 
        kind == SyntaxKind::INSTANCE_DECL 
    }
    // ... standard AstNode implementation
}

impl InstanceDecl {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> { ... }
    pub fn type_name(&self) -> Option<SyntaxToken<BhdlLanguage>> { ... }
    pub fn is_module(&self) -> bool { ... }
    pub fn is_component(&self) -> bool { ... }
    pub fn params(&self) -> Option<ParamList> { ... }
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> { ... }
}
```

### 3. Analyzer (`bhdl-analyzer`)

**Files to modify:**
- `src/passes/collect_definitions.rs` - Handle nested scopes
- `src/passes/resolve_references.rs` - Resolve hierarchical paths
- `src/symbol_table.rs` - Support nested symbol tables
- `src/scope.rs` - Hierarchical scope management

**Changes needed:**

#### 3.1 Symbol Table Enhancement
```rust
// In symbol_table.rs
pub struct SymbolTable {
    // Existing
    symbols: HashMap<String, Symbol>,
    
    // NEW - Support hierarchy
    children: HashMap<String, SymbolTable>,  // Child scopes
    parent: Option<Weak<RefCell<SymbolTable>>>,  // Parent scope
}

impl SymbolTable {
    // NEW - Hierarchical lookup
    pub fn lookup_hierarchical(&self, path: &[String]) -> Option<&Symbol> {
        match path {
            [] => None,
            [name] => self.symbols.get(name),
            [first, rest @ ..] => {
                self.children.get(first)
                    .and_then(|child| child.lookup_hierarchical(rest))
            }
        }
    }
    
    // NEW - Add child scope
    pub fn add_child_scope(&mut self, name: String) -> &mut SymbolTable {
        self.children.entry(name).or_insert_with(SymbolTable::new)
    }
}
```

#### 3.2 Scope Resolution
```rust
// In scope.rs - NEW file
pub struct HierarchicalScope {
    path: Vec<String>,  // Current path in hierarchy
    symbol_table: Rc<RefCell<SymbolTable>>,
}

impl HierarchicalScope {
    pub fn enter_module(&mut self, name: String) {
        self.path.push(name.clone());
        let table = self.symbol_table.borrow_mut();
        let child = table.add_child_scope(name);
        // Update current symbol table reference
    }
    
    pub fn exit_module(&mut self) {
        self.path.pop();
        // Restore parent symbol table reference
    }
    
    pub fn current_path(&self) -> String {
        self.path.join(".")
    }
}
```

#### 3.3 Pass Updates
```rust
// In collect_definitions.rs
impl Pass for CollectDefinitions {
    fn visit_module(&mut self, module: &Module) {
        // Create new scope for module
        self.scope.enter_module(module.name());
        
        // Process module contents
        for pin in module.pins() {
            self.add_pin_to_scope(pin);
        }
        
        // NEW - Process instances
        for instance in module.instances() {
            self.add_instance_to_scope(instance);
        }
        
        // NEW - Process module-level signals
        for signal in module.signal_decls() {
            self.add_signal_to_scope(signal);
        }
        
        // Exit module scope
        self.scope.exit_module();
    }
}
```

### 4. Synthesizer (`bhdl-synthesizer`)

**Files to modify:**
- `src/netlist_builder.rs` - Handle hierarchical instantiation
- `src/instance_path.rs` - NEW file for path management

**Changes needed:**

#### 4.1 Instance Path Management
```rust
// NEW file: src/instance_path.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstancePath {
    segments: Vec<String>,
}

impl InstancePath {
    pub fn new() -> Self {
        Self { segments: vec![] }
    }
    
    pub fn push(&mut self, segment: String) {
        self.segments.push(segment);
    }
    
    pub fn pop(&mut self) {
        self.segments.pop();
    }
    
    pub fn to_string(&self) -> String {
        self.segments.join(".")
    }
    
    pub fn child(&self, name: String) -> Self {
        let mut child = self.clone();
        child.push(name);
        child
    }
}
```

#### 4.2 Netlist Builder Updates
```rust
// In netlist_builder.rs
pub struct NetlistBuilder {
    // Existing
    netlist: Netlist,
    
    // NEW
    current_path: InstancePath,
    module_stack: Vec<ModuleContext>,
}

impl NetlistBuilder {
    pub fn enter_module_instance(&mut self, instance_name: String, module: &Module) {
        self.current_path.push(instance_name.clone());
        
        // Create module context
        let context = ModuleContext {
            module_def: module.clone(),
            instance_name,
            local_nets: HashMap::new(),
        };
        
        self.module_stack.push(context);
    }
    
    pub fn exit_module_instance(&mut self) {
        self.current_path.pop();
        self.module_stack.pop();
    }
    
    pub fn add_component_instance(&mut self, name: String, component_type: String) {
        let full_path = self.current_path.child(name);
        let instance_id = self.netlist.add_instance(
            full_path.to_string(),
            component_type,
        );
        
        // Track in current module context
        if let Some(context) = self.module_stack.last_mut() {
            context.instances.insert(name, instance_id);
        }
    }
}
```

### 5. Netlist (`bhdl-netlist`)

**Files to modify:**
- `src/lib.rs` - Enhance Instance to track hierarchy

**Changes needed:**
```rust
// In lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    pub component: String,
    pub pins: HashMap<String, PinId>,
    
    // NEW fields
    pub instance_path: String,  // Full hierarchical path
    pub parent_module: Option<ModuleId>,  // Parent module instance
    pub is_module_instance: bool,  // Module vs component
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Net {
    pub name: String,
    pub net_type: NetType,
    pub connections: Vec<Connection>,
    
    // NEW field
    pub scope_path: String,  // Module scope this net belongs to
}
```

### 6. Visualizer (`bhdl-visualizer`)

**Files to modify:**
- `src/layout/semantic_layout.rs` - Group by hierarchy
- `src/symbols/module_symbol.rs` - NEW file for module symbols

**Changes needed:**

#### 6.1 Module Symbol
```rust
// NEW file: src/symbols/module_symbol.rs
pub struct ModuleSymbol {
    bounds: Rectangle,
    label: String,
    subcircuit: Option<Box<Layout>>,  // Nested layout
}

impl ModuleSymbol {
    pub fn new(module_name: &str, contents: Layout) -> Self {
        // Calculate bounds based on contents
        let bounds = contents.calculate_bounds().expand(20.0);
        
        Self {
            bounds,
            label: module_name.to_string(),
            subcircuit: Some(Box::new(contents)),
        }
    }
    
    pub fn render_svg(&self) -> svg::node::element::Group {
        let mut group = Group::new();
        
        // Module box
        let rect = Rectangle::new()
            .set("x", self.bounds.x)
            .set("y", self.bounds.y)
            .set("width", self.bounds.width)
            .set("height", self.bounds.height)
            .set("fill", "none")
            .set("stroke", "black")
            .set("stroke-width", 2);
            
        // Module label
        let text = Text::new()
            .set("x", self.bounds.x + 5)
            .set("y", self.bounds.y - 5)
            .add(svg::node::Text::new(&self.label));
            
        group = group.add(rect).add(text);
        
        // Render contents
        if let Some(contents) = &self.subcircuit {
            group = group.add(contents.render_svg());
        }
        
        group
    }
}
```

#### 6.2 Hierarchical Layout
```rust
// In semantic_layout.rs
impl SemanticLayout {
    pub fn layout_hierarchical(&mut self, netlist: &Netlist) {
        // Group instances by parent module
        let hierarchy = self.build_hierarchy_tree(netlist);
        
        // Layout each module recursively
        self.layout_module_recursive(&hierarchy.root);
    }
    
    fn layout_module_recursive(&mut self, module: &ModuleNode) {
        if module.children.is_empty() {
            // Leaf module - layout components
            self.layout_components(&module.instances);
        } else {
            // Layout child modules first
            for child in &module.children {
                self.layout_module_recursive(child);
            }
            
            // Then layout this module with children as blocks
            self.layout_with_submodules(module);
        }
    }
}
```

## Implementation Phases

### Phase 1: Parser & AST (Week 1)
1. Extend module grammar to support instances and connections
2. Add InstanceDecl AST node
3. Update Module AST to expose new elements
4. Write parser tests for hierarchical modules

### Phase 2: Analyzer Support (Week 2)
1. Implement hierarchical symbol tables
2. Update Pass 1 to build nested scopes
3. Update Pass 2 to resolve hierarchical references
4. Add scope path tracking

### Phase 3: Synthesis (Week 3)
1. Implement instance path management
2. Update netlist builder for hierarchy
3. Handle module instantiation vs component instantiation
4. Preserve hierarchy in netlist structure

### Phase 4: Visualization (Week 4)
1. Create module symbol rendering
2. Implement hierarchical layout algorithm
3. Handle cross-hierarchy routing
4. Add expand/collapse for module views

## Testing Strategy

### Parser Tests
```bhdl
// Test nested module parsing
module Parent {
    child1: Child {
        input -> .in;
        .out -> output;
    }
    
    child2: Child {
        input2 -> .in;
        .out -> output2;
    }
}
```

### Analyzer Tests
- Verify hierarchical symbol resolution
- Test cross-module net connections
- Validate scope isolation

### End-to-End Tests
```bhdl
board TestBoard {
    power VCC = 5V;
    ground GND;
    
    // Hierarchical instantiation
    psu: PowerSupply {
        VCC -> .VIN;
        .V3V3 -> mcu.VDD;
        .V1V2 -> mcu.VCORE;
    }
    
    mcu: MCU_Module {
        .VDD -> Cap(100nF).1 -> GND;
    }
}
```

## Migration Guide

### For Existing BHDL Code
- Boards remain unchanged
- Flat modules remain valid
- New hierarchical features are additive

### New Best Practices
1. Group related components into modules
2. Create reusable library modules
3. Use meaningful instance names
4. Keep hierarchy depth reasonable (2-3 levels)

## Benefits

1. **Reusability**: Write once, instantiate many times
2. **Organization**: Logical grouping of functionality
3. **Abstraction**: Hide implementation details
4. **Scalability**: Build complex systems from simple modules
5. **Testing**: Test modules in isolation