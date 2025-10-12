# Advanced Pattern Matching - Complete Implementation

**Date**: October 12, 2025
**Feature**: Advanced Pattern Matching (Power Domain Scalability Enhancement)
**Status**: ✅ Complete

## Overview

This changelog documents the complete implementation of Advanced Pattern Matching for BHDL power domains. This feature extends the wildcard pattern matching system with four new pattern types, enabling fine-grained control over instance selection in power domain distribution blocks.

## Motivation

The existing wildcard system supported:
- **Wildcard**: `sensor[*].VCC` - matches all instances
- **Simple range**: `fpga.VCCO[0..7]` - matches indices 0 through 7
- **Hierarchical wildcards**: `module[*].component.pin` - crosses module boundaries

These patterns covered many use cases, but designers needed more control for:
- Differential pair routing (even/odd instance selection)
- Selective population of specific non-contiguous indices
- Stepped ranges for sampling or interleaving patterns

## New Pattern Types

### 1. Even/Odd Keywords

**Syntax**:
```bhdl
power_domain @VCC_EVEN = 3.3V @ 2A {
    distribution {
        sensor[even].VCC;  // Only sensor_0, sensor_2, sensor_4, sensor_6
    }
}
```

**Use Cases**:
- Differential pair routing
- Interleaved power domains for thermal management
- Redundant system architectures

### 2. Explicit Index Lists

**Syntax**:
```bhdl
power_domain @VCC_SPECIAL = 3.3V @ 1A {
    distribution {
        sensor[0,5,10,15].VCC;  // Only specific indices
    }
}
```

**Use Cases**:
- Custom pin mappings from external constraints
- Selective population of instances
- Non-contiguous groupings

### 3. Stepped Ranges

**Syntax**:
```bhdl
power_domain @VCC_SAMPLED = 3.3V @ 1A {
    distribution {
        sensor[0..15:3].VCC;   // 0, 3, 6, 9, 12, 15
    }
}
```

**Use Cases**:
- Sampling patterns
- Interleaved routing
- Phased array systems

### 4. Single Index (Existing, Refined)

**Syntax**:
```bhdl
power_domain @VCC_SINGLE = 3.3V @ 1A {
    distribution {
        sensor[7].VCC;  // Only sensor_7
    }
}
```

Now handled through ExplicitList pattern with a single element.

## Implementation Architecture

### Phase 1: Parser and AST

#### Parser Enhancement (bhdl-parser/src/top_level.rs)

**New Syntax Kinds**:
- `PATTERN_KEYWORD` - For "even" and "odd" keywords
- `PATTERN_INDICES` - For explicit lists and stepped ranges

**New Helper Methods**:

1. **parse_bracket_contents()** (lines 1167-1193):
   - Handles all bracket pattern types
   - Recognizes wildcards (*), keywords (even/odd), and expressions
   - Dispatches to appropriate parsing logic

2. **parse_pattern_range_or_list()** (lines 1195-1228):
   - Parses ranges ([0..7]), stepped ranges ([0..7:2]), and lists ([0,2,4])
   - Handles commas and colons to distinguish pattern types
   - Creates PATTERN_INDICES nodes

3. **parse_path_segment()** (lines 1153-1165):
   - Parses a single path segment with optional bracket pattern
   - Cleaner code organization for multi-segment paths

#### AST Extension (bhdl-ast/src/items.rs)

**New Types**:

```rust
/// Pattern type classification (lines 866-875)
#[derive(Debug, Clone, PartialEq)]
pub enum PatternType {
    Wildcard,                    // [*]
    SimpleRange(i32, i32),       // [0..7]
    SteppedRange(i32, i32, i32), // [0..15:3]
    ExplicitList(Vec<i32>),      // [0,2,4,8]
    EvenKeyword,                 // [even]
    OddKeyword,                  // [odd]
}

/// Pattern parameters extracted from AST (lines 877-882)
#[derive(Debug, Clone)]
pub struct PatternParams {
    pub pattern_type: PatternType,
    pub indices: Vec<i32>, // Pre-computed indices for matching
}
```

**New Methods on DistributionPinList**:

1. **pattern_type()** (lines 747-784):
   - Analyzes syntax tree to determine pattern type
   - Returns PatternType enum variant

2. **pattern_params()** (lines 839-863):
   - Returns pattern type with pre-computed indices
   - Handles index computation for ranges and stepped ranges

