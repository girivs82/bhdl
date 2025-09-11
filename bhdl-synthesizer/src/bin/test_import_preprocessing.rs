use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, HasName};
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_synthesizer::import_preprocessor::preprocess_and_analyze;
use tokio;

#[tokio::main]
async fn main() {
    println!("=== Testing Import Preprocessing ===\n");
    
    // Test case: Board that imports TPS54331 and uses it
    let code = r#"
import { TPS54331 } from "bhdl-stdlib/components/power/switching_regulators/TPS54331.bhdl";
import { Cap } from "bhdl-stdlib/components/passives/capacitors/Capacitor.bhdl";

board TestImports {
    power VIN = 12V @ 3A;
    power VOUT_5V = 5V @ 2A;
    ground GND;
    
    // Use imported TPS54331
    U1: TPS54331(vout=5V);
    VIN -> U1.VIN;
    U1.GND -> GND;
    U1.EN -> VIN;
    
    // This should be recognized as having virtual pins
    U1.VOUT -> @VOUT_5V;
    
    // Use imported capacitor
    C1: Cap(100µF, voltage=25V);
    VIN -> C1.1;
    C1.2 -> GND;
}
"#;

    println!("1. Parsing...");
    let parse_result = parse(code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✓ Parsing successful");
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Check that imports were parsed
    println!("\n2. Checking imports:");
    let imports: Vec<_> = source_file.imports().collect();
    println!("  Found {} imports", imports.len());
    for import in &imports {
        if let Some(path) = import.path() {
            println!("  - Path: {}", path);
            let names = import.imported_names();
            if !names.is_empty() {
                println!("    Imported: {:?}", names);
            }
        }
    }
    
    println!("\n3. Pre-processing imports and running analysis...");
    let (analysis_result, preprocessor) = match preprocess_and_analyze(&source_file, ".") {
        Ok(result) => result,
        Err(e) => {
            println!("✗ Pre-processing failed: {}", e);
            return;
        }
    };
    
    println!("Analysis complete:");
    println!("  - Diagnostics: {}", analysis_result.diagnostics.len());
    println!("  - Imported modules: {}", preprocessor.imported_modules().len());
    
    for (name, module) in preprocessor.imported_modules() {
        println!("    * Imported module: {}", name);
        if preprocessor.module_has_virtual_pins(name) {
            let virtual_pins = preprocessor.get_virtual_pins(name);
            println!("      Virtual pins: {:?}", virtual_pins);
        }
        
        // List all pins in the imported module
        let pins: Vec<_> = module.pins().collect();
        println!("      Total pins in imported module: {}", pins.len());
        for pin in pins {
            if let Some(pin_name) = pin.name() {
                let pin_text = pin.syntax().text().to_string();
                let first_line = pin_text.lines().next().unwrap_or("");
                println!("        - {}: {}", pin_name.text(), first_line);
            }
        }
    }
    
    for diag in &analysis_result.diagnostics {
        println!("    * {}", diag.message);
    }
    
    println!("\n4. Generating netlist...");
    
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: false,
        flatten_hierarchy: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    generator.set_import_preprocessor(preprocessor);
    match generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(netlist) => {
            println!("✓ Netlist generation successful");
            
            println!("\nNetlist statistics:");
            println!("  - Modules: {}", netlist.modules.len());
            println!("  - Instances: {}", netlist.instances.len());
            
            // Check if TPS54331 was found and if virtual pins were detected
            let mut found_tps = false;
            let mut found_virtual_vout = false;
            
            for (_, module) in &netlist.modules {
                println!("  Module: {} ({:?})", module.name, module.kind);
                if module.name.contains("TPS54331") {
                    found_tps = true;
                    println!("    ✓ Found TPS54331 module: {}", module.name);
                    
                    // Check for VOUT pin
                    for pin_id in &module.pins {
                        if let Some(pin) = netlist.pins.get(*pin_id) {
                            println!("      - Pin: {} ({:?})", pin.name, pin.pin_type);
                            if pin.name == "VOUT" {
                                found_virtual_vout = true;
                                println!("        *** VOUT pin found - should be virtual ***");
                            }
                        }
                    }
                }
            }
            
            if !found_tps {
                println!("\n✗ TPS54331 module not found in netlist");
            }
            
            if found_tps && !found_virtual_vout {
                println!("\n✗ VOUT pin not found on TPS54331");
            }
            
            // Generate SPICE to see the structure
            println!("\n5. SPICE output preview:");
            match bhdl_synthesizer::hierarchical_connectivity::generate_spice_subcircuits(&netlist, &analysis_result) {
                Ok(spice) => {
                    let lines: Vec<&str> = spice.lines().take(30).collect();
                    for line in lines {
                        println!("{}", line);
                    }
                    if spice.lines().count() > 30 {
                        println!("... (output truncated)");
                    }
                }
                Err(e) => {
                    println!("✗ SPICE generation failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Netlist generation failed: {}", e);
        }
    }
    
    println!("\n=== Test Complete ===");
}