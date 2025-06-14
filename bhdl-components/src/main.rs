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
    
    /// Supplier data management
    Supplier {
        #[command(subcommand)]
        action: SupplierCommands,
    },
    
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
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
        
        /// Enable live supplier data lookup (costs API calls)
        #[arg(long)]
        enable_supplier_lookup: bool,
        
        /// Maximum candidates to query suppliers for
        #[arg(long, default_value = "10")]
        max_supplier_queries: usize,
        
        /// Preferred supplier backend (nexar, digikey, auto)
        #[arg(long, default_value = "auto")]
        supplier_backend: String,
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

/// Configuration management
#[derive(Subcommand)]
enum ConfigCommands {
    /// Generate example configuration file
    Init,
    
    /// Show current configuration
    Show,
    
    /// Test API connections
    Test {
        /// Specific backend to test (nexar, digikey, all)
        #[arg(default_value = "all")]
        backend: String,
    },
}

/// Supplier data management
#[derive(Subcommand)]
enum SupplierCommands {
    /// Update supplier data for components
    Update {
        /// Component ID or part number
        component: String,
        
        /// Force update even if data is fresh
        #[arg(short, long)]
        force: bool,
    },
    
    /// Show supplier data for a component
    Show {
        /// Component ID
        component_id: u32,
    },
    
    /// Refresh all stale supplier data
    RefreshAll {
        /// Maximum age in hours before refresh
        #[arg(short, long, default_value = "24")]
        max_age_hours: i64,
    },
    
    /// Show supplier statistics
    Stats,
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
        
        Commands::Supplier { action } => {
            match action {
                SupplierCommands::Update { component, force } => {
                    update_supplier_data(&cli.database, &component, force).await?
                }
                SupplierCommands::Show { component_id } => {
                    show_supplier_data(&cli.database, &component_id).await?
                }
                SupplierCommands::RefreshAll { max_age_hours } => {
                    refresh_all_supplier_data(&cli.database, &max_age_hours).await?
                }
                SupplierCommands::Stats => {
                    show_supplier_stats(&cli.database).await?
                }
            }
        }
        
        Commands::Config { action } => {
            match action {
                ConfigCommands::Init => init_config().await?,
                ConfigCommands::Show => show_config().await?,
                ConfigCommands::Test { backend } => test_supplier_backends(&backend).await?,
            }
        }
        