3. **parse_pattern_indices()** (lines 786-831):
   - Internal helper for analyzing PATTERN_INDICES nodes
   - Distinguishes ranges, stepped ranges, and lists

4. **extract_number_from_expr()** (lines 833-837):
   - Helper to extract integer from expression text

### Phase 2: Analyzer Implementation

#### Power Domain Expansion (bhdl-analyzer/src/passes/power_domain_expansion.rs)

**Pattern Matching Switch** (lines 181-306):

Replaced the old range-only logic with a comprehensive pattern matching system:

```rust
let pattern = pin_list.pattern_params();

match pattern.pattern_type {
    PatternType::Wildcard => { /* existing wildcard logic */ }
    PatternType::SimpleRange(start, end) => { /* range expansion */ }
    PatternType::SteppedRange(start, end, step) => { /* stepped expansion */ }
    PatternType::ExplicitList(indices) => { /* list expansion */ }
    PatternType::EvenKeyword => { /* even filtering */ }
    PatternType::OddKeyword => { /* odd filtering */ }
}
```

**New Helper Function** (lines 252-283):

```rust
/// Extract numeric index from instance name
/// Examples:
///   "sensor_0" -> Some(0)
///   "sensor[5]" -> Some(5)
///   "sensor7" -> Some(7)
///   "sensor_a" -> None
fn extract_index_from_name(name: &str) -> Option<i32>
```

Supports three naming conventions:
- Array notation: `sensor[5]`
- Underscore notation: `sensor_0`
- Trailing digits: `sensor7`

## Test Infrastructure

### Test Circuit (tests/circuits/realistic/test_advanced_patterns.bhdl)

Created comprehensive test circuit with 16 sensor instances exercising all pattern types:

```bhdl
board AdvancedPatternTest {
    // 16 sensor instances (sensor_0 through sensor_15)
    sensor_0: TempSensor();
    // ... sensor_1 through sensor_14 ...
    sensor_15: TempSensor();

    power_domain @VCC_EVEN = 3.3V @ 2A {
        distribution { sensor[even].VCC; }
    }

    power_domain @VCC_ODD = 3.3V @ 2A {
        distribution { sensor[odd].VCC; }
    }

    power_domain @VCC_SPECIAL = 3.3V @ 1A {
        distribution { sensor[0,5,10,15].VCC; }
    }

    power_domain @VCC_SAMPLED = 3.3V @ 1A {
        distribution { sensor[0..15:3].VCC; }
    }

    power_domain @VCC_RANGE = 3.3V @ 1A {
        distribution { sensor[0..4].VCC; }
    }

    power_domain @VCC_SINGLE = 3.3V @ 1A {
        distribution { sensor[7].VCC; }
    }

    power_domain @VCC_ALL = 3.3V @ 5A {
        distribution { sensor[*].VCC; }
    }

    ground GND;
}
```

### Test Binary 1: Parser Test (bhdl-analyzer/src/bin/test_advanced_patterns.rs)

Tests pattern classification at the parser/AST level:

**Results**:
```
✅ All pattern types correctly identified!
- EvenKeyword: ✅
- OddKeyword: ✅
- ExplicitList([0, 5, 10, 15]): ✅
- SteppedRange(0, 15, 3): ✅ with indices [0, 3, 6, 9, 12, 15]
- SimpleRange(0, 4): ✅ with indices [0, 1, 2, 3, 4]
- ExplicitList([7]): ✅
- Wildcard: ✅
```

### Test Binary 2: End-to-End Test (bhdl-analyzer/src/bin/test_pattern_expansion.rs)

Tests complete pattern expansion through the analyzer:

**Results**:
```
Total connections: 48

✅ @VCC_EVEN: 8 connections (indices 0,2,4,6,8,10,12,14)
✅ @VCC_ODD: 8 connections (indices 1,3,5,7,9,11,13,15)
✅ @VCC_SPECIAL: 4 connections (indices 0,5,10,15)
✅ @VCC_SAMPLED: 6 connections (indices 0,3,6,9,12,15)
✅ @VCC_RANGE: 5 connections (indices 0,1,2,3,4)
✅ @VCC_SINGLE: 1 connection (index 7)
✅ @VCC_ALL: 16 connections (all indices 0-15)
```

**Verification**: 8+8+4+6+5+1+16 = 48 connections ✅

## Files Modified

