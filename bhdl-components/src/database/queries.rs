//! Database query implementations

use rusqlite::{Connection, Result as SqliteResult, params, Row};
use crate::types::*;
use super::ComponentStats;
use chrono::{DateTime, Utc};

/// Parse component category string from database
fn parse_component_category(category_str: String) -> ComponentCategory {
    match category_str.as_str() {
        "resistor" => ComponentCategory::Resistor,
        "capacitor" => ComponentCategory::Capacitor,
        "inductor" => ComponentCategory::Inductor,
        "diode" => ComponentCategory::Diode,
        "transistor" => ComponentCategory::Transistor,
        "ic" => ComponentCategory::IC,
        "connector" => ComponentCategory::Connector,
        "crystal" => ComponentCategory::Crystal,
        "led" => ComponentCategory::LED,
        "switch" => ComponentCategory::Switch,
        "relay" => ComponentCategory::Relay,
        "transformer" => ComponentCategory::Transformer,
        "fuse" => ComponentCategory::Fuse,
        other => ComponentCategory::Other(other.to_string()),
    }
}

/// Get component ID by name
pub fn get_component_id_by_name(conn: &Connection, name: &str) -> anyhow::Result<ComponentId> {
    let mut stmt = conn.prepare(
        "SELECT id FROM components WHERE name = ?1 LIMIT 1"
    )?;

    match stmt.query_row([name], |row| Ok(row.get::<_, ComponentId>(0)?)) {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(anyhow::anyhow!("Component '{}' not found in database", name))
        }
        Err(e) => Err(e.into()),
    }
}

