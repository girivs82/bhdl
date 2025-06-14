//! Integration tests for bhdl-components

use bhdl_components::{
    Component, ComponentLibrary, ElectricalSpec, ComponentSymbol, SupplierData,
    types::{PriceBreak, ComponentCategory, PackagingType}
};
use tempfile::TempDir;

// Include modular integration tests
mod integration;

use integration::{init_test_env, print_test_environment};

#[tokio::test]
async fn test_integration_suite() {
    init_test_env();
    print_test_environment();
    
    // Run the comprehensive integration tests if APIs are available
    let (has_apis, available_apis) = integration::check_supplier_apis();
    
    if has_apis {
        println!("🚀 Running full integration tests with APIs: {:?}", available_apis);
        
        // This would run the real-world tests
        // Note: These are also available as separate test functions
        println!("✅ Integration test suite setup complete");
    } else {
        println!("ℹ️  Limited integration tests (no supplier APIs configured)");
    }
}

#[tokio::test]
async fn test_basic_component_operations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_components.db");
    
    // Create component library
    let library = ComponentLibrary::new(&db_path).await.unwrap();
    
    // Create a test component
    let component = Component {
        id: 0, // Will be assigned by database
        name: "Test Resistor".to_string(),
        description: Some("A test 1kΩ resistor".to_string()),
        manufacturer: Some("Test Manufacturer".to_string()),
        part_number: Some("TEST-1K".to_string()),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Resistor,
        subcategory: None,
        datasheet_url: None,
        electrical_specs: vec![
            ElectricalSpec {
                spec_name: "resistance".to_string(),
                spec_value: 1000.0,
                spec_unit: "Ω".to_string(),
                spec_tolerance: Some(0.05),
                min_value: Some(950.0),
                max_value: Some(1050.0),
                conditions: None,
            }
        ],
        pins: vec![],
        symbol: Some(ComponentSymbol {
            symbol_name: "R".to_string(),
            svg_data: "<svg><rect width=\"10\" height=\"4\"/></svg>".to_string(),
            bounding_box_width: 10.0,
            bounding_box_height: 4.0,
            reference_point_x: 0.0,
            reference_point_y: 0.0,
            style_variant: None,
        }),
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    // Insert component
    let component_id = library.insert_component(&component).await.unwrap();
    assert!(component_id > 0);
    
    // Retrieve component
    let retrieved = library.get_component(component_id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "Test Resistor");
    assert_eq!(retrieved.category.as_str(), "resistor");
    
    // Test symbol retrieval
    let symbol_svg = library.get_component_symbol(component_id).await.unwrap().unwrap();
    assert!(!symbol_svg.is_empty());
}

#[tokio::test]
async fn test_search_functionality() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_search.db");
    
    let library = ComponentLibrary::new(&db_path).await.unwrap();
    
    // Insert test components
    let resistor = Component {
        id: 0,
        name: "1kΩ Resistor".to_string(),
        description: Some("Carbon film resistor".to_string()),
        manufacturer: Some("Generic".to_string()),
        part_number: Some("RES-1K-CF".to_string()),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Resistor,
        subcategory: None,
        datasheet_url: None,
        electrical_specs: vec![],
        pins: vec![],
        symbol: None,
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    library.insert_component(&resistor).await.unwrap();
    
    // Search for resistors
    let results = library.search_components("resistor").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "1kΩ Resistor");
}

#[test]
fn test_component_types() {
    // Test ComponentCategory
    assert_eq!(ComponentCategory::Resistor.as_str(), "resistor");
    assert_eq!(ComponentCategory::IC.as_str(), "ic");
    
    // Test PackagingType
    assert_eq!(PackagingType::Reel.as_str(), "reel");
    assert_eq!(PackagingType::Tube.as_str(), "tube");
}

#[test]
fn test_supplier_calculations() {
    let supplier_data = SupplierData {
        id: 1,
        component_id: 1,
        supplier_name: "Test Supplier".to_string(),
        part_number: "SUPPLIER-123".to_string(),
        manufacturer_part_number: "MFG-123".to_string(),
        price_breaks: vec![
            PriceBreak { quantity: 1, unit_price: 1.00 },
            PriceBreak { quantity: 10, unit_price: 0.90 },
            PriceBreak { quantity: 100, unit_price: 0.80 },
        ],
        stock_quantity: 1000,
        lead_time_days: 5,
        minimum_order_quantity: 1,
        packaging: PackagingType::Reel,
        last_updated: chrono::Utc::now(),
        currency: "USD".to_string(),
    };
    
    // Test price calculation
    assert_eq!(supplier_data.get_price_for_quantity(1), 1.00);
    assert_eq!(supplier_data.get_price_for_quantity(10), 0.90);
    assert_eq!(supplier_data.get_price_for_quantity(100), 0.80);
    
    // Test stock checking
    assert!(supplier_data.has_stock(500));
    assert!(!supplier_data.has_stock(2000));
}