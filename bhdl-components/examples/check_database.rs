//! Check database contents without resetting

use bhdl_components::ComponentDatabase;
use std::path::Path;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("📊 Checking Component Database");
    println!("==============================");
    
    let db_path = Path::new("components.db");
    if !db_path.exists() {
        println!("❌ Database not found at: {}", db_path.display());
        return Ok(());
    }
    
    // Open existing database without resetting
    let db = ComponentDatabase::new(db_path).await?;
    
    // Get all components
    let components = db.search_components("").await?;
    
    println!("\n📦 Total components: {}", components.len());
    
    // Count by category
    let mut category_counts = std::collections::HashMap::new();
    for component in &components {
        *category_counts.entry(format!("{:?}", component.category)).or_insert(0) += 1;
    }
    
    println!("\n📋 Components by category:");
    for (category, count) in &category_counts {
        println!("  - {}: {}", category, count);
    }
    
    // Show specific components needed for 7805 circuit
    println!("\n🔍 Components for 7805 circuit:");
    let needed_components = ["R", "C", "LED", "Fuse", "L7805", "LM7805", "TestPoint", "D_TVS"];
    
    for name in &needed_components {
        let found = db.search_components(name).await?;
        if !found.is_empty() {
            println!("  ✅ {} - Found {} variant(s)", name, found.len());
            for comp in found.iter().take(3) {
                println!("     - {} ({:?})", comp.name, comp.category);
            }
        } else {
            println!("  ❌ {} - Not found", name);
        }
    }
    
    Ok(())
}