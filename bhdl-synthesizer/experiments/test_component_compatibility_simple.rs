// Simple test of component compatibility analysis capabilities
use bhdl_synthesizer::component_compatibility::ComponentCompatibilityAnalyzer;
use bhdl_netlist::{Netlist, ModuleKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing component compatibility analysis");
    
    // Create a simple netlist for testing
    let mut netlist = Netlist::new();
    let module_id = netlist.add_module("TestSystem".to_string(), ModuleKind::Board);
    
    // Add some test components
    let reg_instance = netlist.add_instance("reg1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(reg_instance).unwrap().name = "LM7805".to_string();
    
    let mcu_instance = netlist.add_instance("mcu1".to_string(), module_id).unwrap();
    netlist.instances.get_mut(mcu_instance).unwrap().name = "STM32F103C8T6".to_string();
    
    // Add some nets
    let _vin_net = netlist.add_net(Some("VIN".to_string()));
    let _vout_net = netlist.add_net(Some("VOUT_5V".to_string()));
    let _gnd_net = netlist.add_net(Some("GND".to_string()));
    
    println!("Created test netlist with {} components", netlist.instances.len());
    
    // Initialize compatibility analyzer
    let analyzer = ComponentCompatibilityAnalyzer::new();
    println!("Initialized compatibility analyzer");
    
    // Create minimal analysis result for compatibility check
    use bhdl_analyzer::AnalysisResult;
    let analysis = AnalysisResult::default();
    
    // Run compatibility analysis
    match analyzer.analyze_compatibility(&netlist, &analysis) {
        Ok(report) => {
            println!("Compatibility analysis completed successfully");
            println!("Overall compatibility score: {:.1}%", report.overall_compatibility_score * 100.0);
            println!("Power domains analyzed: {}", report.power_domain_analysis.len());
            println!("Interface analyses: {}", report.interface_analysis.len());
            println!("Thermal zones: {}", report.thermal_analysis.len());
            println!("Cross-component checks: {}", report.cross_component_checks.len());
            println!("Critical issues found: {}", report.critical_issues.len());
            println!("Optimization opportunities: {}", report.optimization_opportunities.len());
            println!("Design recommendations: {}", report.design_recommendations.len());
        },
        Err(e) => {
            println!("Analysis failed: {}", e);
            return Err(e.into());
        }
    }
    
    println!("Test completed successfully");
    Ok(())
}