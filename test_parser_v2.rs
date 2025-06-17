use bhdl_parser::parse;

fn test_parse(name: &str, source: &str) {
    println!("\n=== Testing: {} ===", name);
    println!("Source:\n{}", source);
    
    let parsed = parse(source);
    
    // Check for errors
    if parsed.errors().is_empty() {
        println!("✓ Parsing succeeded!");
    } else {
        println!("✗ Parsing failed with errors:");
        for error in parsed.errors() {
            println!("  - {:?}", error);
        }
    }
}

fn main() {
    // Test 1: Flow statement with |> operator
    test_parse("Flow statement with |> operator", r#"
board TestFlow {
    power_flow: VIN |> regulation |> VOUT;
}
"#);

    // Test 2: Direct component instantiation
    test_parse("Direct component instantiation", r#"
board TestDirect {
    VCC -> Res(4.7kΩ).1 -> LED(red).A;
}
"#);

    // Test 3: Power/ground declarations
    test_parse("Power/ground declarations", r#"
board TestPower {
    power VIN = 12V @ 2A;
    ground GND;
}
"#);

    // Test 4: Named flow statement
    test_parse("Named flow statement", r#"
board TestNamedFlow {
    my_flow: A |> B |> C;
}
"#);

    // Test 5: Mixed statements
    test_parse("Mixed statements", r#"
board TestMixed {
    power VCC = 5V;
    ground GND;
    
    power_flow: VCC |> filtering |> output;
    
    VCC -> Res(1kΩ).1 -> LED.A;
    LED.K -> GND;
}
"#);

    // Test 6: Component instance syntax
    test_parse("Component instance syntax", r#"
board TestComponent {
    component U1: LM7805 {
        IN = VIN,
        GND = GND,
        OUT = VOUT
    }
}
"#);

    // Test 7: Generate construct
    test_parse("Generate construct", r#"
board TestGenerate {
    generate for i in 0..7 {
        VCC -> Res(1kΩ).1 -> LED[i].A;
        LED[i].K -> GND;
    }
}
"#);

    // Test 8: Flow block syntax (if it exists)
    test_parse("Flow block syntax", r#"
board TestFlowBlock {
    flow {
        A -> B -> C;
    }
}
"#);

    // Test 9: Just a flow expression without label
    test_parse("Unlabeled flow expression", r#"
board TestUnlabeledFlow {
    A |> B |> C;
}
"#);
}