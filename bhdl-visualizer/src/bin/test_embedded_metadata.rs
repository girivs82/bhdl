use bhdl_visualizer::schematic_knowledge::schematic_knowledge::SchematicKnowledge;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    info!("Testing embedded visualization metadata reading from component BHDL definitions");
    
    // Test 2: Check if schematic knowledge system can find our components
    info!("Testing schematic knowledge system...");
    let knowledge = SchematicKnowledge::new();
    
    // Test LM7805 rules - this should now read from the embedded metadata
    if let Some(lm7805_rules) = knowledge.get_component_rules("LM7805") {
        info!("✅ LM7805 visualization rules found!");
        info!("   Symbol style: {:?}", lm7805_rules.symbol_style);
        info!("   Orientation: {:?}", lm7805_rules.orientation);
        info!("   Pin placement count: {}", lm7805_rules.pin_placement.len());
        
        // Check for specific pin placements that we embedded
        if let Some(in_pin) = lm7805_rules.pin_placement.get("IN") {
            info!("   IN pin: side={:?}, label={}", in_pin.side, in_pin.label);
        }
        if let Some(out_pin) = lm7805_rules.pin_placement.get("OUT") {
            info!("   OUT pin: side={:?}, label={}", out_pin.side, out_pin.label);
        }
        if let Some(gnd_pin) = lm7805_rules.pin_placement.get("GND") {
            info!("   GND pin: side={:?}, label={}", gnd_pin.side, gnd_pin.label);
        }
        
        info!("   Supporting components: {}", lm7805_rules.supporting_components.len());
        for support in &lm7805_rules.supporting_components {
            info!("     - {} ({}) for {}", 
                  support.component_type, 
                  support.typical_value,
                  support.purpose);
        }
    } else {
        info!("❌ LM7805 visualization rules not found - metadata reading may not be working");
    }
    
    // Test 3: Check capacitor rules
    if let Some(cap_rules) = knowledge.get_component_rules("Cap") {
        info!("✅ Capacitor visualization rules found!");
        info!("   Symbol style: {:?}", cap_rules.symbol_style);
        info!("   Orientation: {:?}", cap_rules.orientation);
        info!("   Pin placement count: {}", cap_rules.pin_placement.len());
    } else {
        info!("❌ Capacitor visualization rules not found");
    }
    
    // Test 4: Check resistor rules  
    if let Some(res_rules) = knowledge.get_component_rules("Res") {
        info!("✅ Resistor visualization rules found!");
        info!("   Symbol style: {:?}", res_rules.symbol_style);
        info!("   Orientation: {:?}", res_rules.orientation);
        info!("   Pin placement count: {}", res_rules.pin_placement.len());
    } else {
        info!("❌ Resistor visualization rules not found");
    }
    
    // Test 5: Check LED rules
    if let Some(led_rules) = knowledge.get_component_rules("LED") {
        info!("✅ LED visualization rules found!");
        info!("   Symbol style: {:?}", led_rules.symbol_style);
        info!("   Orientation: {:?}", led_rules.orientation);
        info!("   Pin placement count: {}", led_rules.pin_placement.len());
        info!("   Supporting components: {}", led_rules.supporting_components.len());
    } else {
        info!("❌ LED visualization rules not found");
    }
    
    info!("=== EMBEDDED METADATA TEST SUMMARY ===");
    info!("This test verifies that our embedded visualization metadata in component BHDL files");
    info!("is being read correctly by the schematic knowledge system.");
    info!("");
    info!("Expected behavior:");
    info!("✓ LM7805 should have inputs on left, outputs on right, ground on bottom");
    info!("✓ Capacitors should prefer vertical orientation for power filtering");  
    info!("✓ Resistors should be horizontal with left/right pin placement");
    info!("✓ LEDs should be vertical with anode top, cathode bottom");
    info!("✓ Supporting components should be suggested (input/output caps, current limiting resistors)");
    info!("");
    info!("The visualization metadata is now co-located with component definitions");
    info!("rather than in separate files, as requested by the user.");
    
    Ok(())
}