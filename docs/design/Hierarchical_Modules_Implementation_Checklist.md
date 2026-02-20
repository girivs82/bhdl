# Hierarchical Modules Implementation Checklist

## Quick Reference for Implementation

### Phase 1: Parser & AST ✓ Checklist

#### Parser Grammar (`bhdl-parser/src/grammar.rs`)
- [ ] Add `parse_instance_decl()` function
- [ ] Extend `parse_module_body()` to handle:
  - [ ] `power` declarations
  - [ ] `ground` declarations  
  - [ ] `signal` declarations
  - [ ] Component/module instances
  - [ ] Connection statements
- [ ] Add lookahead logic to distinguish:
  - [ ] `name: Type` (instance)
  - [ ] `name -> ...` (connection)
- [ ] Update `SyntaxKind` enum with:
  - [ ] `INSTANCE_DECL`
  - [ ] `MODULE_INSTANCE`
  - [ ] `SIGNAL_DECL`

#### AST Nodes (`bhdl-ast/src/items.rs`)
- [ ] Create `InstanceDecl` struct with:
  - [ ] `name()` method
  - [ ] `type_name()` method
  - [ ] `is_module()` method
  - [ ] `params()` method
  - [ ] `connections()` method
- [ ] Extend `Module` impl with:
  - [ ] `power_decls()`
  - [ ] `ground_decls()`
  - [ ] `signal_decls()`
  - [ ] `instances()`
  - [ ] `connections()`

### Phase 2: Analyzer ✓ Checklist

#### Symbol Table (`bhdl-analyzer/src/symbol_table.rs`)
- [ ] Add hierarchical structure:
  - [ ] `children: HashMap<String, SymbolTable>`
  - [ ] `parent: Option<Weak<RefCell<SymbolTable>>>`
- [ ] Implement `lookup_hierarchical(path: &[String])`
- [ ] Implement `add_child_scope(name: String)`

#### Scope Management (NEW: `bhdl-analyzer/src/scope.rs`)
- [ ] Create `HierarchicalScope` struct
- [ ] Track current path in hierarchy
- [ ] Implement `enter_module()` / `exit_module()`
- [ ] Support dot-notation path resolution

#### Pass Updates
- [ ] Pass 1 (`collect_definitions.rs`):
  - [ ] Create child scopes for modules
  - [ ] Track instance definitions
  - [ ] Build hierarchy tree
- [ ] Pass 2 (`resolve_references.rs`):
  - [ ] Resolve hierarchical paths (e.g., `module1.signal1`)
  - [ ] Validate instance types exist
  - [ ] Check port connections

### Phase 3: Synthesizer ✓ Checklist

#### Instance Paths (NEW: `bhdl-synthesizer/src/instance_path.rs`)
- [ ] Create `InstancePath` struct
- [ ] Support path operations (push/pop/join)
- [ ] Generate unique hierarchical names

#### Netlist Builder (`bhdl-synthesizer/src/netlist_builder.rs`)
- [ ] Add module instantiation support:
  - [ ] `enter_module_instance()`
  - [ ] `exit_module_instance()`
  - [ ] Track module context stack
- [ ] Generate hierarchical instance names
- [ ] Handle port-to-net mapping
- [ ] Support both component and module instances

### Phase 4: Netlist Updates ✓ Checklist

#### Data Model (`bhdl-netlist/src/lib.rs`)
- [ ] Extend `Instance` with:
  - [ ] `instance_path: String`
  - [ ] `parent_module: Option<ModuleId>`
  - [ ] `is_module_instance: bool`
- [ ] Extend `Net` with:
  - [ ] `scope_path: String`
- [ ] Add methods for hierarchy traversal

### Phase 5: Visualizer ✓ Checklist

#### Module Symbols (NEW: `bhdl-visualizer/src/symbols/module_symbol.rs`)
- [ ] Create `ModuleSymbol` struct
- [ ] Render entity as box with label
- [ ] Support nested content rendering
- [ ] Handle pin connection points

#### Hierarchical Layout (`bhdl-visualizer/src/layout/semantic_layout.rs`)
- [ ] Build hierarchy tree from netlist
- [ ] Implement recursive layout algorithm
- [ ] Group components by parent entity
- [ ] Handle cross-entity routing

## Test Cases to Implement

### Parser Tests
```bhdl
// test_parse_simple_entity_instance
entity Container {
    child: ChildModule {
        input -> .in;
        .out -> output;
    }
}

// test_parse_mixed_instances
entity Mixed {
    R1: Res(10k) { ... }
    mod1: SubModule { ... }
    C1: Cap(100nF) { ... }
}

// test_parse_nested_connections
entity Parent {
    child.internal_signal -> R1.1;
    child.submodule.pin -> output;
}
```

### Analyzer Tests
```rust
// test_hierarchical_symbol_resolution
#[test]
fn test_resolve_nested_module_reference() {
    let source = r#"
        entity Child {
            signal internal = 3.3V;
        }
        entity Parent {
            c: Child {};
            signal test = c.internal;
        }
    "#;
    // Verify c.internal resolves correctly
}

// test_instance_name_conflicts
// test_cross_module_connections
// test_undefined_module_type
```

### End-to-End Tests
```bhdl
// Full hierarchical design test
board TestSystem {
    psu: PowerSupply {
        VIN -> .IN;
        .V3V3 -> system.VDD;
    }
    
    system: DigitalSystem {
        .CLK -> clk_gen.OUT;
    }
}
```

## Critical Path Items

1. **Parser Grammar** - Must handle instance vs connection disambiguation
2. **Symbol Table Hierarchy** - Foundation for everything else
3. **Instance Path Tracking** - Needed for unique naming
4. **Module Context Stack** - For nested instantiation

## Migration Risks to Address

1. **Existing Tests** - May need updates for new AST structure
2. **Netlist Format** - Ensure backward compatibility
3. **Performance** - Hierarchical lookup overhead
4. **Memory Usage** - Nested symbol tables

## Success Criteria

- [ ] Can parse hierarchical entities without ambiguity
- [ ] Symbol resolution works across entity boundaries
- [ ] Netlist preserves full hierarchy
- [ ] Visualizer shows entity boxes with contents
- [ ] All existing tests still pass
- [ ] Performance overhead < 10% for flat designs
- [ ] Example USB charger system processes correctly

## Next Steps After Implementation

1. **Behavioral Modeling** - Can now use proper instance paths
2. **Library System** - Package reusable entities
3. **Incremental Compilation** - Compile entities independently
4. **Entity Testing** - Test entities in isolation