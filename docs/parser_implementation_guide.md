# BHDL Parser Implementation Guide

This guide documents the architecture and implementation details of the BHDL parser after adding complex syntax support.

## Parser Architecture

### Module Structure

```
bhdl-parser/
├── src/
│   ├── lib.rs          # Public API, lexer integration, token mapping
│   ├── syntax.rs       # Token kinds and language definition
│   ├── lexer.rs        # Lexical analysis with logos
│   ├── core.rs         # Core parser structure and utilities
│   ├── parser.rs       # Main parser implementation
│   ├── expressions.rs  # Expression parsing (precedence climbing)
│   ├── top_level.rs    # Top-level constructs (modules, boards, aliases)
│   ├── v2_parsing.rs   # V2.0 specific parsing (imports, connections)
│   ├── items.rs        # Item-level parsing utilities
│   ├── blocks.rs       # Block parsing (generate, constrain)
│   └── error_recovery.rs # Error handling (partially implemented)
```

### Key Components

#### 1. Lexer (`lexer.rs`)
- Uses `logos` crate for tokenization
- Defines `LexerToken` enum with all token types
- Handles keywords through callbacks
- Supports Unicode units (Ω, µF, °C)

#### 2. Token Mapping (`lib.rs`)
- `map_token_stream()` - Post-processes tokens
- Implements smart unit detection (units only after numbers)
- Handles whitespace preservation for CST

#### 3. Expression Parser (`expressions.rs`)
- Precedence climbing algorithm
- `parse_expr(min_bp)` - Main expression parser
- `parse_primary_expr()` - Literals, identifiers, component instantiation
- Supports all operators including ternary

#### 4. Top-Level Parser (`top_level.rs`)
- `parse_source_file()` - Entry point
- Handles modules, boards, interfaces, imports, aliases
- `parse_module_pin_decl()` - Pins with optional 'when' clause
- `parse_const_decl()` - Typed constant declarations

## Implementation Patterns

### Adding a New Keyword

1. Add to `syntax.rs`:
```rust
NEWFOO_KW,  // newfoo
```

2. Add to lexer in `lexer.rs`:
```rust
"newfoo" => SyntaxKind::NEWFOO_KW,
```

3. Handle in appropriate parser:
```rust
Some(SyntaxKind::NEWFOO_KW) => self.parse_newfoo(),
```

### Adding a New Operator

1. Add token to `syntax.rs` if needed
2. Add to `infix_binding_power()` in `expressions.rs`:
```rust
SyntaxKind::NEWOP => Some((left_bp, right_bp)),
```

3. The expression parser automatically handles it

### Supporting Alternative Token Types

Pattern used for pins and aliases that accept NUMBER tokens:

```rust
// Accept IDENT or NUMBER
if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
    self.bump();
} else {
    self.error("Expected identifier or number".to_string());
}
```

### Context-Aware Parsing

Example from unit tokenization:

```rust
// Check previous token to determine context
let mut prev_was_number = false;
for j in (0..result.len()).rev() {
    match result[j].0 {
        SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => continue,
        SyntaxKind::NUMBER => {
            prev_was_number = true;
            break;
        }
        _ => break,
    }
}
```

## Parser State Management

### Parser Structure
```rust
pub struct Parser<'t> {
    tokens: &'t [(SyntaxKind, SmolStr)],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
}
```

### Key Methods
- `peek()` - Look at current token without consuming
- `peek_nth(n)` - Look ahead n tokens
- `bump()` - Consume current token
- `expect()` - Consume and verify token type
- `error()` - Record parse error

## Error Handling

### Current Approach
- Errors are collected but parsing continues
- `expect()` adds error if token doesn't match
- Most errors are "Expected X, found Y"

### Error Recovery (Partial)
- `error_recovery.rs` has infrastructure but not fully utilized
- Parser attempts to continue after errors
- No sophisticated synchronization yet

## Testing Infrastructure

### Test Binaries
Located in `src/bin/`:
- `test_const_parsing.rs` - General parser testing
- `debug_lexer_const.rs` - Token stream inspection
- `test_parser_debug.rs` - Detailed error context

### Running Tests
```bash
# Test specific file
cargo run --bin test_const_parsing path/to/file.bhdl

# Debug lexer output
cargo run --bin debug_lexer_const path/to/file.bhdl

# Debug parser errors
cargo run --bin test_parser_debug path/to/file.bhdl
```

## Common Patterns and Pitfalls

### 1. Token Lookahead
Be careful with lookahead - tokens include trivia:
```rust
// Wrong: doesn't skip whitespace
if self.tokens[self.pos + 1].0 == SyntaxKind::EQ { ... }

// Right: use peek_nth
if self.peek_nth(1) == Some(SyntaxKind::EQ) { ... }
```

### 2. Node Building
Always match start_node with finish_node:
```rust
self.builder.start_node(SyntaxKind::FOO.into());
// ... parse content ...
self.builder.finish_node();
```

### 3. Expression Parsing
Let the precedence climbing handle complexity:
```rust
// Parse any expression
self.parse_expr(0);

// Parse expression with minimum precedence
self.parse_expr(min_bp);
```

### 4. Optional Syntax
Common pattern for optional elements:
```rust
if self.peek() == Some(SyntaxKind::WHEN_KW) {
    self.bump();
    self.parse_expression();
}
```

## Future Extensions

### High Priority
1. Better error recovery with synchronization points
2. Span tracking for better error messages
3. Incremental parsing support

### Medium Priority
1. Parser configuration/dialects
2. Better diagnostic messages
3. Quick fixes/suggestions

### Low Priority
1. Parallel parsing experiments
2. Parser performance optimization
3. Alternative parsing strategies