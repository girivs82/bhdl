//! BHDL Component Library CLI
//! 
//! Command-line interface for managing component libraries, importing KiCad symbols,
//! and working with supplier data for electronic component management.

use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use log::{info, warn, error};

use bhdl_components::{
    ComponentLibrary, 
    kicad::{parser::KiCadSymbolParser, svg_converter::KiCadSvgConverter, extractor::KiCadExtractor}
};

/// BHDL Component Library Management CLI
#[derive(Parser)]
#[command(name = "bhdl-components")]
#[command(about = "Manage electronic component libraries for BHDL")]
#[command(version)]
struct Cli {
    /// Database file path
    #[arg(short, long, default_value = "components.db")]
    database: PathBuf,
    
    /// Verbose output
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import KiCad symbol libraries
    Import {
        /// KiCad symbol library file (.kicad_sym)
        #[arg(short, long)]
        file: PathBuf,
        
        /// Force reimport even if components exist
        #[arg(long)]
        force: bool,
    },
    
    /// Search components in the database
    Search {
        /// Search query
        query: String,
        
        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
        
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Show component details
    Show {
        /// Component ID or name
        component: String,
        
        /// Show SVG symbol
        #[arg(short, long)]
        svg: bool,
    },
    
    /// List components by category
    List {
        /// Component category (resistor, capacitor, ic, etc.)
        #[arg(short, long)]
        category: Option<String>,
        
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    
    /// Database management
    Database {
        #[command(subcommand)]
        action: DatabaseCommands,
    },
    
    /// Synthesis and part selection
    Synthesize {
        /// Component type (resistor, capacitor, etc.)
        component_type: String,
        
        /// Requirements in JSON format
        #[arg(short, long)]
        requirements: String,
        
        /// Number of alternatives to show
        #[arg(short, long, default_value = "5")]
        alternatives: usize,
    },
}

#[derive(Subcommand)]
enum DatabaseCommands {
    /// Show database statistics
    Stats,
    
    /// Initialize/reset database
    Init {
        /// Force reset existing database
        #[arg(short, long)]
        force: bool,
    },
    
    /// Vacuum database to reclaim space
    Vacuum,
    
    /// Export components to JSON
    Export {
        /// Output file
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info", 
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();
    
    info!("BHDL Component Library CLI starting");
    info!("Database: {}", cli.database.display());
    
    match cli.command {
        Commands::Import { file, force } => {
            import_kicad_library(&cli.database, &file, force).await?;
        }
        
        Commands::Search { query, limit, detailed } => {
            search_components(&cli.database, &query, limit, detailed).await?;
        }
        
        Commands::Show { component, svg } => {
            show_component(&cli.database, &component, svg).await?;
        }
        
        Commands::List { category, limit } => {
            list_components(&cli.database, category.as_deref(), limit).await?;
        }
        
        Commands::Database { action } => {
            match action {
                DatabaseCommands::Stats => show_database_stats(&cli.database).await?,
                DatabaseCommands::Init { force } => init_database(&cli.database, force).await?,
                DatabaseCommands::Vacuum => vacuum_database(&cli.database).await?,
                DatabaseCommands::Export { output } => export_database(&cli.database, &output).await?,
            }
        }
        
        Commands::Synthesize { component_type, requirements, alternatives } => {
            synthesize_component(&cli.database, &component_type, &requirements, alternatives).await?;
        }
    }
    
    Ok(())
}

async fn import_kicad_library(db_path: &PathBuf, file_path: &PathBuf, force: bool) -> Result<()> {
    println!("{}",  style("🔧 Importing KiCad Symbol Library").bold().blue());
    println!("📁 File: {}", file_path.display());
    
    // Check if file exists
    if !file_path.exists() {
        error!("KiCad library file not found: {}", file_path.display());
        return Err(anyhow::anyhow!("File not found"));
    }
    
    // Read the file
    let content = std::fs::read_to_string(file_path)?;
    println!("📖 Read {} characters from library file", content.len());
    
    // Parse KiCad symbols
    let parser = KiCadSymbolParser::new();
    let symbols = parser.parse_symbol_library(&content)?;
    println!("✅ Parsed {} symbols", symbols.len());
    
    if symbols.is_empty() {
        warn!("No symbols found in library file");
        return Ok(());
    }
    
    // Create progress bar
    let pb = ProgressBar::new(symbols.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
        .unwrap()
        .progress_chars("#>-"));
    
    // Initialize components
    let library = ComponentLibrary::new(db_path).await?;
    let svg_converter = KiCadSvgConverter::new();
    let extractor = KiCadExtractor::new();
    
    let mut imported = 0;
    let mut skipped = 0;
    let mut errors = 0;
    
    for (i, symbol) in symbols.iter().enumerate() {
        pb.set_position(i as u64);
        pb.set_message(format!("Processing {}", symbol.name));
        
        // Check if component already exists (unless force)
        if !force {
            let search_results = library.search_components(&symbol.name).await?;
            if !search_results.is_empty() {
                skipped += 1;
                continue;
            }
        }
        
        match process_symbol(&library, &svg_converter, &extractor, symbol).await {
            Ok(_) => {
                imported += 1;
                info!("Imported symbol: {}", symbol.name);
            }
            Err(e) => {
                errors += 1;
                error!("Failed to import symbol '{}': {}", symbol.name, e);
            }
        }
    }
    
    pb.finish_with_message("Import complete");
    
    println!("\n📊 Import Summary:");
    println!("  ✅ Imported: {}", style(imported).green());
    if skipped > 0 {
        println!("  ⏭️  Skipped: {} (already exist)", style(skipped).yellow());
    }
    if errors > 0 {
        println!("  ❌ Errors: {}", style(errors).red());
    }
    
    if imported > 0 {
        println!("\n🔍 Run 'bhdl-components search <query>' to find imported components");
    }
    
    Ok(())
}

async fn process_symbol(
    library: &ComponentLibrary,
    svg_converter: &KiCadSvgConverter,
    extractor: &KiCadExtractor,
    symbol: &bhdl_components::kicad::parser::KiCadSymbol,
) -> Result<()> {
    // Convert to SVG
    let svg_data = svg_converter.convert_symbol_to_svg(symbol)?;
    
    // Extract component data
    let component = extractor.extract_component(symbol, svg_data)?;
    
    // Insert into database
    library.insert_component(&component).await?;
    
    Ok(())
}

async fn search_components(db_path: &PathBuf, query: &str, limit: usize, detailed: bool) -> Result<()> {
    println!("{}", style(format!("🔍 Searching for '{}'", query)).bold().blue());
    
    let library = ComponentLibrary::new(db_path).await?;
    let results = library.search_components(query).await?;
    
    if results.is_empty() {
        println!("❌ No components found matching '{}'", query);
        return Ok(());
    }
    
    let display_count = results.len().min(limit);
    println!("✅ Found {} component(s), showing {}", results.len(), display_count);
    println!();
    
    for (i, component) in results.iter().take(limit).enumerate() {
        println!("{}. {} {}", 
                style(i + 1).bold(),
                style(&component.name).cyan().bold(),
                style(format!("(ID: {})", component.id)).dim());
        
        if let Some(description) = &component.description {
            println!("   📝 {}", description);
        }
        
        println!("   📂 Category: {:?}", component.category);
        
        if !component.electrical_specs.is_empty() && detailed {
            println!("   ⚡ Specifications:");
            for spec in &component.electrical_specs {
                println!("      • {}: {:.3} {}", spec.spec_name, spec.spec_value, spec.spec_unit);
            }
        }
        
        if !component.pins.is_empty() {
            println!("   📌 Pins: {}", component.pins.len());
        }
        
        if let Some(package) = &component.package_type {
            println!("   📦 Package: {}", package);
        }
        
        println!();
    }
    
    if results.len() > limit {
        println!("... and {} more results", results.len() - limit);
        println!("Use --limit {} to see more results", results.len());
    }
    
    Ok(())
}

async fn show_component(db_path: &PathBuf, component_id: &str, show_svg: bool) -> Result<()> {
    let library = ComponentLibrary::new(db_path).await?;
    
    // Try to parse as ID first, then search by name
    let component = if let Ok(id) = component_id.parse::<u32>() {
        library.get_component(id).await?
    } else {
        // Search by name
        let results = library.search_components(component_id).await?;
        results.into_iter().find(|c| c.name == component_id)
    };
    
    match component {
        Some(comp) => {
            println!("{}", style(format!("📦 Component: {}", comp.name)).bold().cyan());
            println!("🆔 ID: {}", comp.id);
            
            if let Some(desc) = &comp.description {
                println!("📝 Description: {}", desc);
            }
            
            println!("📂 Category: {:?}", comp.category);
            
            if let Some(manufacturer) = &comp.manufacturer {
                println!("🏭 Manufacturer: {}", manufacturer);
            }
            
            if let Some(part_number) = &comp.part_number {
                println!("🔢 Part Number: {}", part_number);
            }
            
            if let Some(package) = &comp.package_type {
                println!("📦 Package: {}", package);
            }
            
            if !comp.electrical_specs.is_empty() {
                println!("\n⚡ Electrical Specifications:");
                for spec in &comp.electrical_specs {
                    let tolerance = spec.spec_tolerance
                        .map(|t| format!(" (±{:.1}%)", t * 100.0))
                        .unwrap_or_default();
                    println!("  • {}: {:.6} {}{}", 
                            spec.spec_name, spec.spec_value, spec.spec_unit, tolerance);
                }
            }
            
            if !comp.pins.is_empty() {
                println!("\n📌 Pins ({}):", comp.pins.len());
                for pin in &comp.pins {
                    let name = pin.pin_name.as_deref().unwrap_or("~");
                    println!("  • Pin {}: {} ({:?})", pin.pin_number, name, pin.electrical_type);
                }
            }
            
            if show_svg {
                if let Some(svg_data) = library.get_component_symbol(comp.id).await? {
                    println!("\n🎨 SVG Symbol ({} characters):", svg_data.len());
                    println!("{}", svg_data);
                } else {
                    println!("\n❌ No SVG symbol available");
                }
            }
        }
        None => {
            println!("❌ Component not found: {}", component_id);
        }
    }
    
    Ok(())
}

async fn list_components(db_path: &PathBuf, category: Option<&str>, limit: usize) -> Result<()> {
    let library = ComponentLibrary::new(db_path).await?;
    
    match category {
        Some(cat) => {
            println!("{}", style(format!("📋 Components in category: {}", cat)).bold().blue());
            // TODO: Implement category filtering
            println!("⚠️  Category filtering not yet implemented");
        }
        None => {
            println!("{}", style("📋 Recent Components").bold().blue());
            // Get all components (TODO: implement proper listing)
            let results = library.search_components("").await?;
            
            if results.is_empty() {
                println!("❌ No components in database");
                return Ok(());
            }
            
            let display_count = results.len().min(limit);
            println!("📊 Found {} component(s), showing {}", results.len(), display_count);
            println!();
            
            for (i, component) in results.iter().take(limit).enumerate() {
                println!("{}. {} {} {}",
                        style(i + 1).bold(),
                        style(&component.name).cyan(),
                        style(format!("({:?})", component.category)).dim(),
                        style(format!("ID: {}", component.id)).dim());
            }
            
            if results.len() > limit {
                println!("\n... and {} more components", results.len() - limit);
            }
        }
    }
    
    Ok(())
}

async fn show_database_stats(db_path: &PathBuf) -> Result<()> {
    println!("{}", style("📊 Database Statistics").bold().blue());
    
    let library = ComponentLibrary::new(db_path).await?;
    let stats = library.get_stats().await?;
    let cache_stats = library.get_cache_stats();
    
    println!("📁 Database: {}", db_path.display());
    println!("📦 Total Components: {}", style(stats.total_components).cyan());
    println!("🎨 Components with Symbols: {}", style(stats.components_with_symbols).cyan());
    println!("🏪 Components with Supplier Data: {}", style(stats.components_with_supplier_data).cyan());
    
    if !stats.categories.is_empty() {
        println!("\n📂 Categories:");
        for (category, count) in &stats.categories {
            println!("  • {}: {}", category, style(count).cyan());
        }
    }
    
    println!("\n💾 Cache Performance:");
    println!("  • Component hit rate: {:.1}%", cache_stats.component_hit_rate() * 100.0);
    println!("  • Symbol hit rate: {:.1}%", cache_stats.symbol_hit_rate() * 100.0);
    println!("  • Search hit rate: {:.1}%", cache_stats.search_hit_rate() * 100.0);
    
    Ok(())
}

async fn init_database(db_path: &PathBuf, force: bool) -> Result<()> {
    if db_path.exists() && !force {
        println!("❌ Database already exists at {}", db_path.display());
        println!("Use --force to reset the database");
        return Ok(());
    }
    
    if force && db_path.exists() {
        std::fs::remove_file(db_path)?;
        println!("🗑️  Removed existing database");
    }
    
    println!("{}", style("🏗️  Initializing database...").bold().blue());
    
    let _library = ComponentLibrary::new(db_path).await?;
    
    println!("✅ Database initialized at {}", db_path.display());
    println!("📝 Use 'bhdl-components import --file <path>' to add components");
    
    Ok(())
}

async fn vacuum_database(db_path: &PathBuf) -> Result<()> {
    println!("{}", style("🧹 Vacuuming database...").bold().blue());
    
    // TODO: Implement database vacuum
    println!("⚠️  Database vacuum not yet implemented");
    
    Ok(())
}

async fn export_database(db_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    println!("{}", style(format!("📤 Exporting database to {}", output_path.display())).bold().blue());
    
    // TODO: Implement database export
    println!("⚠️  Database export not yet implemented");
    
    Ok(())
}

async fn synthesize_component(
    db_path: &PathBuf, 
    component_type: &str, 
    requirements: &str, 
    alternatives: usize
) -> Result<()> {
    println!("{}", style(format!("🧪 Synthesizing {} component", component_type)).bold().blue());
    println!("📋 Requirements: {}", requirements);
    
    // TODO: Implement component synthesis
    println!("⚠️  Component synthesis not yet implemented");
    println!("🔮 Coming in Phase 3.0.4!");
    
    Ok(())
}
