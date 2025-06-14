//! Real-world integration tests with actual KiCad libraries
//! 
//! These tests validate the complete pipeline:
//! 1. Import KiCad symbol libraries
//! 2. Extract component data
//! 3. Search for supplier information
//! 4. Cache and optimize results

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio_test;

use bhdl_components::{
    ComponentLibrary,
    database::ComponentDatabase,
    kicad::{KiCadLibraryImporter, KiCadSymbolCache},
    supplier::{
        multi_backend::{MultiBackendSupplierService, MultiBackendConfig},
        cache::SupplierDataCache,
    },
    synthesis::two_stage::{TwoStageSynthesizer, TwoStageConfig},
    config::SupplierConfig,
    types::{ComponentRequirements, ComponentType, QuantityRequirement},
};

/// Test importing a real KiCad library and extracting component data
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
    
    // Import the first available library
    let library_path = &kicad_library_paths[0];
    println!("📖 Testing with library: {}", library_path.display());
    
    let mut importer = KiCadLibraryImporter::new();
    let import_result = importer.import_library(library_path, &database).await?;
    
    println!("✅ Import successful:");
    println!("   Components imported: {}", import_result.components_imported);
    println!("   Symbols processed: {}", import_result.symbols_processed);
    println!("   Warnings: {}", import_result.warnings.len());
    
    // Verify components were imported
    assert!(import_result.components_imported > 0, "No components were imported");
    
    // Test component search
    let components = database.search_components("resistor", 10).await?;
    println!("🔍 Found {} resistor components", components.len());
    
    if !components.is_empty() {
        let component = &components[0];
        println!("📦 Sample component: {} ({})", component.name, component.component_type);
        
        if let Some(part_number) = &component.part_number {
            println!("   Part number: {}", part_number);
        }
        
        // Print component properties
        for (key, value) in &component.properties {
            println!("   {}: {}", key, value);
        }
    }
    
    Ok(())
}

