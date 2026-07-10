//! Real-world integration tests with actual KiCad libraries
//!
//! These tests validate the complete pipeline:
//! 1. Parse KiCad symbol libraries and extract component data
//! 2. Store and search components in the database
//! 3. Run two-stage synthesis (spec-only — no supplier APIs required)
//! 4. Cache behaviour and alternative selection

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use bhdl_components::{
    database::ComponentDatabase,
    kicad::{
        extractor::KiCadExtractor, parser::KiCadSymbolParser, svg_converter::KiCadSvgConverter,
    },
    supplier::cache::SupplierDataCache,
    synthesis::two_stage::{TwoStageConfig, TwoStageSynthesizer},
    types::{
        Component, ComponentCategory, ComponentRequirements, ElectricalSpec,
    },
};

/// Test parsing a real KiCad library and importing extracted components
#[tokio::test]
async fn test_kicad_library_import_real_world() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_components.db");

    // Initialize database
    let database = ComponentDatabase::new(&db_path).await?;

    // Try to find KiCad libraries on the system
    let kicad_library_paths = find_kicad_libraries()?;

    if kicad_library_paths.is_empty() {
        println!("⚠️  No KiCad libraries found, skipping real-world test");
        return Ok(());
    }

    println!("📚 Found {} KiCad libraries", kicad_library_paths.len());

    // Parse the first available library
    let library_path = &kicad_library_paths[0];
    println!("📖 Testing with library: {}", library_path.display());

    let content = std::fs::read_to_string(library_path)?;
    let parser = KiCadSymbolParser::new();
    let symbols = parser
        .parse_symbol_library(&content)
        .map_err(|e| anyhow::anyhow!("parse failed: {e:?}"))?;

    println!("✅ Parsed {} symbols", symbols.len());
    assert!(!symbols.is_empty(), "No symbols were parsed");

    // Extract and import components, rendering each symbol's real SVG (the
    // extractor validates the SVG payload, so a placeholder won't do).
    let extractor = KiCadExtractor::new();
    let svg_converter = KiCadSvgConverter::new();
    let mut imported = 0usize;
    for symbol in symbols.iter().take(50) {
        let Ok(svg) = svg_converter.convert_symbol_to_svg(symbol) else { continue };
        if let Ok(component) = extractor.extract_component(symbol, svg) {
            database.insert_component(&component).await?;
            imported += 1;
        }
    }

    println!("✅ Imported {imported} components");
    assert!(imported > 0, "No components were imported");

    // Test component search round-trips through the database
    let sample_name = &symbols[0].name;
    let components = database.search_components(sample_name).await?;
    println!("🔍 Search for '{}' found {} components", sample_name, components.len());

    if let Some(component) = components.first() {
        println!(
            "📦 Sample component: {} ({})",
            component.name,
            component.category.as_str()
        );
        if let Some(part_number) = &component.part_number {
            println!("   Part number: {}", part_number);
        }
        for spec in &component.electrical_specs {
            println!("   {}: {} {}", spec.spec_name, spec.spec_value, spec.spec_unit);
        }
    }

    Ok(())
}

/// Test end-to-end component synthesis (spec-only stage 1, no supplier APIs)
#[tokio::test]
async fn test_end_to_end_component_synthesis() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_synthesis.db");

    // Initialize database with test components
    let database = ComponentDatabase::new(&db_path).await?;
    add_test_components(&database).await?;

    // Spec-only synthesis: supplier lookup disabled so the test runs
    // without DigiKey/Nexar credentials.
    let synthesis_config = TwoStageConfig {
        enable_supplier_lookup: false,
        ..TwoStageConfig::default()
    };
    let synthesizer = TwoStageSynthesizer::new(synthesis_config);

    println!("🎯 Testing component synthesis...");

    // Resistor synthesis: 10kΩ, 0.25W, 5%, qty 100
    println!("🔍 Synthesizing 10kΩ resistors...");
    let resistor_requirements = ComponentRequirements::resistor(10_000.0, 0.125, 0.05, 100);
    let synthesis_result = synthesizer
        .synthesize("resistor", &resistor_requirements, &database, None)
        .await?;

    println!("✅ Synthesis complete:");
    println!("   Recommended: {:?}", synthesis_result.recommended.as_ref().map(|o| &o.component.name));
    println!("   Alternatives: {}", synthesis_result.alternatives.len());
    println!("   Confidence: {:.2}", synthesis_result.confidence);
    for note in &synthesis_result.synthesis_notes {
        println!("   Note: {}", note);
    }

    assert!(
        synthesis_result.recommended.is_some(),
        "Expected a recommended resistor, notes: {:?}",
        synthesis_result.synthesis_notes
    );

    if let Some(option) = &synthesis_result.recommended {
        println!("📦 Recommended: {}", option.component.name);
        println!("   Fitness score: {:.2}", option.fitness_score);
        println!("   Reason: {}", option.selection_reason);
    }

    // Capacitor synthesis: 100nF, 50V, 10%, qty 50
    println!("\n🔍 Synthesizing ceramic capacitors...");
    let capacitor_requirements = ComponentRequirements::capacitor(100e-9, 50.0, 0.10, 50);
    let cap_result = synthesizer
        .synthesize("capacitor", &capacitor_requirements, &database, None)
        .await?;

    println!("✅ Capacitor synthesis complete:");
    println!("   Recommended: {:?}", cap_result.recommended.as_ref().map(|o| &o.component.name));
    println!("   Alternatives: {}", cap_result.alternatives.len());

    Ok(())
}

