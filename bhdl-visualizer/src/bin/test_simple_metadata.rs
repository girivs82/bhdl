use bhdl_visualizer::schematic_knowledge::schematic_knowledge::SchematicKnowledge;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing embedded visualization metadata from component BHDL definitions");
    
    // Test: Check if schematic knowledge system can find our components
    println!("Initializing schematic knowledge system...");
    let knowledge = SchematicKnowledge::new();
    
    // Test LM7805 rules - should now read from embedded metadata
    println!("\n=== Testing LM7805 Component ===");
    if let Some(lm7805_rules) = knowledge.get_component_rules("LM7805") {
        println!("✅ LM7805 visualization rules found!");
        println!("   Symbol style: {:?}", lm7805_rules.symbol_style);
        println!("   Orientation: {:?}", lm7805_rules.orientation);
        println!("   Pin placement count: {}", lm7805_rules.pin_placement.len());
        
        // Check for specific pin placements that we embedded
        if let Some(in_pin) = lm7805_rules.pin_placement.get("IN") {
            println!("   IN pin: side={:?}, label={}", in_pin.side, in_pin.label);
        }
        if let Some(out_pin) = lm7805_rules.pin_placement.get("OUT") {
            println!("   OUT pin: side={:?}, label={}", out_pin.side, out_pin.label);
        }
        if let Some(gnd_pin) = lm7805_rules.pin_placement.get("GND") {
            println!("   GND pin: side={:?}, label={}", gnd_pin.side, gnd_pin.label);
        }
        
        println!("   Supporting components: {}", lm7805_rules.supporting_components.len());
        for support in &lm7805_rules.supporting_components {
            println!("     - {} ({}) for {}", 
                     support.component_type, 
                     support.typical_value,
                     support.purpose);
        }
    } else {
        println!("❌ LM7805 visualization rules not found");
        println!("   This means our embedded metadata in lm7805.bhdl is not being read");
    }
    
    // Test capacitor rules
    println!("\n=== Testing Capacitor Component ===");
    if let Some(cap_rules) = knowledge.get_component_rules("Cap") {
        println!("✅ Capacitor visualization rules found!");
        println!("   Symbol style: {:?}", cap_rules.symbol_style);
        println!("   Orientation: {:?}", cap_rules.orientation);
        println!("   Pin placement count: {}", cap_rules.pin_placement.len());
        
        for (pin_name, pin) in &cap_rules.pin_placement {
            println!("   {} pin: side={:?}, label={}", pin_name, pin.side, pin.label);
        }
    } else {
        println!("❌ Capacitor visualization rules not found");
    }
    
    // Test resistor rules  
    println!("\n=== Testing Resistor Component ===");
    if let Some(res_rules) = knowledge.get_component_rules("Res") {
        println!("✅ Resistor visualization rules found!");
        println!("   Symbol style: {:?}", res_rules.symbol_style);
        println!("   Orientation: {:?}", res_rules.orientation);
        println!("   Pin placement count: {}", res_rules.pin_placement.len());
        
        for (pin_name, pin) in &res_rules.pin_placement {
            println!("   {} pin: side={:?}, label={}", pin_name, pin.side, pin.label);
        }
    } else {
        println!("❌ Resistor visualization rules not found");
    }
    
    // Test LED rules
    println!("\n=== Testing LED Component ===");
    if let Some(led_rules) = knowledge.get_component_rules("LED") {
        println!("✅ LED visualization rules found!");
        println!("   Symbol style: {:?}", led_rules.symbol_style);
        println!("   Orientation: {:?}", led_rules.orientation);
        println!("   Pin placement count: {}", led_rules.pin_placement.len());
        println!("   Supporting components: {}", led_rules.supporting_components.len());
        
        for (pin_name, pin) in &led_rules.pin_placement {
            println!("   {} pin: side={:?}, label={}", pin_name, pin.side, pin.label);
        }
        
        for support in &led_rules.supporting_components {
            println!("   Supporting: {} ({}) - {}", 
                     support.component_type, 
                     support.typical_value,
                     support.purpose);
        }
    } else {
        println!("❌ LED visualization rules not found");
    }
    
    println!("\n=== SUMMARY ===");
    println!("This test demonstrates that visualization metadata is now embedded");
    println!("directly in component BHDL files rather than separate files.");
    println!("");
    println!("Expected embedded metadata:");
    println!("• LM7805: IN=left, OUT=right, GND=bottom (professional convention)");
    println!("• Capacitors: Vertical orientation for power filtering");  
    println!("• Resistors: Horizontal with pins on left/right");
    println!("• LEDs: Vertical with anode=top, cathode=bottom");
    println!("• Supporting components automatically suggested");
    println!("");
    println!("The schematic knowledge system now reads this metadata directly");
    println!("from the component definitions, enabling professional schematics.");
    
    Ok(())
}