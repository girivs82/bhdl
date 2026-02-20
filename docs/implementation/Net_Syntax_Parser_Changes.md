# Parser Changes for Clear Net Syntax

## Goal

Implement clear, unambiguous net syntax:
- `@name` for nets (creation and reference)
- `:` only for component handles
- Remove confusing `name: Component()` pattern for nets

## Current Parser Issues

### 1. Ambiguous Net Assignment Pattern

The current parser seems to support this confusing pattern:
```bhdl
fuse.2 -> protected_vin: TVSDiode(15V).K;
```

This creates a net `protected_vin` but uses `:` which is also used for component handles.

### 2. Intent Requires 'net' Keyword

Current implementation:
```bhdl
net critical: VCC -> Res(10k).1 for delay(3ms);
```

This introduces a declarative keyword against BHDL philosophy.

## Proposed Parser Changes

### 1. Remove Net Assignment Pattern

Delete or deprecate parsing of:
```rust
// REMOVE: Don't allow net_name: Component() pattern
connection -> name: Component().pin
```

### 2. Enhance @ Net Syntax

Update flow expression parsing to handle @ in flows:

```rust
// parse_flow_segment
fn parse_flow_segment(&mut self) {
    match self.peek() {
        Some(SyntaxKind::AT) => {
            // @ indicates net creation/reference
            self.parse_net_in_flow();
        }
        Some(SyntaxKind::IDENT) => {
            // Could be component ref or start of instantiation
            self.parse_component_or_ref();
        }
        _ => { /* other cases */ }
    }
}

fn parse_net_in_flow(&mut self) {
    self.expect(SyntaxKind::AT);
    self.builder.start_node(SyntaxKind::NET_REF.into());
    self.expect(SyntaxKind::IDENT);
    self.builder.finish_node();
}
```

### 3. Update Flow Statement for Intent

Allow optional intent on flow statements:

```rust
pub(crate) fn parse_flow_stmt(&mut self) {
    self.builder.start_node(SyntaxKind::FLOW_STMT.into());
    self.expect(SyntaxKind::IDENT); // Flow name
    self.expect(SyntaxKind::COLON);
    
    // Parse flow expression
    self.parse_flow_expr();
    
    // NEW: Check for optional intent
    if self.peek() == Some(SyntaxKind::FOR_KW) {
        self.parse_intent_clause();
    }
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}
```

### 4. Update Connection Statement for Intent

```rust
pub(crate) fn parse_connection_stmt(&mut self) {
    self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
    
    // Parse the flow expression
    self.parse_flow_expr();
    
    // NEW: Check for optional intent
    if self.peek() == Some(SyntaxKind::FOR_KW) {
        self.parse_intent_clause();
    }
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}
```

### 5. Handle @ in Flow Expressions

```rust
fn parse_flow_expr(&mut self) {
    self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
    
    loop {
        // Parse segment (component, net, expression)
        self.parse_flow_segment();
        
        // Check for flow operators
        match self.peek() {
            Some(SyntaxKind::ARROW) => {
                self.bump(); // ->
                continue;
            }
            Some(SyntaxKind::FLOW_OP) => {
                self.bump(); // |>
                continue;
            }
            _ => break,
        }
    }
    
    self.builder.finish_node();
}
```

### 6. Remove 'net' Keyword Requirement

Remove or deprecate:
```rust
pub(crate) fn parse_net_flow_stmt(&mut self) {
    // DELETE THIS FUNCTION - no more 'net' keyword
}
```

## AST Changes

### 1. Update FlowStmt Node

```rust
pub struct FlowStmt {
    // ... existing fields ...
    pub intent_clause: Option<IntentClause>,
}
```

### 2. Update ConnectionStmt Node

```rust
pub struct ConnectionStmt {
    // ... existing fields ...
    pub intent_clause: Option<IntentClause>,
}
```

### 3. Add NetRef Node

```rust
pub struct NetRef {
    pub name: String,
    pub span: Span,
}
```

## Error Messages

Provide clear error messages for common mistakes:

```rust
// When user forgets @ for net reference
if self.undefined_identifier_might_be_net(name) {
    self.error_with_help(
        format!("Undefined identifier '{}'", name),
        "If this is a net, use @ prefix: @{}".format(name)
    );
}

// When user tries to use : for net
if self.looks_like_net_assignment() {
    self.error_with_help(
        "Invalid syntax for net creation",
        "Use @ prefix for nets: source -> @net_name -> destination"
    );
}
```

## Migration Support

### Phase 1: Deprecation Warnings

```rust
// In parse_net_flow_stmt
self.warning(
    "The 'net' keyword is deprecated. \
     Use 'flow_name: connection for intent' instead"
);

// For old net assignment pattern
self.warning(
    "The 'name: Component()' pattern for nets is deprecated. \
     Use '@name' instead: source -> @name -> Component()"
);
```

### Phase 2: Migration Tool

Create a tool to automatically migrate old syntax:

```rust
// Old: net critical: flow for intent;
// New: critical: flow for intent;

// Old: source -> net_name: Component().pin
// New: source -> @net_name -> Component().pin
```

## Testing

Create comprehensive tests for all patterns:

```bhdl
// Test: Component handles
test "component_handles" {
    VCC -> r1: Res(10k).1;          // ✓ Creates component r1
    r1.2 -> led: LED(red).A;        // ✓ References r1, creates led
}

// Test: Net creation and reference
test "net_patterns" {
    VCC -> @filtered -> amp.IN;     // ✓ Creates net @filtered
    @filtered -> Cap(100n).1;       // ✓ References net @filtered
}

// Test: Intent on flows
test "intent_flows" {
    path: VCC -> @protected -> load for safety_critical;     // ✓
    VCC -> Res(1k).1 -> LED.A for indicator;                // ✓
}

// Test: Deprecated patterns
test "deprecated" {
    net critical: VCC -> load for intent;          // ⚠️ Warning
    source -> protected: Component().pin;          // ⚠️ Warning
}
```

## Summary

These parser changes will:
1. Eliminate confusing syntax ambiguity
2. Make @ the sole indicator for nets
3. Keep : exclusively for component handles
4. Enable natural intent attachment without 'net' keyword
5. Maintain BHDL's flow-based philosophy

The result is a cleaner, more intuitive syntax that's easier to learn and use.