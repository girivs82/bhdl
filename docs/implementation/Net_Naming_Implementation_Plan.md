# Net Naming Implementation Plan

## Overview

This document outlines the implementation plan for the BHDL v2.0 net naming specification with `@` prefix disambiguation.

## Implementation Phases

### Phase 1: Parser Updates (bhdl-parser)

#### 1.1 Lexer Updates
- Add `@` as a valid token type
- No changes needed to identifier lexing

**Files to modify:**
- `bhdl-parser/src/lexer.rs`
- `bhdl-parser/src/syntax.rs` (add `AT` to `SyntaxKind`)

#### 1.2 Grammar Updates
- Add parsing for `@ident` as net reference
- Add parsing for `@ident->` as named arrow
- Update connection parsing to handle both `->` and `@NAME->`

**Files to modify:**
- `bhdl-parser/src/expressions.rs`
  - Update `parse_flow_element()` to recognize `@ident` pattern
  - Add `parse_net_ref()` method
- `bhdl-parser/src/blocks.rs`
  - Update connection operator parsing

**New Grammar Rules:**
```rust
// In parse_flow_element
if self.at_token(AT) {
    self.parse_net_ref()
} else if self.at_token(IDENT) {
    // Could be component ref, pin ref, or component instantiation
    self.parse_ident_element()
}

// New method
fn parse_net_ref(&mut self) {
    let net_ref = self.start();
    self.expect(AT);
    self.expect(IDENT);
    self.finish(net_ref, NET_REF);
}

// In parse_connection_operator
if self.at_token(AT) {
    // Named arrow: @NAME->
    self.parse_named_arrow()
} else if self.eat(ARROW) {
    // Anonymous arrow: ->
}
```

### Phase 2: AST Updates (bhdl-ast)

#### 2.1 New AST Node Types
- Add `NetRef` AST node type for `@ident` references
- Add `NamedArrow` AST node type for `@ident->` operators

**Files to modify:**
- `bhdl-ast/src/common.rs`
  - Add `NetRef` struct
- `bhdl-ast/src/flow.rs`
  - Update flow expression handling
  - Add named arrow support

**Example AST Nodes:**
```rust
#[derive(Debug, Clone)]
pub struct NetRef {
    node: SyntaxNode<BhdlLanguage>,
}

impl NetRef {
    pub fn name(&self) -> Option<SyntaxToken> {
        // Return the identifier after @
    }
}
```

### Phase 3: Analyzer Updates (bhdl-analyzer)

#### 3.1 Symbol Table Updates
- Add separate namespace for nets
- Don't create nets from component handles

**Files to modify:**
- `bhdl-analyzer/src/pass1.rs`
  - Track net declarations separately
  - Add net symbols to symbol table
- `bhdl-analyzer/src/pass2.rs`
  - Resolve net references (must have `@`)
  - Validate net usage

#### 3.2 Power Analysis Updates
- Continue creating power domains as needed
- Don't create nets for component handles

**Files to modify:**
- `bhdl-analyzer/src/power_analysis.rs`
  - Update to work with new net syntax
  - Power domains are separate from net names

### Phase 4: Synthesizer Updates (bhdl-synthesizer)

#### 4.1 Connection Processing
- Remove automatic net creation from component handles
- Only create nets for:
  - Anonymous connections (`->`)
  - Named net declarations (`@NAME->`)
  - Explicit net references (`@NAME`)

**Files to modify:**
- `bhdl-synthesizer/src/lib.rs`
  - Update `parse_connection_endpoint()`
  - Update `process_connection_statement()`
  - Remove net creation from `NetAssignment` case
  - Add handling for `NetRef` AST nodes

**Key Changes:**
```rust
// In parse_connection_endpoint
enum ConnectionEndpoint {
    Net(String),           // @NETNAME
    ComponentPin(String, String), // component.pin
    InlineComponent(String, String, String), // handle: Component().pin
    // Remove NetAssignment variant
}

// Don't create nets from component handles
// Only create nets from @ references
```

#### 4.2 Netlist Generation
- Anonymous nets get synthesizer-generated names (`$net_1`, etc.)
- Named nets use the exact name after `@`
- Component handles never become net names

### Phase 5: Test Updates

#### 5.1 Parser Tests
**New test files:**
- `bhdl-parser/tests/net_naming_tests.rs`

