// Test design pattern recognition capabilities
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{analyze, AnalysisResult};
use bhdl_synthesizer::{Synthesizer, design_pattern_recognition::DesignPatternRecognizer};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Design Pattern Recognition Test ===");
    
    // Test with the intelligent design automation demo file
    let test_file = "demo_intelligent_design_automation.bhdl";
    println!("Reading circuit design: {}", test_file);
    
    let bhdl_source = fs::read_to_string(test_file)
        .map_err(|e| format!("Failed to read {}: {}", test_file, e))?;
    
    println!("Parsing BHDL source...");
    let parse_result = parse(&bhdl_source);
    let syntax = parse_result.syntax();
    
    // Run semantic analysis
    println!("Running semantic analysis...");
    let analysis = analyze(&SourceFile::cast(syntax.clone()).unwrap());
    
    // Generate netlist for pattern analysis
    println!("\nGenerating netlist for pattern analysis...");
    let mut synthesizer = Synthesizer::new();
    
    // Create a netlist representing the intelligent power system
    use bhdl_netlist::{Netlist, ModuleKind};
    let mut netlist = Netlist::new();
    
    // Add a module definition
    let module_id = netlist.add_module("IntelligentPowerSystem".to_string(), ModuleKind::Board);
    
    // Add components that represent common design patterns
    println!("Adding components with recognizable patterns...");
    
    // Linear regulator pattern: LM7805 + capacitors
    let lm7805_instance = netlist.add_instance("main_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(lm7805_instance).unwrap().name = "LM7805".to_string();
    
    let input_cap = netlist.add_instance("C_in".to_string(), module_id).unwrap();
    netlist.instances.get_mut(input_cap).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(input_cap).unwrap().attributes.insert(
        "value".to_string(), "100uF".to_string()
    );
    
    let output_cap = netlist.add_instance("C_out".to_string(), module_id).unwrap();
    netlist.instances.get_mut(output_cap).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(output_cap).unwrap().attributes.insert(
        "value".to_string(), "10uF".to_string()
    );
    
    // Switching regulator pattern: TPS54331 + inductor + feedback network
    let tps54331_instance = netlist.add_instance("switch_reg".to_string(), module_id).unwrap();
    netlist.instances.get_mut(tps54331_instance).unwrap().name = "TPS54331".to_string();
    
    let inductor = netlist.add_instance("L1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(inductor).unwrap().name = "Ind".to_string();
    netlist.instances.get_mut(inductor).unwrap().attributes.insert(
        "value".to_string(), "10uH".to_string()
    );
    
    let feedback_r1 = netlist.add_instance("R_fb1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(feedback_r1).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(feedback_r1).unwrap().attributes.insert(
        "value".to_string(), "10k".to_string()
    );
    
    let feedback_r2 = netlist.add_instance("R_fb2".to_string(), module_id).unwrap();
    netlist.instances.get_mut(feedback_r2).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(feedback_r2).unwrap().attributes.insert(
        "value".to_string(), "3.3k".to_string()
    );
    
    // Voltage divider pattern: Two precision resistors
    let divider_r1 = netlist.add_instance("R1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(divider_r1).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(divider_r1).unwrap().attributes.insert(
        "value".to_string(), "10k".to_string()
    );
    netlist.instances.get_mut(divider_r1).unwrap().attributes.insert(
        "tolerance".to_string(), "0.1%".to_string()
    );
    
    let divider_r2 = netlist.add_instance("R2".to_string(), module_id).unwrap();
    netlist.instances.get_mut(divider_r2).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(divider_r2).unwrap().attributes.insert(
        "value".to_string(), "10k".to_string()
    );
    netlist.instances.get_mut(divider_r2).unwrap().attributes.insert(
        "tolerance".to_string(), "0.1%".to_string()
    );
    
    // Current limiting pattern: Series resistor
    let current_limiter = netlist.add_instance("R_limit".to_string(), module_id).unwrap();
    netlist.instances.get_mut(current_limiter).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(current_limiter).unwrap().attributes.insert(
        "value".to_string(), "100".to_string()
    );
    netlist.instances.get_mut(current_limiter).unwrap().attributes.insert(
        "power_rating".to_string(), "500mW".to_string()
    );
    
    // Protection pattern: TVS diode
    let tvs_diode = netlist.add_instance("TVS".to_string(), module_id).unwrap();
    netlist.instances.get_mut(tvs_diode).unwrap().name = "TVSDiode".to_string();
    netlist.instances.get_mut(tvs_diode).unwrap().attributes.insert(
        "voltage".to_string(), "30V".to_string()
    );
    
    // RC filter pattern: Resistor + capacitor
    let filter_r = netlist.add_instance("R_filter".to_string(), module_id).unwrap();
    netlist.instances.get_mut(filter_r).unwrap().name = "Res".to_string();
    netlist.instances.get_mut(filter_r).unwrap().attributes.insert(
        "value".to_string(), "1k".to_string()
    );
    
    let filter_c = netlist.add_instance("C_filter".to_string(), module_id).unwrap();
    netlist.instances.get_mut(filter_c).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(filter_c).unwrap().attributes.insert(
        "value".to_string(), "100nF".to_string()
    );
    
    // Crystal oscillator pattern: Crystal + load capacitors
    let crystal = netlist.add_instance("XTAL1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(crystal).unwrap().name = "Crystal".to_string();
    netlist.instances.get_mut(crystal).unwrap().attributes.insert(
        "frequency".to_string(), "8MHz".to_string()
    );
    
    let load_c1 = netlist.add_instance("C_load1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(load_c1).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(load_c1).unwrap().attributes.insert(
        "value".to_string(), "22pF".to_string()
    );
    
    let load_c2 = netlist.add_instance("C_load2".to_string(), module_id).unwrap();
    netlist.instances.get_mut(load_c2).unwrap().name = "Cap".to_string();
    netlist.instances.get_mut(load_c2).unwrap().attributes.insert(
        "value".to_string(), "22pF".to_string()
    );
    
    // Add some nets to connect components (simplified connectivity)
    let vin_net = netlist.add_net(Some("VIN".to_string()));
    let vout_5v_net = netlist.add_net(Some("VOUT_5V".to_string()));
    let vout_3v3_net = netlist.add_net(Some("VOUT_3V3".to_string()));
    let gnd_net = netlist.add_net(Some("GND".to_string()));
    let fb_net = netlist.add_net(Some("FB".to_string()));
    let ref_net = netlist.add_net(Some("REF".to_string()));
    let filter_net = netlist.add_net(Some("FILTERED".to_string()));
    let xtal1_net = netlist.add_net(Some("XTAL1".to_string()));
    let xtal2_net = netlist.add_net(Some("XTAL2".to_string()));
    
    println!("Created netlist with {} instances and {} nets", 
             netlist.instances.len(), netlist.nets.len());
    
    // Initialize pattern recognizer
    println!("\n=== Starting Design Pattern Recognition ===");
    let mut pattern_recognizer = DesignPatternRecognizer::new();
    
    // Run pattern recognition
    match pattern_recognizer.recognize_patterns(&netlist, &analysis) {
        Ok(report) => {
            println!("\nPattern Recognition Report:");
            println!("==========================");
            
            // Topology Analysis
            println!("\n📊 Circuit Topology Analysis:");
            println!("  Total components: {}", report.topology_analysis.total_components);
            println!("  Total nets: {}", report.topology_analysis.total_nets);
            println!("  Power domains: {}", report.topology_analysis.power_domains.len());
            println!("  Signal groups: {}", report.topology_analysis.signal_groups.len());
            println!("  Component clusters: {}", report.topology_analysis.component_clusters.len());
            
            // Component Roles
            println!("\n🏷️  Component Role Inference:");
            for (instance_id, role) in &report.component_roles {
                if let Some(instance) = netlist.instances.get(*instance_id) {
                    println!("  {}: {:?}", instance.name, role);
                }
            }
            
            // Recognized Patterns
            println!("\n🔍 Recognized Design Patterns:");
            if report.recognized_patterns.is_empty() {
                println!("  No patterns recognized");
            } else {
                for (i, pattern) in report.recognized_patterns.iter().enumerate() {
                    println!("  {}. {} (Type: {:?})", i + 1, pattern.pattern_name, pattern.pattern_type);
                    println!("     Confidence: {:.1}%", pattern.confidence_score * 100.0);
                    println!("     Components: {} matched", pattern.matched_components.len());
                    
                    if !pattern.design_insights.is_empty() {
                        println!("     Design Insights:");
                        for insight in &pattern.design_insights {
                            println!("       - {}", insight);
                        }
                    }
                    
                    if !pattern.applicable_rules.is_empty() {
                        println!("     Applicable Rules:");
                        for rule in &pattern.applicable_rules {
                            println!("       - {}: {}", rule.name, rule.description);
                        }
                    }
                    println!();
                }
            }
            
            // Design Recommendations
            println!("💡 Design Recommendations:");
            if report.design_recommendations.is_empty() {
                println!("  No specific recommendations");
            } else {
                for (i, rec) in report.design_recommendations.iter().enumerate() {
                    let icon = match rec.category {
                        bhdl_synthesizer::design_pattern_recognition::RecommendationCategory::Error => "❌",
                        bhdl_synthesizer::design_pattern_recognition::RecommendationCategory::Warning => "⚠️",
                        bhdl_synthesizer::design_pattern_recognition::RecommendationCategory::Info => "ℹ️",
                        bhdl_synthesizer::design_pattern_recognition::RecommendationCategory::Suggestion => "💡",
                    };
                    
                    println!("  {}. {} {} (Priority: {})", i + 1, icon, rec.title, rec.priority);
                    println!("     {}", rec.description);
                    println!("     Recommendation: {}", rec.recommendation);
                    println!("     Affects {} components", rec.affected_components.len());
                    println!();
                }
            }
            
            // Pattern Coverage
            println!("📈 Pattern Coverage: {:.1}%", report.pattern_coverage * 100.0);
            println!("   ({} of {} components covered by recognized patterns)", 
                     (report.pattern_coverage * report.topology_analysis.total_components as f64) as usize,
                     report.topology_analysis.total_components);
            
            // Power Domain Analysis
            if !report.topology_analysis.power_domains.is_empty() {
                println!("\n⚡ Power Domain Analysis:");
                for domain in &report.topology_analysis.power_domains {
                    println!("  Domain: {}", domain.name);
                    if let Some(voltage) = domain.voltage_level {
                        println!("    Voltage: {}V", voltage);
                    }
                    println!("    Connected components: {}", domain.connected_components.len());
                }
            }
            
            // Signal Group Analysis
            if !report.topology_analysis.signal_groups.is_empty() {
                println!("\n📡 Signal Group Analysis:");
                for group in &report.topology_analysis.signal_groups {
                    println!("  Group: {:?}", group.group_type);
                    println!("    Nets: {}", group.nets.len());
                    println!("    Characteristics: {}", group.characteristics);
                }
            }
            
            // Component Cluster Analysis
            if !report.topology_analysis.component_clusters.is_empty() {
                println!("\n🔗 Component Cluster Analysis:");
                for (i, cluster) in report.topology_analysis.component_clusters.iter().enumerate() {
                    println!("  Cluster {}: {:?}", i + 1, cluster.cluster_type);
                    println!("    Components: {}", cluster.components.len());
                    println!("    Description: {}", cluster.description);
                }
            }
        },
        Err(e) => {
            println!("Pattern recognition failed: {}", e);
        }
    }
    
    // Demonstration summary
    println!("\n=== Design Pattern Recognition Capabilities Demonstrated ===");
    println!("1. Circuit Topology Analysis:");
    println!("   - Automatic identification of power domains and signal groups");
    println!("   - Component clustering based on connectivity patterns");
    println!("   - Connectivity matrix for analyzing component relationships");
    
    println!("\n2. Component Role Inference:");
    println!("   - Context-aware role assignment based on connectivity");
    println!("   - Distinguishes between different resistor/capacitor functions");
    println!("   - Identifies functional blocks and their purposes");
    
    println!("\n3. Design Pattern Recognition:");
    println!("   - 15+ standard circuit patterns (power, filter, amplifier, etc.)");
    println!("   - Confidence scoring for pattern matches");
    println!("   - Pattern-specific design knowledge and rules");
    
    println!("\n4. Design Recommendations:");
    println!("   - Rule-based analysis with priority levels");
    println!("   - Pattern-specific optimization suggestions");
    println!("   - Thermal, layout, and performance considerations");
    
    println!("\n5. Knowledge Integration:");
    println!("   - Embedded design knowledge for each pattern");
    println!("   - Standard value selection and tolerance considerations");
    println!("   - Application-specific design guidelines");
    
    println!("\nKey Innovation: Pattern recognition enables automatic");
    println!("identification of design intent and application of relevant");
    println!("engineering knowledge for optimization and validation.");
    
    Ok(())
}