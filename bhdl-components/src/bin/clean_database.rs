//! Database cleanup tool to remove components without SVG data
//! 
//! This tool helps maintain database integrity by removing incomplete components

use std::path::Path;
use anyhow::{Result, Context};
use clap::{Arg, Command};
use log::{info, warn};
use rusqlite::Connection;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let matches = Command::new("clean_database")
        .about("Clean component database by removing components without SVG data")
        .arg(Arg::new("database")
             .help("Path to components database")
             .short('d')
             .long("database")
             .default_value("components.db"))
        .arg(Arg::new("dry-run")
             .help("Check without actually deleting")
             .long("dry-run")
             .action(clap::ArgAction::SetTrue))
        .get_matches();

    let database_path = matches.get_one::<String>("database").unwrap();
    let dry_run = matches.get_flag("dry-run");

    info!("🧹 Database Cleanup Tool");
    info!("Database: {}", database_path);
    if dry_run {
        info!("DRY RUN MODE - No actual deletions will be performed");
    }

    // Open database connection
    let conn = Connection::open(database_path)
        .context("Failed to open database")?;

    // Count components without SVG data
    let count_query = r#"
        SELECT COUNT(*) 
        FROM components c
        WHERE NOT EXISTS (
            SELECT 1 FROM component_symbols cs 
            WHERE cs.component_id = c.id 
            AND cs.svg_data IS NOT NULL 
            AND LENGTH(cs.svg_data) > 0
        )
    "#;
    
    let total_components: i64 = conn.query_row(
        "SELECT COUNT(*) FROM components",
        [],
        |row| row.get(0)
    )?;
    
    let components_without_svg: i64 = conn.query_row(
        count_query,
        [],
        |row| row.get(0)
    )?;
    
    info!("📊 Database Statistics:");
    info!("   Total components: {}", total_components);
    info!("   Components without SVG data: {}", components_without_svg);
    info!("   Components with SVG data: {}", total_components - components_without_svg);
    
    if components_without_svg == 0 {
        info!("✅ All components have SVG data! No cleanup needed.");
        return Ok(());
    }
    
    // Get list of components without SVG data
    let list_query = r#"
        SELECT c.id, c.name, c.manufacturer, c.part_number
        FROM components c
        WHERE NOT EXISTS (
            SELECT 1 FROM component_symbols cs 
            WHERE cs.component_id = c.id 
            AND cs.svg_data IS NOT NULL 
            AND LENGTH(cs.svg_data) > 0
        )
        ORDER BY c.name
        LIMIT 100
    "#;
    
    let mut stmt = conn.prepare(list_query)?;
    let components: Vec<(i64, String, Option<String>, Option<String>)> = stmt.query_map([], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?
        ))
    })?.collect::<Result<Vec<_>, _>>()?;
    
    info!("\n🗑️  Components to be removed:");
    for (id, name, manufacturer, part_number) in &components[..components.len().min(20)] {
        let mfr = manufacturer.as_deref().unwrap_or("N/A");
        let pn = part_number.as_deref().unwrap_or("N/A");
        info!("   ID: {}, Name: {}, Manufacturer: {}, Part#: {}", id, name, mfr, pn);
    }
    if components.len() > 20 {
        info!("   ... and {} more", components.len() - 20);
    }
    
    if dry_run {
        info!("\n📝 DRY RUN Complete - no changes made");
        return Ok(());
    }
    
    // Confirm deletion
    info!("\n⚠️  WARNING: This will delete {} components from the database!", components_without_svg);
    info!("Press ENTER to continue or Ctrl+C to cancel...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    
    // Delete components without SVG data
    let delete_query = r#"
        DELETE FROM components 
        WHERE id IN (
            SELECT c.id 
            FROM components c
            WHERE NOT EXISTS (
                SELECT 1 FROM component_symbols cs 
                WHERE cs.component_id = c.id 
                AND cs.svg_data IS NOT NULL 
                AND LENGTH(cs.svg_data) > 0
            )
        )
    "#;
    
    let deleted = conn.execute(delete_query, [])?;
    
    info!("✅ Successfully deleted {} components without SVG data", deleted);
    
    // Verify final state
    let final_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM components",
        [],
        |row| row.get(0)
    )?;
    
    info!("\n📊 Final Database State:");
    info!("   Total components: {}", final_count);
    
    Ok(())
}