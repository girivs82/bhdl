use bhdl_parser::parse;

#[test]
fn test_basic_interface() {
    let code = r#"
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
}
"#;
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}

#[test]
fn test_interface_with_requirements() {
    let code = r#"
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}
"#;
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}

#[test]
fn test_interface_with_optional_signals() {
    let code = r#"
interface UART {
    signal TX: out;
    signal RX: in;
    signal RTS: out optional;
    signal CTS: in optional;
}
"#;
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}

#[test]
fn test_parameterized_interface() {
    let code = r#"
interface SPI(frequency: frequency = 1MHz, mode: int = 0) {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out;
}
"#;
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}

#[test]
fn test_interface_perspectives() {
    let code = r#"
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
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}

#[test]
fn test_hierarchical_interface() {
    let code = r#"
interface USB3 {
    interface SuperSpeed {
        signal TXP: out;
        signal TXN: out;
        signal RXP: in;
        signal RXN: in;
    }
    
    interface USB2 {
        signal DP: inout;
        signal DN: inout;
    }
}
"#;
    
    let parse_result = parse(code);
    assert!(parse_result.errors().is_empty(), "Parse errors: {:?}", parse_result.errors());
}