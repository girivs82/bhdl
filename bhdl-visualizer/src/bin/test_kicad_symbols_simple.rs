use bhdl_netlist::{Netlist, ModuleKind, PinDirection, PinType, ConnectionPoint};
use std::fs;
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== KiCad Symbol Visualization Test (Direct DB) ===\n");
    
    // Query components directly from database using SQLite
    let conn = rusqlite::Connection::open("components.db")?;
    
    // Get LM7805 component and symbol
    let (lm7805_name, lm7805_desc, lm7805_svg) = conn.query_row(
        "SELECT c.name, c.description, s.svg_data 
         FROM components c 
         JOIN component_symbols s ON c.id = s.component_id 
         WHERE c.name = 'LM7805_TO220'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    )?;
    
    println!("Loaded from database:");
    println!("  • {}: {}", lm7805_name, lm7805_desc);
    
    // Get generic capacitor symbol
    let cap_svg: String = conn.query_row(
        "SELECT s.svg_data 
         FROM components c 
         JOIN component_symbols s ON c.id = s.component_id 
         WHERE c.name = 'C' LIMIT 1",
        [],
        |row| row.get(0)
    )?;
    
    // Get generic resistor symbol
    let res_svg: String = conn.query_row(
        "SELECT s.svg_data 
         FROM components c 
         JOIN component_symbols s ON c.id = s.component_id 
         WHERE c.name = 'R' LIMIT 1",
        [],
        |row| row.get(0)
    )?;
    
    println!("  • Generic Capacitor (C)");
    println!("  • Generic Resistor (R)");
    println!();
    
    // Get LM7805 pins
    println!("LM7805 Pin Configuration:");
    let mut stmt = conn.prepare(
        "SELECT p.pin_number, p.pin_name, p.electrical_type 
         FROM components c 
         JOIN component_pins p ON c.id = p.component_id 
         WHERE c.name = 'LM7805_TO220' 
         ORDER BY p.pin_number"
    )?;
    
    let pins = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?
        ))
    })?;
    
    for pin in pins {
        let (num, name, etype) = pin?;
        println!("  • Pin {}: {} ({})", num, name, etype);
    }
    println!();
    
    // Create a proper voltage regulator netlist
    let netlist = create_proper_regulator_netlist();
    
    // Generate SVG with actual KiCad symbols
    let svg = generate_circuit_svg(&lm7805_svg, &cap_svg, &res_svg);
    
    // Save to file
    let output_path = "test_kicad_symbols_output.svg";
    fs::write(output_path, svg)?;
    
    // Also save the netlist
    let netlist_json = serde_json::to_string_pretty(&netlist)?;
    let netlist_path = "test_kicad_symbols_netlist.json";
    fs::write(netlist_path, netlist_json)?;
    
    println!("✅ SUCCESS! KiCad symbol visualization complete.");
    println!("📊 SVG Output: {}", output_path);
    println!("📄 Netlist Output: {}", netlist_path);
    println!("\nKey features:");
    println!("  • Uses actual KiCad symbols from database");
    println!("  • Correct pin definitions: VI (input), GND (ground), VO (output)");
    println!("  • Proper voltage regulator circuit topology");
    
    Ok(())
}

