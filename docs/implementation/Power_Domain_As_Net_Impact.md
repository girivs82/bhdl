# Impact Analysis: Power Domains as Nets

## Current Implementation

The current implementation treats power domains as special entities:

1. **AST Level**: `PowerDecl` and `GroundDecl` are separate AST nodes
2. **Analyzer**: Power analysis extracts voltage, current, and other attributes
3. **Symbol Table**: Power domains stored separately from regular nets
4. **Usage**: Referenced without @ prefix (e.g., `VCC`, `GND`)

## Proposed Change: Power Domains ARE Nets

Treat power domains as nets with metadata:
```bhdl
power VCC = 5V @ 1A;    // Creates net @VCC with power attributes
ground GND;             // Creates net @GND with ground attribute
```

## Impact Analysis

### 1. Parser - MINIMAL Impact

**Current**:
```rust
parse_power_decl() -> PowerDecl AST node
```

**Proposed**:
```rust
parse_power_decl() -> PowerDecl AST node (unchanged)
// But semantically it creates a net with attributes
```

No parser changes needed initially - just semantic interpretation changes.

### 2. Analyzer - MODERATE Impact

**Current**:
```rust
// Pass 1: Collect power domains
if let Some(power_decl) = PowerDecl::cast(node) {
    let name = power_decl.name();
    let domain = PowerDomain::new(name, voltage);
    power_analysis.add_domain(domain);
}
```

**Proposed**:
```rust
// Pass 1: Power declarations create nets with attributes
if let Some(power_decl) = PowerDecl::cast(node) {
    let name = power_decl.name();
    
    // Create net symbol with power attributes
    let mut net_symbol = Symbol::new_net(name);
    net_symbol.add_attribute("power_domain", true);
    net_symbol.add_attribute("voltage", voltage);
    net_symbol.add_attribute("current", current);
    
    symbol_table.add_symbol(name, net_symbol);
    
    // Still track in power analysis for compatibility
    let domain = PowerDomain::new(name, voltage);
    power_analysis.add_domain(domain);
}
```

### 3. Power Analysis - NO Impact

The PowerAnalysisContext can continue to work exactly as before:
- It still tracks PowerDomain structs
- Level shifter logic unchanged
- Power sequencing unchanged
- Just populated from net symbols instead of special declarations

### 4. Symbol Resolution - MINOR Impact

**Current**:
```rust
// Resolve identifier
if let Some(symbol) = symbol_table.lookup(name) {
    // Found regular symbol
} else if power_analysis.get_domain(name).is_some() {
    // Found power domain
}
```

**Proposed**:
```rust
// Resolve identifier - must use @
if name.starts_with('@') {
    let net_name = &name[1..];
    if let Some(symbol) = symbol_table.lookup_net(net_name) {
        // Found net (including power nets)
    }
}
```

### 5. Netlist Generation - MINOR Impact

Power nets are already nets in the netlist, so minimal change:
- Just ensure power net attributes are preserved
- Net naming might need @ prefix handling

### 6. SPICE Integration - NO Impact

SPICE already treats power rails as nets with voltage sources.

## Implementation Strategy

### Phase 1: Dual Support (No Breaking Changes)

1. Power declarations create BOTH:
   - PowerDomain entry (for compatibility)
   - Net symbol with attributes (new)

2. Allow both syntaxes:
   ```bhdl
   VCC -> Res(10k).1;      // Old syntax (deprecated)
   @VCC -> Res(10k).1;     // New syntax (preferred)
   ```

3. Symbol resolution checks both locations

### Phase 2: Migration

1. Update all examples to use @ syntax
2. Add deprecation warnings for non-@ usage
3. Update documentation

### Phase 3: Cleanup

1. Remove special power domain handling in symbol resolution
2. Simplify to single path: all nets use @

## Benefits

1. **Conceptual Clarity**: Power domains are just nets with metadata
2. **Syntax Consistency**: @ for all nets
3. **Simpler Implementation**: One symbol type instead of two
4. **Better Tooling**: IDEs can treat all nets uniformly

## Risks and Mitigation

**Risk**: Existing code breaks
**Mitigation**: Phase approach with backward compatibility

**Risk**: Power analysis becomes complex
**Mitigation**: PowerAnalysisContext unchanged, just populated differently

**Risk**: User confusion during transition
**Mitigation**: Clear migration guide and tool support

## Conclusion

The change is beneficial and manageable:
- Parser needs NO immediate changes
- Analyzer needs moderate updates to create net symbols
- Power analysis continues to work unchanged
- Gradual migration path available

The key insight: PowerAnalysisContext remains the same - it's just populated from net symbols instead of special power declarations. This preserves all downstream functionality while simplifying the mental model.