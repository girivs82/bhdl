//! KiCad Integration Example
//! 
//! Demonstrates the complete KiCad symbol parsing and component extraction pipeline:
//! 1. Parse KiCad S-expression symbol
//! 2. Extract electrical specifications
//! 3. Convert symbol to SVG format
//! 4. Create component database entry

use bhdl_components::{ComponentLibrary, kicad::{parser::KiCadSymbolParser, svg_converter::KiCadSvgConverter, extractor::KiCadExtractor}};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔧 KiCad Integration Demo");
    println!("==========================");

    // Sample KiCad symbol library content (resistor)
    let kicad_symbol_library = r#"
(kicad_symbol_lib (version 20220914) (generator kicad_symbol_editor)
  (symbol "R" (pin_numbers hide) (pin_names (offset 0))
    (in_bom yes) (on_board yes)
    (property "Reference" "R" (at 2.032 0 90)
      (effects (font (size 1.27 1.27))))
    (property "Value" "1kΩ" (at 0 0 90)
      (effects (font (size 1.27 1.27))))
    (property "Footprint" "Resistor_SMD:R_0805_2012Metric" (at -1.778 0 90)
      (effects (font (size 1.27 1.27)) hide))
    (property "Datasheet" "https://example.com/resistor-datasheet.pdf" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (property "Description" "Thick film resistor, ±5% tolerance" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (property "Power" "0.125W" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (symbol "R_0_1"
      (rectangle (start -1.016 -2.54) (end 1.016 2.54)
        (stroke (width 0.254) (type default))
        (fill (type none))
      )
      (pin passive line (at 0 3.81 270) (length 1.27)
        (name "~" (effects (font (size 1.27 1.27))))
        (number "1" (effects (font (size 1.27 1.27))))
      )
      (pin passive line (at 0 -3.81 90) (length 1.27)
        (name "~" (effects (font (size 1.27 1.27))))
        (number "2" (effects (font (size 1.27 1.27))))
      )
    )
  )
  
  (symbol "LM358" (pin_numbers show) (pin_names (offset 0.127))
    (in_bom yes) (on_board yes)
    (property "Reference" "U" (at 3.81 3.175 0)
      (effects (font (size 1.27 1.27))))
    (property "Value" "LM358" (at 5.08 -3.175 0)
      (effects (font (size 1.27 1.27))))
    (property "Footprint" "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (property "Datasheet" "https://www.ti.com/lit/ds/symlink/lm358.pdf" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (property "Description" "Dual operational amplifier, low power" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (property "Voltage" "3V-32V" (at 0 0 0)
      (effects (font (size 1.27 1.27)) hide))
    (symbol "LM358_1_1"
      (rectangle (start -5.08 5.08) (end 5.08 -5.08)
        (stroke (width 0.254) (type default))
        (fill (type background))
      )
      (pin input line (at -7.62 2.54 0) (length 2.54)
        (name "+" (effects (font (size 1.27 1.27))))
        (number "3" (effects (font (size 1.27 1.27))))
      )
      (pin input line (at -7.62 -2.54 0) (length 2.54)
        (name "-" (effects (font (size 1.27 1.27))))
        (number "2" (effects (font (size 1.27 1.27))))
      )
      (pin output line (at 7.62 0 180) (length 2.54)
        (name "~" (effects (font (size 1.27 1.27))))
        (number "1" (effects (font (size 1.27 1.27))))
      )
      (pin power_in line (at -2.54 7.62 270) (length 2.54)
        (name "V+" (effects (font (size 1.27 1.27))))
        (number "8" (effects (font (size 1.27 1.27))))
      )
      (pin power_in line (at -2.54 -7.62 90) (length 2.54)
        (name "V-" (effects (font (size 1.27 1.27))))
        (number "4" (effects (font (size 1.27 1.27))))
      )
    )
  )
)
"#;

    println!("\n📖 Parsing KiCad symbol library...");
    
    // Step 1: Parse KiCad symbols
    let parser = KiCadSymbolParser::new();
    let symbols = parser.parse_symbol_library(kicad_symbol_library)?;
    
    println!("✅ Parsed {} symbols from library", symbols.len());
    
    // Step 2: Process each symbol
    let svg_converter = KiCadSvgConverter::new();
    let extractor = KiCadExtractor::new();
    
    // Create temporary database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("kicad_components.db");
    let library = ComponentLibrary::new(&db_path).await?;
    
    for (i, symbol) in symbols.iter().enumerate() {
        println!("\n🔍 Processing symbol {}: '{}'", i + 1, symbol.name);
        
        // Convert to SVG
        println!("  📐 Converting to SVG...");
        let svg_data = svg_converter.convert_symbol_to_svg(symbol)?;
        println!("  ✅ Generated {} character SVG", svg_data.len());
        
        // Extract component data
        println!("  🔬 Extracting electrical specifications...");
        let component = extractor.extract_component(symbol, svg_data.clone())?;
        
        println!("  📋 Component details:");
        println!("    - Name: {}", component.name);
        println!("    - Category: {:?}", component.category);
        println!("    - Reference: {}", symbol.reference);
        println!("    - Value: {}", symbol.value);
        if let Some(footprint) = &symbol.footprint {
            println!("    - Footprint: {}", footprint);
        }
        if let Some(datasheet) = &symbol.datasheet {
            println!("    - Datasheet: {}", datasheet);
        }
        
        // Show electrical specifications
        if !component.electrical_specs.is_empty() {
            println!("    - Electrical specs:");
            for spec in &component.electrical_specs {
                if let Some(tolerance) = spec.spec_tolerance {
                    println!("      * {}: {:.3} {} (±{:.1}%)", 
                            spec.spec_name, spec.spec_value, spec.spec_unit, tolerance * 100.0);
                } else {
                    println!("      * {}: {:.3} {}", 
                            spec.spec_name, spec.spec_value, spec.spec_unit);
                }
            }
        } else {
            println!("    - No electrical specifications extracted");
        }
        
        // Show pins
        println!("    - Pins: {} total", component.pins.len());
        for pin in &component.pins {
            let pin_name = pin.pin_name.as_deref().unwrap_or("~");
            println!("      * Pin {}: {} ({:?})", pin.pin_number, pin_name, pin.electrical_type);
        }
        
        // Insert into database
        println!("  💾 Inserting into database...");
        let component_id = library.insert_component(&component).await?;
        println!("  ✅ Stored with ID: {}", component_id);
        
        // Show SVG preview (first 200 characters)
        println!("  🎨 SVG preview:");
        let preview = if svg_data.len() > 200 {
            format!("{}...", &svg_data[..200])
        } else {
            svg_data.clone()
        };
        println!("      {}", preview.replace('\n', "\n      "));
    }
    
    // Step 3: Demonstrate search functionality
    println!("\n🔍 Testing component search...");
    
    let resistor_results = library.search_components("resistor").await?;
    println!("Found {} resistor(s)", resistor_results.len());
    
    let op_amp_results = library.search_components("LM358").await?;
    println!("Found {} op-amp(s)", op_amp_results.len());
    
    // Step 4: Show database statistics
    println!("\n📊 Database Statistics:");
    let stats = library.get_stats().await?;
    println!("  - Total components: {}", stats.total_components);
    println!("  - Components with symbols: {}", stats.components_with_symbols);
    println!("  - Components with supplier data: {}", stats.components_with_supplier_data);
    
    if !stats.categories.is_empty() {
        println!("  - Category breakdown:");
        for (category, count) in &stats.categories {
            println!("    * {}: {}", category, count);
        }
    }
    
    // Step 5: Test component retrieval with symbol
    if let Some(first_result) = resistor_results.first() {
        println!("\n🎯 Testing symbol retrieval...");
        if let Some(svg_data) = library.get_component_symbol(first_result.id).await? {
            println!("✅ Retrieved SVG symbol ({} characters)", svg_data.len());
        }
    }
    
    println!("\n🎉 KiCad Integration Demo completed successfully!");
    println!("\nThis demonstrates Phase 3.0.2 KiCad integration features:");
    println!("  ✅ S-expression parser for KiCad symbol libraries");
    println!("  ✅ Electrical specification extraction from symbol properties");
    println!("  ✅ SVG conversion for schematic visualization");
    println!("  ✅ Component categorization and pin mapping");
    println!("  ✅ Database integration with search capabilities");
    println!("  ✅ Comprehensive testing suite");
    
    Ok(())
}