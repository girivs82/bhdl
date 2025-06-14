//! Basic usage example for bhdl-components
//! 
//! This example demonstrates the core functionality of the component library:
//! - Creating a component database
//! - Inserting components with electrical specifications
//! - Searching for components
//! - Working with component symbols

use bhdl_components::{
    ComponentLibrary, Component, ElectricalSpec, ComponentSymbol, PinDefinition,
    types::{ComponentCategory, PinType, PinShape}
};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a temporary database for this demo
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("demo_components.db");
    
    println!("🚀 BHDL Component Library Demo");
    println!("==============================");
    
    // Create component library
    println!("\n📦 Creating component library...");
    let library = ComponentLibrary::new(&db_path).await?;
    println!("✅ Component library created successfully");
    
    // Create some example components
    println!("\n🔧 Creating sample components...");
    
    // Create a 1kΩ resistor
    let resistor_1k = Component {
        id: 0, // Will be assigned by database
        name: "1kΩ Resistor".to_string(),
        description: Some("Carbon film resistor, 5% tolerance".to_string()),
        manufacturer: Some("Generic Electronics".to_string()),
        part_number: Some("RES1K-CF-5PCT".to_string()),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Resistor,
        subcategory: Some("Carbon Film".to_string()),
        datasheet_url: Some("https://example.com/resistor-datasheet.pdf".to_string()),
        electrical_specs: vec![
            ElectricalSpec {
                spec_name: "resistance".to_string(),
                spec_value: 1000.0,
                spec_unit: "Ω".to_string(),
                spec_tolerance: Some(0.05), // 5%
                min_value: Some(950.0),
                max_value: Some(1050.0),
                conditions: Some("25°C".to_string()),
            },
            ElectricalSpec {
                spec_name: "power_rating".to_string(),
                spec_value: 0.125,
                spec_unit: "W".to_string(),
                spec_tolerance: None,
                min_value: None,
                max_value: None,
                conditions: Some("70°C".to_string()),
            }
        ],
        pins: vec![
            PinDefinition {
                pin_number: "1".to_string(),
                pin_name: None,
                electrical_type: PinType::Passive,
                x_position: 0.0,
                y_position: 0.0,
                orientation: 0,
                length: 2.54,
                pin_shape: PinShape::Line,
            },
            PinDefinition {
                pin_number: "2".to_string(),
                pin_name: None,
                electrical_type: PinType::Passive,
                x_position: 10.16,
                y_position: 0.0,
                orientation: 180,
                length: 2.54,
                pin_shape: PinShape::Line,
            }
        ],
        symbol: Some(ComponentSymbol {
            symbol_name: "R".to_string(),
            svg_data: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 16">
  <rect x="10" y="4" width="20" height="8" fill="none" stroke="black" stroke-width="1"/>
  <line x1="0" y1="8" x2="10" y2="8" stroke="black" stroke-width="1"/>
  <line x1="30" y1="8" x2="40" y2="8" stroke="black" stroke-width="1"/>
  <text x="20" y="12" text-anchor="middle" font-size="6">1kΩ</text>
</svg>"#.to_string(),
            bounding_box_width: 40.0,
            bounding_box_height: 16.0,
            reference_point_x: 0.0,
            reference_point_y: 8.0,
            style_variant: None,
        }),
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Create a 100nF capacitor
    let capacitor_100n = Component {
        id: 0,
        name: "100nF Capacitor".to_string(),
        description: Some("Ceramic capacitor, X7R dielectric".to_string()),
        manufacturer: Some("Ceramic Corp".to_string()),
        part_number: Some("CAP100N-X7R-0805".to_string()),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Capacitor,
        subcategory: Some("Ceramic".to_string()),
        datasheet_url: Some("https://example.com/capacitor-datasheet.pdf".to_string()),
        electrical_specs: vec![
            ElectricalSpec {
                spec_name: "capacitance".to_string(),
                spec_value: 100e-9, // 100nF
                spec_unit: "F".to_string(),
                spec_tolerance: Some(0.10), // 10%
                min_value: Some(90e-9),
                max_value: Some(110e-9),
                conditions: Some("25°C, 1kHz".to_string()),
            },
            ElectricalSpec {
                spec_name: "voltage_rating".to_string(),
                spec_value: 50.0,
                spec_unit: "V".to_string(),
                spec_tolerance: None,
                min_value: None,
                max_value: None,
                conditions: Some("DC".to_string()),
            }
        ],
        pins: vec![
            PinDefinition {
                pin_number: "1".to_string(),
                pin_name: Some("+".to_string()),
                electrical_type: PinType::Passive,
                x_position: 0.0,
                y_position: 0.0,
                orientation: 0,
                length: 2.54,
                pin_shape: PinShape::Line,
            },
            PinDefinition {
                pin_number: "2".to_string(),
                pin_name: Some("-".to_string()),
                electrical_type: PinType::Passive,
                x_position: 7.62,
                y_position: 0.0,
                orientation: 180,
                length: 2.54,
                pin_shape: PinShape::Line,
            }
        ],
        symbol: Some(ComponentSymbol {
            symbol_name: "C".to_string(),
            svg_data: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 16">
  <line x1="11" y1="2" x2="11" y2="14" stroke="black" stroke-width="2"/>
  <line x1="19" y1="2" x2="19" y2="14" stroke="black" stroke-width="2"/>
  <line x1="0" y1="8" x2="11" y2="8" stroke="black" stroke-width="1"/>
  <line x1="19" y1="8" x2="30" y2="8" stroke="black" stroke-width="1"/>
  <text x="15" y="12" text-anchor="middle" font-size="6">100nF</text>
</svg>"#.to_string(),
            bounding_box_width: 30.0,
            bounding_box_height: 16.0,
            reference_point_x: 0.0,
            reference_point_y: 8.0,
            style_variant: None,
        }),
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Insert components into database
    let resistor_id = library.insert_component(&resistor_1k).await?;
    let capacitor_id = library.insert_component(&capacitor_100n).await?;
    
    println!("✅ Inserted resistor with ID: {}", resistor_id);
    println!("✅ Inserted capacitor with ID: {}", capacitor_id);
    
    // Demonstrate search functionality
    println!("\n🔍 Testing search functionality...");
    
    let resistor_results = library.search_components("resistor").await?;
    println!("Found {} resistor(s):", resistor_results.len());
    for component in &resistor_results {
        println!("  - {} ({})", component.name, component.part_number.as_deref().unwrap_or("No P/N"));
    }
    
    let capacitor_results = library.search_components("capacitor").await?;
    println!("Found {} capacitor(s):", capacitor_results.len());
    for component in &capacitor_results {
        println!("  - {} ({})", component.name, component.part_number.as_deref().unwrap_or("No P/N"));
    }
    
    // Demonstrate component retrieval with caching
    println!("\n💾 Testing component retrieval and caching...");
    
    let retrieved_resistor = library.get_component(resistor_id).await?.unwrap();
    println!("Retrieved: {}", retrieved_resistor.name);
    println!("  Resistance: {:.0}Ω (±{:.1}%)", 
             retrieved_resistor.get_electrical_spec("resistance").unwrap().spec_value,
             retrieved_resistor.get_electrical_spec("resistance").unwrap().spec_tolerance.unwrap() * 100.0);
    
    // Test symbol retrieval
    let symbol_svg = library.get_component_symbol(resistor_id).await?.unwrap();
    println!("  Symbol SVG length: {} characters", symbol_svg.len());
    
    // Second retrieval should hit cache
    let _retrieved_again = library.get_component(resistor_id).await?.unwrap();
    
    // Display cache statistics
    let cache_stats = library.get_cache_stats();
    println!("\n📊 Cache Performance:");
    println!("  Component cache hit rate: {:.1}%", cache_stats.component_hit_rate() * 100.0);
    println!("  Symbol cache hit rate: {:.1}%", cache_stats.symbol_hit_rate() * 100.0);
    println!("  Search cache hit rate: {:.1}%", cache_stats.search_hit_rate() * 100.0);
    
    // Display database statistics
    let db_stats = library.get_stats().await?;
    println!("\n📈 Database Statistics:");
    println!("  Total components: {}", db_stats.total_components);
    println!("  Components with symbols: {}", db_stats.components_with_symbols);
    println!("  Category breakdown:");
    for (category, count) in &db_stats.categories {
        println!("    {}: {}", category, count);
    }
    
    println!("\n🎉 Demo completed successfully!");
    println!("\nThis demonstrates Phase 3.0.1 core infrastructure:");
    println!("  ✅ SQLite database with full-text search");
    println!("  ✅ Multi-level caching system");
    println!("  ✅ Component type system with electrical specs");
    println!("  ✅ SVG symbol storage and retrieval");
    println!("  ✅ Database migration system");
    println!("  ✅ Comprehensive testing");
    
    Ok(())
}