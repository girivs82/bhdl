//! Quick test for @ net ref parsing

#[cfg(test)]
mod tests {
    use crate::parse;
    use crate::SyntaxKind;
    
    #[test]
    fn test_at_net_ref_parsing() {
        let source = r#"
board TestNet {
    VCC @FILTERED-> r1: Res(10k).1;
    @FILTERED -> c1: Cap(100n).1;
}
"#;
        
        let result = parse(source);
        assert!(result.errors().is_empty(), "Parse errors: {:?}", result.errors());
        
        // Walk the syntax tree looking for NET_REF nodes
        let mut net_ref_count = 0;
        let mut found_at_tokens = 0;
        
        for child in result.syntax().descendants_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(node) => {
                    if node.kind() == SyntaxKind::NET_REF {
                        net_ref_count += 1;
                        // Check that this NET_REF contains @FILTERED
                        let text = node.text().to_string();
                        assert!(text.contains("FILTERED"), "NET_REF should contain 'FILTERED', got: {}", text);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::AT {
                        found_at_tokens += 1;
                    }
                }
            }
        }
        
        // We should have found 2 NET_REF nodes
        assert_eq!(net_ref_count, 2, "Expected 2 NET_REF nodes");
        assert_eq!(found_at_tokens, 2, "Expected 2 @ tokens");
        
        println!("✓ @ net ref parsing test passed!");
        println!("  Found {} NET_REF nodes", net_ref_count);
        println!("  Found {} @ tokens", found_at_tokens);
    }
}