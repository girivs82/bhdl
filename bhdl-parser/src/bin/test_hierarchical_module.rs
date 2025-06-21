use bhdl_parser::parse;

fn main() {
    let test_code = r#"
module BuckConverter(vout: voltage = 3.3V, imax: current = 2A) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: signal in;
    
    attribute description = "Configurable buck converter";
    
    // Module instance within module
    controller: PWMController {
        VCC <- VIN;
        OUT -> switch_node;
        EN <- EN;
    }
    
    // Component instance
    inductor: Inductor(10uH);
    
    // Connection
    switch_node -> inductor.1;
}

board TestBoard {
    power VIN_12V = 12V @ 3A;
    
    // Module instantiation with parameters
    buck: BuckConverter(vout=5V, imax=3A) {
        VIN <- VIN_12V;
        VOUT -> RAIL_5V;
        EN <- enable_signal;
        
        // Scoped attribute
        attribute controller.frequency = 500kHz;
    }
}
"#;

    println!("Parsing hierarchical module test...\n");
    let result = parse(test_code);
    
    println!("Parse errors: {}", result.errors().len());
    for error in result.errors() {
        println!("  - {}", error.message);
    }
    
    println!("\nSyntax tree:");
    println!("{:#?}", result.syntax());
}