use bhdl_parser::parse;

fn main() {
    let input = r#"
board MixedTest {
    power VCC = 5V @ 2A;
    power VCC_3V3 = 3.3V @ 1A;
    ground GND;
    
    // Interface with flow
    main_spi: SPI(3.3V, 10M);
    data_flow: sensor |> main_spi |> processor;
    
    // Named handle with array access
    generate for i in 0..4 {
        VCC_3V3 -> pullup[i]: Res(10k).1 -> button[i]: Switch().1;
        button[i].2 -> GND;
    }
    
    // Conditional with module
    if (use_filter) {
        module NoiseFilter(VCC, VCC_FILTERED, GND) {
            flow: VCC |> filtering |> VCC_FILTERED;
        }
    }
}
"#;
    
    let result = parse(input);
    
    println!("Errors: {}", result.errors().len());
    for (i, error) in result.errors().iter().enumerate() {
        println!("Error {}: {}", i + 1, error.message);
    }
}