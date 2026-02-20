//! BHDL Command Line Interface
//! 
//! This is the main entry point for the BHDL toolchain, providing commands for:
//! - Parsing and validating BHDL files
//! - Analyzing circuits for errors and warnings
//! - Synthesizing netlists
//! - Generating visualizations
//! - Running SPICE analysis
//! - Component role detection

use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use colored::*;
use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use log::info;

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, HasName};
use bhdl_analyzer::{analyze, documentation::{generate_documentation, DocumentationOptions, OutputFormat}};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_visualizer::{render_circuit_with_analysis};
use bhdl_spice::{ComponentRoleDetector, NetlistToSpiceConverter, SpiceAnalysisAugmenter};
use bhdl_common::AnalysisData;
use bhdl_testbench::{TestbenchRunner, WaveformFormat};

#[derive(Parser)]
#[command(name = "bhdl")]
#[command(author, version, about = "BHDL - Board Hardware Description Language toolchain", long_about = None)]
struct Cli {
    /// Input BHDL file
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and check syntax
    Parse {
        /// Output format (ast, pretty, json)
        #[arg(short, long, default_value = "pretty")]
        format: String,
    },
    
    /// Analyze circuit for errors and warnings
    Analyze {
        /// Show all diagnostics including hints
        #[arg(long)]
        all: bool,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Show intent analysis and flow tracking
        #[arg(long)]
        show_intents: bool,
    },
    
