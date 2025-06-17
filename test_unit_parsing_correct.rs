use bhdl_parser::parse;

fn main() {
    // Test parsing with abbreviated units using correct syntax
    println!("=== Testing parser with 'component R1: Res(10k)' ===");
    let test_input = r#"
    board test {
        components {
            component R1: Res(10k);
        }
    }
    "#;
    
    let result = parse(test_input);
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("Parsed successfully!");
    }
    
    // Test parsing with full units
    println!("\n=== Testing parser with 'component R1: Res(10kOhm)' ===");
    let test_input2 = r#"
    board test {
        components {
            component R1: Res(10kOhm);
        }
    }
    "#;
    
    let result2 = parse(test_input2);
    if !result2.errors().is_empty() {
        println!("Parse errors:");
        for error in result2.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("Parsed successfully!");
    }
    
    // Test other abbreviated units
    println!("\n=== Testing parser with various abbreviated units ===");
    let test_cases = vec![
        ("10k", "10kOhm", "Res"),     // kilo-ohm
        ("1M", "1MOhm", "Res"),       // mega-ohm  
        ("100n", "100nF", "Cap"),     // nano-farad
        ("10u", "10uF", "Cap"),       // micro-farad
        ("1m", "1mH", "Ind"),         // milli-henry
    ];
    
    for (abbrev, full, comp_type) in test_cases {
        println!("\nTesting {} vs {} for {}:", abbrev, full, comp_type);
        
        // Test abbreviated
        let input_abbrev = format!(r#"
        board test {{
            components {{
                component C1: {}({});
            }}
        }}
        "#, comp_type, abbrev);
        
        let result = parse(&input_abbrev);
        print!("  {} - ", abbrev);
        if !result.errors().is_empty() {
            println!("FAILED: {}", result.errors()[0].message);
        } else {
            println!("OK");
        }
        
        // Test full
        let input_full = format!(r#"
        board test {{
            components {{
                component C1: {}({});
            }}
        }}
        "#, comp_type, full);
        
        let result = parse(&input_full);
        print!("  {} - ", full);
        if !result.errors().is_empty() {
            println!("FAILED: {}", result.errors()[0].message);
        } else {
            println!("OK");
        }
    }
    
    // Let's also test what happens when "k" appears in the argument list
    println!("\n\n=== Detailed test of 'Res(10k)' parsing ===");
    let detailed_test = r#"
    board test {
        components {
            component R1: Res(10k);
        }
    }
    "#;
    
    let result = parse(detailed_test);
    println!("Number of errors: {}", result.errors().len());
    for (i, error) in result.errors().iter().enumerate() {
        println!("Error {}: {}", i + 1, error.message);
    }
}