### Parser Layer
- **bhdl-parser/src/syntax.rs** (lines 224-226)
  - Added PATTERN_KEYWORD and PATTERN_INDICES syntax kinds

- **bhdl-parser/src/top_level.rs** (lines 1130-1228)
  - parse_path_segment() - Clean segment parsing
  - parse_bracket_contents() - Dispatch to pattern types
  - parse_pattern_range_or_list() - Range/list parsing

### AST Layer
- **bhdl-ast/src/items.rs** (lines 747-882)
  - DistributionPinList::pattern_type() - Pattern classification
  - DistributionPinList::pattern_params() - Parameter extraction
  - DistributionPinList::parse_pattern_indices() - Internal parser
  - DistributionPinList::extract_number_from_expr() - Helper
  - PatternType enum - Six pattern variants
  - PatternParams struct - Type + indices

### Analyzer Layer
- **bhdl-analyzer/src/passes/power_domain_expansion.rs**
  - Line 9: Added PatternType import
  - Lines 181-306: Complete pattern matching switch
  - Lines 252-283: extract_index_from_name() helper

### Test Infrastructure
- **tests/circuits/realistic/test_advanced_patterns.bhdl** - Test circuit
- **bhdl-analyzer/src/bin/test_advanced_patterns.rs** - Parser test
- **bhdl-analyzer/src/bin/test_pattern_expansion.rs** - End-to-end test

## Benefits

### 1. Fine-Grained Control
Designers can now precisely select which instances receive power, enabling:
- Complex routing patterns
- Selective population strategies
- Phased deployment scenarios

### 2. Differential Pair Support
Even/odd keywords natively support differential pair architectures:
```bhdl
power_domain @VCC_P = 3.3V @ 1A {
    distribution { pairs[even].VCC_P; }
}

power_domain @VCC_N = 3.3V @ 1A {
    distribution { pairs[odd].VCC_N; }
}
```

### 3. Sampling and Interleaving
Stepped ranges enable phased array and sampling patterns:
```bhdl
power_domain @VCC_PHASE_A = 3.3V @ 1A {
    distribution { array[0..99:3].VCC; }  // 0, 3, 6, 9, ...
}

power_domain @VCC_PHASE_B = 3.3V @ 1A {
    distribution { array[1..99:3].VCC; }  // 1, 4, 7, 10, ...
}

power_domain @VCC_PHASE_C = 3.3V @ 1A {
    distribution { array[2..99:3].VCC; }  // 2, 5, 8, 11, ...
}
```

### 4. Backward Compatibility
All existing patterns continue to work:
- `[*]` - Wildcard
- `[0..7]` - Simple range
- Hierarchical wildcards

New patterns are additive and don't break existing code.

## Performance

- **Compile-time evaluation**: Pattern matching happens during analysis phase
- **No runtime overhead**: Pre-computed indices stored in PatternParams
- **Complexity**: O(n) where n is number of instances matching base name
- **Memory**: Minimal - indices stored as Vec<i32>

## Edge Cases Handled

1. **Invalid keywords**: `sensor[medium].VCC` → Parser error
2. **Empty ranges**: `sensor[10..5].VCC` → Empty expansion
3. **Zero step**: `sensor[0..10:0].VCC` → Infinite loop prevention
4. **Out of range**: `sensor[20,25].VCC` → No matches (graceful)
5. **Mixed notation**: Works with all instance naming conventions

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

power_domain @VCC = 3.3V @ 5A {
    distribution {
        sensor[primary_sensors].VCC;
    }
}
```

### Percentage-Based Selection
```bhdl
sensor[0..50%].VCC;   // First 50% of instances
sensor[25%..75%].VCC; // Middle 50%
```

## Documentation

- **Design Document**: `docs/implementation/Advanced_Pattern_Matching_Design.md`
- **NEXT_STEPS.md**: Updated with completion status and test results
- **This Changelog**: Complete implementation narrative

## Conclusion

Advanced Pattern Matching completes the power domain scalability feature set by providing fine-grained control over instance selection. The feature:

✅ Integrates cleanly with existing wildcard infrastructure
✅ Maintains backward compatibility
✅ Provides significant expressiveness for complex designs
✅ Has comprehensive test coverage
✅ Follows existing code patterns and architecture

This feature enables designers to specify complex power routing patterns declaratively, letting the analyzer handle the expansion automatically. Combined with hierarchical wildcards, generate block integration, and fuzzy error messages, BHDL now has a complete, production-ready power domain system.
