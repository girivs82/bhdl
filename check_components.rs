use bhdl_components::database::ComponentDatabase;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = Path::new("components.db");
    let db = ComponentDatabase::open(db_path).await?;
    
    println!("Components in database:");
    println!("=====================");
    
    let components = db.search_components("", None, None).await?;
    
    for component in components {
        println!("- {} (Category: {})", component.component_name, component.category);
    }
    
    println!("\nTotal: {} components", components.len());
    
    Ok(())
}