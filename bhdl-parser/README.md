# BHDL Parser

A robust parser for the Board Hardware Description Language (BHDL) v2.0, built using the `rowan` library for lossless syntax trees and `logos` for lexical analysis.

## Features

- **Full BHDL v2.0 Support**: Implements the complete flow-based syntax
- **Complex Expressions**: Ternary operators, member access, comparisons
- **Smart Unit Handling**: Context-aware tokenization for electrical units
- **Conditional Syntax**: Pins with `when` clauses
- **Entity Aliases**: Support for component aliases with numeric names
- **Error Recovery**: Continues parsing after errors for better diagnostics
- **Unicode Support**: Handles Unicode symbols (Ω, µF, °C)

## Quick Start

```rust
use bhdl_parser::{parse, SyntaxKind};

let source = r#"
entity LED(color: string = "red") {
    pin A: signal in;
    pin K: signal out;
    
    const forward_voltage: voltage = 
        color == "red" ? 2.0V : 3.3V;
}
"#;

let result = parse(source);
if result.errors().is_empty() {
    println!("Parsed successfully!");
    let syntax_tree = result.syntax();
    // Process syntax tree...
}
```

## Supported Syntax

### Entities and Components
```bhdl
entity EntityName(param: type = default) {
    pin name: signal in;
    pin vcc: power;
    const value: type = expression;
    attribute name = value;
}
```

### Conditional Pins
```bhdl
pin EN: signal in when package == "TO-220-5" || package == "TO-263-5";
```

### Complex Expressions
```bhdl
// Ternary operator
const voltage: voltage = condition ? 5V : 3.3V;

// Member access  
attribute value = params.forward_voltage;

// Comparisons and logical operators
const valid: bool = (type == "NPN" || type == "PNP") && voltage > 0V;
```

### Entity Aliases
```bhdl
alias 7805 = LM7805;
alias RedLED = LED("red");
```

### Import Statements
```bhdl
import entity.subentity;
import { Type1, Type2 } from "path/to/file.bhdl";
```

## Architecture

The parser uses a multi-stage approach:

1. **Lexing**: `logos` tokenizes the input into a stream of tokens
2. **Token Mapping**: Post-processes tokens for context-aware features
3. **Parsing**: Recursive descent parser builds a Concrete Syntax Tree (CST)
4. **AST Conversion**: Higher-level abstractions can be built on the CST

## Unit Handling

The parser intelligently handles single-letter units that could conflict with identifiers:

- `pin A:` - 'A' is an identifier (pin name)
- `5A` - 'A' is a unit (Amperes)
- `LED.A` - 'A' is a pin reference

This is achieved through context-aware post-processing after lexing.

### ASCII Alternatives

The parser supports both Unicode and ASCII representations for all units:

- `4.7kΩ` or `4.7kOhm` - Kiloohms
- `10µF` or `10uF` - Microfarads  
- `85°C` or `85degC` - Degrees Celsius
- `100µs` or `100us` - Microseconds

See `docs/unit_syntax_guide.md` for a complete reference of supported units and their ASCII alternatives.

## Error Handling

The parser collects all errors rather than failing on the first one:

```rust
let result = parse(source);
for error in result.errors() {
    println!("Error: {:?}", error);
}
```

## Testing Tools

The crate includes several debugging tools:

```bash
# Test parsing a file
cargo run --bin test_const_parsing file.bhdl

# Debug lexer output
cargo run --bin debug_lexer_const file.bhdl

# Debug parser errors with context
cargo run --bin test_parser_debug file.bhdl
```

## Extending the Parser

### Adding a New Keyword

1. Add to `src/syntax.rs`:
```rust
NEWKEYWORD_KW,  // newkeyword
```

2. Add to `src/lexer.rs`:
```rust
"newkeyword" => SyntaxKind::NEWKEYWORD_KW,
```

3. Handle in the appropriate parser function

### Adding a New Operator

Add to `expressions.rs` in `infix_binding_power()`:
```rust
SyntaxKind::NEWOP => Some((left_precedence, right_precedence)),
```

## Performance

The parser is designed for correctness and completeness rather than raw speed:
- Single-pass parsing with minimal backtracking
- Lossless syntax trees preserve all source information
- Suitable for IDE integration and incremental parsing

## Limitations

- Error recovery is basic - no sophisticated synchronization
- No incremental parsing API yet
- Limited quick-fix suggestions

## Future Plans

- [ ] Incremental parsing support
- [ ] Span tracking for precise error locations  
- [ ] Parser configuration for different BHDL versions
- [ ] Integration with tree-sitter for broader tool support