/// Get component by ID
pub fn get_component_by_id(conn: &Connection, id: ComponentId) -> anyhow::Result<Option<Component>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, manufacturer, part_number, package_type, 
                category, subcategory, datasheet_url, created_at, updated_at
         FROM components WHERE id = ?1"
    )?;
    
    let component_result = stmt.query_row([id], |row| {
        Ok(Component {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            manufacturer: row.get(3)?,
            part_number: row.get(4)?,
            package_type: row.get(5)?,
            category: parse_category(row.get::<_, String>(6)?),
            subcategory: row.get(7)?,
            datasheet_url: row.get(8)?,
            electrical_specs: vec![], // Will be loaded separately
            pins: vec![], // Will be loaded separately
            symbol: None, // Will be loaded separately
            footprint: None, // Will be loaded separately
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                .map_err(|e| rusqlite::Error::InvalidColumnType(9, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                .map_err(|e| rusqlite::Error::InvalidColumnType(10, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
        })
    });
    
    match component_result {
        Ok(mut component) => {
            // Load electrical specs
            component.electrical_specs = get_electrical_specs(conn, id)?;
            
            // Load pins
            component.pins = get_component_pins(conn, id)?;
            
            // Load symbol
            component.symbol = get_component_symbol(conn, id)?;
            
            // Load footprint
            component.footprint = get_component_footprint(conn, id)?;
            
            Ok(Some(component))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Search components using full-text search
pub fn search_components(conn: &Connection, query: &str) -> anyhow::Result<Vec<Component>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM components c 
         JOIN components_fts fts ON c.id = fts.rowid 
         WHERE components_fts MATCH ?1 
         ORDER BY bm25(components_fts) 
         LIMIT 100"
    )?;
    
    let component_ids: Vec<ComponentId> = stmt.query_map([query], |row| {
        Ok(row.get(0)?)
    })?.collect::<Result<Vec<_>, _>>()?;
    
    let mut components = Vec::new();
    for id in component_ids {
        if let Some(component) = get_component_by_id(conn, id)? {
            components.push(component);
        }
    }
    
    Ok(components)
}

/// Get component symbol SVG
pub fn get_symbol_svg(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT svg_data FROM component_symbols WHERE component_id = ?1 LIMIT 1"
    )?;
    
    match stmt.query_row([component_id], |row| Ok(row.get::<_, String>(0)?)) {
        Ok(svg) => Ok(Some(svg)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Insert a new component
pub fn insert_component(conn: &Connection, component: &Component) -> anyhow::Result<ComponentId> {
    let mut stmt = conn.prepare(
        "INSERT INTO components (name, description, manufacturer, part_number, package_type, 
                                category, subcategory, datasheet_url, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    )?;
    
    stmt.execute(params![
        component.name,
        component.description,
        component.manufacturer,
        component.part_number,
        component.package_type,
        component.category.as_str(),
        component.subcategory,
        component.datasheet_url,
        component.created_at.to_rfc3339(),
        component.updated_at.to_rfc3339(),
    ])?;
    
    let component_id = conn.last_insert_rowid() as ComponentId;
    
    // Insert electrical specs
    for spec in &component.electrical_specs {
        insert_electrical_spec(conn, component_id, spec)?;
    }
    
    // Insert pins
    for pin in &component.pins {
        insert_component_pin(conn, component_id, pin)?;
    }
    
    // Insert symbol if present
    if let Some(symbol) = &component.symbol {
        insert_component_symbol(conn, component_id, symbol)?;
    }
    
    // Insert footprint if present
    if let Some(footprint) = &component.footprint {
        insert_component_footprint(conn, component_id, footprint)?;
    }
    
    Ok(component_id)
}

/// Update component
pub fn update_component(conn: &Connection, component: &Component) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "UPDATE components SET name = ?1, description = ?2, manufacturer = ?3, 
                              part_number = ?4, package_type = ?5, category = ?6, 
                              subcategory = ?7, datasheet_url = ?8, updated_at = ?9
         WHERE id = ?10"
    )?;
    
    stmt.execute(params![
        component.name,
        component.description,
        component.manufacturer,
        component.part_number,
        component.package_type,
        component.category.as_str(),
        component.subcategory,
        component.datasheet_url,
        chrono::Utc::now().to_rfc3339(),
        component.id,
    ])?;
    
    // TODO: Update related tables (specs, pins, symbol, footprint)
    // For now, we'll keep it simple and only update the main component record
    
    Ok(())
}

/// Delete component
pub fn delete_component(conn: &Connection, id: ComponentId) -> anyhow::Result<()> {
    // Foreign key constraints will handle cascading deletes
    let mut stmt = conn.prepare("DELETE FROM components WHERE id = ?1")?;
    stmt.execute([id])?;
    Ok(())
}

/// Get supplier data for component
pub fn get_supplier_data(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Option<SupplierData>> {
    let mut stmt = conn.prepare(
        "SELECT component_id, supplier_name, supplier_part_number, manufacturer_part_number,
                manufacturer, availability, lead_time_days, moq, price_breaks, datasheet_url, last_updated
         FROM supplier_data WHERE component_id = ?1"
    )?;
    
    let supplier_infos: Result<Vec<SupplierInfo>, _> = stmt.query_map([component_id], |row| {
        let price_breaks_json: String = row.get(8)?;
        let price_breaks: Vec<PriceBreak> = serde_json::from_str(&price_breaks_json)
            .map_err(|_e| rusqlite::Error::InvalidColumnType(8, "price_breaks".to_string(), rusqlite::types::Type::Text))?;
        
        Ok(SupplierInfo {
            supplier_name: row.get(1)?,
            supplier_part_number: row.get(2)?,
            manufacturer_part_number: row.get(3)?,
            manufacturer: row.get(4)?,
            availability: row.get(5)?,
            lead_time_days: row.get(6)?,
            moq: row.get(7)?,
            price_breaks,
            datasheet_url: row.get(9)?,
            last_updated: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                .map_err(|_e| rusqlite::Error::InvalidColumnType(10, "last_updated".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
        })
    })?.collect();
    
    match supplier_infos {
        Ok(suppliers) => {
            if suppliers.is_empty() {
                Ok(None)
            } else {
                Ok(Some(SupplierData {
                    component_id,
                    suppliers,
                    last_updated: chrono::Utc::now(),
                }))
            }
        }
        Err(e) => Err(anyhow::anyhow!("Failed to load supplier data: {}", e)),
    }
}

/// Insert or update supplier data
pub fn upsert_supplier_data(conn: &Connection, supplier_data: &SupplierData) -> anyhow::Result<()> {
    // Start transaction
    let tx = conn.unchecked_transaction()?;
    
    // Delete existing supplier data for this component
    tx.execute(
        "DELETE FROM supplier_data WHERE component_id = ?1",
        [supplier_data.component_id]
    )?;
    
    // Insert new supplier data
    for supplier in &supplier_data.suppliers {
        let price_breaks_json = serde_json::to_string(&supplier.price_breaks)?;
        
        tx.execute(
            "INSERT INTO supplier_data 
             (component_id, supplier_name, supplier_part_number, manufacturer_part_number,
              manufacturer, availability, lead_time_days, moq, price_breaks, datasheet_url, last_updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                supplier_data.component_id,
                supplier.supplier_name,
                supplier.supplier_part_number,
                supplier.manufacturer_part_number,
                supplier.manufacturer,
                supplier.availability,
                supplier.lead_time_days,
                supplier.moq,
                price_breaks_json,
                supplier.datasheet_url,
                supplier.last_updated.to_rfc3339()
            ]
        )?;
    }
    
    // Commit transaction
    tx.commit()?;
    Ok(())
}

/// Find components by electrical specifications
pub fn find_components_by_specs(
    conn: &Connection,
    category: &ComponentCategory,
    specs: &[(String, f64, f64)], // (spec_name, min_value, max_value)
) -> anyhow::Result<Vec<Component>> {
    if specs.is_empty() {
        return get_components_by_category(conn, category);
    }
    
    // Build dynamic query for electrical specs
    let mut query = String::from(
        "SELECT DISTINCT c.id FROM components c 
         WHERE c.category = ?1"
    );
    
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(category.as_str().to_string())];
    
    for (i, (spec_name, _, _)) in specs.iter().enumerate() {
        query.push_str(&format!(
            " AND EXISTS (
                SELECT 1 FROM component_electrical_specs es 
                WHERE es.component_id = c.id 
                AND es.spec_name = ?{} 
                AND es.spec_value >= ?{} 
                AND es.spec_value <= ?{}
            )",
            2 + i * 3,
            3 + i * 3,
            4 + i * 3
        ));
        
        params.push(Box::new(spec_name.clone()));
        params.push(Box::new(specs[i].1));
        params.push(Box::new(specs[i].2));
    }
    
    let mut stmt = conn.prepare(&query)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    
    let component_ids: Vec<ComponentId> = stmt.query_map(&param_refs[..], |row| {
        Ok(row.get(0)?)
    })?.collect::<Result<Vec<_>, _>>()?;
    
    let mut components = Vec::new();
    for id in component_ids {
        if let Some(component) = get_component_by_id(conn, id)? {
            components.push(component);
        }
    }
    
    Ok(components)
}

/// Get all components of a specific category
pub fn get_components_by_category(conn: &Connection, category: &ComponentCategory) -> anyhow::Result<Vec<Component>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM components WHERE category = ?1 ORDER BY name LIMIT 1000"
    )?;
    
    let component_ids: Vec<ComponentId> = stmt.query_map([category.as_str()], |row| {
        Ok(row.get(0)?)
    })?.collect::<Result<Vec<_>, _>>()?;
    
    let mut components = Vec::new();
    for id in component_ids {
        if let Some(component) = get_component_by_id(conn, id)? {
            components.push(component);
        }
    }
    
    Ok(components)
}

/// Get component statistics
pub fn get_component_stats(conn: &Connection) -> anyhow::Result<ComponentStats> {
    // Total components
    let total_components: u32 = conn.query_row(
        "SELECT COUNT(*) FROM components",
        [],
        |row| Ok(row.get(0)?)
    )?;
    
    // Components with symbols
    let components_with_symbols: u32 = conn.query_row(
        "SELECT COUNT(DISTINCT component_id) FROM component_symbols",
        [],
        |row| Ok(row.get(0)?)
    )?;
    
    // Components with supplier data
    let components_with_supplier_data: u32 = conn.query_row(
        "SELECT COUNT(DISTINCT component_id) FROM supplier_data",
        [],
        |row| Ok(row.get(0)?)
    )?;
    
    // Category breakdown
    let mut stmt = conn.prepare(
        "SELECT category, COUNT(*) FROM components GROUP BY category"
    )?;
    
    let categories = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    })?.collect::<Result<std::collections::HashMap<String, u32>, _>>()?;
    
    Ok(ComponentStats {
        total_components,
        components_with_symbols,
        components_with_supplier_data,
        categories,
    })
}

// Helper functions

fn get_electrical_specs(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Vec<ElectricalSpec>> {
    let mut stmt = conn.prepare(
        "SELECT spec_name, spec_value, spec_unit, spec_tolerance, min_value, max_value, conditions
         FROM component_electrical_specs WHERE component_id = ?1"
    )?;
    
    let specs = stmt.query_map([component_id], |row| {
        Ok(ElectricalSpec {
            spec_name: row.get(0)?,
            spec_value: row.get(1)?,
            spec_unit: row.get(2)?,
            spec_tolerance: row.get(3)?,
            min_value: row.get(4)?,
            max_value: row.get(5)?,
            conditions: row.get(6)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(specs)
}

fn get_component_pins(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Vec<PinDefinition>> {
    let mut stmt = conn.prepare(
        "SELECT pin_number, pin_name, electrical_type, x_position, y_position, 
                orientation, length, pin_shape
         FROM component_pins WHERE component_id = ?1"
    )?;
    
    let pins = stmt.query_map([component_id], |row| {
        Ok(PinDefinition {
            pin_number: row.get(0)?,
            pin_name: row.get(1)?,
            electrical_type: parse_pin_type(row.get::<_, String>(2)?),
            x_position: row.get(3)?,
            y_position: row.get(4)?,
            orientation: row.get(5)?,
            length: row.get(6)?,
            pin_shape: parse_pin_shape(row.get::<_, String>(7)?),
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(pins)
}

fn get_component_symbol(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Option<ComponentSymbol>> {
    let mut stmt = conn.prepare(
        "SELECT symbol_name, svg_data, bounding_box_width, bounding_box_height,
                reference_point_x, reference_point_y, style_variant
         FROM component_symbols WHERE component_id = ?1 LIMIT 1"
    )?;
    
    match stmt.query_row([component_id], |row| {
        Ok(ComponentSymbol {
            symbol_name: row.get(0)?,
            svg_data: row.get(1)?,
            bounding_box_width: row.get(2)?,
            bounding_box_height: row.get(3)?,
            reference_point_x: row.get(4)?,
            reference_point_y: row.get(5)?,
            style_variant: row.get(6)?,
        })
    }) {
        Ok(symbol) => Ok(Some(symbol)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_component_footprint(conn: &Connection, component_id: ComponentId) -> anyhow::Result<Option<ComponentFootprint>> {
    // Get footprint metadata
    let mut stmt = conn.prepare(
        "SELECT id, footprint_name, svg_data, pad_count, body_width, body_height, pitch
         FROM component_footprints WHERE component_id = ?1 LIMIT 1"
    )?;

    let footprint_result = stmt.query_row([component_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,  // footprint_id
            row.get::<_, String>(1)?,  // footprint_name
            row.get::<_, String>(2)?,  // svg_data
            row.get::<_, u32>(3)?,  // pad_count
            row.get::<_, f64>(4)?,  // body_width
            row.get::<_, f64>(5)?,  // body_height
            row.get::<_, Option<f64>>(6)?,  // pitch
        ))
    });

    match footprint_result {
        Ok((footprint_id, footprint_name, svg_data, pad_count, body_width, body_height, pitch)) => {
            // Load pads for this footprint
            let pads = get_footprint_pads(conn, footprint_id)?;

            Ok(Some(ComponentFootprint {
                footprint_name,
                svg_data,
                pad_count,
                body_width,
                body_height,
                pitch,
                pads,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_footprint_pads(conn: &Connection, footprint_id: i64) -> anyhow::Result<Vec<FootprintPad>> {
    let mut stmt = conn.prepare(
        "SELECT pad_number, x_position, y_position, width, height, shape, drill_diameter, pad_type
         FROM footprint_pads WHERE footprint_id = ?1"
    )?;

    let pads = stmt.query_map([footprint_id], |row| {
        Ok(FootprintPad {
            pad_number: row.get(0)?,
            x_position: row.get(1)?,
            y_position: row.get(2)?,
            width: row.get(3)?,
            height: row.get(4)?,
            shape: parse_pad_shape(row.get::<_, String>(5)?),
            drill_diameter: row.get(6)?,
            pad_type: parse_pad_type(row.get::<_, String>(7)?),
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(pads)
}

fn insert_electrical_spec(conn: &Connection, component_id: ComponentId, spec: &ElectricalSpec) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO component_electrical_specs 
         (component_id, spec_name, spec_value, spec_unit, spec_tolerance, min_value, max_value, conditions)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    
    stmt.execute(params![
        component_id,
        spec.spec_name,
        spec.spec_value,
        spec.spec_unit,
        spec.spec_tolerance,
        spec.min_value,
        spec.max_value,
        spec.conditions,
    ])?;
    
    Ok(())
}

fn insert_component_pin(conn: &Connection, component_id: ComponentId, pin: &PinDefinition) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO component_pins 
         (component_id, pin_number, pin_name, electrical_type, x_position, y_position, 
          orientation, length, pin_shape)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;
    
    stmt.execute(params![
        component_id,
        pin.pin_number,
        pin.pin_name,
        pin_type_to_string(&pin.electrical_type),
        pin.x_position,
        pin.y_position,
        pin.orientation,
        pin.length,
        pin_shape_to_string(&pin.pin_shape),
    ])?;
    
    Ok(())
}

fn insert_component_symbol(conn: &Connection, component_id: ComponentId, symbol: &ComponentSymbol) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO component_symbols 
         (component_id, symbol_name, svg_data, bounding_box_width, bounding_box_height,
          reference_point_x, reference_point_y, style_variant)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )?;
    
    stmt.execute(params![
        component_id,
        symbol.symbol_name,
        symbol.svg_data,
        symbol.bounding_box_width,
        symbol.bounding_box_height,
        symbol.reference_point_x,
        symbol.reference_point_y,
        symbol.style_variant,
    ])?;
    
    Ok(())
}

pub fn insert_component_footprint(conn: &Connection, component_id: ComponentId, footprint: &ComponentFootprint) -> anyhow::Result<()> {
    // Insert footprint metadata
    let mut stmt = conn.prepare(
        "INSERT INTO component_footprints
         (component_id, footprint_name, svg_data, pad_count, body_width, body_height, pitch)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )?;

    stmt.execute(params![
        component_id,
        footprint.footprint_name,
        footprint.svg_data,
        footprint.pad_count,
        footprint.body_width,
        footprint.body_height,
        footprint.pitch,
    ])?;

    let footprint_id = conn.last_insert_rowid();

    // Insert pads
    for pad in &footprint.pads {
        insert_footprint_pad(conn, footprint_id, pad)?;
    }

    Ok(())
}

fn insert_footprint_pad(conn: &Connection, footprint_id: i64, pad: &FootprintPad) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO footprint_pads
         (footprint_id, pad_number, x_position, y_position, width, height, shape, drill_diameter, pad_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;

    stmt.execute(params![
        footprint_id,
        pad.pad_number,
        pad.x_position,
        pad.y_position,
        pad.width,
        pad.height,
        pad_shape_to_string(&pad.shape),
        pad.drill_diameter,
        pad_type_to_string(&pad.pad_type),
    ])?;

    Ok(())
}

// String parsing helpers

fn parse_category(s: String) -> ComponentCategory {
    match s.as_str() {
        "resistor" => ComponentCategory::Resistor,
        "capacitor" => ComponentCategory::Capacitor,
        "inductor" => ComponentCategory::Inductor,
        "diode" => ComponentCategory::Diode,
        "transistor" => ComponentCategory::Transistor,
        "ic" => ComponentCategory::IC,
        "connector" => ComponentCategory::Connector,
        "crystal" => ComponentCategory::Crystal,
        "led" => ComponentCategory::LED,
        "switch" => ComponentCategory::Switch,
        "relay" => ComponentCategory::Relay,
        "transformer" => ComponentCategory::Transformer,
        "fuse" => ComponentCategory::Fuse,
        other => ComponentCategory::Other(other.to_string()),
    }
}

fn parse_packaging_type(s: String) -> PackagingType {
    match s.as_str() {
        "reel" => PackagingType::Reel,
        "tube" => PackagingType::Tube,
        "tray" => PackagingType::Tray,
        "bulk" => PackagingType::Bulk,
        "cut_tape" => PackagingType::CutTape,
        other => PackagingType::Other(other.to_string()),
    }
}

fn parse_pin_type(s: String) -> PinType {
    match s.as_str() {
        "input" => PinType::Input,
        "output" => PinType::Output,
        "bidirectional" => PinType::Bidirectional,
        "power" => PinType::Power,
        "ground" => PinType::Ground,
        "passive" => PinType::Passive,
        "not_connected" => PinType::NotConnected,
        _ => PinType::Unspecified,
    }
}

fn parse_pin_shape(s: String) -> PinShape {
    match s.as_str() {
        "line" => PinShape::Line,
        "inverted" => PinShape::Inverted,
        "clock" => PinShape::Clock,
        "inverted_clock" => PinShape::InvertedClock,
        "input_low" => PinShape::InputLow,
        "clock_low" => PinShape::ClockLow,
        "output_low" => PinShape::OutputLow,
        "edge_clock_high" => PinShape::EdgeClockHigh,
        "non_logic" => PinShape::NonLogic,
        _ => PinShape::Line,
    }
}

fn pin_type_to_string(pin_type: &PinType) -> &'static str {
    match pin_type {
        PinType::Input => "input",
        PinType::Output => "output",
        PinType::Bidirectional => "bidirectional",
        PinType::Power => "power",
        PinType::Ground => "ground",
        PinType::Passive => "passive",
        PinType::NotConnected => "not_connected",
        PinType::Unspecified => "unspecified",
    }
}

fn pin_shape_to_string(pin_shape: &PinShape) -> &'static str {
    match pin_shape {
        PinShape::Line => "line",
        PinShape::Inverted => "inverted",
        PinShape::Clock => "clock",
        PinShape::InvertedClock => "inverted_clock",
        PinShape::InputLow => "input_low",
        PinShape::ClockLow => "clock_low",
        PinShape::OutputLow => "output_low",
        PinShape::EdgeClockHigh => "edge_clock_high",
        PinShape::NonLogic => "non_logic",
    }
}

fn parse_pad_shape(s: String) -> PadShape {
    match s.as_str() {
        "circle" => PadShape::Circle,
        "rectangle" | "rect" => PadShape::Rectangle,
        "oval" => PadShape::Oval,
        "rounded_rectangle" | "roundrect" => PadShape::RoundedRectangle,
        _ => PadShape::Rectangle,
    }
}

fn pad_shape_to_string(shape: &PadShape) -> &'static str {
    match shape {
        PadShape::Circle => "circle",
        PadShape::Rectangle => "rectangle",
        PadShape::Oval => "oval",
        PadShape::RoundedRectangle => "rounded_rectangle",
    }
}

fn parse_pad_type(s: String) -> PadType {
    match s.as_str() {
        "smd" => PadType::SMD,
        "through_hole" | "thru_hole" => PadType::ThroughHole,
        "npth" => PadType::NPTH,
        _ => PadType::SMD,
    }
}

fn pad_type_to_string(pad_type: &PadType) -> &'static str {
    match pad_type {
        PadType::SMD => "smd",
        PadType::ThroughHole => "through_hole",
        PadType::NPTH => "npth",
    }
}

// Supplier data helper functions

/// Count components with supplier data
pub fn count_components_with_supplier_data(conn: &Connection) -> anyhow::Result<u32> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT component_id) FROM supplier_data",
        [],
        |row| row.get(0)
    )?;
    Ok(count as u32)
}

/// Find components with stale supplier data
pub fn find_components_with_stale_supplier_data(
    conn: &Connection, 
    cutoff_time: DateTime<Utc>
) -> anyhow::Result<Vec<ComponentId>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT component_id FROM supplier_data 
         WHERE datetime(last_updated) < datetime(?1)"
    )?;
    
    let component_ids: Result<Vec<ComponentId>, _> = stmt.query_map(
        [cutoff_time.to_rfc3339()],
        |row| Ok(row.get(0)?)
    )?.collect();
    
    Ok(component_ids?)
}

/// Count total components
pub fn count_components(conn: &Connection) -> anyhow::Result<u32> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM components",
        [],
        |row| row.get(0)
    )?;
    Ok(count as u32)
}

/// Get all components from the database
pub fn get_all_components(conn: &Connection) -> anyhow::Result<Vec<Component>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, description, manufacturer, part_number, package_type, 
                category, subcategory, datasheet_url, created_at, updated_at
         FROM components
         ORDER BY name"
    )?;
    
    let components = stmt.query_map([], |row| {
        Ok(Component {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            manufacturer: row.get(3)?,
            part_number: row.get(4)?,
            package_type: row.get(5)?,
            category: parse_component_category(row.get(6)?),
            subcategory: row.get(7)?,
            datasheet_url: row.get(8)?,
            electrical_specs: Vec::new(), // Would need join query for specs
            pins: Vec::new(), // Would need join query for pins
            symbol: None, // Would need join query for symbol
            footprint: None, // Would need join query for footprint
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(9, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(10, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(components)
}

/// Advanced search with custom WHERE clause and parameters
pub fn search_components_advanced(conn: &Connection, where_clause: &str, params: &[String]) -> anyhow::Result<Vec<Component>> {
    let query = format!(
        "SELECT id, name, description, manufacturer, part_number, package_type, 
                category, subcategory, datasheet_url, created_at, updated_at
         FROM components
         WHERE {}
         ORDER BY name",
        where_clause
    );
    
    let mut stmt = conn.prepare(&query)?;
    
    // Convert string parameters to rusqlite::types::Value for binding
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    
    let components = stmt.query_map(&param_refs[..], |row| {
        Ok(Component {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            manufacturer: row.get(3)?,
            part_number: row.get(4)?,
            package_type: row.get(5)?,
            category: parse_component_category(row.get(6)?),
            subcategory: row.get(7)?,
            datasheet_url: row.get(8)?,
            electrical_specs: Vec::new(), // Would need join query for specs
            pins: Vec::new(), // Would need join query for pins
            symbol: None, // Would need join query for symbol
            footprint: None, // Would need join query for footprint
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(9, "created_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
                .map_err(|_| rusqlite::Error::InvalidColumnType(10, "updated_at".to_string(), rusqlite::types::Type::Text))?
                .with_timezone(&chrono::Utc),
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    
    Ok(components)
}