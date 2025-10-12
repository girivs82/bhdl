# Advanced Pattern Matching - Design Document

**Date**: October 12, 2025
**Status**: Design Phase
**Priority**: Medium (Item #5 in NEXT_STEPS.md)

## Overview

This document outlines the design for extending BHDL's wildcard pattern matching system to support more sophisticated filtering and selection patterns. This feature builds on the existing wildcard infrastructure (items #2, #3, #4) and adds four new pattern types.

## Motivation

The current wildcard system supports:
- **Wildcard**: `sensor[*].VCC` - matches all instances
- **Simple range**: `fpga.VCCO[0..7]` - matches indices 0 through 7
- **Hierarchical wildcards**: `module[*].component.pin` - crosses module boundaries

These cover many use cases, but designers often need more fine-grained control:
- Connecting power only to even-numbered instances (for differential pairs)
- Selecting specific non-contiguous indices for specialized routing
- Stepped ranges for sampling or interleaving patterns

## Proposed Syntax

### 1. Even/Odd Keywords
```bhdl
power_domain @VCC = 3.3V @ 5A {
    distribution {
        sensor[even].VCC;  // Only sensor_0, sensor_2, sensor_4, sensor_6
        sensor[odd].VCC;   // Only sensor_1, sensor_3, sensor_5, sensor_7
    }
}
```

**Use Cases**:
- Differential pair routing (odd pairs on one rail, even pairs on another)
- Interleaved power domains for thermal management
- Redundant system architectures

### 2. Explicit Index Lists
```bhdl
power_domain @VCC_SPECIAL = 3.3V @ 1A {
    distribution {
        sensor[0,2,4,8].VCC;  // Only specific indices
    }
}
```

**Use Cases**:
- Custom pin mappings from external constraints
- Selective population of instances
- Non-contiguous groupings

### 3. Stepped Ranges
```bhdl
power_domain @VCC_A = 3.3V @ 5A {
    distribution {
        sensor[0..15:2].VCC;   // 0, 2, 4, 6, 8, 10, 12, 14
        sensor[1..15:2].VCC;   // 1, 3, 5, 7, 9, 11, 13, 15
    }
}
```

**Syntax**: `[start..end:step]`
- `start`: Starting index (inclusive)
- `end`: Ending index (inclusive)
- `step`: Step size (default 1 if omitted)

**Use Cases**:
- Sampling patterns
- Interleaved routing
- Phased array systems

### 4. Combination Patterns (Future Enhancement)
```bhdl
// Not in scope for initial implementation
sensor[even, 7, 9].VCC;      // Combination of even + explicit indices
sensor[0..10:2, 15, 20].VCC; // Combination of stepped range + explicit
```

## Architecture

### Layer 1: Parser Enhancement

**Current State** (bhdl-parser/src/top_level.rs):
```rust
if self.peek() == Some(SyntaxKind::STAR) {
    // Wildcard: [*]
    self.bump();
} else {
    // Range: [0..7]
    self.parse_expression(); // Start index
    if self.peek() == Some(SyntaxKind::DOT_DOT) {
        self.bump(); // ..
        self.parse_expression(); // End index
    }
}
```

**Proposed Enhancement**:
```rust
if self.peek() == Some(SyntaxKind::STAR) {
    // Wildcard: [*]
    self.bump();
} else if self.peek() == Some(SyntaxKind::IDENT) {
    // Check for keywords: even, odd
    let checkpoint = self.builder.checkpoint();
    let ident_text = self.tokens[self.pos].1.clone();

    if ident_text == "even" || ident_text == "odd" {
        self.builder.start_node_at(checkpoint, SyntaxKind::PATTERN_KEYWORD.into());
        self.bump(); // Consume keyword
        self.builder.finish_node();
    } else {
        self.error("Unknown pattern keyword".to_string());
    }
} else {
    // Range or list
    self.parse_pattern_range_or_list();
}
```

**New Helper Method**:
```rust
fn parse_pattern_range_or_list(&mut self) {
    self.builder.start_node(SyntaxKind::PATTERN_INDICES.into());

    // Parse first expression
    self.parse_expression();

    // Check what follows
    match self.peek() {
        Some(SyntaxKind::DOT_DOT) => {
            // Range: [0..7] or [0..7:2]
            self.bump(); // ..
            self.parse_expression(); // End

            // Check for step
            if self.peek() == Some(SyntaxKind::COLON) {
                self.bump(); // :
                self.parse_expression(); // Step
            }
        }
        Some(SyntaxKind::COMMA) => {
            // List: [0,2,4,8]
            while self.peek() == Some(SyntaxKind::COMMA) {
                self.bump(); // ,
                self.parse_expression(); // Next index
            }
        }
        _ => {
            // Single index: [5]
        }
    }

    self.builder.finish_node();
}
```

**New SyntaxKind Tokens**:
- `PATTERN_KEYWORD` - For "even" and "odd"
- `PATTERN_INDICES` - For explicit lists and stepped ranges

### Layer 2: AST Extension

**New Methods on** `DistributionPinList`:

```rust
impl DistributionPinList {
    /// Get the pattern type from the bracket notation
    pub fn pattern_type(&self) -> PatternType {
        // Analyze syntax tree to determine pattern type
    }

    /// Extract pattern parameters based on type
    pub fn pattern_params(&self) -> PatternParams {
        // Extract indices, keywords, ranges, steps
    }
}

/// Pattern type classification
#[derive(Debug, Clone, PartialEq)]
pub enum PatternType {
    Wildcard,               // [*]
    SimpleRange(i32, i32),  // [0..7]
    SteppedRange(i32, i32, i32), // [0..15:2]
    ExplicitList(Vec<i32>), // [0,2,4,8]
    EvenKeyword,            // [even]
    OddKeyword,             // [odd]
}

/// Pattern parameters extracted from AST
#[derive(Debug, Clone)]
pub struct PatternParams {
    pub pattern_type: PatternType,
    pub indices: Vec<i32>, // Pre-computed indices for matching
}
```

### Layer 3: Analyzer Implementation

**Location**: `bhdl-analyzer/src/passes/power_domain_expansion.rs`

**Current Logic**:
```rust
// Check for range expressions
let ranges = pin_list.ranges();
if ranges.len() >= 2 {
    // Extract start and end from range expressions
    for i in start..=end {
        // Create connections
    }
}
```

**Enhanced Logic**:
```rust
// Get pattern type and params
let pattern = pin_list.pattern_params();

match pattern.pattern_type {
    PatternType::Wildcard => {
        expand_wildcard_instances(net_name, &component, &pin_name, instance_registry, expansion);
    }
    PatternType::SimpleRange(start, end) => {
        for i in start..=end {
            expansion.connections.push(ExpandedConnection { /* ... */ });
        }
    }
    PatternType::SteppedRange(start, end, step) => {
        let mut i = start;
        while i <= end {
            expansion.connections.push(ExpandedConnection { /* ... */ });
            i += step;
        }
    }
    PatternType::ExplicitList(indices) => {
        for i in indices {
            expansion.connections.push(ExpandedConnection { /* ... */ });
        }
    }
    PatternType::EvenKeyword => {
        // Find all instances, filter for even indices
        let matches = instance_registry.find_wildcard_matches(&component);
        for instance_name in matches {
            if let Some(index) = extract_index_from_name(&instance_name) {
                if index % 2 == 0 {
                    expansion.connections.push(ExpandedConnection { /* ... */ });
                }
            }
        }
    }
    PatternType::OddKeyword => {
        // Similar to even, but filter for odd indices
    }
}
```

**New Helper Functions**:
```rust
/// Extract numeric index from instance name
/// Examples:
///   "sensor_0" -> Some(0)
///   "sensor[5]" -> Some(5)
///   "sensor7" -> Some(7)
///   "sensor_a" -> None
fn extract_index_from_name(name: &str) -> Option<i32> {
    // Try array notation first: sensor[5]
    if let Some(start) = name.find('[') {
        if let Some(end) = name.find(']') {
            let index_str = &name[start+1..end];
            return index_str.parse().ok();
        }
    }

    // Try underscore notation: sensor_0
    if let Some(pos) = name.rfind('_') {
        let suffix = &name[pos+1..];
        if suffix.chars().all(|c| c.is_numeric()) {
            return suffix.parse().ok();
        }
    }

    // Try trailing digits: sensor0
    let digits: String = name.chars()
        .rev()
        .take_while(|c| c.is_numeric())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if !digits.is_empty() {
        return digits.parse().ok();
    }

    None
}
```

## Implementation Plan

### Phase 1: Parser and AST (Day 1)
- [ ] Add `PATTERN_KEYWORD` and `PATTERN_INDICES` syntax kinds
- [ ] Implement `parse_pattern_range_or_list()` helper
- [ ] Add keyword recognition for "even" and "odd"
- [ ] Support comma-separated index lists
- [ ] Support colon-separated step notation
- [ ] Add `pattern_type()` and `pattern_params()` to DistributionPinList AST

### Phase 2: Analyzer Logic (Day 2)
- [ ] Implement `extract_index_from_name()` helper
- [ ] Add pattern matching switch in `expand_pin_list()`
- [ ] Implement even/odd filtering
- [ ] Implement explicit list expansion
- [ ] Implement stepped range expansion

### Phase 3: Testing (Day 3)
- [ ] Create test circuit with all pattern types
- [ ] Test even/odd keywords
- [ ] Test explicit index lists
- [ ] Test stepped ranges
- [ ] Test error cases (invalid patterns, out-of-range indices)
- [ ] Test combination with hierarchical wildcards

### Phase 4: Documentation (Day 3)
- [ ] Update NEXT_STEPS.md
- [ ] Create completion changelog
- [ ] Add examples to docs/examples/
- [ ] Update specification document

## Test Cases

### Test Circuit: `test_advanced_patterns.bhdl`
```bhdl
board AdvancedPatternTest {
    // Create 16 sensor instances
    generate for i in 0..15 {
        sensor[i]: TempSensor();
    }

    // Even pattern
    power_domain @VCC_EVEN = 3.3V @ 2A {
        distribution {
            sensor[even].VCC;  // Should match 0,2,4,6,8,10,12,14
        }
    }

    // Odd pattern
    power_domain @VCC_ODD = 3.3V @ 2A {
        distribution {
            sensor[odd].VCC;   // Should match 1,3,5,7,9,11,13,15
        }
    }

    // Explicit list
    power_domain @VCC_SPECIAL = 3.3V @ 1A {
        distribution {
            sensor[0,5,10,15].VCC;  // Only 4 specific sensors
        }
    }

    // Stepped range
    power_domain @VCC_SAMPLED = 3.3V @ 1A {
        distribution {
            sensor[0..15:3].VCC;  // 0, 3, 6, 9, 12, 15
        }
    }

    ground GND;
}
```

### Expected Results

| Pattern | Expected Matches | Count |
|---------|------------------|-------|
| `sensor[even].VCC` | 0,2,4,6,8,10,12,14 | 8 |
| `sensor[odd].VCC` | 1,3,5,7,9,11,13,15 | 8 |
| `sensor[0,5,10,15].VCC` | 0,5,10,15 | 4 |
| `sensor[0..15:3].VCC` | 0,3,6,9,12,15 | 6 |

**Total connections**: 26

## Edge Cases

1. **Invalid Keywords**: `sensor[medium].VCC` → Error: "Unknown pattern keyword 'medium'"
2. **Out of Range**: `sensor[20,25].VCC` → Warning: "Indices 20, 25 not found"
3. **Empty Range**: `sensor[10..5].VCC` → Warning: "Empty range [10..5]"
4. **Zero Step**: `sensor[0..10:0].VCC` → Error: "Step cannot be zero"
5. **Negative Step**: `sensor[10..0:-2].VCC` → Support or error? (Decision needed)

## Future Enhancements

### Combination Patterns
```bhdl
sensor[even, 7, 9].VCC;       // Union of patterns
sensor[0..10 except 5].VCC;   // Exclusion
sensor[0..10 & even].VCC;     // Intersection
```

### Named Patterns
```bhdl
pattern primary_sensors = [0,2,4,8];
pattern backup_sensors = [1,3,5,9];

power_domain @VCC = 3.3V @ 5A {
    distribution {
        sensor[primary_sensors].VCC;
    }
}
```

### Percentage-Based Selection
```bhdl
sensor[0..50%].VCC;  // First 50% of instances
sensor[25%..75%].VCC;  // Middle 50%
```

## Backward Compatibility

All existing patterns remain unchanged:
- `[*]` - Still works
- `[0..7]` - Still works
- Hierarchical wildcards - Still work

New patterns are additive and don't break existing code.

## Performance Considerations

- Pattern evaluation happens at compile time (analysis phase)
- No runtime overhead
- Pre-computed indices stored in `PatternParams`
- Complexity: O(n) where n is number of instances matching base name

## Conclusion

Advanced Pattern Matching extends BHDL's power domain scalability with fine-grained control over instance selection. The feature integrates cleanly with existing wildcard infrastructure and maintains backward compatibility while adding significant expressiveness for complex board designs.
