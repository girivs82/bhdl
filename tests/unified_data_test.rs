#[cfg(test)]
mod tests {
    use bhdl_spice::SpiceAnalysisAugmenter;
    use bhdl_common::analysis_interface::AnalysisData;
    use bhdl_netlist::Netlist;
    use serde_json;
    use std::fs;

    #[test]
    fn test_unified_data_structure() {
        // Load a test netlist
        let netlist_json = fs::read_to_string("tests/outputs/cli_test/netlist.json")
            .expect("Failed to read netlist file");
        let netlist: Netlist = serde_json::from_str(&netlist_json)
            .expect("Failed to parse netlist");
        
        // Create analysis data
        let mut analysis_data = AnalysisData::default();
        
        // Create augmenter and augment the netlist
        let mut augmenter = SpiceAnalysisAugmenter::new();
        augmenter.augment(&netlist, &mut analysis_data)
            .expect("Failed to augment netlist");
        
        // Verify that instance analysis data was added
        assert!(!analysis_data.instance_analysis.is_empty());
        
        // Check specific instances
        if let Some(r8_data) = analysis_data.instance_analysis.get("R8") {
            assert_eq!(r8_data.spice_type, Some("resistor".to_string()));
            assert!(r8_data.electrical_params.is_some());
            if let Some(params) = &r8_data.electrical_params {
                // Check that resistance was extracted (330Ω)
                assert!(params.get("resistance").is_some());
            }
        }
        
        if let Some(c4_data) = analysis_data.instance_analysis.get("C4") {
            assert_eq!(c4_data.spice_type, Some("capacitor".to_string()));
            assert!(c4_data.electrical_params.is_some());
            if let Some(params) = &c4_data.electrical_params {
                // Check that capacitance was extracted
                assert!(params.get("capacitance").is_some());
            }
        }
        
        if let Some(led9_data) = analysis_data.instance_analysis.get("LED9") {
            assert_eq!(led9_data.spice_type, Some("diode".to_string()));
            // LED should have component role detected
            assert!(led9_data.component_role.is_some());
        }
        
        println!("Unified data structure test passed!");
        println!("Found {} instances with analysis data", analysis_data.instance_analysis.len());
    }
}