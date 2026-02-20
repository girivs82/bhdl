# SVG Rendering Issues - Detailed Analysis

**Date**: October 13, 2025
**Status**: Critical Issues Identified

## Executive Summary

Automated analysis of generated SVG schematics reveals **2 critical issues**:
1. ✅ **FIXED**: Component labels now match symbols correctly
2. ❌ **CRITICAL**: No net connections rendered - components appear disconnected
3. ⚠️  **WARNING**: Component overlap in stdlib circuit

## Issue Analysis

### Issue 1: No Net Routing ❌ CRITICAL

**Symptom**: All nets have 0 connection points, so no wires are drawn between components.

**Evidence**:
```
[DEBUG] Routing net: Some("net_r1_...") (NetId(9v1))
[DEBUG]   Net has 2 connections in netlist
[DEBUG]   PinInstance: pin_inst_id=PinInstanceId(13v1)
[DEBUG]     Resolved to instance_id=InstanceId(13v1), pin_def=PinId(13v1)
[DEBUG]     WARNING: Component for instance InstanceId(13v1) not found in layout
[DEBUG]   Total connection points found: 0
```

**Root Cause 1 (FIXED)**: Wrong connection type - code was looking for `InstancePin` but synthesizer creates `PinInstance`.
- ✅ Fixed by handling both `ConnectionPoint::InstancePin` and `ConnectionPoint::PinInstance`

**Root Cause 2 (IDENTIFIED)**: Synthesizer creates ONE DatabaseComponentInstance per MODULE TYPE, not per instance.
- Netlist has 7 component instances: tvs, c1, c2, reg, r1, sense, led
- Database components array only has 4 entries: LED, TVSDiode, Cap, Res
- Multiple instances of same type (c1/c2, r1/sense) share ONE database entry
- Layout only places 4 components instead of 7
- Routing fails because 3 components were never placed

**Evidence**:
```
Netlist instances: 7 (tvs, c1, c2, reg, r1, sense, led)
Database components: 4 (LED, TVSDiode, Cap, Res)
Placed components: 4 (only one per type)
```

**Location**:
- Synthesizer: `bhdl-synthesizer/src/lib.rs` (component_instances generation)
- Visualizer: `bhdl-visualizer/src/layout.rs:475-476` (iterates over database components instead of netlist instances)

**Solution**: Layout must iterate over netlist instances, not database components, and look up database info by module type.

**Impact**:
- Components are placed but appear disconnected
- Circuit functionality cannot be understood from the schematic
- Makes the visualization essentially useless

**Recommended Fixes**:

1. **Short-term**: Add pin name aliasing
   ```rust
   // Try multiple pin name formats
   let pin_candidates = vec![
       pin.name.clone(),                    // Original name
       pin.name.to_uppercase(),             // Uppercase variant
       format!("pin{}", pin.name),          // With prefix
       // Check pin mapping from database
   ];
   ```

2. **Medium-term**: Standardize pin naming in symbol manager
   - When creating components, use consistent pin names
   - Map database pin numbers/names to netlist pin names
   - Use the `pin_mapping` field from `DatabaseComponentInstance`

3. **Long-term**: Add pin resolution layer
   - Create explicit pin name resolution system
   - Support multiple naming conventions
   - Log mismatches for debugging

### Issue 2: Component Overlap ⚠️ WARNING

**Symptom**: ElectrolyticCap and TVSDiode are too close (distance: 24.0 pixels)

**Evidence**:
```
⚠️  Component Overlaps: 1
  - ElectrolyticCap ↔ TVSDiode (distance: 24.0)
```

**Root Cause**: Intent-driven placement algorithm doesn't account for component size

**Location**: `bhdl-visualizer/src/layout.rs:488-509`

**Impact**:
- Components visually overlap
- Hard to distinguish individual components
- Unprofessional appearance

**Recommended Fix**:
```rust
// In place_intent_driven_circuit(), use actual component sizes
let component_size_x = 30.0;  // Should come from symbol
let component_size_y = 20.0;
let min_spacing = (component_size_x + component_size_y) / 2.0 + 10.0;  // Add margin

// Adjust spacing calculation
let y_offset = (*zone_index as f64 - zone_count as f64 / 2.0) * min_spacing;
```

## Test Results

### Simple Circuit (`test_intent_simple_demo.bhdl`)
- ✅ 4 components placed correctly
- ✅ No overlaps
- ✅ Labels match symbols
- ❌ 0 nets rendered (should be 4-6 nets)

### Stdlib Circuit (`test_intent_with_simple_stdlib.bhdl`)
- ✅ 6 components placed correctly
- ⚠️  1 overlap (ElectrolyticCap ↔ TVSDiode)
- ✅ Labels match symbols
- ❌ 0 nets rendered (should be 8-12 nets)

## Priority Actions

1. **URGENT**: Fix pin name matching to enable net routing
   - Debug why `component.get_pin_world_position(&pin.name)` returns None
   - Add logging to show what pin names are in component.pins HashMap
   - Implement pin name aliasing as temporary fix

2. **HIGH**: Fix component overlap in intent-driven layout
   - Use actual component dimensions
   - Increase minimum spacing between components

3. **MEDIUM**: Add comprehensive routing debugging
   - Log all pin lookups (successful and failed)
   - Show netlist pin names vs component pin names
   - Create diagnostic tool to validate pin mappings

## Next Steps

1. Run with maximum debug logging to see pin names:
   ```bash
   RUST_LOG=bhdl_visualizer::layout=trace cargo run -p bhdl-cli --bin bhdl-cli \
       tests/circuits/simple/test_intent_simple_demo.bhdl visualize -o /dev/null 2>&1 | \
       grep -E "(Component pins available|Pin from netlist)"
   ```

2. Check SymbolManager to understand how pins are populated:
   ```rust
   // In create_component(), how are pins added to component.pins HashMap?
   ```

3. Implement pin name resolution:
   ```rust
   fn resolve_pin_name(
       component: &Component,
       netlist_pin_name: &str,
       db_component: Option<&DatabaseComponentInstance>
   ) -> Option<String> {
       // Try direct lookup
       if component.pins.contains_key(netlist_pin_name) {
           return Some(netlist_pin_name.to_string());
       }

       // Try pin mapping from database
       if let Some(db_comp) = db_component {
           if let Some(db_pin) = db_comp.pin_mapping.get(netlist_pin_name) {
               return Some(db_pin.clone());
           }
       }

       // Try common aliases
       // ... more resolution logic

       None
   }
   ```

## Conclusion

The visualization system has a **critical data pipeline issue** preventing net routing. Components are placed and labeled correctly, but the lack of connections makes the schematics unusable.

**Estimated fix time**: 2-4 hours for pin matching fix
**Priority**: P0 - Blocks all useful visualization output
