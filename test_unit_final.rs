use bhdl_parser::parse;

fn main() {
    // Test with correct component instantiation syntax
    println!("=== Testing abbreviated units in component parameters ===\n");
    
    // Test 1: Simple resistor with abbreviated unit
    println!("Test 1: Resistor with 'parameter value = 10k'");
    let test1 = r#"
    board test {
        components {
            component Res R1 {
                parameter value = 10k;
            };
        }
    }
    "#;
    
    let result = parse(test1);
    if !result.errors().is_empty() {
        println!("  FAILED - Errors:");
        for error in result.errors() {
            println!("    - {}", error.message);
        }
    } else {
        println!("  SUCCESS");
    }
    
    // Test 2: Same resistor with full unit
    println!("\nTest 2: Resistor with 'parameter value = 10kOhm'");
    let test2 = r#"
    board test {
        components {
            component Res R1 {
                parameter value = 10kOhm;
            };
        }
    }
    "#;
    
    let result = parse(test2);
    if !result.errors().is_empty() {
        println!("  FAILED - Errors:");
        for error in result.errors() {
            println!("    - {}", error.message);
        }
    } else {
        println!("  SUCCESS");
    }
    
    // Test 3: Let's test if single "k" is being parsed as IDENT
    println!("\nTest 3: Testing what happens with just 'k' after a number");
    let test3 = r#"
    board test {
        components {
            component Res R1 {
                parameter value = 10 k;
            };
        }
    }
    "#;
    
    let result = parse(test3);
    if !result.errors().is_empty() {
        println!("  FAILED - Errors:");
        for error in result.errors() {
            println!("    - {}", error.message);
        }
    } else {
        println!("  SUCCESS");
    }
    
    // Test 4: Various abbreviated units
    println!("\n=== Testing various abbreviated units ===");
    let units_to_test = vec![
        ("k", "kilo", "10k"),
        ("M", "mega", "1M"),
        ("m", "milli", "100m"),
        ("u", "micro", "10u"),
        ("n", "nano", "100n"),
        ("p", "pico", "10p"),
    ];
    
    for (abbrev, name, value) in units_to_test {
        println!("\nTesting {} ({}):", abbrev, name);
        
        let test_code = format!(r#"
        board test {{
            components {{
                component TestComp C1 {{
                    parameter value = {};
                }};
            }}
        }}
        "#, value);
        
        let result = parse(&test_code);
        print!("  {} - ", value);
        if !result.errors().is_empty() {
            // Find the specific error about the unit
            let unit_error = result.errors().iter()
                .find(|e| e.message.contains("COMMA") || e.message.contains("';'"))
                .map(|e| &e.message)
                .unwrap_or(&result.errors()[0].message);
            println!("FAILED: {}", unit_error);
        } else {
            println!("SUCCESS");
        }
    }
    
    // Test 5: Check if the issue is in expression parsing
    println!("\n=== Minimal test case ===");
    let minimal = r#"
    board test {
        components {
            component Res R1 {
                parameter resistance = 10k;
            };
        }
    }
    "#;
    
    let result = parse(minimal);
    println!("\nMinimal test with '10k':");
    println!("  Total errors: {}", result.errors().len());
    for (i, error) in result.errors().iter().enumerate() {
        println!("  Error {}: {}", i + 1, error.message);
    }
}