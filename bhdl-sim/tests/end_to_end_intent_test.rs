//! End-to-end test of intent-based simulation from BHDL source to execution

use bhdl_parser::Parser;
use bhdl_ast::ast::SourceFile;
use bhdl_analyzer::{Analyzer, SymbolTable};
use bhdl_synthesizer::Synthesizer;
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;
use std::sync::Arc;

/// Test complete pipeline with intent declarations
#[test]
fn test_intent_driven_simulation_pipeline() {
    // BHDL source with different simulation intents
    let source = r#"
        board MixedSignalBoard {
            // Power supply section needs accurate analog simulation
            module PowerSupply {
                pin VIN: power in;
                pin VOUT: power out;
                pin GND: ground inout;
                
                // Intent for accurate power analysis
                for accuracy: use spice with temperature=25;
            }
            
            // Digital logic section for speed
            module DigitalController {
                pin CLK: clock in;
                pin ENABLE: signal in;
                pin OUTPUT: signal out;
                
                // Intent for fast digital simulation
                for speed: use digital;
            }
            
            // Timing-critical section
            module TimingCircuit {
                pin CLK_IN: clock in;
                pin CLK_OUT: clock out;
                
                // Intent for timing analysis
                for timing_analysis: use digital with propagation_delays;
            }
            
            // Instantiate modules
            power VCC = 5V;
            ground GND;
            
            VCC -> PowerSupply().VIN;
            PowerSupply().GND -> GND;
            
            PowerSupply().VOUT -> DigitalController().VCC;
            DigitalController().GND -> GND;
            
            CLK -> TimingCircuit().CLK_IN;
            TimingCircuit().CLK_OUT -> DigitalController().CLK;
        }
    "#;
    
    // Parse
    let parser = Parser::new(source);
    let (tree, _) = parser.parse();
    let source_file = SourceFile::cast(tree).expect("Should parse as SourceFile");
    
    // Analyze with intent support
    let mut symbol_table = SymbolTable::new();
    let mut intent_registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut intent_registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(intent_registry.clone()));
    
    let analysis_result = analyzer.analyze(&source_file);
    assert!(analysis_result.diagnostics.iter().all(|d| !d.is_error()), 
            "Analysis should succeed without errors");
    
    // Synthesize netlist
    let synthesizer = Synthesizer::new(&symbol_table, &analysis_result.resolved_constants);
    let netlist = synthesizer.synthesize(&source_file, &analysis_result);
    assert!(netlist.is_ok(), "Netlist synthesis should succeed");
    let netlist = netlist.unwrap();
    
    // Create simulation coordinator
    let flow_tracker = analyzer.get_flow_tracker()
        .expect("Analyzer should have flow tracker");
    
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // Verify partitioning based on intents
    let partitions = coordinator.get_partitions();
    assert!(partitions.len() >= 3, "Should have multiple partitions for different intents");
    
    // Verify each module is in the correct partition
    let partition_modes: Vec<(String, SimMode)> = partitions.iter()
        .flat_map(|p| p.instances.iter().map(|&inst_id| {
            let inst = coordinator.netlist.get_instance(inst_id).unwrap();
            (inst.name.clone(), p.mode)
        }))
        .collect();
    
    // Check PowerSupply is in analog partition
    assert!(partition_modes.iter().any(|(name, mode)| 
        name.contains("PowerSupply") && *mode == SimMode::AnalogRequired),
        "PowerSupply should be in analog partition");
    
    // Check DigitalController is in digital partition  
    assert!(partition_modes.iter().any(|(name, mode)|
        name.contains("DigitalController") && *mode == SimMode::PureDigital),
        "DigitalController should be in digital partition");
    
    // Check TimingCircuit is in timed digital partition
    assert!(partition_modes.iter().any(|(name, mode)|
        name.contains("TimingCircuit") && *mode == SimMode::DigitalWithTiming),
        "TimingCircuit should be in timed digital partition");
    
    // Verify interfaces exist between partitions
    let interfaces = coordinator.get_interfaces();
    assert!(!interfaces.is_empty(), "Should have interfaces between different simulation domains");
}