        Commands::Synthesize { component_type, requirements, alternatives, enable_supplier_lookup, max_supplier_queries, supplier_backend } => {
            synthesize_component(&cli.database, &component_type, &requirements, alternatives, enable_supplier_lookup, max_supplier_queries, &supplier_backend).await?;
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
    alternatives: usize,
    enable_supplier_lookup: bool,
    max_supplier_queries: usize,
    supplier_backend: &str,
) -> Result<()> {
    use bhdl_components::{ComponentLibrary, synthesis::SynthesisEngine};
    use bhdl_components::types::ComponentRequirements;
    
    println!("{}", style(format!("🧪 Synthesizing {} component", component_type)).bold().blue());
    println!("📋 Requirements: {}", requirements);
    
    // Parse JSON requirements
    let parsed_requirements: ComponentRequirements = match serde_json::from_str(requirements) {
        Ok(req) => req,
        Err(e) => {
            // Try to create basic requirements for common component types
            match component_type.to_lowercase().as_str() {
                "resistor" => {
                    println!("📝 Using default resistor requirements (1kΩ, 0.25W, 5%)");
                    ComponentRequirements::resistor(1000.0, 0.25, 0.05, 100)
                }
                "capacitor" => {
                    println!("📝 Using default capacitor requirements (100nF, 50V, 20%)");
                    ComponentRequirements::capacitor(100e-9, 50.0, 0.20, 100)
                }
                "inductor" => {
                    println!("📝 Using default inductor requirements (1µH, 1A, 20%)");
                    ComponentRequirements::inductor(1e-6, 1.0, 0.20, 100)
                }
                _ => {
                    println!("❌ Failed to parse requirements JSON: {}", e);
                    println!("💡 For custom requirements, use JSON format like:");
                    println!("   {{\"resistance\": 1000.0, \"power_rating\": 0.25, \"tolerance\": 0.05, \"quantity\": 100}}");
                    return Ok(());
                }
            }
        }
    };

    // Create services
    let library = ComponentLibrary::new(db_path).await?;
    let synthesis_engine = SynthesisEngine::new();

    // Perform synthesis
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Synthesizing component...");

    let result = synthesis_engine.synthesize_component(component_type, &parsed_requirements, library.get_database()).await?;

    pb.finish_with_message("Synthesis complete");

    // Display results
    println!("\n🎯 Synthesis Results:");
    println!("📊 Confidence: {:.1}%", result.confidence * 100.0);
    println!("📝 Notes:");
    for note in &result.synthesis_notes {
        println!("  • {}", note);
    }

    if let Some(recommended) = &result.recommended {
        println!("\n✅ Recommended Component:");
        println!("  🏷️  {}", style(&recommended.component.name).cyan().bold());
        println!("  🆔 ID: {}", recommended.component.id);
        if let Some(manufacturer) = &recommended.component.manufacturer {
            println!("  🏭 Manufacturer: {}", manufacturer);
        }
        if let Some(part_number) = &recommended.component.part_number {
            println!("  🔢 Part Number: {}", part_number);
        }
        println!("  💰 Unit Price: ${:.4}", recommended.supplier_choice.unit_price);
        println!("  📦 Total Cost: ${:.2}", recommended.total_cost);
        println!("  📊 Stock: {}", recommended.supplier_choice.quantity_available);
        println!("  ⏱️  Lead Time: {} days", recommended.lead_time);
        println!("  🎯 Score: {:.2}", recommended.fitness_score);
        println!("  📝 Reason: {}", recommended.selection_reason);
    }

    if !result.alternatives.is_empty() {
        println!("\n🔄 Alternative Options:");
        for (i, alt) in result.alternatives.iter().enumerate().take(alternatives - 1) {
            println!("{}. {} (Score: {:.2}, ${:.4}/unit)", 
                    style(i + 2).bold(),
                    style(&alt.component.name).cyan(),
                    alt.fitness_score,
                    alt.supplier_choice.unit_price);
        }
    }

    if result.alternatives.is_empty() && result.recommended.is_none() {
        println!("\n❌ No suitable components found");
        println!("💡 Try adjusting your requirements or importing more components");
    }

    Ok(())
}

async fn update_supplier_data(db_path: &PathBuf, component: &str, force: bool) -> Result<()> {
    use bhdl_components::supplier::{SupplierService, trustedparts::TrustedPartsConfig};
    
    println!("{}", style(format!("🔄 Updating supplier data for '{}'", component)).bold().blue());
    
    // Create supplier service
    let config = TrustedPartsConfig::default();
    let supplier_service = SupplierService::new(db_path, config).await?;
    
    // Try to parse as component ID first, then search by name
    let library = ComponentLibrary::new(db_path).await?;
    
    if let Ok(component_id) = component.parse::<u32>() {
        // Component ID provided
        if let Some(comp) = library.get_component(component_id).await? {
            let part_number = comp.part_number.as_deref()
                .or(Some(&comp.name))
                .unwrap_or(component);
            
            if force || supplier_service.needs_refresh(component_id).await? {
                supplier_service.update_component_supplier_data(component_id, part_number).await?;
                println!("✅ Updated supplier data for component {}", component_id);
            } else {
                println!("ℹ️  Supplier data is up to date (use --force to refresh anyway)");
            }
        } else {
            println!("❌ Component ID {} not found", component_id);
        }
    } else {
        // Search by name/part number
        let search_results = library.search_components(component).await?;
        if let Some(comp) = search_results.first() {
            let part_number = comp.part_number.as_deref()
                .or(Some(&comp.name))
                .unwrap_or(component);
            
            if force || supplier_service.needs_refresh(comp.id).await? {
                supplier_service.update_component_supplier_data(comp.id, part_number).await?;
                println!("✅ Updated supplier data for component '{}' (ID: {})", comp.name, comp.id);
            } else {
                println!("ℹ️  Supplier data is up to date (use --force to refresh anyway)");
            }
        } else {
            println!("❌ Component '{}' not found", component);
        }
    }
    
    Ok(())
}

async fn show_supplier_data(db_path: &PathBuf, component_id: &u32) -> Result<()> {
    println!("{}", style(format!("📦 Supplier Data for Component {}", component_id)).bold().blue());
    
    let library = ComponentLibrary::new(db_path).await?;
    
    if let Some(supplier_data) = library.get_supplier_data(*component_id).await? {
        println!("🕐 Last Updated: {}", supplier_data.last_updated.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("🏪 {} supplier(s) found:\n", supplier_data.suppliers.len());
        
        for (i, supplier) in supplier_data.suppliers.iter().enumerate() {
            println!("{}. {}", style(i + 1).bold(), style(&supplier.supplier_name).cyan().bold());
            println!("   🏭 Manufacturer: {}", supplier.manufacturer);
            println!("   🔢 MPN: {}", supplier.manufacturer_part_number);
            println!("   📦 Stock: {}", supplier.availability);
            
            if let Some(lead_time) = supplier.lead_time_days {
                println!("   ⏱️  Lead Time: {} days", lead_time);
            }
            
            println!("   📊 MOQ: {}", supplier.moq);
            
            if !supplier.price_breaks.is_empty() {
                println!("   💰 Pricing:");
                for price_break in &supplier.price_breaks {
                    println!("      • {}+ units: {:.4} {}", 
                            price_break.quantity, 
                            price_break.unit_price, 
                            price_break.currency);
                }
            }
            
            if let Some(datasheet) = &supplier.datasheet_url {
                println!("   📋 Datasheet: {}", datasheet);
            }
            
            println!("   🕐 Updated: {}", supplier.last_updated.format("%Y-%m-%d %H:%M:%S UTC"));
            println!();
        }
    } else {
        println!("❌ No supplier data found for component {}", component_id);
        println!("💡 Use 'bhdl-components supplier update {}' to fetch supplier data", component_id);
    }
    
    Ok(())
}

async fn refresh_all_supplier_data(db_path: &PathBuf, max_age_hours: &i64) -> Result<()> {
    use bhdl_components::supplier::{SupplierService, trustedparts::TrustedPartsConfig};
    
    println!("{}", style("🔄 Refreshing Stale Supplier Data").bold().blue());
    println!("⏰ Max age: {} hours", max_age_hours);
    
    let config = TrustedPartsConfig::default();
    let mut supplier_service = SupplierService::new(db_path, config).await?;
    supplier_service.set_refresh_interval_hours(*max_age_hours);
    
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap());
    pb.set_message("Finding stale components...");
    
    let result = supplier_service.refresh_stale_data().await?;
    
    pb.finish_with_message("Refresh complete");
    
    println!("\n📊 Refresh Summary:");
    println!("  ✅ Successful: {}", style(result.successful_updates).green());
    println!("  ❌ Failed: {}", style(result.failed_updates).red());
    println!("  📈 Success Rate: {:.1}%", result.success_rate() * 100.0);
    
    if !result.errors.is_empty() {
        println!("\n⚠️  Errors:");
        for error in &result.errors {
            println!("  • {}", error);
        }
    }
    
    Ok(())
}

async fn show_supplier_stats(db_path: &PathBuf) -> Result<()> {
    use bhdl_components::supplier::{SupplierService, trustedparts::TrustedPartsConfig};
    
    println!("{}", style("📊 Supplier Data Statistics").bold().blue());
    
    let config = TrustedPartsConfig::default();
    let supplier_service = SupplierService::new(db_path, config).await?;
    let stats = supplier_service.get_supplier_stats().await?;
    
    println!("📁 Database: {}", db_path.display());
    println!("📦 Total Components: {}", style(stats.total_components).cyan());
    println!("🏪 With Supplier Data: {}", style(stats.components_with_supplier_data).green());
    println!("❌ Without Supplier Data: {}", style(stats.components_without_supplier_data).red());
    println!("⚠️  Stale Data: {}", style(stats.stale_components).yellow());
    println!("📈 Coverage: {:.1}%", stats.cache_coverage_percent);
    
    if stats.components_without_supplier_data > 0 {
        println!("\n💡 Tip: Use 'bhdl-components supplier refresh-all' to update stale data");
    }
    
    Ok(())
}


async fn init_config() -> Result<()> {
    use bhdl_components::config::SupplierConfig;
    
    println!("{}", style("🔧 Initializing BHDL Supplier Configuration").bold().blue());
    
    // Create example config
    SupplierConfig::create_example_config()?;
    
    println!("
📝 Example configuration file created: bhdl-supplier-config.example.toml");
    println!("
🚀 Next steps:");
    println!("  1. Copy the example file:");
    println!("     cp bhdl-supplier-config.example.toml bhdl-supplier-config.toml");
    println!("
  2. Edit the file with your API credentials:");
    println!("     - Nexar: Get free API access at https://nexar.com/api");
    println!("     - DigiKey: Register at https://developer.digikey.com");
    println!("
  3. Or set environment variables:");
    println!("     export NEXAR_CLIENT_ID='your_client_id'");
    println!("     export NEXAR_CLIENT_SECRET='your_client_secret'");
    println!("
  4. Test your configuration:");
    println!("     bhdl-components config test");
    
    Ok(())
}

async fn show_config() -> Result<()> {
    use bhdl_components::config::SupplierConfig;
    
    println!("{}", style("📋 Current BHDL Supplier Configuration").bold().blue());
    
    let config = SupplierConfig::load()?;
    
    println!("
🔌 Configured Backends:");
    
    if config.has_nexar() {
        println!("  ✅ Nexar API");
    } else {
        println!("  ❌ Nexar API (not configured)");
    }
    
    if config.has_digikey() {
        println!("  ✅ DigiKey API");
    } else {
        println!("  ❌ DigiKey API (not configured)");
    }
    
    println!("  ⚠️  TrustedParts API (requires business account)");
    
    println!("
⚙️  Settings:");
    println!("  Default backend: {}", config.default_backend);
    println!("  Max concurrent requests: 3");
    
    println!("
💡 To configure credentials:");
    println!("  - Run 'bhdl-components config init' to create a config file");
    println!("  - Or set environment variables (NEXAR_CLIENT_ID, etc.)");
    
    Ok(())
}

async fn test_supplier_backends(backend: &str) -> Result<()> {
    println!("{}", style("🧪 Testing Supplier Backend Connections").bold().blue());
    println!("🔍 Testing backend: {}", backend);
    
    // For now, just show that the feature is available
    // Full implementation would require the multi-backend service to be integrated
    println!("
⚠️  Backend testing feature coming soon!");
    println!("💡 Use environment variables to configure APIs:");
    println!("   export NEXAR_CLIENT_ID='your_client_id'");
    println!("   export NEXAR_CLIENT_SECRET='your_client_secret'");
    println!("   export DIGIKEY_CLIENT_ID='your_client_id'");
    println!("   export DIGIKEY_CLIENT_SECRET='your_client_secret'");
    
    Ok(())
}
