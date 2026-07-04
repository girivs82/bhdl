use bhdl_parser::parse;

fn main() {
    println!("Testing Interface Parsing\n");
    
    // Test 1: Basic interface
    println!("Test 1: Basic interface");
    let code1 = r#"
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
}
"#;
    let result1 = parse(code1);
    println!("Errors: {:?}", result1.errors());
    println!("Success: {}\n", result1.errors().is_empty());
    
    // Test 2: Interface with requirements
    println!("Test 2: Interface with requirements");
    let code2 = r#"
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}
"#;
    let result2 = parse(code2);
    println!("Errors: {:?}", result2.errors());
    println!("Success: {}\n", result2.errors().is_empty());
    
    // Test 3: Optional signals
    println!("Test 3: Optional signals");
    let code3 = r#"
interface UART {
    signal TX: out;
    signal RX: in;
    signal RTS: out optional;
    signal CTS: in optional;
}
"#;
    let result3 = parse(code3);
    println!("Errors: {:?}", result3.errors());
    println!("Success: {}\n", result3.errors().is_empty());
    
    // Test 4: Parameterized interface
    println!("Test 4: Parameterized interface");
    let code4 = r#"
interface SPI(frequency: frequency = 1MHz, mode: int = 0) {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out;
}
"#;
    let result4 = parse(code4);
    println!("Errors: {:?}", result4.errors());
    println!("Success: {}\n", result4.errors().is_empty());
    
    // Test 5: Perspectives
    println!("Test 5: Perspectives");
    let code5 = r#"
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out;
    }
    
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
        signal SCK: in;
        signal CS: in;
    }
}
"#;
    let result5 = parse(code5);
    println!("Errors: {:?}", result5.errors());
    println!("Success: {}\n", result5.errors().is_empty());
}