fn create_proper_regulator_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Define component modules
    let reg_mod = netlist.add_module("LM7805_TO220".to_string(), ModuleKind::PhysicalComponent);
    let cap_mod = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let res_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    
    // Add pins for LM7805 (based on actual KiCad pinout)
    let _vi_pin = netlist.add_pin(reg_mod, "VI".to_string(), PinDirection::In, PinType::Power).unwrap();
    let _gnd_pin = netlist.add_pin(reg_mod, "GND".to_string(), PinDirection::InOut, PinType::Ground).unwrap();
    let _vo_pin = netlist.add_pin(reg_mod, "VO".to_string(), PinDirection::Out, PinType::Power).unwrap();
    
    // Capacitor pins
    let _cap_p = netlist.add_pin(cap_mod, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let _cap_n = netlist.add_pin(cap_mod, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    // Resistor pins  
    let _res_1 = netlist.add_pin(res_mod, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let _res_2 = netlist.add_pin(res_mod, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    // Create component instances
    let u1 = netlist.add_instance("U1".to_string(), reg_mod).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_mod).unwrap();
    let c2 = netlist.add_instance("C2".to_string(), cap_mod).unwrap();
    let r1 = netlist.add_instance("R1".to_string(), res_mod).unwrap();
    
    // Create pin instances
    let u1_pins = netlist.create_pin_instances(u1).unwrap();
    let c1_pins = netlist.create_pin_instances(c1).unwrap();
    let c2_pins = netlist.create_pin_instances(c2).unwrap();
    let r1_pins = netlist.create_pin_instances(r1).unwrap();
    
    // Define nets
    let vin = netlist.add_net(Some("VIN".to_string()));
    let vout = netlist.add_net(Some("VOUT".to_string()));
    let gnd = netlist.add_net(Some("GND".to_string()));
    
    // Connect components properly:
    // VIN: 12V input -> C1+ and U1.VI
    netlist.connect(vin, ConnectionPoint::PinInstance(u1_pins[0])).unwrap(); // VI
    netlist.connect(vin, ConnectionPoint::PinInstance(c1_pins[0])).unwrap(); // C1+
    
    // VOUT: U1.VO -> C2+ and R1.1  
    netlist.connect(vout, ConnectionPoint::PinInstance(u1_pins[2])).unwrap(); // VO
    netlist.connect(vout, ConnectionPoint::PinInstance(c2_pins[0])).unwrap(); // C2+
    netlist.connect(vout, ConnectionPoint::PinInstance(r1_pins[0])).unwrap(); // R1.1
    
    // GND: Common ground
    netlist.connect(gnd, ConnectionPoint::PinInstance(u1_pins[1])).unwrap(); // GND
    netlist.connect(gnd, ConnectionPoint::PinInstance(c1_pins[1])).unwrap(); // C1-
    netlist.connect(gnd, ConnectionPoint::PinInstance(c2_pins[1])).unwrap(); // C2-
    netlist.connect(gnd, ConnectionPoint::PinInstance(r1_pins[1])).unwrap(); // R1.2
    
    netlist
}

fn generate_circuit_svg(lm7805_svg: &str, cap_svg: &str, res_svg: &str) -> String {
    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="900" height="600" viewBox="0 0 900 600">
  <title>LM7805 Linear Voltage Regulator - KiCad Symbols</title>
  <rect width="100%" height="100%" fill="white"/>
  
  <!-- Circuit Schematic -->
  <g id="circuit">
    
    <!-- Title -->
    <text x="450" y="50" text-anchor="middle" font-size="24" font-weight="bold">
      LM7805 5V Linear Regulator Circuit
    </text>
    <text x="450" y="75" text-anchor="middle" font-size="14" fill="#666">
      Using Authentic KiCad Component Symbols
    </text>
    
    <!-- Input Section -->
    <text x="100" y="280" font-size="16" font-weight="bold" fill="blue">VIN</text>
    <text x="100" y="300" font-size="12" fill="blue">12V DC</text>
    <line x1="140" y1="285" x2="200" y2="285" stroke="blue" stroke-width="2"/>
    
    <!-- C1 Input Capacitor (10µF) -->
    <g transform="translate(220, 285) scale(30) rotate(90)">
      {}
    </g>
    <text x="220" y="240" text-anchor="middle" font-size="14" font-weight="bold">C1</text>
    <text x="220" y="255" text-anchor="middle" font-size="12">10µF</text>
    <circle cx="220" cy="285" r="3" fill="blue"/>
    
    <!-- LM7805 Voltage Regulator -->
    <g transform="translate(400, 285) scale(80)">
      {}
    </g>
    <text x="400" y="200" text-anchor="middle" font-size="16" font-weight="bold">U1</text>
    <text x="400" y="220" text-anchor="middle" font-size="12">LM7805</text>
    
    <!-- Connect input to regulator -->
    <line x1="220" y1="285" x2="340" y2="285" stroke="blue" stroke-width="2"/>
    
    <!-- C2 Output Capacitor (100nF) -->
    <g transform="translate(580, 285) scale(30) rotate(90)">
      {}
    </g>
    <text x="580" y="240" text-anchor="middle" font-size="14" font-weight="bold">C2</text>
    <text x="580" y="255" text-anchor="middle" font-size="12">100nF</text>
    <circle cx="580" cy="285" r="3" fill="red"/>
    
    <!-- R1 Load Resistor (1kΩ) -->
    <g transform="translate(700, 285) scale(30)">
      {}
    </g>
    <text x="700" y="240" text-anchor="middle" font-size="14" font-weight="bold">R1</text>
    <text x="700" y="255" text-anchor="middle" font-size="12">1kΩ</text>
    <circle cx="700" cy="285" r="3" fill="red"/>
    
    <!-- Connect output -->
    <line x1="460" y1="285" x2="750" y2="285" stroke="red" stroke-width="2"/>
    
    <!-- Output label -->
    <text x="780" y="280" font-size="16" font-weight="bold" fill="red">VOUT</text>
    <text x="780" y="300" font-size="12" fill="red">5V DC</text>
    
    <!-- Ground connections -->
    <line x1="100" y1="420" x2="800" y2="420" stroke="black" stroke-width="3"/>
    
    <!-- Ground drops from components -->
    <line x1="220" y1="315" x2="220" y2="420" stroke="black" stroke-width="1"/>
    <line x1="400" y1="345" x2="400" y2="420" stroke="black" stroke-width="1"/>
    <line x1="580" y1="315" x2="580" y2="420" stroke="black" stroke-width="1"/>
    <line x1="700" y1="315" x2="700" y2="420" stroke="black" stroke-width="1"/>
    
    <!-- Ground connection dots -->
    <circle cx="220" cy="420" r="4" fill="black"/>
    <circle cx="400" cy="420" r="4" fill="black"/>
    <circle cx="580" cy="420" r="4" fill="black"/>
    <circle cx="700" cy="420" r="4" fill="black"/>
    
    <!-- Ground symbol -->
    <g transform="translate(450, 420)">
      <line x1="0" y1="0" x2="0" y2="20" stroke="black" stroke-width="2"/>
      <line x1="-25" y1="20" x2="25" y2="20" stroke="black" stroke-width="3"/>
      <line x1="-18" y1="28" x2="18" y2="28" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="36" x2="10" y2="36" stroke="black" stroke-width="1"/>
    </g>
    <text x="450" y="465" text-anchor="middle" font-size="14" font-weight="bold">GND</text>
    
    <!-- Specifications Box -->
    <rect x="50" y="500" width="250" height="80" fill="#f0f0f0" stroke="black"/>
    <text x="60" y="520" font-size="12" font-weight="bold">Specifications:</text>
    <text x="60" y="540" font-size="11">Input: 7-35V DC</text>
    <text x="60" y="555" font-size="11">Output: 5V DC @ 1A max</text>
    <text x="60" y="570" font-size="11">Dropout: 2V typical</text>
    
    <!-- Component Values Box -->
    <rect x="600" y="500" width="250" height="80" fill="#f0f0f0" stroke="black"/>
    <text x="610" y="520" font-size="12" font-weight="bold">Component Values:</text>
    <text x="610" y="540" font-size="11">C1: 10µF electrolytic (input filter)</text>
    <text x="610" y="555" font-size="11">C2: 100nF ceramic (output bypass)</text>
    <text x="610" y="570" font-size="11">R1: 1kΩ (load resistor)</text>
    
    <!-- Attribution -->
    <text x="450" y="590" text-anchor="middle" font-size="10" fill="#999">
      KiCad symbols loaded from component database
    </text>
  </g>
</svg>"#, cap_svg, lm7805_svg, cap_svg, res_svg)
}