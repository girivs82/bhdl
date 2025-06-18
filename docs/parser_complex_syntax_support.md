# BHDL Parser Complex Syntax Support

This document describes the complex syntax features added to the BHDL parser to support real-world component modules like LED and LM7805.

## Overview

The BHDL parser has been extended to support advanced language features required by the standard library. These features enable more expressive and maintainable hardware descriptions.

## Implemented Features

### 1. Const Declarations with Type Annotations

**Syntax:**
```bhdl
const name: type = value;
```

**Example:**
```bhdl
const forward_voltage: voltage = 2.0V;
const params: LEDParams = LED_PARAMS_RED;
```

**Implementation:**
- Added `parse_const_decl()` in `top_level.rs`
- Creates `PARAM_DECL` AST nodes
- Supports any type reference and initializer expression

### 2. Smart Unit Tokenization

**Problem:** Single-letter units (V, A, F, etc.) conflicted with pin names and identifiers.

**Solution:** Context-aware tokenization - units are only recognized after numbers.

**Examples:**
```bhdl
pin A: signal in;      // 'A' is an identifier
const current = 2.0A;  // 'A' is a unit
```

**Implementation:**
- Post-processing in `map_token_stream()` in `lib.rs`
- Checks if previous non-whitespace token was NUMBER
- Single letters (A, F, H, K, V, W, s) treated as IDENT unless preceded by number

### 3. Expression Features

#### Ternary Operator
**Syntax:** `condition ? true_expr : false_expr`

**Example:**
```bhdl
const params: LEDParams = 
    color == "red" ? LED_PARAMS_RED :
    color == "yellow" ? LED_PARAMS_YELLOW :
    LED_PARAMS_DEFAULT;
```

**Implementation:**
- Already existed in `expressions.rs`
- Binding power: (4, 3) - right-associative
- Creates `TERNARY_EXPR` nodes

#### String Comparisons
**Syntax:** `string1 == string2`, `string1 != string2`

**Example:**
```bhdl
color == "red"
package != "TO-220"
```

**Implementation:**
- EQEQ and NEQ operators with binding power (14, 15)
- Works with any expression types including STRING tokens

#### Logical OR Operator
**Syntax:** `expr1 || expr2`

**Example:**
```bhdl
package == "TO-220-5" || package == "TO-263-5"
```

**Implementation:**
- PIPEPIPE operator with binding power (4, 5)
- Lower precedence than comparisons, allowing natural grouping

#### Member Access
**Syntax:** `object.member`

**Example:**
```bhdl
attribute forward_voltage = params.forward_voltage;
```

**Implementation:**
- DOT operator handling in `parse_primary_expr()`
- Currently creates PIN_REF nodes (could be generalized to MEMBER_ACCESS)

### 4. Conditional Pin Declarations

**Syntax:**
```bhdl
pin name: type direction when condition;
```

**Example:**
```bhdl
pin EN: signal in when package == "TO-220-5" || package == "TO-263-5";
```

**Implementation:**
- Extended `parse_module_pin_decl()` in `top_level.rs`
- Optional WHEN_KW followed by expression
- Condition becomes part of PIN_DECL node

### 5. Module Aliases

**Syntax:**
```bhdl
alias new_name = existing_module;
alias module new_name = existing_module;  // optional 'module' keyword
```

**Example:**
```bhdl
alias 7805 = LM7805;
alias L7805 = LM7805;
```

**Design Decision:**
- Originally, LM7805 used `module 7805 = LM7805;` syntax
- Changed to explicit `alias` keyword to avoid parser ambiguity
- Cleaner grammar, no lookahead required

**Implementation:**
- Added ALIAS_KW to syntax kinds and lexer
- Added `parse_alias_stmt()` in `top_level.rs`
- Supports numeric names (NUMBER tokens) as alias names
- Creates ALIAS nodes in AST

### 6. Import Destructuring

**Syntax:**
```bhdl
import { Item1, Item2, ... } from "path/to/file.bhdl";
```

**Example:**
```bhdl
import { LEDParams, LED_PARAMS_RED, LED_PARAMS_GREEN } from "../electrical_params.bhdl";
```

**Implementation:**
- Extended `parse_import_stmt()` in `v2_parsing.rs`
- Added FROM_KW to lexer
- Supports both simple imports and destructuring imports

## Token Precedence and Binding Powers

### Operator Precedence (lowest to highest):
1. Flow operators: `|>` (1,2), `<=>` (1,2)
2. Connection operators: `->`, `<->` (2,3)
3. Ternary: `?` `:` (4,3)
4. Logical OR: `||` (4,5)
5. Logical AND: `&&` (6,7)
6. Bitwise OR: `|` (8,9)
7. Bitwise XOR: `^` (10,11)
8. Bitwise AND: `&` (12,13)
9. Equality: `==`, `!=` (14,15)
10. Comparison: `<`, `>`, `<=`, `>=` (14,15)
11. Addition/Subtraction: `+`, `-` (16,17)
12. Multiplication/Division: `*`, `/`, `%` (18,19)

### Unary Operators:
- Prefix: `+`, `-`, `!`, `~` with binding power 13

## AST Node Types

### New/Modified AST Nodes:
- `PARAM_DECL` - Const declarations
- `PIN_DECL` - Pin declarations (with optional when clause)
- `TERNARY_EXPR` - Ternary conditional expressions
- `ALIAS` - Module alias declarations
- `IMPORT_TARGET_GROUP` - Destructuring import list

## Testing

### Test Programs Created:
1. `test_const_parsing.rs` - General parser testing
2. `debug_lexer_const.rs` - Token debugging
3. `test_parser_debug.rs` - Detailed error reporting

### Validated Modules:
- `bhdl-stdlib/passives/led.bhdl` - Uses all features except aliases
- `bhdl-stdlib/regulators/lm7805.bhdl` - Uses conditional pins and aliases

## Migration Notes

### For Existing BHDL Code:
1. Replace `module Name = Target;` with `alias Name = Target;`
2. Replace `or` operator with `||` in conditions
3. Ensure const declarations include type annotations

### For Parser Extensions:
1. Unit handling is now in `map_token_stream()`, not lexer
2. Keywords must be added to both `syntax.rs` and `lexer.rs`
3. Numeric identifiers require special handling in relevant contexts

## Future Considerations

### Remaining TODO Items:
1. Type definitions: `type Name = { ... }`
2. Struct/record literals: `{ field1: value1, field2: value2 }`
3. Nullable types: `type?`
4. Null literal: `null`

These features are not currently used in stdlib modules but may be needed for future extensions.

### Potential Improvements:
1. Generalize PIN_REF to MEMBER_ACCESS for dot notation
2. Add OR_KW as alternative to || operator
3. Support method calls vs just member access
4. Add better error recovery for complex expressions