/// Test cache performance and hit rates
#[tokio::test]
async fn test_cache_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let cache_path = temp_dir.path().join("test_cache_perf.db");

    // Initialize cache
    let cache = SupplierDataCache::new(cache_path.to_string_lossy().to_string(), 100)?;

    println!("🏃 Testing cache performance...");

    // Test rate limiting
    let can_request_initial = cache.check_rate_limit("DigiKey").await?;
    println!("✅ Initial rate limit check: {}", can_request_initial);

    // Make several rapid requests to test rate limiting
    let mut allowed_requests = 0;
    for _ in 0..20 {
        if cache.check_rate_limit("DigiKey").await? {
            allowed_requests += 1;
        }

        // Small delay to avoid overwhelming the test
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    println!("📊 Rate limiting test: {}/20 requests allowed", allowed_requests);
    assert!(allowed_requests > 0, "Rate limiting should allow some requests");

    // Test cache statistics
    let stats = cache.get_stats().await?;
    println!("📈 Cache statistics:");
    println!("   Persistent entries: {}", stats.total_persistent_entries);
    println!("   Memory entries: {}", stats.memory_entries);
    println!("   Memory capacity: {}", stats.memory_cache_capacity);

    // Test cache cleanup
    let cleaned = cache.cleanup_expired().await?;
    println!("🧹 Cleaned up {} expired entries", cleaned);

    Ok(())
}

/// Test component alternative selection
#[tokio::test]
async fn test_component_alternatives() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_alternatives.db");

    // Initialize database with test data
    let database = ComponentDatabase::new(&db_path).await?;
    add_comprehensive_test_components(&database).await?;

    println!("🔄 Testing component alternative selection...");

    // Search for the 10k resistors seeded above
    let similar_components = database.search_components("10k").await?;
    println!("🔍 Found {} similar resistor components", similar_components.len());

    // Match by electrical spec (resistance == 10kΩ)
    let matching_components: Vec<&Component> = similar_components
        .iter()
        .filter(|component| {
            component.electrical_specs.iter().any(|spec| {
                spec.spec_name == "resistance" && (spec.spec_value - 10_000.0).abs() < 1.0
            })
        })
        .collect();

    println!("✅ Found {} exact resistance matches", matching_components.len());
    assert!(!matching_components.is_empty(), "Expected 10k resistance matches");

    // Show component alternatives with different packages
    let mut package_alternatives: std::collections::HashMap<String, Vec<&str>> =
        std::collections::HashMap::new();
    for component in &matching_components {
        if let Some(package) = &component.package_type {
            package_alternatives
                .entry(package.clone())
                .or_default()
                .push(component.name.as_str());
        }
    }

    println!("📦 Package alternatives:");
    for (package, components) in &package_alternatives {
        println!("   {}: {} options", package, components.len());
    }
    assert!(
        package_alternatives.len() > 1,
        "Expected 10k alternatives in more than one package"
    );

    Ok(())
}

// Helper functions

