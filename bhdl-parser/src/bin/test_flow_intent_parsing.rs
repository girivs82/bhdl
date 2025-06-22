// Test program to demonstrate flow-based intent parsing
// This shows how we could modify the parser to support intents without 'net' keyword

use bhdl_parser::{parse, SyntaxKind};

fn main() {
    println!("Flow-Based Intent Parsing Test\n");
    
    // Test the current implementation first
    println!("=== Current Implementation (requires 'net' keyword) ===");
    let current_syntax = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    net critical: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#;
    
    let result = parse(current_syntax);
    if result.errors().is_empty() {
        println!("✓ Current syntax parses successfully");
        find_intent_nodes(&result.syntax(), 0);
    } else {
        println!("✗ Parse errors: {:?}", result.errors());
    }
    
    // Show what we want to support (flow-based)
    println!("\n=== Desired Flow-Based Syntax (no 'net' keyword) ===");
    let desired_syntaxes = vec![
        (
            "Named flow with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // This is what we want - natural flow syntax
    critical_path: VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        (
            "Direct connection with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Anonymous flow with intent
    VCC -> Res(10k).1 -> LED(red).A for delay(3ms);
}
"#
        ),
        (
            "@ syntax with intent",
            r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Named net with @ and intent
    VCC @filtered-> Cap(0.1uF).1 -> amp.in for anti_alias(before: adc);
}
"#
        ),
    ];
    
    for (name, syntax) in desired_syntaxes {
        println!("\n--- {} ---", name);
        let result = parse(syntax);
        if result.errors().is_empty() {
            println!("✓ Would parse successfully with updated parser");
        } else {
            println!("✗ Currently fails (expected): {:?}", result.errors().get(0).map(|e| &e.message));
        }
    }
    
    // Show the parser changes needed
    println!("\n=== Parser Changes Required ===");
    println!("1. Modify parse_flow_stmt() to check for intent clause after flow expression");
    println!("2. Modify parse_v2_connection_expr() to check for intent clause before semicolon");
    println!("3. Update flow expression parsing to handle @ syntax with intent");
    println!("\nExample implementation:");
    println!("
// In parse_flow_stmt():
pub(crate) fn parse_flow_stmt(&mut self) {{
    self.builder.start_node(SyntaxKind::FLOW_STMT.into());
    self.expect(SyntaxKind::IDENT); // Flow name
    self.expect(SyntaxKind::COLON);
    
    // Parse flow expression
    self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
    self.parse_expr(0);
    self.builder.finish_node();
    
    // NEW: Check for optional intent clause
    if self.has_intent_clause() {{
        self.parse_intent_clause();
    }}
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}}

// In parse_v2_connection_expr():
pub(crate) fn parse_v2_connection_expr(&mut self) {{
    self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
    self.parse_expr(0);
    
    // NEW: Check for optional intent clause
    if self.has_intent_clause() {{
        self.parse_intent_clause();
    }}
    
    self.expect(SyntaxKind::SEMI);
    self.builder.finish_node();
}}
");
}

fn find_intent_nodes(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    if node.kind() == SyntaxKind::INTENT_CLAUSE {
        println!("{}Found INTENT_CLAUSE: {}", indent, node.text());
    }
    
    for child in node.children() {
        find_intent_nodes(&child, depth + 1);
    }
}