//! Database schema definitions

/// SQL schema for the component database
pub const SCHEMA_SQL: &str = r#"
-- Core component information
CREATE TABLE IF NOT EXISTS components (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    manufacturer TEXT,
    part_number TEXT,
    package_type TEXT,
    category TEXT NOT NULL,
    subcategory TEXT,
    datasheet_url TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Electrical specifications
CREATE TABLE IF NOT EXISTS component_electrical_specs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    spec_name TEXT NOT NULL,
    spec_value REAL NOT NULL,
    spec_unit TEXT NOT NULL,
    spec_tolerance REAL,
    min_value REAL,
    max_value REAL,
    conditions TEXT
);

-- Pin/pad definitions with electrical properties
CREATE TABLE IF NOT EXISTS component_pins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    pin_number TEXT NOT NULL,
    pin_name TEXT,
    electrical_type TEXT NOT NULL,
    x_position REAL NOT NULL,
    y_position REAL NOT NULL,
    orientation INTEGER NOT NULL DEFAULT 0,
    length REAL NOT NULL DEFAULT 2.54,
    pin_shape TEXT NOT NULL DEFAULT 'line'
);

-- SVG symbol data (pre-rendered)
CREATE TABLE IF NOT EXISTS component_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    symbol_name TEXT NOT NULL,
    svg_data TEXT NOT NULL,
    bounding_box_width REAL NOT NULL,
    bounding_box_height REAL NOT NULL,
    reference_point_x REAL NOT NULL DEFAULT 0,
    reference_point_y REAL NOT NULL DEFAULT 0,
    style_variant TEXT
);

-- Physical footprint data
CREATE TABLE IF NOT EXISTS component_footprints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    footprint_name TEXT NOT NULL,
    svg_data TEXT NOT NULL,
    pad_count INTEGER NOT NULL,
    body_width REAL NOT NULL,
    body_height REAL NOT NULL,
    pitch REAL
);

-- Footprint pads
CREATE TABLE IF NOT EXISTS footprint_pads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    footprint_id INTEGER NOT NULL REFERENCES component_footprints(id) ON DELETE CASCADE,
    pad_number TEXT NOT NULL,
    x_position REAL NOT NULL,
    y_position REAL NOT NULL,
    width REAL NOT NULL,
    height REAL NOT NULL,
    shape TEXT NOT NULL,
    drill_diameter REAL,
    pad_type TEXT NOT NULL
);

-- Supply chain data (updated for multi-supplier support)
CREATE TABLE IF NOT EXISTS supplier_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
    supplier_name TEXT NOT NULL,
    supplier_part_number TEXT NOT NULL,
    manufacturer_part_number TEXT NOT NULL,
    manufacturer TEXT NOT NULL,
    availability INTEGER NOT NULL DEFAULT 0,
    lead_time_days INTEGER, -- Can be NULL if unknown
    moq INTEGER NOT NULL DEFAULT 1, -- Minimum Order Quantity
    price_breaks TEXT NOT NULL, -- JSON array of price breaks with currency
    datasheet_url TEXT,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_components_name ON components(name);
CREATE INDEX IF NOT EXISTS idx_components_category ON components(category);
CREATE INDEX IF NOT EXISTS idx_components_manufacturer ON components(manufacturer);
CREATE INDEX IF NOT EXISTS idx_components_part_number ON components(part_number);

CREATE INDEX IF NOT EXISTS idx_electrical_specs_component_id ON component_electrical_specs(component_id);
CREATE INDEX IF NOT EXISTS idx_electrical_specs_lookup ON component_electrical_specs(component_id, spec_name);
CREATE INDEX IF NOT EXISTS idx_electrical_specs_value_range ON component_electrical_specs(spec_name, spec_value);

CREATE INDEX IF NOT EXISTS idx_pins_component_id ON component_pins(component_id);
CREATE INDEX IF NOT EXISTS idx_pins_number ON component_pins(component_id, pin_number);

CREATE INDEX IF NOT EXISTS idx_symbols_component_id ON component_symbols(component_id);
CREATE INDEX IF NOT EXISTS idx_footprints_component_id ON component_footprints(component_id);
CREATE INDEX IF NOT EXISTS idx_pads_footprint_id ON footprint_pads(footprint_id);

CREATE INDEX IF NOT EXISTS idx_supplier_data_component_id ON supplier_data(component_id);
CREATE INDEX IF NOT EXISTS idx_supplier_data_supplier ON supplier_data(supplier_name);
CREATE INDEX IF NOT EXISTS idx_supplier_data_mpn ON supplier_data(manufacturer_part_number);

-- Full-text search for components
CREATE VIRTUAL TABLE IF NOT EXISTS components_fts USING fts5(
    name, description, manufacturer, part_number, category, subcategory,
    content='components', content_rowid='id'
);

-- Triggers to keep FTS table in sync
CREATE TRIGGER IF NOT EXISTS components_fts_insert AFTER INSERT ON components BEGIN
    INSERT INTO components_fts(rowid, name, description, manufacturer, part_number, category, subcategory)
    VALUES (new.id, new.name, new.description, new.manufacturer, new.part_number, new.category, new.subcategory);
END;

CREATE TRIGGER IF NOT EXISTS components_fts_delete AFTER DELETE ON components BEGIN
    INSERT INTO components_fts(components_fts, rowid, name, description, manufacturer, part_number, category, subcategory)
    VALUES ('delete', old.id, old.name, old.description, old.manufacturer, old.part_number, old.category, old.subcategory);
END;

CREATE TRIGGER IF NOT EXISTS components_fts_update AFTER UPDATE ON components BEGIN
    INSERT INTO components_fts(components_fts, rowid, name, description, manufacturer, part_number, category, subcategory)
    VALUES ('delete', old.id, old.name, old.description, old.manufacturer, old.part_number, old.category, old.subcategory);
    INSERT INTO components_fts(rowid, name, description, manufacturer, part_number, category, subcategory)
    VALUES (new.id, new.name, new.description, new.manufacturer, new.part_number, new.category, new.subcategory);
END;
"#;

/// Database version for migration tracking
pub const SCHEMA_VERSION: i32 = 2;

/// Check if database exists and has correct schema
pub fn check_schema_version(conn: &rusqlite::Connection) -> rusqlite::Result<i32> {
    // Check if schema_version table exists
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'"
    )?;
    
    let table_exists = stmt.exists([])?;
    
    if !table_exists {
        return Ok(0); // No schema version table means version 0
    }
    
    // Get current schema version
    let version: i32 = conn.query_row(
        "SELECT version FROM schema_version ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0)
    ).unwrap_or(0);
    
    Ok(version)
}

/// Set schema version
pub fn set_schema_version(conn: &rusqlite::Connection, version: i32) -> rusqlite::Result<()> {
    // Create schema_version table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            version INTEGER NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    
    // Insert new version
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [version],
    )?;
    
    Ok(())
}