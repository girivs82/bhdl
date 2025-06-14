//! Database migration system

use rusqlite::{Connection, Result as SqliteResult};
use super::schema::{SCHEMA_SQL, SCHEMA_VERSION, check_schema_version, set_schema_version};

/// Run all necessary migrations to bring database to current schema
pub fn run_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    let current_version = check_schema_version(conn)?;
    
    if current_version < SCHEMA_VERSION {
        println!("Upgrading database schema from version {} to {}", current_version, SCHEMA_VERSION);
        
        // Run migrations in order
        for version in (current_version + 1)..=SCHEMA_VERSION {
            run_migration(conn, version)?;
        }
        
        println!("Database schema upgrade complete");
    } else if current_version > SCHEMA_VERSION {
        return Err(anyhow::anyhow!(
            "Database schema version {} is newer than supported version {}. Please upgrade the application.",
            current_version, SCHEMA_VERSION
        ));
    }
    
    Ok(())
}

/// Run a specific migration
fn run_migration(conn: &mut Connection, version: i32) -> anyhow::Result<()> {
    let tx = conn.transaction()?;
    
    match version {
        1 => {
            // Initial schema creation
            println!("Creating initial database schema...");
            tx.execute_batch(SCHEMA_SQL)?;
            set_schema_version(&tx, 1)?;
        }
        2 => {
            // Update supplier_data table for multi-supplier support
            println!("Updating supplier_data table for multi-supplier support...");
            
            // Drop existing supplier_data table and recreate with new schema
            tx.execute("DROP TABLE IF EXISTS supplier_data", [])?;
            
            tx.execute(r#"
                CREATE TABLE supplier_data (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    component_id INTEGER NOT NULL REFERENCES components(id) ON DELETE CASCADE,
                    supplier_name TEXT NOT NULL,
                    supplier_part_number TEXT NOT NULL,
                    manufacturer_part_number TEXT NOT NULL,
                    manufacturer TEXT NOT NULL,
                    availability INTEGER NOT NULL DEFAULT 0,
                    lead_time_days INTEGER,
                    moq INTEGER NOT NULL DEFAULT 1,
                    price_breaks TEXT NOT NULL,
                    datasheet_url TEXT,
                    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
            "#, [])?;
            
            // Recreate supplier data indexes
            tx.execute("CREATE INDEX IF NOT EXISTS idx_supplier_data_component_id ON supplier_data(component_id)", [])?;
            tx.execute("CREATE INDEX IF NOT EXISTS idx_supplier_data_supplier ON supplier_data(supplier_name)", [])?;
            tx.execute("CREATE INDEX IF NOT EXISTS idx_supplier_data_mpn ON supplier_data(manufacturer_part_number)", [])?;
            
            set_schema_version(&tx, 2)?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown migration version: {}", version));
        }
    }
    
    tx.commit()?;
    println!("Migration to version {} completed", version);
    
    Ok(())
}

/// Validate database integrity after migrations
pub fn validate_schema(conn: &Connection) -> anyhow::Result<()> {
    // Check that all required tables exist
    let required_tables = [
        "components",
        "component_electrical_specs", 
        "component_pins",
        "component_symbols",
        "component_footprints",
        "footprint_pads",
        "supplier_data",
        "components_fts",
        "schema_version",
    ];
    
    for table in &required_tables {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1"
        )?;
        
        let exists = stmt.exists([table])?;
        if !exists {
            return Err(anyhow::anyhow!("Required table '{}' not found", table));
        }
    }
    
    // Check that indexes exist
    let required_indexes = [
        "idx_components_name",
        "idx_components_category",
        "idx_electrical_specs_lookup",
        "idx_supplier_data_component_id",
    ];
    
    for index in &required_indexes {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='index' AND name=?1"
        )?;
        
        let exists = stmt.exists([index])?;
        if !exists {
            return Err(anyhow::anyhow!("Required index '{}' not found", index));
        }
    }
    
    println!("Database schema validation passed");
    Ok(())
}

/// Reset database (drop all tables and recreate)
pub fn reset_database(conn: &mut Connection) -> anyhow::Result<()> {
    println!("Resetting database...");
    
    let tx = conn.transaction()?;
    
    // Drop all tables
    let tables = [
        "components_fts",
        "footprint_pads",
        "component_footprints", 
        "component_symbols",
        "component_pins",
        "component_electrical_specs",
        "supplier_data",
        "components",
        "schema_version",
    ];
    
    for table in &tables {
        tx.execute(&format!("DROP TABLE IF EXISTS {}", table), [])?;
    }
    
    // Drop indexes (they'll be recreated with tables)
    let indexes = [
        "idx_components_name",
        "idx_components_category", 
        "idx_components_manufacturer",
        "idx_components_part_number",
        "idx_electrical_specs_component_id",
        "idx_electrical_specs_lookup",
        "idx_electrical_specs_value_range",
        "idx_pins_component_id",
        "idx_pins_number",
        "idx_symbols_component_id",
        "idx_footprints_component_id",
        "idx_pads_footprint_id",
        "idx_supplier_data_component_id",
        "idx_supplier_data_supplier",
        "idx_supplier_data_mpn",
    ];
    
    for index in &indexes {
        tx.execute(&format!("DROP INDEX IF EXISTS {}", index), [])?;
    }
    
    tx.commit()?;
    
    // Run migrations to recreate everything
    run_migrations(conn)?;
    
    println!("Database reset complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_initial_migration() {
        let mut conn = Connection::open_in_memory().unwrap();
        
        // Should start with version 0
        assert_eq!(check_schema_version(&conn).unwrap(), 0);
        
        // Run migrations
        run_migrations(&mut conn).unwrap();
        
        // Should now be at current version
        assert_eq!(check_schema_version(&conn).unwrap(), SCHEMA_VERSION);
        
        // Validate schema
        validate_schema(&conn).unwrap();
    }
    
    #[test]
    fn test_reset_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        
        // Create initial schema
        run_migrations(&mut conn).unwrap();
        
        // Reset database
        reset_database(&mut conn).unwrap();
        
        // Should still be at current version after reset
        assert_eq!(check_schema_version(&conn).unwrap(), SCHEMA_VERSION);
        
        // Validate schema
        validate_schema(&conn).unwrap();
    }
}