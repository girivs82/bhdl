use bhdl_parser::parse;

fn main() {
    println!("Test 1: Module without parameters");
    let code1 = r#"
entity SimpleModule {
    pin VCC: power in;
}
"#;
    let result1 = parse(code1);
    println!("Errors: {}", result1.errors().len());
    for err in result1.errors() {
        println!("  - {}", err.message);
    }
    
    println!("\nTest 2: Module with empty parameters");
    let code2 = r#"
entity ModuleWithEmptyParams() {
    pin VCC: power in;
}
"#;
    let result2 = parse(code2);
    println!("Errors: {}", result2.errors().len());
    for err in result2.errors() {
        println!("  - {}", err.message);
    }
    
    println!("\nTest 3: Module with one parameter");
    let code3 = r#"
entity ModuleWithParam(value: resistance) {
    pin VCC: power in;
}
"#;
    let result3 = parse(code3);
    println!("Errors: {}", result3.errors().len());
    for err in result3.errors() {
        println!("  - {}", err.message);
    }
    
    println!("\nTest 4: Module with parameter and default");
    let code4 = r#"
entity ModuleWithDefault(value: resistance = 10k) {
    pin VCC: power in;
}
"#;
    let result4 = parse(code4);
    println!("Errors: {}", result4.errors().len());
    for err in result4.errors() {
        println!("  - {}", err.message);
    }
}