/// Test intent propagation through hierarchy
#[test]
fn test_hierarchical_intent_propagation() {
    let source = r#"
        board HierarchicalBoard {
            // Top-level module with intent
            module AnalogSection {
                pin IN: signal in;
                pin OUT: signal out;
                
                // This intent should propagate to all submodules
                for accuracy: use spice;
                
                // Submodule without explicit intent
                module Amplifier {
                    pin IN: signal in;
                    pin OUT: signal out;
                    pin VCC: power in;
                    pin GND: ground inout;
                }
                
                // Another submodule
                module Filter {
                    pin IN: signal in;
                    pin OUT: signal out;
                }
                
                // Connect submodules
                IN -> Amplifier().IN;
                Amplifier().OUT -> Filter().IN;
                Filter().OUT -> OUT;
            }
            
            // Instantiate the analog section
            signal_in -> AnalogSection().IN;
            AnalogSection().OUT -> signal_out;
        }
    "#;
    
    // Parse and analyze
    let parser = Parser::new(source);
    let (tree, _) = parser.parse();
    let source_file = SourceFile::cast(tree).expect("Should parse");
    
    let mut symbol_table = SymbolTable::new();
    let mut intent_registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut intent_registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(intent_registry));
    
    let analysis_result = analyzer.analyze(&source_file);
    
    // Synthesize and create coordinator
    let synthesizer = Synthesizer::new(&symbol_table, &analysis_result.resolved_constants);
    let netlist = synthesizer.synthesize(&source_file, &analysis_result).unwrap();
    
    let flow_tracker = analyzer.get_flow_tracker().unwrap();
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // All instances in AnalogSection should be in analog partition
    let partitions = coordinator.get_partitions();
    let analog_partition = partitions.iter()
        .find(|p| p.mode == SimMode::AnalogRequired)
        .expect("Should have analog partition");
    
    // Count instances that belong to AnalogSection hierarchy
    let analog_instances = analog_partition.instances.iter()
        .filter_map(|&inst_id| coordinator.netlist.get_instance(inst_id))
        .filter(|inst| inst.name.contains("Amplifier") || inst.name.contains("Filter"))
        .count();
    
    assert!(analog_instances >= 2, 
            "Submodules should inherit analog intent from parent module");
}

/// Test intent override in hierarchy
#[test]
fn test_intent_override_in_hierarchy() {
    let source = r#"
        board OverrideTestBoard {
            module MixedSection {
                // Parent has analog intent
                for accuracy: use spice;
                
                module AnalogPart {
                    pin IN: signal in;
                    pin OUT: signal out;
                    // Inherits analog intent
                }
                
                module DigitalPart {
                    pin IN: signal in; 
                    pin OUT: signal out;
                    // Override with digital intent
                    for speed: use digital;
                }
                
                IN -> AnalogPart().IN;
                AnalogPart().OUT -> DigitalPart().IN;
                DigitalPart().OUT -> OUT;
            }
        }
    "#;
    
    // Parse and analyze
    let parser = Parser::new(source);
    let (tree, _) = parser.parse();
    let source_file = SourceFile::cast(tree).expect("Should parse");
    
    let mut symbol_table = SymbolTable::new();
    let mut intent_registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut intent_registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(intent_registry));
    
    let analysis_result = analyzer.analyze(&source_file);
    
    // Synthesize and create coordinator
    let synthesizer = Synthesizer::new(&symbol_table, &analysis_result.resolved_constants);
    let netlist = synthesizer.synthesize(&source_file, &analysis_result).unwrap();
    
    let flow_tracker = analyzer.get_flow_tracker().unwrap();
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // Should have both analog and digital partitions
    let partitions = coordinator.get_partitions();
    let has_analog = partitions.iter().any(|p| p.mode == SimMode::AnalogRequired);
    let has_digital = partitions.iter().any(|p| p.mode == SimMode::PureDigital);
    
    assert!(has_analog, "Should have analog partition for AnalogPart");
    assert!(has_digital, "Should have digital partition for DigitalPart (override)");
    
    // Should have interface between the two domains
    let interfaces = coordinator.get_interfaces();
    assert!(!interfaces.is_empty(), "Should have interface between analog and digital domains");
}