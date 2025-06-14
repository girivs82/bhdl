//! Basic functionality integration tests

use bhdl_components::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_component_database_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_components.db");
    
    // Create component library
    let library = ComponentLibrary::new(&db_path).await.unwrap();
    
    // Verify database was created
    assert!(db_path.exists());
    
    // Test basic search (should be empty initially)
    let results = library.search_components("resistor").await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_component_insertion_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_components.db");
    
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
                x_position: 5.08,
                y_position: 0.0,
                orientation: 180,
                length: 2.54,
                pin_shape: PinShape::Line,
            }
        ],
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
    let component_id = library.database.insert_component(&component).await.unwrap();
    assert!(component_id > 0);
    
    // Retrieve component
    let retrieved = library.get_component(component_id).await.unwrap().unwrap();
    assert_eq!(retrieved.name, "Test Resistor");
    assert_eq!(retrieved.category.as_str(), "resistor");
    assert_eq!(retrieved.electrical_specs.len(), 1);
    assert_eq!(retrieved.pins.len(), 2);
    assert!(retrieved.symbol.is_some());
    
    // Test symbol retrieval
    let symbol_svg = library.get_component_symbol(component_id).await.unwrap().unwrap();
    assert!(!symbol_svg.is_empty());
}

#[tokio::test]
async fn test_component_search() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_components.db");
    
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
    
    let capacitor = Component {
        id: 0,
        name: "100nF Capacitor".to_string(),
        description: Some("Ceramic capacitor".to_string()),
        manufacturer: Some("Generic".to_string()),
        part_number: Some("CAP-100N-CER".to_string()),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Capacitor,
        subcategory: None,
        datasheet_url: None,
        electrical_specs: vec![],
        pins: vec![],
        symbol: None,
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    library.database.insert_component(&resistor).await.unwrap();
    library.database.insert_component(&capacitor).await.unwrap();
    
    // Search for resistors
    let resistor_results = library.search_components("resistor").await.unwrap();
    assert_eq!(resistor_results.len(), 1);
    assert_eq!(resistor_results[0].name, "1kΩ Resistor");
    
    // Search for capacitors
    let capacitor_results = library.search_components("capacitor").await.unwrap();
    assert_eq!(capacitor_results.len(), 1);
    assert_eq!(capacitor_results[0].name, "100nF Capacitor");
    
    // Search by manufacturer
    let generic_results = library.search_components("Generic").await.unwrap();
    assert_eq!(generic_results.len(), 2); // Should find both components
}

#[tokio::test]
async fn test_component_cache() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_components.db");
    
    let library = ComponentLibrary::new(&db_path).await.unwrap();
    
    // Create and insert a test component
    let component = Component {
        id: 0,
        name: "Cache Test Component".to_string(),
        description: Some("Component for cache testing".to_string()),
        manufacturer: Some("Test Mfg".to_string()),
        part_number: Some("CACHE-TEST".to_string()),
        package_type: Some("TEST".to_string()),
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
    
    let component_id = library.database.insert_component(&component).await.unwrap();
    
    // First access (should miss cache, hit database)
    let _result1 = library.get_component(component_id).await.unwrap();
    
    // Second access (should hit cache)
    let _result2 = library.get_component(component_id).await.unwrap();
    
    // Verify cache statistics
    let stats = library.cache.get_stats();
    assert!(stats.component_hits > 0);
    assert!(stats.component_misses > 0);
    assert!(stats.component_hit_rate() > 0.0);
}

#[test]
fn test_component_types() {
    // Test ComponentCategory
    assert_eq!(ComponentCategory::Resistor.as_str(), "resistor");
    assert_eq!(ComponentCategory::IC.as_str(), "ic");
    
    // Test PackagingType
    assert_eq!(PackagingType::Reel.as_str(), "reel");
    assert_eq!(PackagingType::Tube.as_str(), "tube");
    
    // Test PinType variants
    assert!(matches!(PinType::Power, PinType::Power));
    assert!(matches!(PinType::Ground, PinType::Ground));
    assert!(matches!(PinType::Passive, PinType::Passive));
}

#[test]
fn test_supplier_choice_calculation() {
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
    
    // Test price calculation for different quantities
    assert_eq!(supplier_data.get_price_for_quantity(1), 1.00);
    assert_eq!(supplier_data.get_price_for_quantity(10), 0.90);
    assert_eq!(supplier_data.get_price_for_quantity(100), 0.80);
    assert_eq!(supplier_data.get_price_for_quantity(50), 0.90); // Should use 10+ price
    
    // Test stock checking
    assert!(supplier_data.has_stock(500));
    assert!(!supplier_data.has_stock(2000));
    
    // Test supplier choice creation
    let choice = SupplierChoice::new(supplier_data, 50);
    assert_eq!(choice.unit_price, 0.90);
    assert_eq!(choice.total_price, 45.0); // 50 * 0.90
    assert_eq!(choice.quantity_available, 50);
    assert!(choice.score > 0.0);
}