/// Find KiCad libraries on the system
pub fn find_kicad_libraries() -> Result<Vec<PathBuf>> {
    let mut libraries = Vec::new();

    // Common KiCad library locations
    let possible_paths = vec![
        PathBuf::from("/usr/share/kicad/symbols"),
        PathBuf::from("/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols"),
        PathBuf::from("/opt/kicad/share/kicad/symbols"),
        PathBuf::from("C:\\Program Files\\KiCad\\share\\kicad\\symbols"),
        // User libraries
        dirs::home_dir().map(|h| h.join("Documents/KiCad/symbols")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join("KiCad/symbols")).unwrap_or_default(),
    ];

    for path in possible_paths {
        if path.exists() && path.is_dir() {
            // Look for .kicad_sym files
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    if let Some(extension) = entry.path().extension() {
                        if extension == "kicad_sym" {
                            libraries.push(entry.path());
                            break; // Just need one library per directory
                        }
                    }
                }
            }
        }
    }

    Ok(libraries)
}

/// Build a passive test component from (spec_name, value, unit) triples
fn make_passive(
    name: &str,
    category: ComponentCategory,
    specs: &[(&str, f64, &str)],
    package: &str,
    description: &str,
) -> Component {
    Component {
        id: 0, // Will be assigned by database
        name: name.to_string(),
        description: Some(description.to_string()),
        manufacturer: Some("Test Manufacturer".to_string()),
        part_number: Some(format!("TEST_{name}")),
        package_type: Some(package.to_string()),
        category,
        subcategory: None,
        datasheet_url: None,
        electrical_specs: specs
            .iter()
            .map(|(spec_name, spec_value, spec_unit)| ElectricalSpec {
                spec_name: spec_name.to_string(),
                spec_value: *spec_value,
                spec_unit: spec_unit.to_string(),
                spec_tolerance: None,
                min_value: None,
                max_value: None,
                conditions: None,
            })
            .collect(),
        pins: vec![],
        symbol: None,
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Add basic test components to database
async fn add_test_components(database: &ComponentDatabase) -> Result<()> {
    // Some basic resistors
    let resistors = vec![
        ("R_10k_0805", 10_000.0, "0805"),
        ("R_10k_0603", 10_000.0, "0603"),
        ("R_1k_0805", 1_000.0, "0805"),
        ("R_100_0805", 100.0, "0805"),
    ];

    for (name, resistance, package) in resistors {
        let component = make_passive(
            name,
            ComponentCategory::Resistor,
            &[("resistance", resistance, "Ω"), ("power_rating", 0.25, "W")],
            package,
            &format!("{resistance}Ω resistor, {package} package"),
        );
        database.insert_component(&component).await?;
    }

    // Some capacitors
    let capacitors = vec![
        ("C_100nF_0805", 100e-9, "0805"),
        ("C_1uF_0805", 1e-6, "0805"),
        ("C_10uF_1206", 10e-6, "1206"),
    ];

    for (name, capacitance, package) in capacitors {
        let component = make_passive(
            name,
            ComponentCategory::Capacitor,
            &[("capacitance", capacitance, "F"), ("voltage_rating", 100.0, "V")],
            package,
            &format!("{capacitance}F capacitor, {package} package"),
        );
        database.insert_component(&component).await?;
    }

    println!("✅ Added test components to database");
    Ok(())
}

/// Add comprehensive test components for alternative testing
async fn add_comprehensive_test_components(database: &ComponentDatabase) -> Result<()> {
    // Add the basic components first
    add_test_components(database).await?;

    // Add more varied resistor alternatives
    let advanced_resistors = vec![
        ("R_10k_0402", 10_000.0, "0402"),
        ("R_10k_1206", 10_000.0, "1206"),
        ("R_10k_2512", 10_000.0, "2512"),
        ("R_10k_TH", 10_000.0, "THT"), // Through-hole
    ];

    for (name, resistance, package) in advanced_resistors {
        let component = make_passive(
            name,
            ComponentCategory::Resistor,
            &[("resistance", resistance, "Ω"), ("power_rating", 0.25, "W")],
            package,
            &format!("{resistance}Ω precision resistor, {package} package"),
        );
        database.insert_component(&component).await?;
    }

    println!("✅ Added comprehensive test components");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_kicad_libraries() {
        let libraries = find_kicad_libraries().unwrap();
        // This test might not find libraries on all systems
        println!("Found {} KiCad libraries", libraries.len());
        for lib in libraries.iter().take(3) {
            println!("  - {}", lib.display());
        }
    }
}
