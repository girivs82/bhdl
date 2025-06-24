# BHDL v2.0 Flow Syntax and Expression Support Implementation

## Overview

This document describes the implementation of full BHDL v2.0 flow syntax support in the analyzer, including component library loading fixes and parameter validation improvements. Additionally, it covers the implementation of a comprehensive BHDL expression evaluator for runtime model execution.

## Key Changes

### 1. Expression Evaluator Implementation

Created a full BHDL expression evaluator in `bhdl-common` that supports:
- Module parsing and constant extraction
- Expression evaluation with parameter resolution
- Struct literal parsing
- Electrical unit conversion (kΩ → Ω, mA → A, etc.)
- Path expressions (e.g., `params.output_voltage`)

**Files Changed:**
- `bhdl-common/src/expression_evaluator.rs` (new)
- `bhdl-common/src/lib.rs` (added module export)
- `bhdl-common/Cargo.toml` (added dependencies)

### 2. Analyzer v2.0 Flow Syntax Support

Updated the BHDL analyzer to fully support v2.0 flow syntax:

#### Pass 2 (Reference Resolution):
- Added FLOW_EXPR handling to recognize component instantiations within flow expressions
- Fixed IDENT_REF validation to skip instance names being created in flow/connection contexts
- Added detection of instance creation patterns (name: ComponentType)

**Key Changes in `bhdl-analyzer/src/pass2.rs`:**
```rust
// Added FLOW_EXPR case to handle flow expressions
SyntaxKind::FLOW_EXPR => {
    // Process component instantiations in flow
    // Validate component types exist
}

// Fixed IDENT_REF to handle flow context
SyntaxKind::IDENT_REF => {
    // Check if in flow/connection context
    // Skip validation for instance names being created
}
```

#### Pass 3 (Constant Evaluation):
- Fixed parameter validation to recognize string values (colors, packages) vs constant references
- Added whitelist of common string parameter values that shouldn't be evaluated as constants

**Key Changes in `bhdl-analyzer/src/pass3.rs`:**
```rust
// Skip constant evaluation for known string parameters
if matches!(text.as_ref(), "red" | "green" | "blue" | ...) {
    // Don't evaluate as constant
}
```

### 3. Component Library Loading Fix

Fixed component type extraction in flow syntax:
- Detected when COMPONENT_INST is inside CONNECTION_STMT (v2.0 inline syntax)
- Extracted component type directly for inline instantiations like `Res(10k).1`
- Used appropriate AST handler based on context

**Key Changes in `bhdl-analyzer/src/lib.rs`:**
```rust
// Check if COMPONENT_INST is in connection context
if in_connection {
    // Extract type directly for v2.0 inline syntax
    // Process with flow handler
}
```

### 4. Parser Enhancements

Added support for:
- `where` keyword for connection constraints
- `with` keyword for grouped constraints
- Improved keyword recognition in lexer

**Files Changed:**
- `bhdl-parser/src/lexer.rs`
- `bhdl-parser/src/syntax.rs`
- `bhdl-parser/src/v2_parsing.rs`

## Test Suite

Created comprehensive tests to verify the implementation:

### 1. V2.0 Flow Syntax Test (`test_v2_flow_syntax.rs`)
Tests three scenarios:
- Simple flow with component instantiation
- Flow with undefined component (error case)
- Flow with LM7805 regulator

### 2. LM7805 End-to-End Test (`test_lm7805_end_to_end.rs`)
- Tests full BHDL expression evaluation
- Verifies runtime model execution
- Tests voltage regulator behavior

### 3. Simple Test Circuit (`test_7805_cli_simple.bhdl`)
```bhdl
board VoltageRegulator {
    power VIN = 12V @ 500mA;
    power VOUT = 5V @ 400mA;
    ground GND;
    
    @VIN -> U1: LM7805().IN;
    U1.GND -> @GND;
    U1.OUT -> @VOUT;
}
```

## Results

### Before:
- Component types in flow syntax extracted as "Unknown"
- Color parameters like "red" treated as undefined constants
- Instance names in flows flagged as undefined symbols

### After:
- ✅ Component types correctly extracted (Res, LED, LM7805, etc.)
- ✅ String parameters properly recognized
- ✅ Instance creation in flows handled correctly
- ✅ Full BHDL expression evaluation support
- ✅ Runtime model execution with parameter resolution

## Example Output

```
Test 1: Simple flow with component instantiation
Diagnostics:
  ✅ No diagnostics - v2.0 flow syntax correctly recognized!

Test 2: Flow with undefined component
Diagnostics:
  Expected: Component Inference: Unknown component type 'UndefinedComp'

Test 3: Flow with LM7805 regulator
Processing inline component: LM7805
Processing v2.0 component instantiation: U4 (type: LM7805)
```

## Future Work

1. Implement flow-based intent system using `for` keyword
2. Add more component inference rules
3. Enhance SPICE resolution for components with placeholder parameters
4. Extend expression evaluator to support more complex expressions

## Migration Guide

For users updating existing code:
1. No changes needed - v2.0 syntax is already the standard
2. Component instantiations in flows now properly validated
3. String parameters (colors, packages) no longer need quotes