/// Test end-to-end component synthesis with real supplier data
#[tokio::test]
async fn test_end_to_end_component_synthesis() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test_synthesis.db");
    let cache_path = temp_dir.path().join("test_cache.db");
    
    // Initialize database
    let database = ComponentDatabase::new(&db_path).await?;
    
    // Add some test components to the database
    add_test_components(&database).await?;
    
    // Load supplier configuration
    let supplier_config = SupplierConfig::load()?;
    
    // Skip test if no supplier APIs are configured
    if !supplier_config.has_digikey() && !supplier_config.has_nexar() {
        println!("⚠️  No supplier APIs configured, skipping synthesis test");
        println!("💡 Set DIGIKEY_CLIENT_ID/SECRET or NEXAR_CLIENT_ID/SECRET to run this test");
        return Ok(());
    }
    
    println!("🔧 Setting up supplier service...");
    
    // Configure multi-backend supplier service
    let mut backend_config = MultiBackendConfig::default();
    backend_config.nexar = supplier_config.to_nexar_config();
    backend_config.digikey = supplier_config.to_digikey_config();
    
    let supplier_service = MultiBackendSupplierService::new(
        backend_config, 
        cache_path.to_string_lossy().to_string()
    ).await?;
    
    // Initialize two-stage synthesizer
    let synthesis_config = TwoStageConfig {
        max_stage1_candidates: 50,
        max_stage2_candidates: 10,
        enable_supplier_lookup: true,
        supplier_cache_hours: 4,
    };
    
    let synthesizer = TwoStageSynthesizer::new(
        database.clone(),
        Box::new(supplier_service),
        synthesis_config,
    )?;
    
    println!("🎯 Testing component synthesis...");
    
    // Test resistor synthesis
    let resistor_requirements = ComponentRequirements {
        component_type: ComponentType::Resistor,
        properties: vec![
            ("resistance".to_string(), "10k".to_string()),
            ("tolerance".to_string(), "5%".to_string()),
            ("power_rating".to_string(), "0.25W".to_string()),
        ].into_iter().collect(),
        quantity: Some(QuantityRequirement::Exactly(100)),
        max_cost_per_unit: Some(0.10), // 10 cents max
        preferred_packages: vec!["0805".to_string(), "0603".to_string()],
        temperature_range: Some((-40.0, 85.0)),
        notes: Some("Standard 5% resistor for digital circuits".to_string()),
    };
    
    println!("🔍 Synthesizing 10kΩ resistors...");
    let synthesis_result = synthesizer.synthesize_component(&resistor_requirements).await?;
    
    println!("✅ Synthesis complete:");
    println!("   Stage 1 candidates: {}", synthesis_result.stage1_candidates_found);
    println!("   Stage 2 lookups: {}", synthesis_result.stage2_api_calls);
    println!("   Final options: {}", synthesis_result.component_options.len());
    
    // Verify we got results
    assert!(!synthesis_result.component_options.is_empty(), "No component options found");
    
    // Show top 3 options
    for (i, option) in synthesis_result.component_options.iter().take(3).enumerate() {
        println!("📦 Option {}: {}", i + 1, option.component.name);
        println!("   Match score: {:.2}", option.match_score);
        println!("   Estimated cost: ${:.4}", option.estimated_unit_cost);
        
        if let Some(supplier_choice) = &option.supplier_choice {
            println!("   Supplier: {} ({})", supplier_choice.supplier_name, supplier_choice.supplier_part_number);
            println!("   Stock: {}, MOQ: {}", supplier_choice.availability, supplier_choice.moq);
        }
        
        println!("   Reason: {}", option.selection_reason);
    }
    
    // Test capacitor synthesis
    println!("\n🔍 Synthesizing ceramic capacitors...");
    let capacitor_requirements = ComponentRequirements {
        component_type: ComponentType::Capacitor,
        properties: vec![
            ("capacitance".to_string(), "100nF".to_string()),
            ("voltage_rating".to_string(), "50V".to_string()),
            ("dielectric".to_string(), "X7R".to_string()),
        ].into_iter().collect(),
        quantity: Some(QuantityRequirement::Exactly(50)),
        max_cost_per_unit: Some(0.15),
        preferred_packages: vec!["0805".to_string()],
        temperature_range: Some((-55.0, 125.0)),
        notes: Some("Decoupling capacitor for analog circuits".to_string()),
    };
    
    let cap_result = synthesizer.synthesize_component(&capacitor_requirements).await?;
    
    println!("✅ Capacitor synthesis complete:");
    println!("   Final options: {}", cap_result.component_options.len());
    
    if !cap_result.component_options.is_empty() {
        let best_option = &cap_result.component_options[0];
        println!("📦 Best capacitor: {}", best_option.component.name);
        println!("   Score: {:.2}, Cost: ${:.4}", best_option.match_score, best_option.estimated_unit_cost);
    }
    
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
    for i in 0..20 {
        if cache.check_rate_limit("DigiKey").await? {
            allowed_requests += 1;
        }
        
        // Small delay to avoid overwhelming the test
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
    
    println!("📊 Rate limiting test: {}/20 requests allowed", allowed_requests);
    assert!(allowed_requests < 20, "Rate limiting should block some requests");
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
    
    // Search for similar resistors
    let similar_components = database.search_components("resistor 10k", 10).await?;
    println!("🔍 Found {} similar resistor components", similar_components.len());
    
    // Test component matching by properties
    let mut matching_components = Vec::new();
    for component in similar_components {
        if let Some(resistance) = component.properties.get("resistance") {
            if resistance.contains("10k") || resistance.contains("10000") {
                matching_components.push(component);
            }
        }
    }
    
    println!("✅ Found {} exact resistance matches", matching_components.len());
    
    // Show component alternatives with different packages
    let mut package_alternatives = std::collections::HashMap::new();
    for component in &matching_components {
        if let Some(package) = component.properties.get("package") {
            package_alternatives.entry(package.clone())
                .or_insert_with(Vec::new)
                .push(&component.name);
        }
    }
    
    println!("📦 Package alternatives:");
    for (package, components) in package_alternatives {
        println!("   {}: {} options", package, components.len());
    }
    
    Ok(())
}

// Helper functions

