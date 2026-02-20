use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::{analyze, hierarchical_symbol_table::{HierarchicalSymbolTable, SymbolPath}};

fn main() {
    let test_code = r#"
entity PWMController(frequency: frequency = 100kHz) {
    pin VCC: power in;
    pin OUT: signal out;
    pin EN: signal in;

    parameter duty_cycle: percentage = 50%;
}

entity PowerRegulator(vin: voltage, vout: voltage = 3.3V) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: signal in;

    parameter switching_freq: frequency = 500kHz;

    // Nested entity instance
    pwm: PWMController(frequency=switching_freq) {
        VCC <- VIN;
        OUT -> switch_node;
        EN <- EN;
    }
    
    // Component instance
    l1: Inductor(10uH);
    
    // Connection
    switch_node -> l1.1;
}

board TestBoard {
    power VIN_12V = 12V @ 3A;
    
    // Entity instance with parameters
    reg1: PowerRegulator(vin=12V, vout=5V) {
        VIN <- VIN_12V;
        VOUT -> RAIL_5V;
        EN <- enable_signal;
        
        // Scoped attribute
        attribute pwm.frequency = 500kHz;
    }
    
    // Another regulator instance
    reg2: PowerRegulator(vin=12V, vout=3.3V) {
        VIN <- VIN_12V;
        VOUT -> RAIL_3V3;
        EN <- enable_signal2;
    }
}
"#;

    println!("Testing hierarchical symbol table...\n");
    
    // Parse the code
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Run analysis
    println!("Running analysis...");
    let analysis_result = analyze(&source_file);
    
    println!("\nAnalysis diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("  - {}", diag.message);
    }
    
    // Create hierarchical symbol table
    let mut hier_table = HierarchicalSymbolTable::new(
        analysis_result.global_scope.clone(),
        analysis_result.definition_scopes.clone()
    );
    
    // Test symbol resolution
    println!("\n--- Testing Symbol Resolution ---");
    
    // Test paths
    let test_paths = vec![
        "TestBoard",
        "PowerRegulator", 
        "PWMController",
        "reg1",
        "reg2",
        "VIN_12V",
    ];
    
    for path_str in test_paths {
        let path = SymbolPath::from_str(path_str);
        let result = hier_table.resolve_path(&path, None);
        match result {
            Some(symbol) => {
                println!("✓ {} -> {:?} (kind: {:?})", path_str, symbol.name, symbol.kind);
            }
            None => {
                println!("✗ {} -> Not found", path_str);
            }
        }
    }
    
    // Test hierarchical paths (these would need the instance registration to work)
    println!("\n--- Testing Hierarchical Paths ---");
    let hier_paths = vec![
        "reg1.pwm",
        "reg1.pwm.frequency",
        "reg1.switching_freq",
        "reg2.pwm.duty_cycle",
    ];
    
    for path_str in hier_paths {
        let path = SymbolPath::from_str(path_str);
        // Note: This won't resolve properly yet because we need to register entity instances
        let result = hier_table.resolve_path(&path, None);
        match result {
            Some(symbol) => {
                println!("✓ {} -> {:?} (kind: {:?})", path_str, symbol.name, symbol.kind);
            }
            None => {
                println!("✗ {} -> Not found (expected - instance registration not implemented)", path_str);
            }
        }
    }
}