// Test parsing of behavioral model annotations

use bhdl_parser::{parse, SyntaxKind};

fn main() {
    println!("Testing behavioral model parsing...\n");
    
    // Test 1: Simple behavioral model
    let simple_model = r#"
entity BuckConverter(vin_nom: voltage, vout: voltage) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    
    @behavioral_model analytical {
        model_type: "equations",
        L_min: "(vin_nom - vout) * vout / (vin_nom * 0.3 * 2A * 500kHz)",
        C_min: "0.3 * 2A / (8 * 500kHz * 50mV)",
        runtime: 1ms,
        accuracy: 0.7,
    }
}"#;
    
    println!("Test 1: Simple behavioral model");
    println!("Input:\n{}", simple_model);
    
    let result = parse(simple_model);
    if result.errors().is_empty() {
        println!("✓ Parsed successfully\n");
        print_ast(&result.syntax(), 0);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    println!("\n{}\n", "=".repeat(60));
    
    // Test 2: Multiple models at different abstraction levels
    let multi_model = r#"
entity BuckConverterComplete(vin_nom: voltage, vout: voltage) {
    pin VIN: power in;
    pin VOUT: power out;
    
    @behavioral_model analytical {
        model_type: "equations",
        L_min: "10uH",
        runtime: 1ms,
        accuracy: 0.7,
    }
    
    @behavioral_model averaged {
        model_type: "state_space",
        A_matrix: [[-100, -1000], [1000, -200]],
        B_matrix: [[100], [0]],
        runtime: 100ms,
        accuracy: 0.9,
    }
    
    @behavioral_model switching {
        model_type: "behavioral_switching",
        switch_model: "ideal_with_Ron",
        Ron: 10mOhm,
        runtime: 10s,
        accuracy: 0.95,
    }
}"#;
    
    println!("Test 2: Multiple behavioral models");
    println!("Input:\n{}", multi_model);
    
    let result = parse(multi_model);
    if result.errors().is_empty() {
        println!("✓ Parsed successfully\n");
        
        // Count behavioral models
        let models = count_nodes(&result.syntax(), SyntaxKind::BEHAVIORAL_MODEL);
        println!("Found {} behavioral models", models);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    println!("\n{}\n", "=".repeat(60));
    
    // Test 3: Optimization strategy
    let optimization = r#"
entity OptimizedBuck() {
    pin VIN: power in;
    pin VOUT: power out;
    
    @optimization_strategy {
        phase1: {
            name: "Initial Sizing",
            model: "analytical",
            algorithm: "grid_search",
            parameters: ["L", "C"],
        },
        phase2: {
            name: "Control Loop",
            model: "averaged",
            algorithm: "nelder_mead",
            parameters: ["R_comp", "C_comp"],
        },
        phase3: {
            name: "Verification",
            model: "switching",
            algorithm: "none",
            verify: ["ripple", "efficiency"],
        }
    }
}"#;
    
    println!("Test 3: Optimization strategy");
    println!("Input:\n{}", optimization);
    
    let result = parse(optimization);
    if result.errors().is_empty() {
        println!("✓ Parsed successfully\n");
        
        // Check for optimization strategy
        let has_strategy = has_node(&result.syntax(), SyntaxKind::OPTIMIZATION_STRATEGY);
        println!("Has optimization strategy: {}", has_strategy);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    println!("\n{}\n", "=".repeat(60));
    
    // Test 4: Component knowledge
    let knowledge = r#"
entity SmartBuck() {
    pin VIN: power in;
    pin VOUT: power out;
    
    @component_knowledge {
        good_starting_points: [
            {condition: "fsw > 1MHz", L: "10µH", C: "22µF"},
            {condition: "fsw < 500kHz", L: "47µH", C: "100µF"},
        ],
        coupled_parameters: [["L", "fsw"], ["C", "ripple"]],
        common_issues: [
            {
                name: "subharmonic_oscillation",
                condition: "duty_cycle > 0.5",
                fix: "add_slope_compensation()",
            }
        ]
    }
}"#;
    
    println!("Test 4: Component knowledge");
    println!("Input:\n{}", knowledge);
    
    let result = parse(knowledge);
    if result.errors().is_empty() {
        println!("✓ Parsed successfully\n");
        
        // Check for component knowledge
        let has_knowledge = has_node(&result.syntax(), SyntaxKind::COMPONENT_KNOWLEDGE);
        println!("Has component knowledge: {}", has_knowledge);
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    println!("\n{}\n", "=".repeat(60));
    
    // Test 5: Complete example with all annotations
    let complete = r#"
entity CompleteBuckConverter(
    vin_nom: voltage = 12V,
    vout: voltage = 5V,
    iout_max: current = 2A,
    f_sw: frequency = 500kHz
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    pin FB: signal in;
    pin EN: signal in;
    
    @behavioral_model analytical {
        model_type: "equations",
        L_min: "(vin_nom - vout) * vout / (vin_nom * 0.3 * iout_max * f_sw)",
        C_min: "0.3 * iout_max / (8 * f_sw * 50mV)",
    }
    
    @behavioral_model averaged {
        model_type: "state_space_averaged",
        transfer_function: "Gvd = vout * (1 + s*L/R) / (1 + s*L/R + s^2*L*C)",
    }
    
    @optimization_strategy {
        initial_sizing: {
            model: "analytical",
            algorithm: "grid_search",
        },
        control_loop: {
            model: "averaged",
            algorithm: "nelder_mead",
        }
    }
    
    @component_knowledge {
        good_starting_points: [{L: "22µH", C: "100µF"}],
        scaling_rules: {
            L: "inversely proportional to f_sw",
            C: "inversely proportional to f_sw",
        }
    }
    
    @simulation_requirements {
        dc_analysis: {required: true},
        ac_analysis: {
            required: true,
            frequency_range: [1Hz, 1MHz],
        },
        transient: {
            required: true,
            events: ["startup", "load_step"],
        }
    }
    
    @test_sequences {
        stability: {
            description: "Test control loop stability",
            measure: ["phase_margin", "gain_margin"],
            pass_criteria: {phase_margin: "> 45°"},
        },
        efficiency: {
            description: "Measure conversion efficiency",
            measure: ["Pin", "Pout"],
            pass_criteria: {efficiency: "> 0.9"},
        }
    }
}"#;
    
    println!("Test 5: Complete buck converter with all annotations");
    println!("Input:\n{}", complete);
    
    let result = parse(complete);
    if result.errors().is_empty() {
        println!("✓ Parsed successfully\n");
        
        // Count all simulation-related nodes
        println!("Simulation annotations found:");
        println!("  Behavioral models: {}", count_nodes(&result.syntax(), SyntaxKind::BEHAVIORAL_MODEL));
        println!("  Optimization strategy: {}", has_node(&result.syntax(), SyntaxKind::OPTIMIZATION_STRATEGY));
        println!("  Component knowledge: {}", has_node(&result.syntax(), SyntaxKind::COMPONENT_KNOWLEDGE));
        println!("  Simulation requirements: {}", has_node(&result.syntax(), SyntaxKind::SIMULATION_REQUIREMENTS));
        println!("  Test sequences: {}", has_node(&result.syntax(), SyntaxKind::TEST_SEQUENCES));
    } else {
        println!("✗ Parse errors:");
        for error in result.errors() {
            println!("  - {:?}", error);
        }
    }
    
    println!("\n{}", "=".repeat(60));
    println!("\nAll behavioral model parsing tests complete!");
}

// Helper function to print AST
fn print_ast(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let kind = node.kind();
    let text = if node.children().next().is_none() {
        format!(" \"{}\"", node.text())
    } else {
        String::new()
    };
    
    println!("{}{:?}{}", "  ".repeat(indent), kind, text);
    
    for child in node.children() {
        print_ast(&child, indent + 1);
    }
}

// Helper function to count nodes of a specific kind
fn count_nodes(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, target_kind: SyntaxKind) -> usize {
    let mut count = 0;
    if node.kind() == target_kind {
        count += 1;
    }
    for child in node.children() {
        count += count_nodes(&child, target_kind);
    }
    count
}

// Helper function to check if a node type exists
fn has_node(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, target_kind: SyntaxKind) -> bool {
    count_nodes(node, target_kind) > 0
}