    /// Synthesize netlist
    Synthesize {
        /// Output netlist file
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Netlist format (json, spice)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
    
    /// Generate circuit visualization
    Visualize {
        /// Output SVG file
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Layout algorithm (semantic, force, analytical)
        #[arg(short, long, default_value = "semantic")]
        layout: String,
        
        /// Show component values
        #[arg(long)]
        show_values: bool,
    },
    
    /// Run SPICE analysis
    Spice {
        /// Analysis type (dc, ac, transient, roles)
        #[arg(short, long, default_value = "roles")]
        analysis: String,
        
        /// Output SPICE netlist
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Use pin metadata for role detection
        #[arg(long)]
        use_metadata: bool,
    },
    
    /// Run complete pipeline (parse -> analyze -> synthesize -> visualize)
    Pipeline {
        /// Output directory for all artifacts
        #[arg(short, long, default_value = "./output")]
        output_dir: PathBuf,
        
        /// Skip visualization
        #[arg(long)]
        no_viz: bool,
        
        /// Skip SPICE analysis
        #[arg(long)]
        no_spice: bool,
    },
    
    /// Run simulation with testbench
    Simulate {
        /// Testbench file
        #[arg(short, long)]
        testbench: PathBuf,

        /// Output directory for simulation results
        #[arg(short, long, default_value = "./sim_results")]
        output: PathBuf,

        /// Waveform format (vcd, csv, json)
        #[arg(short, long, default_value = "vcd")]
        format: String,

        /// Show real-time progress
        #[arg(long)]
        verbose: bool,
    },

    /// Analyze design intent and flow tracking
    Intents {
        /// Show synthesis hints for each flow
        #[arg(long)]
        show_hints: bool,

        /// Show validation rules for each flow
        #[arg(long)]
        show_rules: bool,

        /// Filter by intent name
        #[arg(short, long)]
        filter: Option<String>,

        /// Output format (text, json)
        #[arg(short = 'o', long, default_value = "text")]
        format: String,
    },

    /// Generate power domain documentation
    Doc {
        /// Output file path
        #[arg(short, long, default_value = "power_domains.md")]
        output: PathBuf,

        /// Generate only Bill of Materials
        #[arg(long)]
        bom_only: bool,

        /// Generate only power budget analysis
        #[arg(long)]
        budget_only: bool,

        /// Disable power tree visualization
        #[arg(long)]
        no_tree: bool,

        /// Disable pattern detection
        #[arg(long)]
        no_patterns: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    if cli.verbose {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Info)
            .init();
    }
    
    // Read input file
    let input_content = fs::read_to_string(&cli.input)
        .with_context(|| format!("Failed to read file: {}", cli.input.display()))?;
    
    // Always start with parsing
    let parse_result = parse(&input_content);
    
    if !parse_result.errors().is_empty() {
        eprintln!("{}", "Parse errors:".red().bold());
        for error in parse_result.errors() {
            eprintln!("  {} {}", "•".red(), error.message);
        }
        std::process::exit(1);
    }
    
    let root = parse_result.syntax();
    let source_file = SourceFile::cast(root.clone())
        .context("Failed to cast to SourceFile")?;
    
    // Handle commands
    match cli.command {
        None => {
            // Default: run analysis
            run_analysis(&source_file, false, "text").await?;
        }
        
        Some(Commands::Parse { format }) => {
            run_parse(&source_file, &root, &format)?;
        }
        
        Some(Commands::Analyze { all, format, show_intents }) => {
            run_analysis_with_intents(&source_file, all, &format, show_intents).await?;
        }
        
        Some(Commands::Synthesize { output, format }) => {
            run_synthesis(&source_file, output, &format).await?;
        }
        
        Some(Commands::Visualize { output, layout, show_values }) => {
            run_visualization(&source_file, output, &layout, show_values).await?;
        }
        
        Some(Commands::Spice { analysis, output, use_metadata }) => {
            run_spice(&source_file, &analysis, output, use_metadata).await?;
        }
        
        Some(Commands::Pipeline { output_dir, no_viz, no_spice }) => {
            run_pipeline(&source_file, &cli.input, output_dir, no_viz, no_spice).await?;
        }
        
        Some(Commands::Simulate { testbench, output, format, verbose: _verbose }) => {
            run_simulation(&source_file, testbench, output, &format).await?;
        }

        Some(Commands::Doc { output, bom_only, budget_only, no_tree, no_patterns }) => {
            cmd_doc(&source_file, output, bom_only, budget_only, no_tree, no_patterns).await?;
        }

        Some(Commands::Intents { show_hints, show_rules, filter, format }) => {
            run_intents_analysis(&source_file, show_hints, show_rules, filter, &format).await?;
        }
    }

    Ok(())
}

fn run_parse(source_file: &SourceFile, root: &bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>, format: &str) -> Result<()> {
    match format {
        "ast" => {
            println!("{:#?}", root);
        }
        "pretty" => {
            println!("{}", "✓ Parse successful".green().bold());
            println!("\n{}", "AST Summary:".bold());
            
            let boards: Vec<_> = source_file.boards().collect();
            let entities: Vec<_> = source_file.entities().collect();

            println!("  Boards: {}", boards.len());
            for board in boards {
                if let Some(name) = board.name() {
                    println!("    • {}", name.text());
                }
            }

            println!("  Entities: {}", entities.len());
            for entity in entities {
                if let Some(name) = entity.name() {
                    println!("    • {}", name.text());
                }
            }
        }
        "json" => {
            // TODO: Implement JSON serialization of AST
            println!("{{\"status\": \"parsed\", \"boards\": {}, \"entities\": {}}}",
                source_file.boards().count(),
                source_file.entities().count()
            );
        }
        _ => {
            eprintln!("Unknown format: {}", format);
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_analysis(source_file: &SourceFile, _show_all: bool, format: &str) -> Result<()> {
    run_analysis_with_intents(source_file, _show_all, format, false).await
}

async fn run_analysis_with_intents(source_file: &SourceFile, _show_all: bool, format: &str, show_intents: bool) -> Result<()> {
    let result = analyze(source_file);

    match format {
        "text" => {
            if result.diagnostics.is_empty() {
                println!("{}", "✓ Analysis successful - no issues found".green().bold());
            } else {
                println!("{}", format!("Analysis found {} diagnostics",
                    result.diagnostics.len()).yellow().bold());

                for diag in &result.diagnostics {
                    println!("  • {}", diag.message);
                }
            }

            // Show intent analysis if requested
            if show_intents {
                println!();
                if let Some(ref flow_tracker) = result.flow_tracker {
                    let flows = flow_tracker.get_flow_paths();
                    let sim_mode = flow_tracker.get_required_sim_mode();

                    println!("{}", "Intent Analysis:".bold());
                    println!("  Flow paths tracked: {}", flows.len());
                    println!("  Required simulation mode: {:?}", sim_mode);

                    // Count intents by category
                    let mut intent_counts: HashMap<String, usize> = HashMap::new();
                    for flow in flows {
                        if let Some(ref intent) = flow.intent {
                            *intent_counts.entry(intent.name.clone()).or_insert(0) += 1;
                        }
                    }

                    if !intent_counts.is_empty() {
                        println!("  Intent usage:");
                        for (intent, count) in intent_counts.iter() {
                            println!("    • {}: {} times", intent, count);
                        }
                    }
                } else {
                    println!("{}", "  No intent analysis available (no boards found)".yellow());
                }
            }
        }
        "json" => {
            // TODO: AnalysisResult doesn't implement Serialize
            let intent_count = result.flow_tracker
                .as_ref()
                .map(|ft| ft.get_flow_paths().len())
                .unwrap_or(0);
            println!("{{\"diagnostics_count\": {}, \"intent_flows\": {}}}",
                result.diagnostics.len(), intent_count);
        }
        _ => {
            eprintln!("Unknown format: {}", format);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_intents_analysis(
    source_file: &SourceFile,
    show_hints: bool,
    show_rules: bool,
    filter: Option<String>,
    format: &str
) -> Result<()> {
    // Run analysis to get intent data
    let result = analyze(source_file);

    if let Some(ref flow_tracker) = result.flow_tracker {
        let flows = flow_tracker.get_flow_paths();
        let sim_mode = flow_tracker.get_required_sim_mode();

        match format {
            "text" => {
                println!("{}", "╔═══════════════════════════════════════════════════════════════════╗".bold());
                println!("{}", "║              BHDL INTENT ANALYSIS                                 ║".bold());
                println!("{}", "╚═══════════════════════════════════════════════════════════════════╝".bold());
                println!();

                println!("{}", "Summary:".bold());
                println!("  Total flow paths: {}", flows.len());
                println!("  Required simulation mode: {:?}", sim_mode);
                println!();

                // Filter flows if requested
                let filtered_flows: Vec<_> = if let Some(ref filter_str) = filter {
                    flows.iter()
                        .filter(|f| f.intent.as_ref()
                            .map(|i| i.name.contains(filter_str))
                            .unwrap_or(false))
                        .collect()
                } else {
                    flows.iter().collect()
                };

                if filtered_flows.is_empty() {
                    if filter.is_some() {
                        println!("{}", format!("No flows matching filter: {}", filter.unwrap()).yellow());
                    } else {
                        println!("{}", "No flow paths with intents found".yellow());
                    }
                    return Ok(());
                }

                println!("{}", format!("Flow Paths ({} shown):", filtered_flows.len()).bold());
                println!();

                for (i, flow) in filtered_flows.iter().enumerate() {
                    println!("{}. Flow Path:", i + 1);
                    println!("   Nets: {}", flow.nets.join(" -> "));

                    if let Some(ref intent) = flow.intent {
                        println!("   Intent: {}", intent.name.bold().green());

                        // Show parameters
                        if !intent.params.is_empty() {
                            println!("   Parameters:");
                            for param in &intent.params {
                                match param {
                                    bhdl_common::IntentParam::Named(name, value) => {
                                        println!("     • {}: {:?}", name, value);
                                    }
                                    bhdl_common::IntentParam::Positional(value) => {
                                        println!("     • {:?}", value);
                                    }
                                }
                            }
                        }

                        if let Some(ref intent_result) = flow.intent_result {
                            println!("   Simulation Mode: {:?}", intent_result.sim_mode);

                            // Show synthesis hints if requested
                            if show_hints && !intent_result.synthesis_hints.is_empty() {
                                println!("   Synthesis Hints:");
                                for hint in &intent_result.synthesis_hints {
                                    match hint {
                                        bhdl_common::SynthesisHint::Custom(s) => {
                                            println!("     • {}", s);
                                        }
                                        _ => println!("     • {:?}", hint),
                                    }
                                }
                            }

                            // Show validation rules if requested
                            if show_rules && !intent_result.validation_rules.is_empty() {
                                println!("   Validation Rules:");
                                for rule in &intent_result.validation_rules {
                                    println!("     • Condition: {}", rule.condition);
                                    println!("       Error if violated: {}", rule.error_message);
                                }
                            }
                        }
                    } else {
                        println!("   Intent: {}", "none".dimmed());
                    }

                    println!();
                }

                // Print statistics
                println!("{}", "Intent Statistics:".bold());
                let mut intent_counts: HashMap<String, usize> = HashMap::new();
                let mut sim_mode_counts: HashMap<String, usize> = HashMap::new();

                for flow in flows.iter() {
                    if let Some(ref intent) = flow.intent {
                        *intent_counts.entry(intent.name.clone()).or_insert(0) += 1;

                        if let Some(ref result) = flow.intent_result {
                            let mode_str = format!("{:?}", result.sim_mode);
                            *sim_mode_counts.entry(mode_str).or_insert(0) += 1;
                        }
                    }
                }

                if !intent_counts.is_empty() {
                    println!("  Intent usage:");
                    let mut sorted_intents: Vec<_> = intent_counts.iter().collect();
                    sorted_intents.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
                    for (intent, count) in sorted_intents {
                        println!("    • {}: {} times", intent, count);
                    }
                }

                if !sim_mode_counts.is_empty() {
                    println!("  Simulation mode distribution:");
                    for (mode, count) in sim_mode_counts.iter() {
                        println!("    • {}: {} flows", mode, count);
                    }
                }
            }
            "json" => {
                // Simple JSON output
                let intent_names: Vec<_> = flows.iter()
                    .filter_map(|f| f.intent.as_ref().map(|i| i.name.clone()))
                    .collect();

                let json = serde_json::json!({
                    "flow_count": flows.len(),
                    "required_sim_mode": format!("{:?}", sim_mode),
                    "intents": intent_names,
                });

                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            _ => {
                eprintln!("Unknown format: {}", format);
                std::process::exit(1);
            }
        }
    } else {
        println!("{}", "No intent analysis available (no boards found in circuit)".yellow());
        println!("Make sure your BHDL file contains at least one board definition.");
    }

    Ok(())
}

async fn run_synthesis(source_file: &SourceFile, output: Option<PathBuf>, format: &str) -> Result<()> {
    // First run analysis
    let analysis = analyze(source_file);
    
    // Note: Simple diagnostics don't have severity, so we can't check for errors specifically
    if !analysis.diagnostics.is_empty() {
        eprintln!("{}", "Warning: Analysis found issues".yellow().bold());
    }
    
    // Synthesize
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await
        .context("Failed to synthesize netlist")?;
    
    println!("{}", "✓ Synthesis successful".green().bold());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    // Output netlist
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&netlist)?;
            if let Some(path) = output {
                fs::write(&path, json)?;
                println!("  Written to: {}", path.display());
            } else {
                println!("\n{}", json);
            }
        }
        "spice" => {
            // Convert to SPICE format
            let mut converter = NetlistToSpiceConverter::new();
            let circuit = converter.convert(&netlist)?;
            
            let mut spice_netlist = String::new();
            spice_netlist.push_str("* BHDL Generated SPICE Netlist\n");
            spice_netlist.push_str(&format!("* Circuit: BHDL Circuit\n\n"));
            
            for (_, component) in circuit.branches() {
                // Format depends on component type
                let nodes_str = component.nodes().iter()
                    .map(|n| format!("n{}", n.index()))
                    .collect::<Vec<_>>()
                    .join(" ");
                
                // Use component_type field to determine formatting
                match component.component_type.as_str() {
                    "Resistor" => {
                        spice_netlist.push_str(&format!("{} {} {:.0}\n", 
                            component.name(), nodes_str, component.value));
                    }
                    "Capacitor" => {
                        spice_netlist.push_str(&format!("{} {} {:.1e}\n", 
                            component.name(), nodes_str, component.value));
                    }
                    "Inductor" => {
                        spice_netlist.push_str(&format!("{} {} {:.1e}\n", 
                            component.name(), nodes_str, component.value));
                    }
                    "VoltageSource" => {
                        spice_netlist.push_str(&format!("{} {} DC {}\n", 
                            component.name(), nodes_str, component.value));
                    }
                    _ => {
                        spice_netlist.push_str(&format!("{} {} ; TODO: format model\n", 
                            component.name(), nodes_str));
                    }
                }
            }
            
            if let Some(path) = output {
                fs::write(&path, spice_netlist)?;
                println!("  Written to: {}", path.display());
            } else {
                println!("\n{}", spice_netlist);
            }
        }
        _ => {
            eprintln!("Unknown format: {}", format);
            std::process::exit(1);
        }
    }
    
    Ok(())
}

async fn run_visualization(source_file: &SourceFile, output: Option<PathBuf>, layout: &str, show_values: bool) -> Result<()> {
    // Run full pipeline to get netlist
    let analysis = analyze(source_file);

    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;

    // Configure visualization (layout algorithm is handled by semantic visualizer)
    // TODO: Pass layout algorithm preference when API supports it
    info!("Using layout algorithm: {}", layout);
    info!("Show values: {}", show_values);

    // Get database component instances from synthesizer
    let components = generator.get_component_instances();
    info!("Retrieved {} database component instances for visualization", components.len());

    // Generate SVG using semantic visualizer
    let svg = render_circuit_with_analysis(&netlist, &components, Some(&analysis), None).await?;

    let output_path = output.unwrap_or_else(|| PathBuf::from("circuit.svg"));
    fs::write(&output_path, svg)?;

    println!("{}", "✓ Visualization generated".green().bold());
    println!("  Output: {}", output_path.display());

    Ok(())
}

async fn run_spice(source_file: &SourceFile, analysis_type: &str, _output: Option<PathBuf>, use_metadata: bool) -> Result<()> {
    // Run pipeline to get netlist
    let analysis_result = analyze(source_file);
    
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis_result).await?;
    
    // Create unified analysis data and augment with SPICE information
    let mut analysis_data = AnalysisData::default();
    let mut augmenter = SpiceAnalysisAugmenter::new();
    augmenter.augment(&netlist, &mut analysis_data)?;
    
    // Convert to SPICE
    let mut converter = NetlistToSpiceConverter::new();
    let circuit = converter.convert(&netlist)?;
    let instance_mapping = HashMap::new(); // TODO: Get proper instance mapping
    
    match analysis_type {
        "roles" => {
            // Component role detection
            // TODO: Re-enable metadata support once analysis data conversion is implemented
            let detector = ComponentRoleDetector::with_netlist(circuit.clone(), &netlist, instance_mapping);
            if use_metadata {
                eprintln!("Warning: Pin metadata support temporarily disabled due to architecture refactoring");
            }
            
            let roles = detector.detect_all_roles();
            
            println!("{}", "Component Role Analysis:".bold());
            println!("  Using metadata: {}", if use_metadata { "yes" } else { "no" });
            println!();
            
            for (comp_id, component) in circuit.branches() {
                if let Some(role) = roles.get(&comp_id) {
                    println!("  {} ({}) -> {:?}", 
                        component.name().bold(),
                        &component.component_type,
                        role
                    );
                }
            }
        }
        "dc" => {
            println!("DC analysis not yet implemented in CLI");
        }
        _ => {
            eprintln!("Unknown analysis type: {}", analysis_type);
            std::process::exit(1);
        }
    }
    
    Ok(())
}

async fn run_pipeline(source_file: &SourceFile, _input_path: &PathBuf, output_dir: PathBuf, no_viz: bool, no_spice: bool) -> Result<()> {
    println!("{}", "Running complete BHDL pipeline...".bold());
    
    // Create output directory
    fs::create_dir_all(&output_dir)?;
    
    // Step 1: Analysis
    println!("\n{}", "1. Analysis".blue().bold());
    let analysis = analyze(source_file);
    
    // TODO: AnalysisResult doesn't implement Serialize
    // let analysis_path = output_dir.join("analysis.json");
    // fs::write(&analysis_path, serde_json::to_string_pretty(&analysis)?)?;
    // println!("  ✓ Analysis saved to {}", analysis_path.display());
    println!("  ✓ Analysis complete (JSON export not implemented)");
    
    // Step 2: Synthesis
    println!("\n{}", "2. Synthesis".blue().bold());
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;
    
    let netlist_path = output_dir.join("netlist.json");
    fs::write(&netlist_path, serde_json::to_string_pretty(&netlist)?)?;
    println!("  ✓ Netlist saved to {}", netlist_path.display());
    
    // Step 3: Visualization
    if !no_viz {
        println!("\n{}", "3. Visualization".blue().bold());
        // Get database component instances from synthesizer
        let components = generator.get_component_instances();
        info!("Retrieved {} database component instances for visualization", components.len());
        let svg = render_circuit_with_analysis(&netlist, &components, Some(&analysis), None).await?;
        
        let svg_path = output_dir.join("circuit.svg");
        fs::write(&svg_path, svg)?;
        println!("  ✓ SVG saved to {}", svg_path.display());
    }
    
    // Step 4: SPICE Analysis
    if !no_spice {
        println!("\n{}", "4. SPICE Analysis".blue().bold());
        
        // Create unified analysis data
        let mut analysis_data = AnalysisData::new();
        
        // Convert analyzer results to common format
        // TODO: Implement proper conversion once analyzer exports this
        // analysis_data = convert_to_analysis_data(&analysis);
        
        // Augment with SPICE analysis
        let mut augmenter = SpiceAnalysisAugmenter::new();
        augmenter.augment(&netlist, &mut analysis_data)?;
        
        // Save augmented analysis data
        let analysis_path = output_dir.join("analysis_augmented.json");
        fs::write(&analysis_path, serde_json::to_string_pretty(&analysis_data)?)?;
        println!("  ✓ Augmented analysis saved to {}", analysis_path.display());
        
        // Extract component roles from augmented data
        let mut roles_output = String::new();
        for (instance_name, instance_data) in &analysis_data.instance_analysis {
            if let Some(spice_type) = &instance_data.spice_type {
                roles_output.push_str(&format!("{}: {} ({})\n", 
                    instance_name, 
                    spice_type,
                    instance_data.component_role.as_ref().unwrap_or(&"unknown".to_string())
                ));
            }
        }
        
        let roles_path = output_dir.join("component_roles.txt");
        fs::write(&roles_path, roles_output)?;
        println!("  ✓ Component roles saved to {}", roles_path.display());
    }
    
    println!("\n{}", "✓ Pipeline complete!".green().bold());
    println!("  All outputs saved to: {}", output_dir.display());
    
    Ok(())
}

async fn run_simulation(source_file: &SourceFile, testbench_path: PathBuf, output_dir: PathBuf, format: &str) -> Result<()> {
    println!("{}", "Running BHDL simulation...".bold());
    
    // Create output directory
    fs::create_dir_all(&output_dir)?;
    
    // Step 1: Run analysis on circuit
    println!("\n{}", "1. Analyzing circuit".blue().bold());
    let analysis = analyze(source_file);
    
    if !analysis.diagnostics.is_empty() {
        eprintln!("{}", "Warning: Circuit has diagnostics".yellow());
        for diag in &analysis.diagnostics {
            eprintln!("  • {}", diag.message);
        }
    }
    
    // Step 2: Synthesize netlist
    println!("\n{}", "2. Synthesizing netlist".blue().bold());
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(source_file, &analysis).await?;
    println!("  ✓ Netlist generated: {} instances, {} nets", 
        netlist.instances.len(), netlist.nets.len());
    
    // Step 3: Parse testbench
    println!("\n{}", "3. Loading testbench".blue().bold());
    let testbench_content = fs::read_to_string(&testbench_path)
        .with_context(|| format!("Failed to read testbench: {}", testbench_path.display()))?;
    
    // Parse the testbench
    let parse_result = bhdl_parser::parse(&testbench_content);
    if !parse_result.errors().is_empty() {
        for error in parse_result.errors() {
            eprintln!("Parse error: {:?}", error);
        }
        anyhow::bail!("Failed to parse testbench due to errors");
    }
    
    // Convert to AST
    let ast = bhdl_ast::SourceFile::cast(parse_result.syntax())
        .ok_or_else(|| anyhow::anyhow!("Failed to get SourceFile from parse result"))?;
    
    // Find the testbench definition
    let testbench_def = ast.testbenches()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No testbench found in file"))?;
    
    // Compile testbench to runtime structure
    let testbench = bhdl_testbench::compiler::compile_testbench(&testbench_def)?;
    
    println!("  ✓ Testbench loaded: {}", testbench.name);
    println!("    Duration: {}ms", testbench.simulation_config.duration.value);
    println!("    Timestep: {}µs", testbench.simulation_config.timestep.value);
    
    // Step 4: Run simulation
    println!("\n{}", "4. Running simulation".blue().bold());
    
    // Get flow tracker if using behavioral simulation
    use bhdl_testbench::testbench::SolverType;
    let flow_tracker = if matches!(testbench.simulation_config.solver_type, 
                                  SolverType::Behavioral | SolverType::MixedSignal { .. }) {
        // TODO: Get flow tracker from analyzer
        None
    } else {
        None
    };
    
    let mut runner = TestbenchRunner::new(testbench, netlist, flow_tracker)?;
    
    // Set up waveform output
    let waveform_format = match format {
        "vcd" => WaveformFormat::VCD,
        "csv" => WaveformFormat::CSV,
        "json" => WaveformFormat::JSON,
        _ => {
            eprintln!("Unknown waveform format: {}, using VCD", format);
            WaveformFormat::VCD
        }
    };
    
    let waveform_path = output_dir.join(format!("simulation.{}", format));
    runner.add_waveform_output(waveform_format, &waveform_path)?;
    
    // Run simulation
    let results = runner.run()?;
    
    // Step 5: Report results
    println!("\n{}", "5. Simulation Results".blue().bold());
    
    if results.passed {
        println!("{}", "  ✓ All assertions passed".green());
    } else {
        println!("{}", format!("  ✗ {} assertions failed", results.violations.len()).red());
        for violation in &results.violations {
            println!("    • {} @ {:.3}ms: {}", 
                violation.assertion_name,
                violation.time * 1000.0,
                violation.message
            );
        }
    }
    
    if !results.measurements.is_empty() {
        println!("\n  Measurements:");
        for (name, value) in &results.measurements {
            println!("    {}: {:.3}", name, value);
        }
    }
    
    println!("\n  Waveform saved to: {}", waveform_path.display());
    println!("  Simulation time: {:.3}ms", results.simulation_time * 1000.0);
    
    // Save results summary
    let summary_path = output_dir.join("simulation_summary.json");
    let summary = serde_json::json!({
        "passed": results.passed,
        "violations": results.violations.len(),
        "measurements": results.measurements,
        "simulation_time_ms": results.simulation_time * 1000.0,
        "waveform_file": waveform_path.file_name().unwrap().to_str().unwrap()
    });
    
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    println!("  Summary saved to: {}", summary_path.display());

    Ok(())
}

async fn cmd_doc(
    source_file: &SourceFile,
    output: PathBuf,
    bom_only: bool,
    budget_only: bool,
    no_tree: bool,
    no_patterns: bool,
) -> Result<()> {
    println!("{}", "Generating power domain documentation...".bold());

    // Step 1: Run analysis to get power domain expansion
    println!("\n{}", "1. Analyzing circuit".blue().bold());
    let analysis = analyze(source_file);

    if !analysis.diagnostics.is_empty() {
        eprintln!("{}", "Warning: Circuit has diagnostics".yellow());
        for diag in &analysis.diagnostics {
            eprintln!("  • {}", diag.message);
        }
    }

    // Check if there are power domains to document
    let expansion = &analysis.power_domain_expansion;
    if expansion.connections.is_empty() && expansion.decoupling_caps.is_empty() {
        eprintln!("{}", "Warning: No power domains found in circuit".yellow());
        eprintln!("  Make sure your circuit defines power domains using:");
        eprintln!("    power_domain @VCC_3V3 = 3.3V @ 1A {{ ... }}");
        return Ok(());
    }

    // Count unique domains from connections
    let domain_names: std::collections::HashSet<_> = expansion.connections
        .iter()
        .map(|conn| &conn.source_net)
        .collect();

    println!("  ✓ Found {} power domain(s)", domain_names.len());
    println!("    Connections: {}", expansion.connections.len());
    println!("    Capacitors: {}", expansion.decoupling_caps.len());

    // Step 2: Configure documentation options based on flags
    println!("\n{}", "2. Configuring documentation options".blue().bold());

    // Handle mutually exclusive flags
    let (include_bom, include_budget, include_connections, include_tree) = if bom_only {
        println!("  Mode: BOM only");
        (true, false, false, false)
    } else if budget_only {
        println!("  Mode: Budget analysis only");
        (false, true, false, false)
    } else {
        println!("  Mode: Full documentation");
        (true, true, true, !no_tree)
    };

    let options = DocumentationOptions {
        format: OutputFormat::Markdown,
        include_power_tree: include_tree,
        include_bom,
        include_budget,
        include_connections,
        include_summary: true, // Always include summary
        show_patterns: !no_patterns,
    };

    // Step 3: Generate documentation
    println!("\n{}", "3. Generating documentation".blue().bold());
    let documentation = generate_documentation(expansion, options)
        .context("Failed to generate documentation")?;

    // Step 4: Write to output file
    fs::write(&output, &documentation)
        .with_context(|| format!("Failed to write to: {}", output.display()))?;

    // Step 5: Print success summary
    println!("\n{}", "✓ Documentation generated".green().bold());
    println!("  Output: {}", output.display());
    println!("  Size: {} bytes", documentation.len());

    // Print section breakdown
    let sections: Vec<&str> = documentation
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();

    if !sections.is_empty() {
        println!("  Sections:");
        for section in sections {
            println!("    • {}", section.trim_start_matches("## "));
        }
    }

    Ok(())
}