**Test cases:**
- Parse `@ident` as net reference
- Parse `@ident->` as named arrow
- Parse mixed anonymous and named arrows
- Error on invalid syntax (e.g., `@@NAME`)

#### 5.2 Analyzer Tests
**Update test files:**
- `bhdl-analyzer/src/tests/`

**Test cases:**
- Net symbols properly tracked
- Net references must use `@`
- Component handles don't create nets
- Power domains still work correctly

#### 5.3 End-to-End Tests
**New test files:**
- `tests/net_naming_e2e_tests.rs`

**Test cases:**
- Simple circuit with named nets
- Power supply with multiple named nets
- Mixed anonymous and named nets
- Verify netlist has correct net names

### Phase 6: Documentation Updates

1. Update `docs/spec/BHDL_Complete_Specification.md`
   - Add net naming syntax section
   - Update examples throughout
2. Update `CLAUDE.md`
   - Note the new syntax
   - Add examples
3. Update all example files in `docs/examples/`
   - Migrate to new syntax where appropriate

## Migration Strategy

### Backward Compatibility Approach

**Option 1: Hard Cut**
- New parser only accepts new syntax
- Provide migration tool

**Option 2: Transition Period**
- Parser accepts both syntaxes
- Emit deprecation warnings for old syntax
- Remove old syntax support in next major version

**Recommendation:** Option 1 - Hard Cut
- Cleaner implementation
- Forces consistent usage
- Provide clear migration guide

### Migration Tool

Create `tools/migrate_net_syntax.rs`:
```rust
// Automated migration tool
// 1. Parse with old syntax
// 2. Identify component handles that become nets
// 3. Rewrite using @ syntax where appropriate
// 4. Preserve anonymous nets
```

## Testing Strategy

### Unit Tests
1. Parser: Each new parsing rule
2. AST: Node creation and traversal
3. Analyzer: Symbol resolution
4. Synthesizer: Net creation logic

### Integration Tests
1. Parse -> Analyze -> Synthesize pipeline
2. Verify correct netlist generation
3. Test error messages

### Example Test Cases

```bhdl
// Test 1: Basic named net
board Test1 {
    power VCC = 5V @ 1A;
    ground GND;
    
    VCC @FILTERED-> r1: Res(10k).1;
    @FILTERED -> c1: Cap(100n).1;
    
    // Verify: Net "FILTERED" has 2 connections
}

// Test 2: No implicit nets from handles
board Test2 {
    power VCC = 5V @ 1A;
    
    VCC -> r1: Res(10k).1;
    r1.2 -> c1: Cap(100n).1;
    
    // Verify: No net named "r1" exists
    // Verify: Two anonymous nets exist
}

// Test 3: Error case
board Test3 {
    power VCC = 5V @ 1A;
    
    // Error: Cannot reference net without @
    VCC -> r1: Res(10k).1;
    FILTERED -> c1: Cap(100n).1;  // ERROR: Unknown identifier
}
```

## Timeline Estimate

1. **Phase 1 (Parser)**: 2-3 days
   - Lexer updates: 2 hours
   - Grammar updates: 1-2 days
   - Parser tests: 1 day

2. **Phase 2 (AST)**: 1 day
   - New node types: 2-3 hours
   - Integration: 2-3 hours

3. **Phase 3 (Analyzer)**: 2 days
   - Symbol table: 1 day
   - Power analysis: 1 day

4. **Phase 4 (Synthesizer)**: 2-3 days
   - Connection processing: 1-2 days
   - Testing and debugging: 1 day

5. **Phase 5 (Tests)**: 2 days
   - Unit tests: 1 day
   - Integration tests: 1 day

6. **Phase 6 (Documentation)**: 1 day

**Total: 10-12 days**

## Success Criteria

1. All tests pass with new syntax
2. Clear error messages for invalid syntax
3. Example circuits work correctly
4. Netlist generation produces expected results
5. No regressions in existing functionality
6. Documentation is complete and clear

## Risk Mitigation

1. **Risk**: Breaking existing code
   - **Mitigation**: Comprehensive test suite, migration tool

2. **Risk**: Parser complexity
   - **Mitigation**: Incremental implementation, extensive tests

3. **Risk**: User confusion
   - **Mitigation**: Clear documentation, good error messages

4. **Risk**: Integration issues
   - **Mitigation**: Test each phase independently