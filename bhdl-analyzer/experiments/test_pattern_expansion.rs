use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::passes::{build_instance_registry, expand_power_domains};
use std::fs;

fn main() {
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/realistic/test_advanced_patterns.bhdl".to_string());

    println!("Testing Advanced Pattern Matching - End-to-End");
    println!("==============================================\n");
    println!("Reading file: {}\n", test_file);

    let source = fs::read_to_string(&test_file)
        .expect("Failed to read test file");

    let parse_result = parse(&source);

    if !parse_result.errors().is_empty() {
        println!("❌ PARSE ERRORS:");
        for error in parse_result.errors() {
            println!("  {:?}", error);
        }
        return;
    }

    let source_file = SourceFile::cast(parse_result.syntax()).expect("Failed to cast to SourceFile");

    println!("✅ File parsed successfully\n");

    // Build instance registry
    println!("Building instance registry...");
    let registry = build_instance_registry(&source_file);

    println!("  Registered {} instances\n", registry.get_instance_names().len());

    // Expand power domains
    println!("Expanding power domains...\n");
    let expansion = expand_power_domains(&source_file, &registry);

    // Verify results
    println!("\nExpansion Results");
    println!("=================\n");

    if !expansion.diagnostics.is_empty() {
        println!("Diagnostics:");
        for diag in &expansion.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }

    println!("Total connections: {}", expansion.connections.len());
    println!();

    // Group connections by source net
    let mut by_net: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
    for conn in &expansion.connections {
        by_net.entry(conn.source_net.clone()).or_insert_with(Vec::new).push(conn);
    }

    // Expected results for each pattern
    let expected = vec![
        ("VCC_EVEN", 8, vec![0, 2, 4, 6, 8, 10, 12, 14]),
        ("VCC_ODD", 8, vec![1, 3, 5, 7, 9, 11, 13, 15]),
        ("VCC_SPECIAL", 4, vec![0, 5, 10, 15]),
        ("VCC_SAMPLED", 6, vec![0, 3, 6, 9, 12, 15]),
        ("VCC_RANGE", 5, vec![0, 1, 2, 3, 4]),
        ("VCC_SINGLE", 1, vec![7]),
        ("VCC_ALL", 16, (0..=15).collect()),
    ];

    let mut all_passed = true;

    for (net, expected_count, expected_indices) in expected {
        let connections = by_net.get(net).map(|v| v.as_slice()).unwrap_or(&[]);

        println!("Power Domain @{}:", net);
        println!("  Expected: {} connections to indices {:?}", expected_count, expected_indices);
        println!("  Actual: {} connections", connections.len());

        if connections.len() == expected_count {
            // Extract actual indices from connections
            let mut actual_indices: Vec<i32> = Vec::new();
            for conn in connections {
                // Try to extract index from component name or pin name
                let index = extract_index(&conn.component).or_else(|| extract_index(&conn.pin));
                if let Some(idx) = index {
                    actual_indices.push(idx);
                }
            }
            actual_indices.sort();

            if actual_indices == expected_indices {
                println!("  ✅ PASS - Correct indices: {:?}", actual_indices);
            } else {
                println!("  ❌ FAIL - Wrong indices");
                println!("     Expected: {:?}", expected_indices);
                println!("     Got: {:?}", actual_indices);
                all_passed = false;
            }
        } else {
            println!("  ❌ FAIL - Wrong connection count");
            all_passed = false;
        }
        println!();
    }

    println!("========================================");
    if all_passed {
        println!("✅ All pattern expansions passed!");
    } else {
        println!("❌ Some pattern expansions failed");
        std::process::exit(1);
    }
}

/// Extract index from a string (component or pin name)
fn extract_index(s: &str) -> Option<i32> {
    // Try array notation: sensor[5] or VCC[5]
    if let Some(start) = s.find('[') {
        if let Some(end) = s.find(']') {
            let index_str = &s[start+1..end];
            if let Ok(idx) = index_str.parse() {
                return Some(idx);
            }
        }
    }

    // Try underscore notation: sensor_0
    if let Some(pos) = s.rfind('_') {
        let suffix = &s[pos+1..];
        if suffix.chars().all(|c| c.is_numeric()) && !suffix.is_empty() {
            if let Ok(idx) = suffix.parse() {
                return Some(idx);
            }
        }
    }

    // Try trailing digits: sensor0
    let digits: String = s.chars()
        .rev()
        .take_while(|c| c.is_numeric())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if !digits.is_empty() {
        if let Ok(idx) = digits.parse() {
            return Some(idx);
        }
    }

    None
}
