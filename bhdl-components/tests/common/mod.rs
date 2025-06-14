//! Common test utilities

use bhdl_components::*;

/// Create a test resistor component
pub fn create_test_resistor(id: ComponentId, resistance: f64, name: &str) -> Component {
    Component {
        id,
        name: name.to_string(),
        description: Some(format!("Test {}Ω resistor", resistance)),
        manufacturer: Some("Test Manufacturer".to_string()),
        part_number: Some(format!("RES-{}", resistance as u32)),
        package_type: Some("0805".to_string()),
        category: ComponentCategory::Resistor,
        subcategory: None,
        datasheet_url: None,
        electrical_specs: vec![
            ElectricalSpec {
                spec_name: "resistance".to_string(),
                spec_value: resistance,
                spec_unit: "Ω".to_string(),
                spec_tolerance: Some(0.05),
                min_value: Some(resistance * 0.95),
                max_value: Some(resistance * 1.05),
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
            svg_data: format!("<svg><rect width=\"10\" height=\"4\"/><text>{}</text></svg>", name),
            bounding_box_width: 10.0,
            bounding_box_height: 4.0,
            reference_point_x: 0.0,
            reference_point_y: 0.0,
            style_variant: None,
        }),
        footprint: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Create test supplier data
pub fn create_test_supplier_data(component_id: ComponentId, supplier_name: &str) -> SupplierData {
    SupplierData {
        id: 0, // Will be assigned by database
        component_id,
        supplier_name: supplier_name.to_string(),
        part_number: format!("{}-PART", supplier_name.to_uppercase()),
        manufacturer_part_number: format!("MFG-{}", component_id),
        price_breaks: vec![
            PriceBreak { quantity: 1, unit_price: 1.00 },
            PriceBreak { quantity: 10, unit_price: 0.90 },
            PriceBreak { quantity: 100, unit_price: 0.80 },
        ],
        stock_quantity: 1000,
        lead_time_days: 7,
        minimum_order_quantity: 1,
        packaging: PackagingType::Reel,
        last_updated: chrono::Utc::now(),
        currency: "USD".to_string(),
    }
}