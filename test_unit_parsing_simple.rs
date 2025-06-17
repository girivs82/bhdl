use bhdl_parser::parse;

fn main() {
    // Test parsing with abbreviated units
    println!("=== Testing parser with 'Res(10k)' ===");
    let test_input = r#"
    board test {
        components {
            R1: Res(10k);
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
    println!("\n=== Testing parser with 'Res(10kOhm)' ===");
    let test_input2 = r#"
    board test {
        components {
            R1: Res(10kOhm);
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
        ("10k", "10kOhm"),     // kilo-ohm
        ("1M", "1MOhm"),       // mega-ohm  
        ("100n", "100nF"),     // nano-farad
        ("10u", "10uF"),       // micro-farad
        ("1m", "1mH"),         // milli-henry
    ];
    
    for (abbrev, full) in test_cases {
        println!("\nTesting {} vs {}:", abbrev, full);
        
        // Test abbreviated
        let input_abbrev = format!(r#"
        board test {{
            components {{
                C1: Cap({});
            }}
        }}
        "#, abbrev);
        
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
                C1: Cap({});
            }}
        }}
        "#, full);
        
        let result = parse(&input_full);
        print!("  {} - ", full);
        if !result.errors().is_empty() {
            println!("FAILED: {}", result.errors()[0].message);
        } else {
            println!("OK");
        }
    }
}