/// Find KiCad libraries on the system
fn find_kicad_libraries() -> Result<Vec<PathBuf>> {
    let mut libraries = Vec::new();
    
    // Common KiCad library locations
    let possible_paths = vec![
        "/usr/share/kicad/symbols",
        "/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols",
        "/opt/kicad/share/kicad/symbols",
        "C:\\Program Files\\KiCad\\share\\kicad\\symbols",
        // User libraries
        dirs::home_dir().map(|h| h.join("Documents/KiCad/symbols")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join("KiCad/symbols")).unwrap_or_default(),
    ];
    
    for path_str in possible_paths {
        let path = PathBuf::from(path_str);
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

/// Add basic test components to database
async fn add_test_components(database: &ComponentDatabase) -> Result<()> {
    use bhdl_components::types::{Component, ComponentType};
    
    // Add some basic resistors
    let resistors = vec![
        ("R_10k_0805", "10k", "0805", "5%", "0.125W"),
        ("R_10k_0603", "10k", "0603", "5%", "0.1W"),
        ("R_1k_0805", "1k", "0805", "5%", "0.125W"),
        ("R_100_0805", "100", "0805", "1%", "0.125W"),
    ];
    
    for (name, resistance, package, tolerance, power) in resistors {
        let mut properties = std::collections::HashMap::new();
        properties.insert("resistance".to_string(), resistance.to_string());
        properties.insert("package".to_string(), package.to_string());
        properties.insert("tolerance".to_string(), tolerance.to_string());
        properties.insert("power_rating".to_string(), power.to_string());
        
        let component = Component {
            id: 0, // Will be assigned by database
            name: name.to_string(),
            component_type: ComponentType::Resistor.to_string(),
            part_number: Some(format!("TEST_{}", name)),
            manufacturer: Some("Test Manufacturer".to_string()),
            description: Some(format!("{} resistor, {} package", resistance, package)),
            properties,
            package_info: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        database.insert_component(&component).await?;
    }
    
    // Add some capacitors
    let capacitors = vec![
        ("C_100nF_0805", "100nF", "0805", "50V", "X7R"),
        ("C_1uF_0805", "1uF", "0805", "25V", "X7R"),
        ("C_10uF_1206", "10uF", "1206", "16V", "X5R"),
    ];
    
    for (name, capacitance, package, voltage, dielectric) in capacitors {
        let mut properties = std::collections::HashMap::new();
        properties.insert("capacitance".to_string(), capacitance.to_string());
        properties.insert("package".to_string(), package.to_string());
        properties.insert("voltage_rating".to_string(), voltage.to_string());
        properties.insert("dielectric".to_string(), dielectric.to_string());
        
        let component = Component {
            id: 0,
            name: name.to_string(),
            component_type: ComponentType::Capacitor.to_string(),
            part_number: Some(format!("TEST_{}", name)),
            manufacturer: Some("Test Manufacturer".to_string()),
            description: Some(format!("{} capacitor, {} package", capacitance, package)),
            properties,
            package_info: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
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
        ("R_10k_0402", "10k", "0402", "1%", "0.063W"),
        ("R_10k_1206", "10k", "1206", "5%", "0.25W"),
        ("R_10k_2512", "10k", "2512", "1%", "1W"),
        ("R_10k_TH", "10k", "THT", "5%", "0.25W"), // Through-hole
    ];
    
    for (name, resistance, package, tolerance, power) in advanced_resistors {
        let mut properties = std::collections::HashMap::new();
        properties.insert("resistance".to_string(), resistance.to_string());
        properties.insert("package".to_string(), package.to_string());
        properties.insert("tolerance".to_string(), tolerance.to_string());
        properties.insert("power_rating".to_string(), power.to_string());
        
        let component = bhdl_components::types::Component {
            id: 0,
            name: name.to_string(),
            component_type: ComponentType::Resistor.to_string(),
            part_number: Some(format!("ADV_{}", name)),
            manufacturer: Some("Advanced Test Mfg".to_string()),
            description: Some(format!("{} precision resistor, {} package", resistance, package)),
            properties,
            package_info: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
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